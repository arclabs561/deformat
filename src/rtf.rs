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
/// Returns [`Error::Io`] if the file cannot be read, [`Error::Parse`]
/// if RTF parsing fails, or [`Error::EmptyResult`] if no text is extracted.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let content = std::fs::read_to_string(path)?;
    extract_string(&content)
}

/// Extract text from RTF bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the input is not valid UTF-8 or RTF
/// parsing fails, or [`Error::EmptyResult`] if no text is extracted.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let content = std::str::from_utf8(bytes)
        .map_err(|e| Error::Parse(format!("RTF is not valid UTF-8: {e}")))?;
    extract_string(content)
}

fn extract_string(content: &str) -> Result<Extracted, Error> {
    let doc = rtf_parser_tt::RtfDocument::try_from(normalize_source(content))
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

/// Rewrite the parts of an RTF source the parser mishandles.
///
/// - `\par` becomes `\line`, so paragraphs end with a newline instead of
///   running together.
/// - Runs of `\'hh` escapes are decoded with the document's `\ansicpg` code
///   page (cp1252 when absent) and inserted as literal text; the parser reads
///   each byte as a Unicode scalar, so `\'85` became U+0085 instead of `…`.
///   Runs directly after a `\uN` control word are its ASCII fallback and are
///   left for the parser, which skips them.
fn normalize_source(content: &str) -> String {
    let encoding = code_page_encoding(content);
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;
    let mut after_unicode = false;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let next = bytes[i + 1..]
                .iter()
                .position(|&b| b == b'\\')
                .map_or(bytes.len(), |p| i + 1 + p);
            out.push_str(&content[i..next]);
            after_unicode = false;
            i = next;
            continue;
        }
        match bytes.get(i + 1) {
            Some(b'\'') => {
                let start = i;
                let mut run = Vec::new();
                while bytes.get(i) == Some(&b'\\') && bytes.get(i + 1) == Some(&b'\'') {
                    let Some(byte) = bytes
                        .get(i + 2..i + 4)
                        .and_then(|h| std::str::from_utf8(h).ok())
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    else {
                        break;
                    };
                    run.push(byte);
                    i += 4;
                }
                if run.is_empty() {
                    // Malformed escape: leave it for the parser to report.
                    out.push_str(&content[start..start + 2]);
                    i = start + 2;
                } else if after_unicode {
                    out.push_str(&content[start..i]);
                } else {
                    let (decoded, _, _) = encoding.decode(&run);
                    for ch in decoded.chars() {
                        if matches!(ch, '\\' | '{' | '}') {
                            out.push('\\');
                        }
                        out.push(ch);
                    }
                }
                after_unicode = false;
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let word_start = i + 1;
                let mut j = word_start;
                while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                    j += 1;
                }
                let word = &content[word_start..j];
                if word == "par" {
                    out.push_str("\\line");
                } else {
                    out.push_str(&content[i..j]);
                }
                // Numeric parameter and the optional space delimiter.
                let param_start = j;
                if bytes.get(j) == Some(&b'-') {
                    j += 1;
                }
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if bytes.get(j) == Some(&b' ') {
                    j += 1;
                }
                out.push_str(&content[param_start..j]);
                after_unicode = word == "u" && j > param_start;
                i = j;
            }
            Some(_) => {
                // Control symbol (`\\`, `\{`, `\~`, ...): copy it whole.
                let len = content[i + 1..].chars().next().map_or(1, char::len_utf8);
                out.push_str(&content[i..i + 1 + len]);
                after_unicode = false;
                i += 1 + len;
            }
            None => {
                out.push('\\');
                i += 1;
            }
        }
    }
    out
}

/// The encoding named by the first `\ansicpgN` control word, or cp1252.
fn code_page_encoding(content: &str) -> &'static encoding_rs::Encoding {
    let code_page = content.find("\\ansicpg").and_then(|at| {
        let digits = &content[at + "\\ansicpg".len()..];
        let end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        digits[..end].parse::<u32>().ok()
    });
    let label = match code_page {
        Some(874 | 1250..=1258) => format!("windows-{}", code_page.unwrap_or(1252)),
        Some(932) => "shift_jis".to_string(),
        Some(936) => "gbk".to_string(),
        Some(949) => "euc-kr".to_string(),
        Some(950) => "big5".to_string(),
        Some(10000) => "macintosh".to_string(),
        Some(20866) => "koi8-r".to_string(),
        Some(cp @ 28592..=28606) => format!("iso-8859-{}", cp - 28590),
        _ => "windows-1252".to_string(),
    };
    encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252)
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
