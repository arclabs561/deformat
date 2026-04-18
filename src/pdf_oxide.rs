//! PDF text extraction via the `pdf_oxide` crate.
//!
//! Alternative backend to [`crate::pdf`]. `pdf_oxide` reports faster
//! extraction and higher pass rates than `pdf-extract` on large corpora,
//! but those numbers are vendor-provided — this crate has not
//! independently benchmarked them. Opt into this backend explicitly via
//! the `pdf_oxide` feature.
//!
//! Both backends can be enabled simultaneously. The modules expose
//! parallel APIs; callers choose which one to invoke.

use crate::{Error, Extracted, Extractor, Format};
use std::path::Path;

/// Extract text from a PDF file using `pdf_oxide`.
///
/// # Errors
///
/// Returns [`Error::Parse`] on failure, or [`Error::EmptyResult`] if no
/// pages produce any text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let mut doc = pdf_oxide::PdfDocument::open(path)
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    extract_doc(&mut doc)
}

/// Extract text from PDF bytes using `pdf_oxide`.
///
/// # Errors
///
/// Returns [`Error::Parse`] on failure, or [`Error::EmptyResult`] if no
/// pages produce any text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let mut doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    extract_doc(&mut doc)
}

fn extract_doc(doc: &mut pdf_oxide::PdfDocument) -> Result<Extracted, Error> {
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Parse(format!("PDF (oxide) page_count: {e}")))?;
    let mut pages: Vec<String> = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let page_text = doc
            .extract_text(i)
            .map_err(|e| Error::Parse(format!("PDF (oxide) page {i}: {e}")))?;
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
    fn extract_pdf_oxide_invalid_bytes() {
        let result = extract_bytes(b"not a pdf");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_oxide_empty_bytes() {
        let result = extract_bytes(b"");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_oxide_nonexistent_file() {
        let result = extract_file(Path::new("/nonexistent/path/to/file.pdf"));
        assert!(result.is_err());
    }
}
