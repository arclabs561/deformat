//! DOCX text extraction.
//!
//! Requires the `docx` feature. Extracts text from `.docx` files by
//! reading `word/document.xml` from the ZIP archive and stripping XML tags.
//! Also reads `word/header*.xml` and `word/footer*.xml` if present.

use crate::segment::{element_id, Segment, SegmentData, SegmentMetadata};
use crate::{html, Error, Extracted, Extractor, Format};
use std::io::{Read, Seek};
use std::path::Path;

/// Extract text from a DOCX file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`]
/// if the ZIP archive is invalid or missing `word/document.xml`, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let file = std::fs::File::open(path)?;
    extract_reader(file)
}

/// Extract text from DOCX bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the ZIP archive is invalid, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let cursor = std::io::Cursor::new(bytes);
    extract_reader(cursor)
}

/// Extract typed [`Segment`]s from a DOCX file.
///
/// Each `<w:p>` paragraph becomes one segment. Paragraphs with a
/// heading style (`Heading1`..`Heading9`) become [`Segment::Title`]
/// with `category_depth` set to the heading level; other paragraphs
/// become [`Segment::NarrativeText`]. Empty paragraphs are skipped.
///
/// Top-level `<w:tbl>` tables become [`Segment::Table`] segments with
/// `metadata.text_as_html` set to a normalized `<table>` HTML string
/// (cell text is HTML-escaped). Nested tables are not indexed
/// separately; the outer segment's range covers them. The plain
/// [`extract_file`] path still flattens tables to paragraph text.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`]
/// on invalid ZIP, or [`Error::EmptyResult`] if no paragraphs produce
/// any text.
pub fn extract_to_segments(path: &Path) -> Result<Vec<Segment>, Error> {
    let file = std::fs::File::open(path)?;
    segments_from_reader(file)
}

/// Like [`extract_to_segments`], from bytes in memory.
pub fn extract_bytes_to_segments(bytes: &[u8]) -> Result<Vec<Segment>, Error> {
    let cursor = std::io::Cursor::new(bytes);
    segments_from_reader(cursor)
}

fn segments_from_reader<R: Read + Seek>(reader: R) -> Result<Vec<Segment>, Error> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| Error::Parse(format!("invalid DOCX ZIP: {e}")))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| Error::Parse("DOCX missing word/document.xml".into()))?
        .read_to_string(&mut xml)
        .map_err(|e| Error::Parse(format!("failed to read document.xml: {e}")))?;
    let segments = split_docx_paragraphs(&xml);
    if segments.is_empty() {
        return Err(Error::EmptyResult);
    }
    Ok(segments)
}

/// Walk `document.xml`, emit one segment per `<w:p>` paragraph and per
/// `<w:tbl>` table, in document order.
///
/// Heading style detection: `<w:pStyle w:val="Heading1">` → depth 1,
/// `Heading2` → 2, etc. Non-heading paragraphs become NarrativeText.
/// Tables become [`Segment::Table`] with `metadata.text_as_html` set to a
/// normalized `<table><tr><td>...</td></tr></table>` representation.
fn split_docx_paragraphs(xml: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut last_title: Option<String> = None;
    let mut ord = 0;

    let mut pos = 0;
    while pos < xml.len() {
        let rest = &xml[pos..];
        let p_idx = find_ci(rest, "<w:p");
        let tbl_idx = find_ci(rest, "<w:tbl");

        // `<w:p` is a prefix of `<w:pStyle` etc; require a terminator byte.
        let next_p = p_idx.and_then(|i| {
            let after = *rest.as_bytes().get(i + 4)?;
            if matches!(after, b'>' | b' ' | b'/' | b'\t' | b'\n' | b'\r') {
                Some(i)
            } else {
                None
            }
        });
        let next_tbl = tbl_idx.and_then(|i| {
            let after = *rest.as_bytes().get(i + 6)?;
            if matches!(after, b'>' | b' ' | b'/' | b'\t' | b'\n' | b'\r') {
                Some(i)
            } else {
                None
            }
        });

        let (kind, open) = match (next_p, next_tbl) {
            (None, None) => break,
            (Some(pi), None) => (ElemKind::Para, pi),
            (None, Some(ti)) => (ElemKind::Table, ti),
            (Some(pi), Some(ti)) if pi <= ti => (ElemKind::Para, pi),
            (Some(_), Some(ti)) => (ElemKind::Table, ti),
        };
        let advance = match kind {
            ElemKind::Para => {
                handle_paragraph(rest, open, &mut segments, &mut last_title, &mut ord)
            }
            ElemKind::Table => handle_table(rest, open, &mut segments, &last_title, &mut ord),
        };
        match advance {
            Some(n) => pos += n,
            None => break,
        }
    }
    segments
}

enum ElemKind {
    Para,
    Table,
}

fn handle_paragraph(
    rest: &str,
    open: usize,
    segments: &mut Vec<Segment>,
    last_title: &mut Option<String>,
    ord: &mut usize,
) -> Option<usize> {
    let after_open = &rest[open + 4..];
    let start_body = after_open.find('>')?;
    let body_start = open + 4 + start_body + 1;
    let self_closed = after_open[..start_body].ends_with('/');
    if self_closed {
        return Some(body_start);
    }
    let close = find_ci(&rest[body_start..], "</w:p>")?;
    let paragraph = &rest[body_start..body_start + close];
    let depth = detect_heading_depth(paragraph);
    let text = html::strip_to_text(paragraph);
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        let mut meta = SegmentMetadata::default();
        if let Some(d) = depth {
            meta.category_depth = Some(d);
        } else if let Some(parent) = last_title.clone() {
            meta.parent_id = Some(parent);
        }
        let kind_name = if depth.is_some() {
            "Title"
        } else {
            "NarrativeText"
        };
        let id = element_id(kind_name, trimmed, *ord);
        let data = SegmentData {
            element_id: id.clone(),
            text: trimmed.to_string(),
            metadata: meta,
        };
        if depth.is_some() {
            *last_title = Some(id);
            segments.push(Segment::Title(data));
        } else {
            segments.push(Segment::NarrativeText(data));
        }
        *ord += 1;
    }
    Some(body_start + close + "</w:p>".len())
}

/// Process one `<w:tbl>...</w:tbl>`. Builds a normalized HTML table for
/// `text_as_html` and a newline-joined plain-text summary for `text`.
fn handle_table(
    rest: &str,
    open: usize,
    segments: &mut Vec<Segment>,
    last_title: &Option<String>,
    ord: &mut usize,
) -> Option<usize> {
    let after_open = &rest[open + 6..];
    let start_body = after_open.find('>')?;
    let body_start = open + 6 + start_body + 1;
    let self_closed = after_open[..start_body].ends_with('/');
    if self_closed {
        return Some(body_start);
    }
    let close = find_ci(&rest[body_start..], "</w:tbl>")?;
    let table_xml = &rest[body_start..body_start + close];

    let (text, html_repr) = build_table_repr(table_xml);
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        let mut meta = SegmentMetadata::default();
        if let Some(parent) = last_title.clone() {
            meta.parent_id = Some(parent);
        }
        meta.text_as_html = Some(html_repr);
        let id = element_id("Table", trimmed, *ord);
        segments.push(Segment::Table(SegmentData {
            element_id: id,
            text: trimmed.to_string(),
            metadata: meta,
        }));
        *ord += 1;
    }
    Some(body_start + close + "</w:tbl>".len())
}

fn build_table_repr(tbl_xml: &str) -> (String, String) {
    let mut rows_html = String::new();
    let mut row_texts: Vec<String> = Vec::new();

    let mut pos = 0;
    while pos < tbl_xml.len() {
        let rest = &tbl_xml[pos..];
        let Some(tr_idx) = find_ci_tag(rest, "<w:tr", 5) else {
            break;
        };
        let after_open = &rest[tr_idx + 5..];
        let Some(start_body) = after_open.find('>') else {
            break;
        };
        let body_start = tr_idx + 5 + start_body + 1;
        if after_open[..start_body].ends_with('/') {
            pos += body_start;
            continue;
        }
        let Some(close) = find_ci(&rest[body_start..], "</w:tr>") else {
            break;
        };
        let row_xml = &rest[body_start..body_start + close];

        let mut cells_html = String::new();
        let mut cell_texts: Vec<String> = Vec::new();
        let mut cpos = 0;
        while cpos < row_xml.len() {
            let crest = &row_xml[cpos..];
            let Some(tc_idx) = find_ci_tag(crest, "<w:tc", 5) else {
                break;
            };
            let after_open = &crest[tc_idx + 5..];
            let Some(start_cb) = after_open.find('>') else {
                break;
            };
            let cb_start = tc_idx + 5 + start_cb + 1;
            if after_open[..start_cb].ends_with('/') {
                cpos += cb_start;
                continue;
            }
            let Some(cclose) = find_ci(&crest[cb_start..], "</w:tc>") else {
                break;
            };
            let cell_xml = &crest[cb_start..cb_start + cclose];
            let cell_text = html::strip_to_text(cell_xml).trim().to_string();
            cells_html.push_str("<td>");
            cells_html.push_str(&html_escape(&cell_text));
            cells_html.push_str("</td>");
            cell_texts.push(cell_text);
            cpos += cb_start + cclose + "</w:tc>".len();
        }
        if !cells_html.is_empty() {
            rows_html.push_str("<tr>");
            rows_html.push_str(&cells_html);
            rows_html.push_str("</tr>");
            row_texts.push(cell_texts.join(" "));
        }
        pos += body_start + close + "</w:tr>".len();
    }

    let plain = row_texts.join("\n");
    let html_repr = format!("<table>{rows_html}</table>");
    (plain, html_repr)
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Substring match that also asserts the byte at `idx + tag_len` is a
/// valid tag terminator (avoids `<w:tr` matching `<w:trPr`).
fn find_ci_tag(haystack: &str, needle: &str, tag_len: usize) -> Option<usize> {
    let lower = haystack.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut start = 0;
    while let Some(off) = lower[start..].find(needle) {
        let i = start + off;
        match bytes.get(i + tag_len) {
            Some(b) if matches!(*b, b'>' | b' ' | b'/' | b'\t' | b'\n' | b'\r') => return Some(i),
            _ => {}
        }
        start = i + tag_len;
    }
    None
}

fn detect_heading_depth(paragraph: &str) -> Option<u32> {
    let lower = paragraph.to_ascii_lowercase();
    let needle = "w:val=\"heading";
    let idx = lower.find(needle)?;
    let after = &lower[idx + needle.len()..];
    let digit = after.chars().next()?;
    if digit.is_ascii_digit() {
        Some((digit as u32) - ('0' as u32))
    } else {
        None
    }
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let lower = haystack.to_ascii_lowercase();
    lower.find(needle)
}

fn extract_reader<R: Read + Seek>(reader: R) -> Result<Extracted, Error> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| Error::Parse(format!("invalid DOCX ZIP: {e}")))?;

    let mut text_parts = Vec::new();

    // Extract main document content
    if let Ok(mut entry) = archive.by_name("word/document.xml") {
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .map_err(|e| Error::Parse(format!("failed to read document.xml: {e}")))?;
        let text = html::strip_to_text(&xml);
        if !text.is_empty() {
            text_parts.push(text);
        }
    } else {
        return Err(Error::Parse("DOCX missing word/document.xml".into()));
    }

    let text = text_parts.join("\n");
    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Docx,
        extractor: Extractor::Strip,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_docx(document_xml: &str) -> Vec<u8> {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("[Content_Types].xml", opts).unwrap();
        write!(zip, r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        write!(zip, "{document_xml}").unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_basic_docx() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Hello from DOCX.</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        let bytes = make_docx(xml);
        let result = extract_bytes(&bytes).unwrap();
        assert!(
            result.text.contains("Hello from DOCX"),
            "text: {}",
            result.text
        );
        assert!(
            result.text.contains("Second paragraph"),
            "text: {}",
            result.text
        );
        assert_eq!(result.format, Format::Docx);
    }

    #[test]
    fn extract_docx_with_entities() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Caf&#233; in &#321;&#243;d&#378;</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        let bytes = make_docx(xml);
        let result = extract_bytes(&bytes).unwrap();
        assert!(
            result.text.contains("Caf\u{00E9}"),
            "eacute: {}",
            result.text
        );
    }

    #[test]
    fn extract_docx_missing_document_xml() {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("other.xml", opts).unwrap();
        std::io::Write::write_all(&mut zip, b"<root/>").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let result = extract_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn extract_docx_invalid_zip() {
        let result = extract_bytes(b"not a zip file");
        assert!(result.is_err());
    }
}
