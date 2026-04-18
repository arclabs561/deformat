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

    let notes_by_slide: std::collections::HashMap<u32, String> = notes_entries
        .iter()
        .map(|(n, name)| (*n, name.clone()))
        .collect();

    let mut parts: Vec<String> = Vec::with_capacity(slide_entries.len());
    for (slide_num, slide_name) in &slide_entries {
        let mut xml = String::new();
        if let Ok(mut entry) = archive.by_name(slide_name) {
            entry
                .read_to_string(&mut xml)
                .map_err(|e| Error::Parse(format!("failed to read {slide_name}: {e}")))?;
        }
        let text = html::strip_to_text(&xml);
        if !text.trim().is_empty() {
            parts.push(text);
        }

        if let Some(notes_name) = notes_by_slide.get(slide_num) {
            let mut notes_xml = String::new();
            if let Ok(mut entry) = archive.by_name(notes_name) {
                let _ = entry.read_to_string(&mut notes_xml);
            }
            let notes_text = html::strip_to_text(&notes_xml);
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
