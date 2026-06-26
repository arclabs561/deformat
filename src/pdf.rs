//! PDF text extraction.
//!
//! Requires the `pdf` feature. Extracts text from PDF files using
//! the crate's default PDF backend.

use crate::{Error, Extracted, Extractor, Format};
use std::path::Path;

/// Extract text from a PDF file.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the file cannot be read or parsed, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let mut doc =
        ::pdf_oxide::PdfDocument::open(path).map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    extract_doc(&mut doc)
}

/// Extract text from PDF bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if parsing fails, or [`Error::EmptyResult`]
/// if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let mut doc = ::pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    extract_doc(&mut doc)
}

fn extract_doc(doc: &mut ::pdf_oxide::PdfDocument) -> Result<Extracted, Error> {
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Parse(format!("PDF page_count: {e}")))?;
    let mut pages = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let page_text = doc
            .extract_text(i)
            .map_err(|e| Error::Parse(format!("PDF page {i}: {e}")))?;
        if !page_text.trim().is_empty() {
            pages.push(page_text);
        }
    }
    let text = pages.join("\n\n");
    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Pdf,
        extractor: Extractor::PdfOxide,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pdf_invalid_bytes() {
        let result = extract_bytes(b"not a pdf");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_empty_bytes() {
        let result = extract_bytes(b"");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_nonexistent_file() {
        let path = Path::new("/nonexistent/path/to/file.pdf");
        let result = extract_file(path);
        assert!(result.is_err(), "should fail on missing file");
    }
}
