//! Typed element output compatible with the Unstructured.io JSON schema.
//!
//! [`strip_to_segments`] walks an HTML document and emits a
//! `Vec<Segment>` where each segment is tagged with one of a small set
//! of structural types (`Title`, `NarrativeText`, `ListItem`, ...).
//!
//! The JSON form matches the wire format consumed by
//! `langchain-community`'s `UnstructuredLoader` (element mode) and
//! Haystack's `UnstructuredDocumentConverter`, so Rust pipelines can
//! interoperate with Python RAG tooling without writing a shim.
//!
//! ```json
//! [
//!   {
//!     "type": "Title",
//!     "element_id": "a1b2c3d4e5f6789a",
//!     "text": "Climate Change",
//!     "metadata": { "category_depth": 1 }
//!   },
//!   {
//!     "type": "NarrativeText",
//!     "element_id": "b2c3d4e5f6a1789a",
//!     "text": "Rising temperatures threaten biodiversity.",
//!     "metadata": { "parent_id": "a1b2c3d4e5f6789a" }
//!   }
//! ]
//! ```

use std::hash::{Hash, Hasher};

/// One typed extraction unit in an [`Unstructured.io`]-compatible
/// output stream.
///
/// [`Unstructured.io`]: https://docs.unstructured.io/open-source/concepts/document-elements
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[non_exhaustive]
pub enum Segment {
    /// Document title or section heading.
    Title(SegmentData),
    /// Running prose (paragraphs, blockquotes, standalone text).
    NarrativeText(SegmentData),
    /// A single list item from an ordered or unordered list.
    ListItem(SegmentData),
    /// Tabular data. Plain text in `text`; HTML form (if available) in
    /// `metadata.text_as_html`.
    Table(SegmentData),
    /// Image reference. `text` holds the alt attribute.
    Image(SegmentData),
    /// Caption attached to an image or figure.
    FigureCaption(SegmentData),
    /// Document header (masthead, page header).
    Header(SegmentData),
    /// Document footer.
    Footer(SegmentData),
    /// Block of code (fenced or `<pre><code>`); language in
    /// `metadata.languages` when detected.
    CodeSnippet(SegmentData),
    /// Math formula or equation.
    Formula(SegmentData),
    /// Hard page break (PDF-driven).
    PageBreak(SegmentData),
    /// Content that did not fit a more specific category.
    UncategorizedText(SegmentData),
}

/// Payload common to every [`Segment`] variant.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SegmentData {
    /// Short stable identifier; content-hashed so repeated parses of
    /// the same input produce the same id.
    pub element_id: String,
    /// Extracted text. Whitespace normalized, entities decoded.
    pub text: String,
    /// Optional per-segment metadata. Omitted fields serialize as
    /// absent JSON keys, matching Unstructured's wire format.
    pub metadata: SegmentMetadata,
}

/// Metadata carried alongside a segment's text.
///
/// Every field is optional and is skipped during JSON serialization
/// when `None`. The shape matches the core fields of Unstructured.io's
/// `ElementMetadata` plus a small compatibility carve-out:
/// `category_depth` is how Unstructured encodes heading level, and is
/// also used by Docling (`SectionHeaderItem.level`).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SegmentMetadata {
    /// 1-based page number (set by PDF / DOCX extractors; not set for HTML).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub page_number: Option<u32>,
    /// `element_id` of the nearest enclosing `Title` or `Header`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub parent_id: Option<String>,
    /// Heading level (1 for `<h1>`, 2 for `<h2>`, ...). Only set on
    /// [`Segment::Title`].
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub category_depth: Option<u32>,
    /// ISO 639-3 language codes detected in the segment's text.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub languages: Option<Vec<String>>,
    /// Source filename, when extracted from a file.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub filename: Option<String>,
    /// Source MIME type.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub filetype: Option<String>,
    /// HTML form of the segment (used for [`Segment::Table`]).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub text_as_html: Option<String>,
}

impl Segment {
    /// Access the common payload regardless of variant.
    #[must_use]
    pub fn data(&self) -> &SegmentData {
        match self {
            Segment::Title(d)
            | Segment::NarrativeText(d)
            | Segment::ListItem(d)
            | Segment::Table(d)
            | Segment::Image(d)
            | Segment::FigureCaption(d)
            | Segment::Header(d)
            | Segment::Footer(d)
            | Segment::CodeSnippet(d)
            | Segment::Formula(d)
            | Segment::PageBreak(d)
            | Segment::UncategorizedText(d) => d,
        }
    }

    /// The Unstructured `type` string for this segment.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Segment::Title(_) => "Title",
            Segment::NarrativeText(_) => "NarrativeText",
            Segment::ListItem(_) => "ListItem",
            Segment::Table(_) => "Table",
            Segment::Image(_) => "Image",
            Segment::FigureCaption(_) => "FigureCaption",
            Segment::Header(_) => "Header",
            Segment::Footer(_) => "Footer",
            Segment::CodeSnippet(_) => "CodeSnippet",
            Segment::Formula(_) => "Formula",
            Segment::PageBreak(_) => "PageBreak",
            Segment::UncategorizedText(_) => "UncategorizedText",
        }
    }
}

/// Build a content-derived element id.
///
/// Unstructured's Python default is a random 16-hex-char id. We use
/// a content hash so the same input produces the same ids every run,
/// which is what reproducible pipelines (caching, snapshot tests) need.
pub(crate) fn element_id(kind: &str, text: &str, ord: usize) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    text.hash(&mut h);
    ord.hash(&mut h);
    let full = h.finish();
    // Unstructured uses 16 hex chars; `u64` is 16 hex chars exactly.
    format!("{full:016x}")
}

/// Extract typed [`Segment`]s from an HTML document.
///
/// Each segment corresponds to one block-level region of the source:
/// one heading, one paragraph, one list item, one code block, etc. The
/// output order follows the document order.
///
/// The returned [`Vec`] is Unstructured.io wire-compatible — with the
/// `serde` feature enabled, `serde_json::to_value(&segments)` produces
/// the JSON shape that `langchain-community`'s `UnstructuredLoader` and
/// Haystack's `UnstructuredDocumentConverter` accept directly.
///
/// # Examples
///
/// ```
/// use deformat::html::strip_to_segments;
///
/// let segs = strip_to_segments(
///     "<article><h1>Greeting</h1><p>Hello, world!</p></article>",
/// );
/// assert_eq!(segs.len(), 2);
/// assert_eq!(segs[0].type_name(), "Title");
/// assert_eq!(segs[0].data().text, "Greeting");
/// assert_eq!(segs[1].type_name(), "NarrativeText");
/// ```
#[must_use]
pub fn strip_to_segments(html: &str) -> Vec<Segment> {
    let (full_text, path_spans) = crate::html::strip_to_text_with_paths(html);
    if path_spans.is_empty() {
        return Vec::new();
    }

    // Group consecutive path_spans that share the same block-level
    // ancestor. Each group becomes one Segment.
    let mut segments: Vec<Segment> = Vec::new();
    let mut current: Option<GroupAcc> = None;
    let mut last_title_id: Option<String> = None;

    for span in path_spans {
        let (block_key, block_tag, depth) = classify_block(&span.path);
        let text_slice = full_text
            .get(span.output_start..span.output_end)
            .unwrap_or("");

        match current {
            Some(ref mut acc) if acc.block_key == block_key => {
                if !acc.text.is_empty() && !text_slice.is_empty() {
                    acc.text.push(' ');
                }
                acc.text.push_str(text_slice);
            }
            _ => {
                if let Some(acc) = current.take() {
                    finish_group(acc, segments.len(), &mut segments, &mut last_title_id);
                }
                current = Some(GroupAcc {
                    block_key,
                    block_tag,
                    depth,
                    text: text_slice.to_string(),
                });
            }
        }
    }
    if let Some(acc) = current {
        finish_group(acc, segments.len(), &mut segments, &mut last_title_id);
    }
    segments
}

struct GroupAcc {
    /// Identity used for grouping: full path to the block-level ancestor.
    block_key: String,
    /// The innermost block-level tag name (or `""` if none).
    block_tag: String,
    /// Heading depth, if the block tag is h1..h6.
    depth: Option<u32>,
    text: String,
}

/// Parse a PathSpan path and return:
/// - `block_key`: path up to and including the innermost block-level tag
/// - `block_tag`: that tag's name
/// - `depth`: Some(n) if the tag is h1..h6
fn classify_block(path: &str) -> (String, String, Option<u32>) {
    let mut block_idx: Option<usize> = None;
    let parts: Vec<&str> = path.split('/').collect();
    for (i, raw) in parts.iter().enumerate() {
        let name = raw.split('[').next().unwrap_or(raw);
        if is_block_like(name) {
            block_idx = Some(i);
        }
    }
    match block_idx {
        Some(i) => {
            let block_key = parts[..=i].join("/");
            let block_tag = parts[i].split('[').next().unwrap_or(parts[i]).to_string();
            let depth = heading_depth(&block_tag);
            (block_key, block_tag, depth)
        }
        None => (path.to_string(), String::new(), None),
    }
}

fn is_block_like(tag: &str) -> bool {
    matches!(
        tag,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "li"
            | "blockquote"
            | "pre"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "caption"
            | "figcaption"
            | "header"
            | "footer"
            | "dt"
            | "dd"
    )
}

fn heading_depth(tag: &str) -> Option<u32> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn finish_group(
    acc: GroupAcc,
    ord: usize,
    out: &mut Vec<Segment>,
    last_title_id: &mut Option<String>,
) {
    let text = acc.text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let type_name = block_tag_to_type(&acc.block_tag);
    let eid = element_id(type_name, &text, ord);
    let mut meta = SegmentMetadata::default();
    if let Some(d) = acc.depth {
        meta.category_depth = Some(d);
    }
    if type_name != "Title" && type_name != "Header" {
        if let Some(parent) = last_title_id.clone() {
            meta.parent_id = Some(parent);
        }
    }
    let data = SegmentData {
        element_id: eid.clone(),
        text,
        metadata: meta,
    };
    let seg = match type_name {
        "Title" => {
            *last_title_id = Some(eid.clone());
            Segment::Title(data)
        }
        "Header" => Segment::Header(data),
        "Footer" => Segment::Footer(data),
        "ListItem" => Segment::ListItem(data),
        "Table" => Segment::Table(data),
        "FigureCaption" => Segment::FigureCaption(data),
        "CodeSnippet" => Segment::CodeSnippet(data),
        "UncategorizedText" => Segment::UncategorizedText(data),
        _ => Segment::NarrativeText(data),
    };
    out.push(seg);
}

fn block_tag_to_type(tag: &str) -> &'static str {
    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "Title",
        "li" => "ListItem",
        "table" | "tr" | "td" | "th" => "Table",
        "pre" => "CodeSnippet",
        "figcaption" | "caption" => "FigureCaption",
        "header" => "Header",
        "footer" => "Footer",
        "p" | "blockquote" | "dt" | "dd" => "NarrativeText",
        "" => "UncategorizedText",
        _ => "NarrativeText",
    }
}
