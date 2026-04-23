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
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// Page-relative bounding box for this segment, when the source
    /// format carries layout information (currently: PDF via
    /// `pdf_oxide`). Absent for HTML / DOCX / EPUB / RTF.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub coordinates: Option<Coordinates>,
}

/// Bounding box coordinates for a segment, matching the shape in
/// Unstructured.io's `ElementMetadata.coordinates`.
///
/// Points are in the order `[top-left, top-right, bottom-right,
/// bottom-left]`. The `system` string is the coordinate frame
/// (commonly `"PixelSpace"` or `"PointSpace"` for PDF pages).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Coordinates {
    /// Four `(x, y)` corner points.
    pub points: Vec<(f64, f64)>,
    /// Name of the coordinate system (e.g. `"PixelSpace"`).
    pub system: String,
    /// Layout width of the page.
    pub layout_width: f64,
    /// Layout height of the page.
    pub layout_height: f64,
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
/// Variant of [`strip_to_segments`] that drops blocks with a high
/// link-text to total-text ratio.
///
/// Implements Trafilatura's link-density heuristic: for each block,
/// compute the fraction of text bytes that live inside `<a>` anchors.
/// Blocks above the threshold are navigation / boilerplate and are
/// dropped. `Title` and `Header` segments are preserved regardless.
///
/// `link_ratio_cap` is a value in `[0.0, 1.0]`. A Trafilatura-calibrated
/// default is around `0.45`; measured on WCXB, this is the single
/// biggest F1 mover for article / documentation page types.
///
/// # Examples
///
/// ```
/// use deformat::html::strip_to_segments_filtered;
///
/// let segs = strip_to_segments_filtered(
///     "<nav><a href='/'>Home</a> <a href='/a'>About</a></nav>\
///      <article><p>The real article content goes here.</p></article>",
///     0.45,
/// );
/// // Nav block dropped (100% link text); article block kept.
/// assert!(segs.iter().all(|s| !s.data().text.contains("Home")));
/// ```
#[must_use]
pub fn strip_to_segments_filtered(html: &str, link_ratio_cap: f32) -> Vec<Segment> {
    strip_to_segments_inner(html, Some(link_ratio_cap.clamp(0.0, 1.0)))
}

/// Extract typed [`Segment`]s from an HTML document.
///
/// Use [`strip_to_segments_filtered`] if you also want link-density
/// boilerplate filtering.
#[must_use]
pub fn strip_to_segments(html: &str) -> Vec<Segment> {
    strip_to_segments_inner(html, None)
}

fn strip_to_segments_inner(html: &str, link_ratio_cap: Option<f32>) -> Vec<Segment> {
    let (full_text, path_spans) = crate::html::strip_to_text_with_paths(html);
    if path_spans.is_empty() {
        return Vec::new();
    }

    // Pre-index top-level `<table>...</table>` source ranges so Table
    // segments can carry `text_as_html` matching the Unstructured wire
    // format. Only top-level (depth 1) tables are indexed — nested
    // tables share the outer table's HTML which is consistent with
    // emitting one Segment per outermost table.
    let table_ranges = index_table_ranges(html);

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
        let text_len = text_slice.chars().count();
        let under_anchor = path_has_anchor_below_block(&span.path, &block_key);
        let is_synthetic = span.kind == crate::html::SpanKind::Synthetic;

        match current {
            Some(ref mut acc) if acc.block_key == block_key => {
                if !acc.text.is_empty() && !text_slice.is_empty() {
                    acc.text.push(' ');
                }
                acc.text.push_str(text_slice);
                acc.src_end = acc.src_end.max(span.source_end);
                acc.total_chars += text_len;
                if under_anchor {
                    acc.link_chars += text_len;
                }
                if !is_synthetic {
                    acc.all_synthetic = false;
                }
            }
            _ => {
                if let Some(acc) = current.take() {
                    finish_group(
                        acc,
                        segments.len(),
                        &mut segments,
                        &mut last_title_id,
                        html,
                        &table_ranges,
                        link_ratio_cap,
                    );
                }
                current = Some(GroupAcc {
                    block_key,
                    block_tag,
                    depth,
                    text: text_slice.to_string(),
                    src_start: span.source_start,
                    src_end: span.source_end,
                    link_chars: if under_anchor { text_len } else { 0 },
                    total_chars: text_len,
                    all_synthetic: is_synthetic,
                });
            }
        }
    }
    if let Some(acc) = current {
        finish_group(
            acc,
            segments.len(),
            &mut segments,
            &mut last_title_id,
            html,
            &table_ranges,
            link_ratio_cap,
        );
    }
    segments
}

/// Find every top-level `<table>...</table>` byte range in `html`.
///
/// Nested tables are skipped — the outer range covers them. Returns
/// a sorted `Vec<(start, end)>` where `start` is the byte offset of
/// the `<` in `<table` and `end` is the byte offset just past the
/// `>` in `</table>`.
fn index_table_ranges(html: &str) -> Vec<(usize, usize)> {
    let bytes = html.as_bytes();
    let mut ranges = Vec::new();
    let mut depth: u32 = 0;
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if starts_with_ci(bytes, i + 1, b"table") {
            let after = i + 1 + b"table".len();
            if after >= bytes.len() {
                break;
            }
            let next = bytes[after];
            if next == b'>' || next == b' ' || next == b'\t' || next == b'\n' || next == b'/' {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
        } else if starts_with_ci(bytes, i + 1, b"/table") {
            let after = i + 1 + b"/table".len();
            if after >= bytes.len() {
                break;
            }
            let next = bytes[after];
            if matches!(next, b'>' | b' ' | b'\t' | b'\n') && depth > 0 {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        // Find the terminating `>` to include it.
                        let mut end = after;
                        while end < bytes.len() && bytes[end] != b'>' {
                            end += 1;
                        }
                        if end < bytes.len() {
                            ranges.push((s, end + 1));
                        }
                    }
                }
            }
        }
        i += 1;
    }
    ranges
}

fn starts_with_ci(bytes: &[u8], pos: usize, needle: &[u8]) -> bool {
    if pos + needle.len() > bytes.len() {
        return false;
    }
    bytes[pos..pos + needle.len()].eq_ignore_ascii_case(needle)
}

struct GroupAcc {
    /// Identity used for grouping: full path to the block-level ancestor.
    block_key: String,
    /// The innermost block-level tag name (or `""` if none).
    block_tag: String,
    /// Heading depth, if the block tag is h1..h6.
    depth: Option<u32>,
    text: String,
    /// Earliest source byte touched by this group.
    src_start: usize,
    /// Latest source byte touched by this group (exclusive).
    src_end: usize,
    /// Output-byte count of this group's text from inside `<a>` anchors.
    link_chars: usize,
    /// Output-byte count of all text in this group.
    total_chars: usize,
    /// True until a non-Synthetic span contributes to this group. Groups
    /// that only ever accumulate Synthetic spans (e.g., standalone `<img>`
    /// alt text) become [`Segment::Image`] rather than NarrativeText.
    all_synthetic: bool,
}

/// Parse a PathSpan path and return:
/// - `block_key`: path up to and including the innermost block-level tag
/// - `block_tag`: that tag's name
/// - `depth`: Some(n) if the tag is h1..h6
fn classify_block(path: &str) -> (String, String, Option<u32>) {
    let mut block_idx: Option<usize> = None;
    let mut table_idx: Option<usize> = None;
    let parts: Vec<&str> = path.split('/').collect();
    for (i, raw) in parts.iter().enumerate() {
        let name = raw.split('[').next().unwrap_or(raw);
        if name == "table" {
            table_idx = Some(i);
        }
        if is_block_like(name) {
            block_idx = Some(i);
        }
    }
    // A `<table>` ancestor dominates its cells: every td/tr/th inside
    // the same table groups into one Segment::Table. Without this, each
    // cell would emit its own Segment.
    let use_idx = table_idx.or(block_idx);
    match use_idx {
        Some(i) => {
            let block_key = parts[..=i].join("/");
            let block_tag = parts[i].split('[').next().unwrap_or(parts[i]).to_string();
            let depth = heading_depth(&block_tag);
            (block_key, block_tag, depth)
        }
        None => (path.to_string(), String::new(), None),
    }
}

/// Does `path` contain an `<a>` component after the block prefix?
///
/// The block's path is a prefix of `path`; any `a` element strictly
/// below that prefix means the text slice was rendered inside an
/// anchor, which is the signal Trafilatura uses for link-density
/// boilerplate detection.
fn path_has_anchor_below_block(path: &str, block_key: &str) -> bool {
    let after = match path.strip_prefix(block_key) {
        Some(rest) => rest.trim_start_matches('/'),
        None => return false,
    };
    after
        .split('/')
        .any(|c| c.split('[').next().unwrap_or(c) == "a")
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
    html: &str,
    table_ranges: &[(usize, usize)],
    link_ratio_cap: Option<f32>,
) {
    let text = acc.text.trim().to_string();
    if text.is_empty() {
        return;
    }
    // If every contributing span was Synthetic (typically `<img alt>`),
    // emit as Image regardless of the surrounding block tag. This covers
    // standalone `<img>` at any nesting level (`<figure><img>`,
    // `<body><img>`, `<article><img>`).
    let type_name = if acc.all_synthetic {
        "Image"
    } else {
        block_tag_to_type(&acc.block_tag)
    };
    // Link-density filter: drop over-link blocks (nav / tag clouds),
    // but always keep structural segments. Titles, Headers, ListItems,
    // and Tables reach this point only when the scanner-level skip list
    // already rejected pure-nav `<nav>`/`<footer>`/`<aside>` content, so
    // surviving list items and tables are content (TOCs, product grids,
    // blog indexes) — not navigation. Dropping them regresses
    // listing-style pages without meaningfully improving precision on
    // article-style pages.
    if let Some(cap) = link_ratio_cap {
        let is_structural = matches!(type_name, "Title" | "Header" | "Table");
        if !is_structural && acc.total_chars > 0 {
            let ratio = acc.link_chars as f32 / acc.total_chars as f32;
            if ratio > cap {
                return;
            }
        }
    }
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
    // Populate text_as_html for Table segments from the pre-indexed
    // `<table>...</table>` ranges.
    if type_name == "Table" {
        if let Some(&(ts, te)) = table_ranges
            .iter()
            .find(|(ts, te)| *ts <= acc.src_start && *te >= acc.src_end)
        {
            if let Some(slice) = html.get(ts..te) {
                meta.text_as_html = Some(slice.to_string());
            }
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
        "Image" => Segment::Image(data),
        "UncategorizedText" => Segment::UncategorizedText(data),
        _ => Segment::NarrativeText(data),
    };
    out.push(seg);
}

/// Drop segments that look like navigation, menus, or label fragments.
///
/// Pragmatic text-level heuristic, not Trafilatura's link-density pass:
///
/// - Segments shorter than `min_chars` whose text contains no sentence-
///   ending punctuation (`.`, `?`, `!`) are dropped. Typical targets:
///   `"About"`, `"Contact Us"`, `"Sign up"`, menu entries.
/// - ListItems with no sentence punctuation AND no space (single word /
///   single hyphenated token) are dropped.
/// - `Title` and `Header` / `Footer` segments are always preserved.
///
/// The filter is conservative by design: it trades a bit of recall for
/// a precision gain on navigation-heavy pages (forum, product,
/// listing, collection page types on the WCXB benchmark). Measure
/// before/after on your own corpus — the right threshold depends on
/// the source population.
#[must_use]
pub fn filter_boilerplate(segments: Vec<Segment>, min_chars: usize) -> Vec<Segment> {
    segments
        .into_iter()
        .filter(|seg| keep_segment(seg, min_chars))
        .collect()
}

fn keep_segment(seg: &Segment, min_chars: usize) -> bool {
    if matches!(
        seg,
        Segment::Title(_) | Segment::Header(_) | Segment::Footer(_)
    ) {
        return true;
    }
    let text = seg.data().text.trim();
    let has_sentence = text.chars().any(|c| matches!(c, '.' | '?' | '!'));

    match seg {
        Segment::ListItem(_) => has_sentence || text.contains(' '),
        _ => {
            if text.chars().count() < min_chars && !has_sentence {
                return false;
            }
            true
        }
    }
}

/// Drop [`Segment::NarrativeText`] / [`Segment::UncategorizedText`]
/// segments whose *sentence density* falls below
/// `min_sentences_per_100_words`.
///
/// Sentence density = count of `.`, `?`, `!` per 100 words. Typical
/// prose clocks in around 4-8 sentences per 100 words; navigation lists,
/// tag clouds, and pasted metadata usually score 0 because they have
/// many words but no sentence-ending punctuation. The link-density
/// filter catches anchor-heavy boilerplate; this filter catches text
/// blocks that look prose-shaped on the surface but lack sentence
/// structure.
///
/// This complements [`strip_to_segments_filtered`] (which drops blocks
/// by link ratio) and [`filter_boilerplate`] (which drops short
/// label-like fragments). A typical pipeline:
///
/// ```
/// use deformat::html::{filter_boilerplate, filter_low_sentence_density, strip_to_segments_filtered};
///
/// let html = "<nav><a href='/'>Home</a></nav>\
///             <article><p>The real article content goes here, with sentences.</p></article>";
/// let segs = strip_to_segments_filtered(html, 0.45);
/// let segs = filter_low_sentence_density(segs, 1.0);
/// let segs = filter_boilerplate(segs, 40);
/// assert!(segs.iter().all(|s| !s.data().text.contains("Home")));
/// ```
///
/// Preserved regardless of density:
/// [`Segment::Title`], [`Segment::Header`], [`Segment::Footer`],
/// [`Segment::ListItem`], [`Segment::Table`], [`Segment::CodeSnippet`],
/// [`Segment::Formula`], [`Segment::Image`], [`Segment::FigureCaption`],
/// [`Segment::PageBreak`]. These categories legitimately lack sentence
/// punctuation (list bullets, code, captions) or carry their own
/// structural signal.
///
/// Very short blocks (< `MIN_WORDS_FOR_DENSITY` words) are also
/// preserved because sentence density is noisy below that threshold.
///
/// `min_sentences_per_100_words` is a value in `[0.0, 100.0]`. Sensible
/// starting points:
/// - `0.5`  — very permissive; drops only true zero-punctuation blobs.
/// - `1.0`  — a reasonable default on article-heavy corpora.
/// - `2.0`  — aggressive; expect some real paragraphs to be dropped.
///
/// Measure before committing a threshold on your own corpus — content
/// distributions vary (docs pages are heavier on code blocks and
/// bulleted lists than blog posts).
#[must_use]
pub fn filter_low_sentence_density(
    segments: Vec<Segment>,
    min_sentences_per_100_words: f32,
) -> Vec<Segment> {
    let cap = min_sentences_per_100_words.max(0.0);
    segments
        .into_iter()
        .filter(|seg| keep_by_sentence_density(seg, cap))
        .collect()
}

/// Minimum word count below which sentence-density gating is skipped.
/// Short text blocks have too small a denominator for the density ratio
/// to be a reliable signal.
const MIN_WORDS_FOR_DENSITY: usize = 15;

fn keep_by_sentence_density(seg: &Segment, cap: f32) -> bool {
    if !matches!(
        seg,
        Segment::NarrativeText(_) | Segment::UncategorizedText(_)
    ) {
        return true;
    }
    let text = seg.data().text.trim();
    let words = text.split_whitespace().count();
    if words < MIN_WORDS_FOR_DENSITY {
        return true;
    }
    let sentences = text
        .chars()
        .filter(|c| matches!(c, '.' | '?' | '!'))
        .count();
    let density = (sentences as f32) * 100.0 / (words as f32);
    density >= cap
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
