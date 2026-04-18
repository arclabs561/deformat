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

/// Walk `document.xml`, emit one segment per `<w:p>` paragraph.
///
/// Heading style detection: `<w:pStyle w:val="Heading1">` → depth 1,
/// `Heading2` → 2, etc. Non-heading paragraphs become NarrativeText.
fn split_docx_paragraphs(xml: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut last_title: Option<String> = None;
    let mut ord = 0;

    let mut rest = xml;
    while let Some(open) = find_ci(rest, "<w:p") {
        // Accept both `<w:p>` and `<w:p ...>` (with attrs).
        let after_open = &rest[open + 4..];
        // Skip past `>` or self-closing `/>`.
        let Some(start_body) = after_open.find('>') else {
            break;
        };
        let body_start = open + 4 + start_body + 1;
        let self_closed = after_open[..start_body].ends_with('/');
        if self_closed {
            rest = &rest[body_start..];
            continue;
        }
        // Find matching `</w:p>`; DOCX paragraphs don't nest.
        let Some(close) = find_ci(&rest[body_start..], "</w:p>") else {
            break;
        };
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
            let id = element_id(kind_name, trimmed, ord);
            let data = SegmentData {
                element_id: id.clone(),
                text: trimmed.to_string(),
                metadata: meta,
            };
            if depth.is_some() {
                last_title = Some(id);
                segments.push(Segment::Title(data));
            } else {
                segments.push(Segment::NarrativeText(data));
            }
            ord += 1;
        }
        rest = &rest[body_start + close + 6..];
    }
    segments
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
