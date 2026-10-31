//! PPTX presentation text extraction.
//!
//! Requires the `pptx` feature. Reads slide XML (`ppt/slides/slide*.xml`)
//! and the optional speaker-notes XML (`ppt/notesSlides/notesSlide*.xml`)
//! from the ZIP archive, strips XML tags, and emits one slide per
//! paragraph in presentation order.
//!
//! Speaker notes (if present) follow their slide, prefixed with
//! `Notes:`. Slide numbering preserves the order in which the file lists
//! them, not the order of appearance in the deck — PPTX sorts slides
//! lexicographically by filename (`slide1.xml`, `slide10.xml`,
//! `slide2.xml`), so this module sorts them numerically.

use crate::{html, Error, Extracted, Extractor, Format};
use std::io::{Read, Seek};
use std::path::Path;

/// Extract text from a PPTX file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`] if
/// the ZIP archive is invalid, or [`Error::EmptyResult`] if no slides
/// produce any text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let file = std::fs::File::open(path)?;
    extract_reader(file)
}

/// Extract text from PPTX bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] on invalid ZIP, or [`Error::EmptyResult`] if
/// no slides produce any text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let cursor = std::io::Cursor::new(bytes);
    extract_reader(cursor)
}

fn extract_reader<R: Read + Seek>(reader: R) -> Result<Extracted, Error> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| Error::Parse(format!("invalid PPTX ZIP: {e}")))?;

    let mut slide_entries: Vec<(u32, String)> = Vec::new();
    let mut notes_entries: Vec<(u32, String)> = Vec::new();
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index(i) else {
            continue;
        };
        let name = entry.name().to_string();
        if let Some(rest) = name.strip_prefix("ppt/slides/slide") {
            if let Some(num) = rest.strip_suffix(".xml") {
                if let Ok(n) = num.parse::<u32>() {
                    slide_entries.push((n, name));
                }
            }
        } else if let Some(rest) = name.strip_prefix("ppt/notesSlides/notesSlide") {
            if let Some(num) = rest.strip_suffix(".xml") {
                if let Ok(n) = num.parse::<u32>() {
                    notes_entries.push((n, name));
                }
            }
        }
    }
    slide_entries.sort_by_key(|(n, _)| *n);
    notes_entries.sort_by_key(|(n, _)| *n);

    // Notes belong to the slide whose relationships part points at them;
    // writers number notes parts independently of slides (python-pptx names
    // the only notes part `notesSlide1.xml` whatever slide owns it). A slide
    // without a relationships part falls back to the same-numbered notes
    // part, unless another slide's relationship claims it.
    let mut linked_notes: std::collections::HashMap<u32, Option<String>> =
        std::collections::HashMap::new();
    for (slide_num, slide_name) in &slide_entries {
        let rels_name = format!("ppt/slides/_rels/slide{slide_num}.xml.rels");
        let Ok(mut entry) = archive.by_name(&rels_name) else {
            continue;
        };
        let mut rels = String::new();
        if crate::zip_entry::read_to_string(&mut entry, &mut rels).is_err() {
            continue;
        }
        linked_notes.insert(*slide_num, notes_target(slide_name, &rels));
    }
    let claimed: std::collections::HashSet<&str> = linked_notes
        .values()
        .flatten()
        .map(String::as_str)
        .collect();
    let notes_by_slide: std::collections::HashMap<u32, String> = slide_entries
        .iter()
        .filter_map(|(slide_num, _)| {
            let notes = match linked_notes.get(slide_num) {
                Some(target) => target.clone(),
                None => notes_entries
                    .iter()
                    .find(|(n, name)| n == slide_num && !claimed.contains(name.as_str()))
                    .map(|(_, name)| name.clone()),
            };
            notes.map(|name| (*slide_num, name))
        })
        .collect();

    let mut parts: Vec<String> = Vec::with_capacity(slide_entries.len());
    for (slide_num, slide_name) in &slide_entries {
        let mut xml = String::new();
        if let Ok(mut entry) = archive.by_name(slide_name) {
            crate::zip_entry::read_to_string(&mut entry, &mut xml)
                .map_err(|e| Error::Parse(format!("failed to read {slide_name}: {e}")))?;
        }
        let text = strip_slide_xml(&xml);
        if !text.trim().is_empty() {
            parts.push(text);
        }

        if let Some(notes_name) = notes_by_slide.get(slide_num) {
            let mut notes_xml = String::new();
            if let Ok(mut entry) = archive.by_name(notes_name) {
                let _ = crate::zip_entry::read_to_string(&mut entry, &mut notes_xml);
            }
            let notes_text = strip_slide_xml(&notes_xml);
            if !notes_text.trim().is_empty() {
                parts.push(format!("Notes: {notes_text}"));
            }
        }
    }

    let text = parts.join("\n\n");
    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Pptx,
        extractor: Extractor::Strip,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

/// The ZIP entry name of the notes part a slide's relationships point at.
fn notes_target(slide_name: &str, rels: &str) -> Option<String> {
    let dir = slide_name.rfind('/').map_or("", |i| &slide_name[..=i]);
    for (start, _) in rels.match_indices("<Relationship ") {
        let end = rels[start..].find('>').map_or(rels.len(), |e| start + e);
        let element = &rels[start..end];
        let is_notes = xml_attr(element, "Type").is_some_and(|t| t.ends_with("/notesSlide"));
        if !is_notes {
            continue;
        }
        let target = xml_attr(element, "Target")?;
        let joined = match target.strip_prefix('/') {
            Some(absolute) => absolute.to_string(),
            None => format!("{dir}{target}"),
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                _ => parts.push(part),
            }
        }
        return Some(parts.join("/"));
    }
    None
}

/// An attribute value from an XML start tag, single- or double-quoted.
fn xml_attr<'a>(element: &'a str, attr: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let pattern = format!(" {attr}={quote}");
        if let Some(idx) = element.find(&pattern) {
            let start = idx + pattern.len();
            let end = element[start..].find(quote)? + start;
            return Some(&element[start..end]);
        }
    }
    None
}

/// Strip slide or notes XML to text, one line per `a:p` paragraph.
///
/// `a:p` is not an HTML block tag, and PowerPoint writes no whitespace
/// between paragraphs, so each paragraph end is marked as a line break.
fn strip_slide_xml(xml: &str) -> String {
    html::strip_to_text(&xml.replace("</a:p>", "</a:p><br>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pptx(slides: &[(u32, &str)], notes: &[(u32, &str)]) -> Vec<u8> {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("[Content_Types].xml", opts).unwrap();
        write!(zip, r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
        zip.start_file("ppt/presentation.xml", opts).unwrap();
        write!(zip, r#"<?xml version="1.0"?><presentation/>"#).unwrap();
        for (n, body) in slides {
            zip.start_file(format!("ppt/slides/slide{n}.xml"), opts)
                .unwrap();
            write!(zip, r#"<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"#).unwrap();
        }
        for (n, body) in notes {
            zip.start_file(format!("ppt/notesSlides/notesSlide{n}.xml"), opts)
                .unwrap();
            write!(zip, r#"<?xml version="1.0"?><p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:notes>"#).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_basic_pptx() {
        let bytes = make_pptx(
            &[
                (1, "<a:t>Title slide</a:t>"),
                (2, "<a:t>Second slide content</a:t>"),
            ],
            &[],
        );
        let result = extract_bytes(&bytes).unwrap();
        assert!(result.text.contains("Title slide"), "text: {}", result.text);
        assert!(
            result.text.contains("Second slide content"),
            "text: {}",
            result.text
        );
        assert_eq!(result.format, Format::Pptx);
    }

    #[test]
    fn extract_pptx_with_speaker_notes() {
        let bytes = make_pptx(
            &[(1, "<a:t>Visible</a:t>")],
            &[(1, "<a:t>Private speaker commentary</a:t>")],
        );
        let result = extract_bytes(&bytes).unwrap();
        assert!(result.text.contains("Visible"));
        assert!(
            result.text.contains("Private speaker commentary"),
            "notes missing: {}",
            result.text
        );
        assert!(
            result.text.contains("Notes:"),
            "notes label missing: {}",
            result.text
        );
    }

    /// PowerPoint writes slide XML with no whitespace between elements, so
    /// paragraph boundaries must come from `a:p` itself, in notes too.
    #[test]
    fn paragraphs_are_separated_without_source_whitespace() {
        let bytes = make_pptx(
            &[(
                1,
                "<p:sp><p:txBody><a:p><a:r><a:t>Quarterly results</a:t></a:r></a:p><a:p><a:pPr lvl=\"1\"/><a:r><a:t>Revenue up</a:t></a:r></a:p></p:txBody></p:sp>",
            )],
            &[(
                1,
                "<p:sp><p:txBody><a:p><a:r><a:t>First note</a:t></a:r></a:p><a:p><a:r><a:t>Second note</a:t></a:r></a:p></p:txBody></p:sp>",
            )],
        );
        let result = extract_bytes(&bytes).unwrap();
        assert_eq!(
            result.text,
            "Quarterly results\nRevenue up\n\nNotes: First note\nSecond note"
        );
    }

    #[test]
    fn extract_pptx_sorts_slides_numerically() {
        // `slide10` must come after `slide2`, not before (naive string sort would
        // put it before).
        let bytes = make_pptx(
            &[
                (1, "<a:t>one</a:t>"),
                (2, "<a:t>two</a:t>"),
                (10, "<a:t>ten</a:t>"),
            ],
            &[],
        );
        let result = extract_bytes(&bytes).unwrap();
        let one = result.text.find("one").unwrap();
        let two = result.text.find("two").unwrap();
        let ten = result.text.find("ten").unwrap();
        assert!(one < two && two < ten, "got: {}", result.text);
    }

    #[test]
    fn extract_pptx_empty_deck_returns_empty_result() {
        let bytes = make_pptx(&[], &[]);
        let result = extract_bytes(&bytes);
        assert!(matches!(result, Err(Error::EmptyResult)));
    }

    #[test]
    fn extract_pptx_invalid_zip() {
        let result = extract_bytes(b"not a zip file");
        assert!(matches!(result, Err(Error::Parse(_))));
    }
}
