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
/// Returns [`Error::PdfExtract`] if the file cannot be read or parsed, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let text = pdf_extract::extract_text(path).map_err(|e| Error::PdfExtract(e.to_string()))?;

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
/// Returns [`Error::PdfExtract`] if parsing fails, or [`Error::EmptyResult`]
/// if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let text =
        pdf_extract::extract_text_from_mem(bytes).map_err(|e| Error::PdfExtract(e.to_string()))?;

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
