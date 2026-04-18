//! PDF text extraction.
//!
//! Requires the `pdf` feature. Extracts text from PDF files using
//! the `pdf-extract` crate.

use crate::{Error, Extracted, Format};
use std::path::Path;

/// Extract text from a PDF file.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the file cannot be read or parsed, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let text = pdf_extract::extract_text(path).map_err(|e| Error::Parse(format!("PDF: {e}")))?;

    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Pdf,
        extractor: crate::Extractor::PdfExtract,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

/// Extract text from PDF bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if parsing fails, or [`Error::EmptyResult`]
/// if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let text =
        pdf_extract::extract_text_from_mem(bytes).map_err(|e| Error::Parse(format!("PDF: {e}")))?;

    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Pdf,
        extractor: crate::Extractor::PdfExtract,
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
