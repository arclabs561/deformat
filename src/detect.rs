//! Format detection from content bytes, strings, and file extensions.
//!
//! Zero dependencies. Uses magic bytes for binary formats and content
//! heuristics for text formats.

use std::path::Path;

/// Detected document format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Plain text (no conversion needed).
    PlainText,
    /// HTML or XHTML.
    Html,
    /// PDF document (binary).
    Pdf,
    /// Markdown (light markup, mostly passthrough).
    Markdown,
    /// Format could not be determined.
    Unknown,
}

impl Format {
    /// MIME type string for this format.
    #[must_use]
    pub fn mime_type(&self) -> &'static str {
        match self {
            Format::PlainText => "text/plain",
            Format::Html => "text/html",
            Format::Pdf => "application/pdf",
            Format::Markdown => "text/markdown",
            Format::Unknown => "application/octet-stream",
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::PlainText => write!(f, "plain text"),
            Format::Html => write!(f, "HTML"),
            Format::Pdf => write!(f, "PDF"),
            Format::Markdown => write!(f, "Markdown"),
            Format::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detect format from a text string.
///
/// Checks for HTML markers in the first 1024 characters. Returns
/// [`Format::PlainText`] for content with no markup signals.
#[must_use]
pub fn detect_str(content: &str) -> Format {
    if content.is_empty() {
        return Format::PlainText;
    }
    if is_html(content) {
        return Format::Html;
    }
    Format::PlainText
}

/// Detect format from raw bytes.
///
/// Checks magic bytes for binary formats (PDF), then falls back to
/// text-based detection for UTF-8 content.
#[must_use]
pub fn detect_bytes(bytes: &[u8]) -> Format {
    if bytes.is_empty() {
        return Format::PlainText;
    }

    // PDF: %PDF-
    if bytes.starts_with(b"%PDF") {
        return Format::Pdf;
    }

    // Try as UTF-8 text
    if let Ok(text) = std::str::from_utf8(bytes) {
        return detect_str(text);
    }

    Format::Unknown
}

/// Detect format from a file path (extension-based).
///
/// Uses the file extension as a hint. Does not read the file.
/// Combine with [`detect_bytes`] for content-based detection.
///
/// Accepts any type that can be referenced as a path (`&str`, `&Path`,
/// `&PathBuf`, etc.).
#[must_use]
pub fn detect_path(path: impl AsRef<Path>) -> Format {
    match path
        .as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("html" | "htm" | "xhtml") => Format::Html,
        Some("pdf") => Format::Pdf,
        Some("md" | "markdown" | "mkd") => Format::Markdown,
        Some("txt" | "text") => Format::PlainText,
        _ => Format::Unknown,
    }
}

/// Detect whether content looks like HTML.
///
/// Examines the first 1024 characters for HTML markers: doctype
/// declarations, `<html>`, `<head>`+`<body>`, XML processing
/// instructions, or paired tags.
#[must_use]
pub fn is_html(content: &str) -> bool {
    // Examine at most the first ~1024 bytes, rounding down to a char boundary.
    let mut end = content.len().min(1024);
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    let window = &content[..end];
    // Quick rejection: no '<' means no HTML tags.
    if !window.as_bytes().contains(&b'<') {
        return false;
    }
    let trimmed = window.trim_start();
    trimmed.starts_with("<!")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
        || trimmed.starts_with("<?xml")
        || (trimmed.contains("<head") && trimmed.contains("<body"))
        || (starts_with_tag(trimmed) && trimmed.contains("</"))
}

/// Check if content starts with what looks like an HTML tag (`<` followed by an ASCII letter).
///
/// Rules out `<3`, `<=`, `<script>` comparisons in code, etc.
fn starts_with_tag(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('<')) && matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
}

/// Detect whether raw bytes start with the PDF magic number.
#[must_use]
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== detect_str =====

    #[test]
    fn detect_str_html_doctype() {
        assert_eq!(detect_str("<!DOCTYPE html><html>"), Format::Html);
    }

    #[test]
    fn detect_str_html_tag() {
        assert_eq!(
            detect_str("<html><head><body>text</body></html>"),
            Format::Html
        );
    }

    #[test]
    fn detect_str_html_fragment() {
        assert_eq!(detect_str("<p>Hello</p>"), Format::Html);
    }

    #[test]
    fn detect_str_xml() {
        assert_eq!(detect_str("<?xml version=\"1.0\"?><root/>"), Format::Html);
    }

    #[test]
    fn detect_str_plain_text() {
        assert_eq!(detect_str("Just plain text."), Format::PlainText);
    }

    #[test]
    fn detect_str_empty() {
        assert_eq!(detect_str(""), Format::PlainText);
    }

    #[test]
    fn detect_str_markdown_not_detected_as_html() {
        assert_eq!(detect_str("# Heading\n\nSome text."), Format::PlainText);
    }

    #[test]
    fn detect_str_leading_whitespace() {
        assert_eq!(detect_str("  \n<!DOCTYPE html>\n<html>"), Format::Html);
    }

    // ===== detect_bytes =====

    #[test]
    fn detect_bytes_pdf() {
        assert_eq!(detect_bytes(b"%PDF-1.4 ..."), Format::Pdf);
    }

    #[test]
    fn detect_bytes_html() {
        assert_eq!(
            detect_bytes(b"<html><body>text</body></html>"),
            Format::Html
        );
    }

    #[test]
    fn detect_bytes_plain() {
        assert_eq!(detect_bytes(b"Hello world"), Format::PlainText);
    }

    #[test]
    fn detect_bytes_invalid_utf8() {
        assert_eq!(detect_bytes(&[0xFF, 0xFE, 0x00, 0x01]), Format::Unknown);
    }

    #[test]
    fn detect_bytes_empty() {
        assert_eq!(detect_bytes(b""), Format::PlainText);
    }

    // ===== detect_path =====

    #[test]
    fn detect_path_html() {
        assert_eq!(detect_path(Path::new("page.html")), Format::Html);
        assert_eq!(detect_path(Path::new("page.htm")), Format::Html);
        assert_eq!(detect_path(Path::new("page.xhtml")), Format::Html);
    }

    #[test]
    fn detect_path_pdf() {
        assert_eq!(detect_path(Path::new("report.pdf")), Format::Pdf);
    }

    #[test]
    fn detect_path_markdown() {
        assert_eq!(detect_path(Path::new("README.md")), Format::Markdown);
        assert_eq!(detect_path(Path::new("notes.markdown")), Format::Markdown);
    }

    #[test]
    fn detect_path_plain() {
        assert_eq!(detect_path(Path::new("data.txt")), Format::PlainText);
    }

    #[test]
    fn detect_path_unknown() {
        assert_eq!(detect_path(Path::new("image.png")), Format::Unknown);
        assert_eq!(detect_path(Path::new("no_extension")), Format::Unknown);
    }

    #[test]
    fn detect_path_case_insensitive() {
        assert_eq!(detect_path(Path::new("PAGE.HTML")), Format::Html);
        assert_eq!(detect_path(Path::new("REPORT.PDF")), Format::Pdf);
    }

    // ===== is_html =====

    #[test]
    fn is_html_positive() {
        assert!(is_html("<!DOCTYPE html><html><head>"));
        assert!(is_html("<html><head><body>"));
        assert!(is_html("  \n<!DOCTYPE html>\n<html>"));
        assert!(is_html("<?xml version=\"1.0\"?><html>"));
        assert!(is_html("<p>Hello</p>"));
    }

    #[test]
    fn is_html_negative() {
        assert!(!is_html("Tim Cook announced new products today."));
        assert!(!is_html("The patient has no history of diabetes."));
        assert!(!is_html("# Markdown heading\n\nSome text."));
        assert!(!is_html(""));
    }

    // ===== is_pdf =====

    #[test]
    fn is_pdf_positive() {
        assert!(is_pdf(b"%PDF-1.4"));
        assert!(is_pdf(b"%PDF-2.0 some content"));
    }

    #[test]
    fn is_pdf_negative() {
        assert!(!is_pdf(b"<html>"));
        assert!(!is_pdf(b"Hello"));
        assert!(!is_pdf(b""));
    }

    // ===== Format display / mime =====

    #[test]
    fn format_display() {
        assert_eq!(Format::Html.to_string(), "HTML");
        assert_eq!(Format::Pdf.to_string(), "PDF");
        assert_eq!(Format::PlainText.to_string(), "plain text");
    }

    #[test]
    fn format_mime() {
        assert_eq!(Format::Html.mime_type(), "text/html");
        assert_eq!(Format::Pdf.mime_type(), "application/pdf");
        assert_eq!(Format::PlainText.mime_type(), "text/plain");
    }

    // ===== Edge cases =====

    #[test]
    fn is_html_body_only() {
        // Content with just a <body> tag and closing tag
        assert!(is_html("<body><p>Content</p></body>"));
    }

    #[test]
    fn is_html_angle_bracket_in_text() {
        // Mathematical or comparison text with < should NOT be HTML
        assert!(!is_html("if x < 10 then y > 20"));
        assert!(!is_html("a < b and c > d"));
    }

    #[test]
    fn is_html_email_template_marker() {
        // Some email HTML starts with whitespace + html tag
        assert!(is_html("\r\n\r\n<html>\r\n<head>"));
    }

    #[test]
    fn detect_path_dotfile() {
        assert_eq!(detect_path(Path::new(".hidden")), Format::Unknown);
    }

    #[test]
    fn detect_path_multiple_dots() {
        assert_eq!(detect_path(Path::new("file.backup.html")), Format::Html);
        assert_eq!(detect_path(Path::new("report.2024.pdf")), Format::Pdf);
    }

    #[test]
    fn detect_bytes_utf8_bom() {
        // UTF-8 BOM followed by HTML
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"<html><body>text</body></html>");
        // BOM bytes make this non-PDF, and the UTF-8 content should be detectable
        assert_ne!(detect_bytes(&bytes), Format::Pdf);
    }

    #[test]
    fn format_equality() {
        assert_eq!(Format::Html, Format::Html);
        assert_ne!(Format::Html, Format::Pdf);
    }

    #[test]
    fn format_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Format::Html);
        set.insert(Format::Pdf);
        set.insert(Format::Html); // duplicate
        assert_eq!(set.len(), 2);
    }
}
