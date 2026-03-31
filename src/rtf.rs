//! RTF text extraction.
//!
//! Requires the `rtf` feature. Extracts text from Rich Text Format documents
//! using the `rtf-parser-tt` crate.

use crate::{Error, Extracted, Extractor, Format};
use std::path::Path;

/// Extract text from an RTF file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::PdfExtract`]
/// if RTF parsing fails, or [`Error::EmptyResult`] if no text is extracted.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let content = std::fs::read_to_string(path)?;
    extract_string(&content)
}

/// Extract text from RTF bytes in memory.
///
/// # Errors
///
/// Returns [`Error::PdfExtract`] if the input is not valid UTF-8 or RTF
/// parsing fails, or [`Error::EmptyResult`] if no text is extracted.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let content = std::str::from_utf8(bytes)
        .map_err(|e| Error::PdfExtract(format!("RTF is not valid UTF-8: {e}")))?;
    extract_string(content)
}

fn extract_string(content: &str) -> Result<Extracted, Error> {
    let doc = rtf_parser_tt::RtfDocument::try_from(content.to_string())
        .map_err(|e| Error::PdfExtract(format!("RTF parsing failed: {e}")))?;
    let text = doc.get_text();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(Error::EmptyResult);
    }
    Ok(Extracted {
        text: trimmed,
        format: Format::Rtf,
        extractor: Extractor::Strip,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_basic_rtf() {
        let rtf = r"{\rtf1\ansi{\fonttbl\f0\fswiss Helvetica;}\f0\pard Hello from RTF.\par Second paragraph.\par}";
        let result = extract_bytes(rtf.as_bytes()).unwrap();
        assert!(
            result.text.contains("Hello from RTF"),
            "text: {}",
            result.text
        );
        assert!(
            result.text.contains("Second paragraph"),
            "p2: {}",
            result.text
        );
        assert_eq!(result.format, Format::Rtf);
    }

    #[test]
    fn extract_rtf_with_formatting() {
        let rtf = r"{\rtf1\ansi {\b Bold text} and {\i italic text}.\par}";
        let result = extract_bytes(rtf.as_bytes()).unwrap();
        assert!(result.text.contains("Bold text"), "bold: {}", result.text);
        assert!(
            result.text.contains("italic text"),
            "italic: {}",
            result.text
        );
    }

    #[test]
    fn extract_rtf_invalid() {
        let result = extract_bytes(b"not rtf content at all");
        assert!(result.is_err(), "should fail on non-RTF");
    }

    #[test]
    fn extract_rtf_empty() {
        let rtf = r"{\rtf1\ansi }";
        let result = extract_bytes(rtf.as_bytes());
        assert!(result.is_err(), "empty RTF should return EmptyResult");
    }
}
