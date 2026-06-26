#![warn(missing_docs)]
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
//! let result = extract("<p>Hello <b>world</b>!</p>").unwrap();
//! assert_eq!(result.text, "Hello world!");
//! assert_eq!(result.format, Format::Html);
//!
//! // Plain text passes through unchanged
//! let result = extract("Just plain text.").unwrap();
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
//! | `serde` | `serde` | [`serde::Serialize`]/[`serde::Deserialize`] on [`Extracted`], [`Format`], [`Extractor`], and [`html::HtmlMetadata`] |
//! | `readability` | `dom_smoothie` | Mozilla Readability article extraction |
//! | `html2text` | `html2text` | DOM-based HTML-to-text with layout awareness |
//! | `pdf` | `pdf_oxide` | PDF text extraction |
//! | `pdf_oxide` | `pdf_oxide` | PDF text extraction with coordinate-aware segment APIs |
//! | `docx` | `zip` | DOCX text extraction from file paths or bytes |
//! | `epub` | `zip` | EPUB text extraction with reading-order chapters |
//! | `rtf` | `rtf-parser-tt` | RTF text extraction |
//! | `xlsx` | `calamine` | XLSX/XLS/ODS spreadsheet text extraction |
//! | `pptx` | `zip` | PPTX presentation text extraction |
//! | `whichlang` | `whichlang` | Natural-language detection via [`html::detect_language`] |
//! | `encoding_rs` | `encoding_rs` | Charset sniffing and decoding of non-UTF-8 input via [`detect::decode_bytes`] |

pub mod detect;
pub mod error;
pub mod html;
pub mod page_type;
pub mod segment;

#[cfg(feature = "pdf")]
pub mod pdf;

#[cfg(feature = "pdf_oxide")]
pub mod pdf_oxide;

#[cfg(feature = "docx")]
pub mod docx;

#[cfg(feature = "epub")]
pub mod epub;

#[cfg(feature = "rtf")]
pub mod rtf;

#[cfg(feature = "xlsx")]
pub mod xlsx;

#[cfg(feature = "pptx")]
pub mod pptx;

pub use detect::Format;
pub use error::Error;

/// Which extraction strategy produced an [`Extracted`] result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Extractor {
    /// Fast tag stripping (always available).
    Strip,
    /// Mozilla Readability article extraction (feature `readability`).
    Readability,
    /// DOM-based HTML-to-text with layout awareness (feature `html2text`).
    Html2text,
    /// PDF text extraction via the historical `pdf-extract` backend.
    PdfExtract,
    /// PDF text extraction via `pdf_oxide` (features `pdf` or `pdf_oxide`).
    PdfOxide,
    /// Input passed through unchanged (plain text, markdown, unknown).
    Passthrough,
}

impl std::fmt::Display for Extractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Extractor::Strip => write!(f, "strip"),
            Extractor::Readability => write!(f, "readability"),
            Extractor::Html2text => write!(f, "html2text"),
            Extractor::PdfExtract => write!(f, "pdf-extract"),
            Extractor::PdfOxide => write!(f, "pdf-oxide"),
            Extractor::Passthrough => write!(f, "passthrough"),
        }
    }
}

/// Extracted text with metadata about the source document.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Extracted {
    /// The extracted plain text content.
    pub text: String,
    /// The detected (or specified) source format.
    pub format: Format,
    /// Which extractor produced this result.
    pub extractor: Extractor,
    /// Article title, if extracted (readability only).
    pub title: Option<String>,
    /// Article excerpt/description, if extracted (readability only).
    pub excerpt: Option<String>,
    /// `true` if a richer extractor failed and the result fell back to tag stripping.
    pub fallback: bool,
}

/// Extract plain text from content, auto-detecting the format.
///
/// Examines the content to determine whether it is HTML or plain text,
/// then applies the appropriate extraction strategy. Plain text and
/// markdown pass through unchanged.
///
/// For raw bytes (binary formats, or text in an unknown encoding), use
/// [`extract_from_bytes`] instead; for PDF specifically, the `pdf`
/// module (requires the `pdf` feature).
///
/// # Errors
///
/// Returns [`Error::UnsupportedFormat`] if the detected format cannot be
/// extracted from a string. String detection currently only yields
/// [`Format::Html`] or [`Format::PlainText`], both of which extract
/// infallibly, so this function does not return `Err` today; the
/// `Result` is kept so future detectable formats can fail without a
/// breaking change.
///
/// # Examples
///
/// ```
/// let result = deformat::extract("<html><body><p>Hello</p></body></html>").unwrap();
/// assert!(result.text.contains("Hello"));
/// assert_eq!(result.format, deformat::Format::Html);
/// ```
pub fn extract(content: &str) -> Result<Extracted, Error> {
    let format = detect::detect_str(content);
    extract_as(content, format)
}

/// Extract plain text with an explicit format override.
///
/// Skips format detection and applies the specified extraction strategy
/// directly.
///
/// # Errors
///
/// Returns [`Error::UnsupportedFormat`] for the binary formats
/// ([`Format::Pdf`], [`Format::Rtf`], [`Format::Docx`], [`Format::Epub`],
/// [`Format::Xlsx`], [`Format::Pptx`]), which cannot be extracted from a
/// `&str` -- use [`extract_from_bytes`] or the per-format module's
/// `extract_file` / `extract_bytes` (behind the matching feature flag)
/// instead. This holds regardless of which features are enabled.
///
pub fn extract_as(content: &str, format: Format) -> Result<Extracted, Error> {
    match format {
        Format::Html => {
            let text = html::strip_to_text(content);
            Ok(Extracted {
                text,
                format,
                extractor: Extractor::Strip,
                title: None,
                excerpt: None,
                fallback: false,
            })
        }
        // XML: strip tags just like HTML
        Format::Xml => {
            let text = html::strip_to_text(content);
            Ok(Extracted {
                text,
                format,
                extractor: Extractor::Strip,
                title: None,
                excerpt: None,
                fallback: false,
            })
        }
        // Text-like formats pass through unchanged
        Format::PlainText | Format::Markdown | Format::Unknown => Ok(Extracted {
            text: content.to_string(),
            format,
            extractor: Extractor::Passthrough,
            title: None,
            excerpt: None,
            fallback: false,
        }),
        // Binary formats cannot be extracted from a string
        Format::Pdf | Format::Rtf | Format::Docx | Format::Epub | Format::Xlsx | Format::Pptx => {
            Err(Error::UnsupportedFormat(format!(
                "{format} cannot be extracted from a string; use binary extraction methods"
            )))
        }
    }
}

/// Extract plain text from raw bytes, auto-detecting the format.
///
/// One-call end-to-end convenience over [`detect::detect_bytes`] plus
/// the per-format `extract_bytes` fns. Dispatches to the right backend:
///
/// - HTML / XML / plain text / markdown: decoded (UTF-8 assumed; use
///   [`detect::decode_bytes`] first if the input is `windows-1252` /
///   CJK / etc., with the `encoding_rs` feature) and fed to
///   [`html::strip_to_text`] or passed through.
/// - PDF (feature `pdf`): [`pdf::extract_bytes`].
/// - DOCX (feature `docx`), EPUB (`epub`), RTF (`rtf`), XLSX
///   (`xlsx`), PPTX (`pptx`): the matching module's `extract_bytes`.
///
/// # Errors
///
/// - [`Error::UnsupportedFormat`] if the detected format requires a
///   crate feature that is not enabled in the current build.
/// - [`Error::Io`] / [`Error::Parse`] / [`Error::EmptyResult`] from the
///   underlying extractor.
///
/// # Examples
///
/// ```no_run
/// let bytes = std::fs::read("document.docx")?;
/// let result = deformat::extract_from_bytes(&bytes)?;
/// println!("{}", result.text);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn extract_from_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let format = detect::detect_bytes(bytes);
    match format {
        Format::Html | Format::Xml | Format::PlainText | Format::Markdown | Format::Unknown => {
            let text = std::str::from_utf8(bytes).map_err(|e| {
                Error::Parse(format!(
                    "input is not valid UTF-8 ({e}); decode with the `encoding_rs` feature first"
                ))
            })?;
            extract_as(text, format)
        }
        #[cfg(feature = "pdf")]
        Format::Pdf => pdf::extract_bytes(bytes),
        #[cfg(not(feature = "pdf"))]
        Format::Pdf => Err(Error::UnsupportedFormat(
            "PDF extraction requires the `pdf` feature".into(),
        )),
        #[cfg(feature = "docx")]
        Format::Docx => docx::extract_bytes(bytes),
        #[cfg(not(feature = "docx"))]
        Format::Docx => Err(Error::UnsupportedFormat(
            "DOCX extraction requires the `docx` feature".into(),
        )),
        #[cfg(feature = "epub")]
        Format::Epub => epub::extract_bytes(bytes),
        #[cfg(not(feature = "epub"))]
        Format::Epub => Err(Error::UnsupportedFormat(
            "EPUB extraction requires the `epub` feature".into(),
        )),
        #[cfg(feature = "rtf")]
        Format::Rtf => rtf::extract_bytes(bytes),
        #[cfg(not(feature = "rtf"))]
        Format::Rtf => Err(Error::UnsupportedFormat(
            "RTF extraction requires the `rtf` feature".into(),
        )),
        #[cfg(feature = "xlsx")]
        Format::Xlsx => xlsx::extract_bytes(bytes),
        #[cfg(not(feature = "xlsx"))]
        Format::Xlsx => Err(Error::UnsupportedFormat(
            "XLSX extraction requires the `xlsx` feature".into(),
        )),
        #[cfg(feature = "pptx")]
        Format::Pptx => pptx::extract_bytes(bytes),
        #[cfg(not(feature = "pptx"))]
        Format::Pptx => Err(Error::UnsupportedFormat(
            "PPTX extraction requires the `pptx` feature".into(),
        )),
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
        Some((text, title, excerpt)) => Extracted {
            text,
            format: Format::Html,
            extractor: Extractor::Readability,
            title,
            excerpt,
            fallback: false,
        },
        None => {
            let text = html::strip_to_text(html);
            Extracted {
                text,
                format: Format::Html,
                extractor: Extractor::Strip,
                title: None,
                excerpt: None,
                fallback: true,
            }
        }
    }
}

/// Extract HTML by cascading from scanner stripping to readability.
///
/// Runs [`html::strip_to_text`] first (cheap, no DOM parse). Falls back
/// to [`extract_readable`] when readability finds substantially more
/// content than the scanner -- specifically, when readability's output
/// is more than `2x` the scanner's. Returns whichever the cascade
/// chooses, with `extractor` reporting which backend won.
///
/// The `2x` ratio is borrowed from Trafilatura's
/// [`compare_extraction`](https://github.com/adbar/trafilatura/blob/master/trafilatura/external.py)
/// step, which is the only published cascade architecture with measured
/// F1 gains (+1.7 to +1.9pp on Trafilatura's own evaluation suite).
/// Pure length-threshold cascades are folk wisdom; the ratio condition
/// is what the comparative-benchmark literature validates.
///
/// As a small optimization, the readability call is short-circuited
/// when the scanner's output is already at least `1000` Unicode scalars
/// (call this the "obviously enough" floor). Below that, both backends
/// run and the ratio decides.
///
/// Requires the `readability` feature. Without it, prefer [`extract`] or
/// [`html::strip_to_text`] directly -- the cascade has nothing to fall
/// back to.
///
/// `fallback` on the returned [`Extracted`] is always `false`: the
/// cascade picked the better of two intended outputs; neither path is
/// degraded.
///
/// # Examples
///
/// ```
/// // Page with proper semantics: scanner already extracts plenty,
/// // readability is short-circuited.
/// let html = "<html><body><article>".to_string()
///     + &"<p>Real article content. </p>".repeat(80)
///     + "</article></body></html>";
/// let result = deformat::extract_html_cascade(&html);
/// assert!(result.text.len() > 1000);
/// assert_eq!(result.extractor, deformat::Extractor::Strip);
/// ```
#[cfg(feature = "readability")]
#[must_use]
pub fn extract_html_cascade(html: &str) -> Extracted {
    /// Above this scanner-output length, skip the readability call entirely.
    /// Scanner already has plenty; running DOM parse is wasted compute.
    const SHORT_CIRCUIT_FLOOR: usize = 1000;
    /// Readability must beat the scanner by this multiple to win. Matches
    /// Trafilatura's `len_algo > 2 * len_text` condition.
    const READABILITY_WIN_RATIO: usize = 2;

    let strip_text = html::strip_to_text(html);
    let strip_chars = strip_text.chars().count();

    if strip_chars >= SHORT_CIRCUIT_FLOOR {
        return Extracted {
            text: strip_text,
            format: Format::Html,
            extractor: Extractor::Strip,
            title: None,
            excerpt: None,
            fallback: false,
        };
    }

    match html::extract_with_readability(html, "") {
        Some((readable_text, title, excerpt))
            if readable_text.chars().count() > READABILITY_WIN_RATIO * strip_chars =>
        {
            Extracted {
                text: readable_text,
                format: Format::Html,
                extractor: Extractor::Readability,
                title,
                excerpt,
                fallback: false,
            }
        }
        _ => Extracted {
            text: strip_text,
            format: Format::Html,
            extractor: Extractor::Strip,
            title: None,
            excerpt: None,
            fallback: false,
        },
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
        Ok(text) => Extracted {
            text,
            format: Format::Html,
            extractor: Extractor::Html2text,
            title: None,
            excerpt: None,
            fallback: false,
        },
        Err(_) => {
            let text = html::strip_to_text(html);
            Extracted {
                text,
                format: Format::Html,
                extractor: Extractor::Strip,
                title: None,
                excerpt: None,
                fallback: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_html_auto() {
        let result = extract("<p>Hello <b>world</b>!</p>").unwrap();
        assert_eq!(result.text, "Hello world!");
        assert_eq!(result.format, Format::Html);
    }

    #[test]
    fn extract_full_html_doc() {
        let html = "<!DOCTYPE html><html><head><title>T</title></head>\
                     <body><p>Content here.</p></body></html>";
        let result = extract(html).unwrap();
        assert!(result.text.contains("Content here"));
        assert!(!result.text.contains("<title>"), "tags should be stripped");
        assert_eq!(result.format, Format::Html);
    }

    #[test]
    fn extract_plain_text() {
        let result = extract("Just plain text, no markup.").unwrap();
        assert_eq!(result.text, "Just plain text, no markup.");
        assert_eq!(result.format, Format::PlainText);
    }

    #[test]
    fn extract_as_html() {
        let result = extract_as("<b>bold</b> text", Format::Html).unwrap();
        assert_eq!(result.text, "bold text");
    }

    #[test]
    fn extract_as_plain() {
        let result = extract_as("<b>not html</b>", Format::PlainText).unwrap();
        assert_eq!(result.text, "<b>not html</b>");
    }

    #[test]
    fn extract_metadata_has_extractor() {
        let result = extract("<p>Hello</p>").unwrap();
        assert_eq!(result.extractor, Extractor::Strip);
    }

    #[test]
    fn extract_empty_string() {
        let result = extract("").unwrap();
        assert_eq!(result.text, "");
        assert_eq!(result.format, Format::PlainText);
    }

    #[test]
    fn extract_as_pdf_returns_error() {
        let result = extract_as("fake pdf content", Format::Pdf);
        assert!(result.is_err(), "PDF str extraction should return Err");
        let err = result.unwrap_err();
        assert!(
            matches!(err, Error::UnsupportedFormat(_)),
            "should be UnsupportedFormat, got: {err}"
        );
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
        assert_eq!(result.extractor, Extractor::Readability);
    }

    #[cfg(feature = "readability")]
    #[test]
    fn extract_readable_fallback_on_short() {
        let result = extract_readable("<p>Short</p>", None);
        assert!(result.fallback);
    }

    #[cfg(feature = "readability")]
    #[test]
    fn cascade_picks_strip_when_output_is_long_enough() {
        let body: String = (0..80)
            .map(|i| {
                format!("<p>Paragraph number {i} carries enough text to clear the threshold. </p>")
            })
            .collect();
        let html = format!("<html><body><article>{body}</article></body></html>");
        let result = extract_html_cascade(&html);
        assert_eq!(result.extractor, Extractor::Strip);
        assert!(result.text.chars().count() >= 1000);
        assert!(result.text.contains("Paragraph number 0"));
    }

    #[cfg(feature = "readability")]
    #[test]
    fn cascade_short_circuits_above_floor_without_calling_readability() {
        // Output above the 1000-char short-circuit must come from strip
        // exactly -- no readability mutation, no whitespace collapsing
        // disagreement.
        let body: String = (0..50)
            .map(|i| format!("<p>Sentence {i} ends with a period. Another sentence here too. </p>"))
            .collect();
        let html = format!("<html><body><main>{body}</main></body></html>");
        let strip_text = html::strip_to_text(&html);
        let cascade = extract_html_cascade(&html);
        assert_eq!(cascade.extractor, Extractor::Strip);
        assert_eq!(cascade.text, strip_text);
    }

    #[cfg(feature = "readability")]
    #[test]
    fn cascade_keeps_strip_when_ratio_is_under_2x() {
        // Both backends extract similar amounts. Readability output
        // doesn't beat strip by 2x, so strip wins (no churn).
        let body: String = (0..15)
            .map(|i| format!("<p>Body paragraph {i} with several sentences of real content. </p>"))
            .collect();
        let html = format!("<html><body><article>{body}</article></body></html>");
        let result = extract_html_cascade(&html);
        assert_eq!(result.extractor, Extractor::Strip);
    }

    #[cfg(feature = "readability")]
    #[test]
    fn cascade_falls_back_to_readability_when_strip_is_short() {
        // The article body lives inside <aside>, which strip_to_text
        // skips entirely. Readability scores the DOM independently and
        // recovers the article via content-density heuristics
        // (article-body matches dom_smoothie's positive vocabulary).
        // Asserting on `extractor == Readability` is what makes this a
        // real regression guard: a silent-bail bug in
        // extract_with_readability would flip the extractor back to
        // Strip and the test would fail.
        let html = "<!DOCTYPE html><html><body>\
            <aside class=\"article-body\">\
            <article>\
            <p>This is a long-form article with substantial paragraphs that \
            readability identifies as the main content. It contains enough \
            sentences to satisfy readability's content-density heuristics, \
            and the class name is in dom_smoothie's positive vocabulary.</p>\
            <p>A second paragraph adds more weight to the article scoring. \
            It mentions specific entities and quotes to look like real prose. \
            The scanner skips the entire aside, so the cascade has to fall \
            back to readability or it returns nothing.</p>\
            </article>\
            </aside>\
            </body></html>";
        let strip = crate::html::strip_to_text(html);
        let result = extract_html_cascade(html);
        assert!(
            strip.len() < 100,
            "strip should drop content-in-aside: {strip:?}"
        );
        assert_eq!(
            result.extractor,
            Extractor::Readability,
            "cascade must elect readability when strip is empty"
        );
        assert!(result.text.contains("long-form article"));
    }

    #[cfg(feature = "readability")]
    #[test]
    fn cascade_returns_strip_when_both_extractors_fail_to_find_content() {
        let result = extract_html_cascade("<html><body></body></html>");
        assert_eq!(result.extractor, Extractor::Strip);
        assert!(!result.fallback);
    }

    #[cfg(feature = "html2text")]
    #[test]
    fn extract_html2text_basic() {
        let result = extract_html2text("<p>Hello <b>world</b>!</p>", 80);
        assert!(result.text.contains("Hello"));
        assert!(result.text.contains("world"));
        assert_eq!(result.extractor, Extractor::Html2text);
    }
}
