//! De-format: extract plain text from HTML, PDF, and other document formats.
//!
//! NER engines, LLM pipelines, and search indexers need plain text.
//! `deformat` sits upstream: it takes formatted documents and returns clean
//! text. No I/O -- it operates on `&str` and `&[u8]` inputs.
//!
//! # Quick start
//!
//! ```
//! use deformat::{extract, Format};
//!
//! // Auto-detect format and extract text
//! let result = extract("<p>Hello <b>world</b>!</p>");
//! assert_eq!(result.text, "Hello world!");
//! assert_eq!(result.format, Format::Html);
//!
//! // Plain text passes through unchanged
//! let result = extract("Just plain text.");
//! assert_eq!(result.text, "Just plain text.");
//! assert_eq!(result.format, Format::PlainText);
//! ```
//!
//! # Feature flags
//!
//! All features are opt-in. The default build has one dependency: `memchr`
//! (SIMD-accelerated byte scanning).
//!
//! | Feature | Crate | What it adds |
//! |---------|-------|-------------|
//! | `readability` | [`dom_smoothie`] | Mozilla Readability article extraction |
//! | `html2text` | [`html2text`] | DOM-based HTML-to-text with layout awareness |
//! | `pdf` | [`pdf-extract`] | PDF text extraction from file paths |

pub mod detect;
pub mod error;
pub mod html;

#[cfg(feature = "pdf")]
pub mod pdf;

use std::collections::HashMap;

pub use detect::Format;
pub use error::Error;

/// Extracted text with metadata about the source document.
#[derive(Debug, Clone)]
pub struct Extracted {
    /// The extracted plain text content.
    pub text: String,
    /// The detected (or specified) source format.
    pub format: Format,
    /// Metadata from extraction (title, extractor used, language, etc.).
    pub metadata: HashMap<String, String>,
}

/// Extract plain text from content, auto-detecting the format.
///
/// Examines the content to determine whether it is HTML or plain text,
/// then applies the appropriate extraction strategy. Plain text and
/// markdown pass through unchanged.
///
/// For PDF extraction, use [`extract_bytes`] or the [`pdf`] module
/// (requires the `pdf` feature).
///
/// # Examples
///
/// ```
/// let result = deformat::extract("<html><body><p>Hello</p></body></html>");
/// assert!(result.text.contains("Hello"));
/// assert_eq!(result.format, deformat::Format::Html);
/// ```
pub fn extract(content: &str) -> Extracted {
    let format = detect::detect_str(content);
    extract_as(content, format)
}

/// Extract plain text with an explicit format override.
///
/// Skips format detection and applies the specified extraction strategy
/// directly.
pub fn extract_as(content: &str, format: Format) -> Extracted {
    match format {
        Format::Html => {
            let text = html::strip_to_text(content);
            let mut metadata = HashMap::new();
            metadata.insert("extractor".into(), "strip".into());
            Extracted {
                text,
                format,
                metadata,
            }
        }
        Format::PlainText | Format::Markdown | Format::Unknown => Extracted {
            text: content.to_string(),
            format,
            metadata: HashMap::new(),
        },
        Format::Pdf => Extracted {
            text: String::new(),
            format,
            metadata: HashMap::from([(
                "error".into(),
                "PDF requires file path; use deformat::pdf::extract_file()".into(),
            )]),
        },
    }
}

/// Extract article content from HTML using readability analysis.
///
/// Attempts Mozilla Readability extraction first (content-focused,
/// removes boilerplate). Falls back to tag stripping if readability
/// fails or produces insufficient content (< 50 chars).
///
/// Requires the `readability` feature.
///
/// # Arguments
///
/// * `html` - HTML content to extract from.
/// * `url` - Optional source URL (improves link resolution and metadata).
#[cfg(feature = "readability")]
pub fn extract_readable(html: &str, url: Option<&str>) -> Extracted {
    match html::extract_with_readability(html, url.unwrap_or("")) {
        Some((text, title, excerpt)) => {
            let mut metadata = HashMap::new();
            metadata.insert("extractor".into(), "readability".into());
            if let Some(t) = title {
                metadata.insert("title".into(), t);
            }
            if let Some(e) = excerpt {
                metadata.insert("excerpt".into(), e);
            }
            Extracted {
                text,
                format: Format::Html,
                metadata,
            }
        }
        None => {
            let text = html::strip_to_text(html);
            let mut metadata = HashMap::new();
            metadata.insert("extractor".into(), "strip".into());
            metadata.insert("readability_fallback".into(), "true".into());
            Extracted {
                text,
                format: Format::Html,
                metadata,
            }
        }
    }
}

/// Extract text from HTML using DOM-based conversion with layout awareness.
///
/// Produces formatted text that respects block structure, tables, and
/// link footnotes. Falls back to tag stripping on parse errors.
///
/// Requires the `html2text` feature.
///
/// # Arguments
///
/// * `html` - HTML content to convert.
/// * `width` - Target line width for wrapping (e.g., 80, 120, or 10000
///   for effectively no wrapping).
#[cfg(feature = "html2text")]
pub fn extract_html2text(html: &str, width: usize) -> Extracted {
    match ::html2text::from_read(html.as_bytes(), width) {
        Ok(text) => {
            let mut metadata = HashMap::new();
            metadata.insert("extractor".into(), "html2text".into());
            Extracted {
                text,
                format: Format::Html,
                metadata,
            }
        }
        Err(_) => {
            let text = html::strip_to_text(html);
            let mut metadata = HashMap::new();
            metadata.insert("extractor".into(), "strip".into());
            metadata.insert("html2text_fallback".into(), "true".into());
            Extracted {
                text,
                format: Format::Html,
                metadata,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_html_auto() {
        let result = extract("<p>Hello <b>world</b>!</p>");
        assert_eq!(result.text, "Hello world!");
        assert_eq!(result.format, Format::Html);
    }

    #[test]
    fn extract_full_html_doc() {
        let html = "<!DOCTYPE html><html><head><title>T</title></head>\
                     <body><p>Content here.</p></body></html>";
        let result = extract(html);
        assert!(result.text.contains("Content here"));
        assert!(!result.text.contains("<title>"), "tags should be stripped");
        assert_eq!(result.format, Format::Html);
    }

    #[test]
    fn extract_plain_text() {
        let result = extract("Just plain text, no markup.");
        assert_eq!(result.text, "Just plain text, no markup.");
        assert_eq!(result.format, Format::PlainText);
    }

    #[test]
    fn extract_as_html() {
        let result = extract_as("<b>bold</b> text", Format::Html);
        assert_eq!(result.text, "bold text");
    }

    #[test]
    fn extract_as_plain() {
        let result = extract_as("<b>not html</b>", Format::PlainText);
        assert_eq!(result.text, "<b>not html</b>");
    }

    #[test]
    fn extract_metadata_has_extractor() {
        let result = extract("<p>Hello</p>");
        assert_eq!(result.metadata.get("extractor").unwrap(), "strip");
    }

    #[test]
    fn extract_empty_string() {
        let result = extract("");
        assert_eq!(result.text, "");
        assert_eq!(result.format, Format::PlainText);
    }

    #[cfg(feature = "readability")]
    #[test]
    fn extract_readable_with_article() {
        let html = r#"<!DOCTYPE html>
        <html><head><title>Test Article</title></head>
        <body>
            <nav><a href="/">Home</a></nav>
            <article>
                <h1>Test Article</h1>
                <p>A team of researchers at the University of Cambridge has announced
                   the discovery of a previously unknown species. The discovery was
                   published in the journal Nature. The finding represents one of the
                   most significant discoveries in recent years and has drawn attention
                   from conservation organizations worldwide.</p>
                <p>Lead researcher Dr. Sarah Chen said the species was found during
                   an expedition in January. Chen and her team spent three weeks
                   collecting specimens and documenting the habitat conditions where
                   the species was found along tributary streams.</p>
                <p>Conservation groups including the World Wildlife Fund have called
                   for increased protection of the region. Local communities have long
                   known about the species but it had never been formally described.</p>
                <p>The research was funded by a grant from the European Research Council.
                   Additional specimens will be housed at the Natural History Museum in
                   London. Future expeditions are planned to search for related species
                   in neighboring regions.</p>
            </article>
            <footer>Copyright 2026</footer>
        </body></html>"#;
        let result = extract_readable(html, Some("https://example.com/article"));
        assert!(result.text.contains("Dr. Sarah Chen"));
        assert_eq!(result.metadata.get("extractor").unwrap(), "readability");
    }

    #[cfg(feature = "readability")]
    #[test]
    fn extract_readable_fallback_on_short() {
        let result = extract_readable("<p>Short</p>", None);
        assert_eq!(result.metadata.get("readability_fallback").unwrap(), "true");
    }

    #[cfg(feature = "html2text")]
    #[test]
    fn extract_html2text_basic() {
        let result = extract_html2text("<p>Hello <b>world</b>!</p>", 80);
        assert!(result.text.contains("Hello"));
        assert!(result.text.contains("world"));
        assert_eq!(result.metadata.get("extractor").unwrap(), "html2text");
    }
}
