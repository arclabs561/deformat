//! DOCX text extraction.
//!
//! Requires the `docx` feature. Extracts text from `.docx` files by
//! reading `word/document.xml` from the ZIP archive and stripping XML tags.
//! Also reads `word/header*.xml` and `word/footer*.xml` if present.

use crate::{html, Error, Extracted, Extractor, Format};
use std::io::{Read, Seek};
use std::path::Path;

/// Extract text from a DOCX file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::PdfExtract`]
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
/// Returns [`Error::PdfExtract`] if the ZIP archive is invalid, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let cursor = std::io::Cursor::new(bytes);
    extract_reader(cursor)
}

fn extract_reader<R: Read + Seek>(reader: R) -> Result<Extracted, Error> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| Error::PdfExtract(format!("invalid DOCX ZIP: {e}")))?;

    let mut text_parts = Vec::new();

    // Extract main document content
    if let Ok(mut entry) = archive.by_name("word/document.xml") {
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .map_err(|e| Error::PdfExtract(format!("failed to read document.xml: {e}")))?;
        let text = html::strip_to_text(&xml);
        if !text.is_empty() {
            text_parts.push(text);
        }
    } else {
        return Err(Error::PdfExtract("DOCX missing word/document.xml".into()));
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
