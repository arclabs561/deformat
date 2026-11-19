//! RTF text extraction.
//!
//! Requires the `rtf` feature. Extracts text from Rich Text Format documents
//! using the bundled `rtf-parser-tt` parser.

// Keep the imported parser formatted as upstream ships it.
#[rustfmt::skip]
mod parser_core;

use crate::{Error, Extracted, Extractor, Format};
use std::path::Path;

/// Extract text from an RTF file.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`]
/// if RTF parsing fails, or [`Error::EmptyResult`] if no text is extracted.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    extract_bytes(&std::fs::read(path)?)
}

/// Extract text from RTF bytes in memory.
///
/// Non-UTF-8 input is decoded using the document's code page. Hidden text
/// is omitted. Table cells are separated by tabs, and notes follow the body.
///
/// # Errors
///
/// Returns [`Error::Parse`] if RTF parsing fails, or
/// [`Error::EmptyResult`] if no text is extracted.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let doc = parser_core::RtfDocument::from_bytes(bytes)
        .map_err(|e| Error::Parse(format!("RTF parsing failed: {e}")))?;
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
    fn raw_code_page_bytes_decode_with_ansicpg() {
        // 0xE9 is é in cp1252 and й in cp1251.
        let cp1252 = b"{\\rtf1\\ansi\\ansicpg1252 Caf\xe9 au lait.\\par}";
        assert_eq!(extract_bytes(cp1252).unwrap().text, "Caf\u{e9} au lait.");
        let cp1251 = b"{\\rtf1\\ansi\\ansicpg1251 \xe9\\par}";
        assert_eq!(extract_bytes(cp1251).unwrap().text, "\u{439}");
        // No \ansicpg: cp1252.
        let none = b"{\\rtf1\\ansi Caf\xe9.\\par}";
        assert_eq!(extract_bytes(none).unwrap().text, "Caf\u{e9}.");
    }

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
