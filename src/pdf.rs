//! PDF text extraction.
//!
//! Requires the `pdf` feature. Extracts text from PDF files using
//! the `pdf-extract` crate.

use crate::{Error, Extracted, Format};
use std::collections::HashMap;
use std::path::Path;

/// Extract text from a PDF file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, or
/// [`Error::EmptyResult`] if extraction produces no text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let text = pdf_extract::extract_text(path).map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("PDF extraction failed: {e}"),
        ))
    })?;

    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    let mut metadata = HashMap::new();
    metadata.insert("extractor".into(), "pdf-extract".into());

    Ok(Extracted {
        text,
        format: Format::Pdf,
        metadata,
    })
}

/// Extract text from PDF bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Io`] if parsing fails, or [`Error::EmptyResult`]
/// if extraction produces no text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("PDF extraction failed: {e}"),
        ))
    })?;

    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    let mut metadata = HashMap::new();
    metadata.insert("extractor".into(), "pdf-extract".into());

    Ok(Extracted {
        text,
        format: Format::Pdf,
        metadata,
    })
}
