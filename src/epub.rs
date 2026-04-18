//! EPUB text extraction.
//!
//! Requires the `epub` feature. Extracts text from `.epub` files by
//! reading XHTML content documents from the ZIP archive and stripping
//! tags. Reads the OPF manifest to find content files in reading order.

use crate::{html, Error, Extracted, Extractor, Format};
use std::io::{Read, Seek};
use std::path::Path;

/// Extract text from an EPUB file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`]
/// if the ZIP/EPUB structure is invalid, or [`Error::EmptyResult`] if
/// extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let file = std::fs::File::open(path)?;
    extract_reader(file)
}

/// Extract text from EPUB bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the ZIP archive is invalid, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let cursor = std::io::Cursor::new(bytes);
    extract_reader(cursor)
}

fn extract_reader<R: Read + Seek>(reader: R) -> Result<Extracted, Error> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| Error::Parse(format!("invalid EPUB ZIP: {e}")))?;

    // Find content files: look for .xhtml, .html, .htm files in the archive
    let mut content_files: Vec<String> = Vec::new();

    // Try to read the OPF file for proper reading order
    let opf_path = find_opf_path(&mut archive);
    if let Some(ref opf) = opf_path {
        if let Ok(items) = parse_opf_spine(&mut archive, opf) {
            content_files = items;
        }
    }

    // Fallback: collect all HTML/XHTML files sorted by name
    if content_files.is_empty() {
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm") {
                    content_files.push(name);
                }
            }
        }
        content_files.sort();
    }

    let mut chapters = Vec::new();
    for name in &content_files {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut xhtml = String::new();
            if entry.read_to_string(&mut xhtml).is_ok() {
                let text = html::strip_to_text(&xhtml);
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    chapters.push(trimmed);
                }
            }
        }
    }

    let text = chapters.join("\n\n");
    if text.is_empty() {
        return Err(Error::EmptyResult);
    }

    // Try to extract title from OPF metadata
    let title = opf_path.and_then(|p| extract_opf_title(&mut archive, &p));

    Ok(Extracted {
        text,
        format: Format::Epub,
        extractor: Extractor::Strip,
        title,
        excerpt: None,
        fallback: false,
    })
}

/// Find the OPF file path from META-INF/container.xml.
fn find_opf_path<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let mut entry = archive.by_name("META-INF/container.xml").ok()?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).ok()?;
    // Look for full-path="..." in the rootfile element
    let idx = xml.find("full-path=\"")?;
    let start = idx + 11;
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_string())
}

/// Parse the OPF spine to get content files in reading order.
fn parse_opf_spine<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    opf_path: &str,
) -> Result<Vec<String>, ()> {
    let mut entry = archive.by_name(opf_path).map_err(|_| ())?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).map_err(|_| ())?;

    // Extract the directory prefix for relative paths
    let prefix = if let Some(slash) = opf_path.rfind('/') {
        &opf_path[..=slash]
    } else {
        ""
    };

    // Parse manifest: <item id="..." href="..." media-type="..."/>
    let mut id_to_href = std::collections::HashMap::new();
    for item_start in xml.match_indices("<item ").map(|(i, _)| i) {
        let item_end = match xml[item_start..]
            .find("/>")
            .or_else(|| xml[item_start..].find('>'))
        {
            Some(e) => item_start + e,
            None => continue,
        };
        let item = &xml[item_start..item_end];

        let id = extract_xml_attr(item, "id");
        let href = extract_xml_attr(item, "href");
        let media = extract_xml_attr(item, "media-type");

        if let (Some(id), Some(href), Some(media)) = (id, href, media) {
            if media.contains("xhtml") || media.contains("html") {
                id_to_href.insert(id.to_string(), format!("{prefix}{href}"));
            }
        }
    }

    // Parse spine: <itemref idref="..."/>
    let mut ordered = Vec::new();
    for itemref_start in xml.match_indices("<itemref ").map(|(i, _)| i) {
        let itemref_end = match xml[itemref_start..]
            .find("/>")
            .or_else(|| xml[itemref_start..].find('>'))
        {
            Some(e) => itemref_start + e,
            None => continue,
        };
        let itemref = &xml[itemref_start..itemref_end];
        if let Some(idref) = extract_xml_attr(itemref, "idref") {
            if let Some(href) = id_to_href.get(idref) {
                ordered.push(href.clone());
            }
        }
    }

    if ordered.is_empty() {
        Err(())
    } else {
        Ok(ordered)
    }
}

/// Extract title from OPF metadata.
fn extract_opf_title<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    opf_path: &str,
) -> Option<String> {
    let mut entry = archive.by_name(opf_path).ok()?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).ok()?;

    let start = xml
        .find("<dc:title")?
        .checked_add(xml[xml.find("<dc:title")?..].find('>')?)?;
    let start = start + 1;
    let end = xml[start..].find("</dc:title>")? + start;
    let title = xml[start..end].trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Extract an attribute value from an XML element string.
fn extract_xml_attr<'a>(element: &'a str, attr: &str) -> Option<&'a str> {
    let pattern = format!("{attr}=\"");
    let idx = element.find(&pattern)?;
    let start = idx + pattern.len();
    let end = element[start..].find('"')? + start;
    Some(&element[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_epub(chapters: &[(&str, &str)], title: &str) -> Vec<u8> {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("mimetype", opts).unwrap();
        write!(zip, "application/epub+zip").unwrap();

        zip.start_file("META-INF/container.xml", opts).unwrap();
        write!(zip, r#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();

        let mut manifest = String::new();
        let mut spine = String::new();
        for (i, (name, _)) in chapters.iter().enumerate() {
            manifest.push_str(&format!(
                r#"<item id="ch{i}" href="{name}" media-type="application/xhtml+xml"/>"#
            ));
            spine.push_str(&format!(r#"<itemref idref="ch{i}"/>"#));
        }

        zip.start_file("content.opf", opts).unwrap();
        write!(zip, r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#).unwrap();

        for (name, content) in chapters {
            zip.start_file(*name, opts).unwrap();
            write!(zip, "{content}").unwrap();
        }

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_basic_epub() {
        let epub = make_epub(
            &[
                (
                    "ch1.xhtml",
                    "<html><body><h1>Chapter One</h1><p>The story begins.</p></body></html>",
                ),
                (
                    "ch2.xhtml",
                    "<html><body><h1>Chapter Two</h1><p>It continues.</p></body></html>",
                ),
            ],
            "Test Book",
        );
        let result = extract_bytes(&epub).unwrap();
        assert!(result.text.contains("Chapter One"), "ch1: {}", result.text);
        assert!(result.text.contains("Chapter Two"), "ch2: {}", result.text);
        assert!(
            result.text.contains("story begins"),
            "content: {}",
            result.text
        );
        assert_eq!(result.format, Format::Epub);
        assert_eq!(result.title.as_deref(), Some("Test Book"));
    }

    #[test]
    fn extract_epub_reading_order() {
        let epub = make_epub(
            &[
                (
                    "z_last.xhtml",
                    "<html><body><p>Last chapter.</p></body></html>",
                ),
                (
                    "a_first.xhtml",
                    "<html><body><p>First chapter.</p></body></html>",
                ),
            ],
            "Order Test",
        );
        let result = extract_bytes(&epub).unwrap();
        // OPF spine should be respected: z_last before a_first
        let last_pos = result.text.find("Last chapter").unwrap();
        let first_pos = result.text.find("First chapter").unwrap();
        assert!(
            last_pos < first_pos,
            "spine order respected: last@{last_pos}, first@{first_pos}"
        );
    }

    #[test]
    fn extract_epub_entities() {
        let epub = make_epub(
            &[(
                "ch1.xhtml",
                "<html><body><p>Caf&#233; &amp; th&#233;.</p></body></html>",
            )],
            "Entity Test",
        );
        let result = extract_bytes(&epub).unwrap();
        assert!(
            result.text.contains("Caf\u{00E9}"),
            "eacute: {}",
            result.text
        );
        assert!(result.text.contains("&"), "amp: {}", result.text);
    }

    #[test]
    fn extract_epub_invalid_zip() {
        let result = extract_bytes(b"not an epub");
        assert!(result.is_err());
    }
}
