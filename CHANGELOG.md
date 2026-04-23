# Changelog

All notable changes land here. The crate uses SemVer: breaking changes
bump the minor version while in 0.x.

## Unreleased

### Fixed

- `strip_to_text_with_paths`: HTML5 void elements (`<img>`, `<br>`,
  `<hr>`, `<input>`, etc.) were pushed onto `path_stack` and never
  popped, leaking into the `PathSpan.path` of any following text in
  the same block. Void elements are now excluded from the stack. The
  alt-text span for `<img>` still carries the surrounding container's
  path (previously the path ended in `img`).

### Changed

- `strip_to_segments` now emits `Segment::Image` for blocks whose text
  comes entirely from `SpanKind::Synthetic` spans — the typical case
  is a standalone `<img>` inside `<figure>`, `<body>`, or at the root.
  Inline `<img>` inside a paragraph keeps the enclosing
  `NarrativeText` (the all-synthetic check flips off as soon as a
  Direct/EntityDecoded span contributes).
- Structural block roles (`Title`, `Header`, `Footer`, `ListItem`,
  `Table`, `CodeSnippet`, `FigureCaption`) always win over
  `Image` — an `<img>` inside `<h1>`, `<td>`, `<li>`, or `<pre>`
  belongs to that container's semantic role.
- `<summary>` now classifies as `Title` and sets `last_title_id`, so
  paragraphs inside a `<details>` carry `parent_id` pointing at the
  summary. `category_depth` stays unset (summary isn't h1-h6).
- The link-density filter preserves `Table` segments alongside the
  existing `Title` / `Header`. Tables that reach the segmenter past
  the scanner-level nav/footer/aside skip are content (product specs,
  comparison grids, TOC tables on documentation pages). WCXB
  triple-pipeline: listing F1 0.580 → 0.613 (+3.3pp); overall F1
  0.765 → 0.767.

### Added

- `examples/segments_json.rs` — emit pure `Vec<Segment>` JSON to
  stdout. Pairs with `scripts/langchain_interop.py` to demonstrate
  the LangChain wire-format round-trip end-to-end.
- `scripts/langchain_interop.py` — stdlib-only Python script that
  deserializes `segments.json` into `(page_content, metadata)` tuples
  matching `langchain_core.documents.Document`.
- `examples/filter_pipeline.rs` — runnable walkthrough of the
  three-filter composition (link-density → sentence-density →
  boilerplate) on a single HTML page.

### Tests

- `tests/spanmap.rs`: 67 → 69. Void-element regression guards.
- `tests/segments.rs`: 29 → 40. `Segment::Image` emission, structural
  roles overriding Image, `<summary>` classification, Table
  preservation under link-density.
- `tests/bench_real_html.rs`: migrated from live URLs (scrapinghub,
  example.com, Wikipedia, HN) to WCXB-fixture smoke tests.
- Dropped unused `flate2` dev-dependency.

## 0.12.0 — 2026-04-22

### Fixed
- `strip_to_text_with_paths`: span `output_end` was rebased only for
  leading-whitespace trim, not trailing. With trailing whitespace in
  source, `output_end` could exceed the returned trimmed text length
  and panic callers on `text[span.output_start..span.output_end]`.
  Spans are now clamped to the trimmed output length on both sides.
- `remap_spans` demoted `SpanKind::Direct` → `EntityDecoded` only on
  byte-count changes. Whitespace runs like `" \n"` collapse to `"\n"`
  with the count preserved but the byte value swapped, leaving a
  Direct span whose output `\n` claimed byte-exact correspondence to
  source `' '`. Now also compares bytes and demotes on content
  mismatch. Surfaced by proptest on `"a<span> </span><h1 />'"`.

### Added
- `strip_to_text_with_spans` and `strip_to_text_with_paths` now emit a
  single whole-input span on the plain-text fast path (input with no
  `<`). Kind is `Direct` when output bytes equal source, else
  `EntityDecoded`. Previously the fast path returned an empty
  `SpanMap`, which was API-inconsistent with the tagged path.
- `html::filter_low_sentence_density(segments, min_sentences_per_100_words)`
  drops `NarrativeText` / `UncategorizedText` segments whose
  `(punctuation count) / (word count) * 100` falls below the floor.
  Catches tag-cloud paragraphs that the link-density filter misses
  because they aren't wrapped in anchors. Preserves Title, Header,
  Footer, ListItem, Table, CodeSnippet, Formula, Image, FigureCaption,
  PageBreak, and short blocks under 15 words.
- DOCX tables emit `Segment::Table` with `metadata.text_as_html`
  populated from a normalized `<table><tr><td>…</td></tr></table>`
  representation that mirrors the HTML pre-pass. HTML-sensitive
  characters in cell text (`<`, `>`, `&`, `"`) are escaped.

### Tests
- `tests/spanmap.rs` grew from 36 → 66 tests: regression guards for
  the 0.11.0 `</a>` path-leak, sibling indexing in paths, UTF-8
  char-boundary safety, per-`SpanKind` source_position semantics,
  whitespace-collapse demotion, self-closing tags, unclosed tags,
  multibyte text, trim-end OOB, and more.
- `tests/proptest.rs` grew from 22 → 32 tests: invariants for span
  bounds, sort order, non-overlap, source_range monotonicity,
  Direct first-byte byte-exactness, and plain-strip output parity.
- `tests/segments.rs` grew from 22 → 29 tests: DOCX table extraction,
  escaped special chars in `text_as_html`, sentence-density filter
  composing with link-density and boilerplate filters.
- Total `cargo test --all-features --all-targets`: 467 (0.10.0) →
  558 passing. 14 doc-tests.

## 0.11.0 — 2026-04-18

### Fixed
- `strip_to_text_with_paths`: closing tags whose bare name matched no
  stack entry left `path_stack` unchanged, so text emitted after
  `</a>` (and other inline closes) carried stale `/a` in its
  `PathSpan.path`. Bug was a chained `unwrap_or` that fell back to the
  full `tag_lower` (including the leading `/`) when the intermediate
  strip found nothing. Replaced with `trim_matches('/')`. This
  affects any consumer that read `PathSpan.path` directly; the
  user-visible extracted text is unchanged.

### Added
- `html::strip_to_segments_filtered(html, link_ratio_cap)` applies a
  Trafilatura-style link-density filter. Measured on WCXB at
  `cap=0.45`: F1 0.740 → 0.748 (+0.8pp overall), precision +2.2pp,
  recall −1.4pp. Per-type: article +1.2, forum +1.4, service +1.6;
  listing −3.4 (legitimate listings are link-heavy). Titles and
  headers are always preserved.
- `pdf_oxide::extract_to_segments_with_coords` /
  `extract_bytes_to_segments_with_coords` emit one Segment per
  detected text line with `metadata.coordinates` populated from
  `pdf_oxide::extract_text_lines` — Docling-style page anchors for
  RAG citations.
- `Coordinates` is now re-exported from `deformat::html`.

## 0.10.0 — 2026-04-18

### Breaking
- `SegmentData` and `SegmentMetadata` are now `#[non_exhaustive]`.
  Construct via field assignment from `SegmentMetadata::default()`
  rather than struct literals; future field additions are now
  non-breaking.
- `SegmentMetadata` gains `coordinates: Option<Coordinates>` for PDF
  bounding boxes (Unstructured-compatible wire shape).

### Added
- `html::filter_boilerplate(segments, min_chars)` drops short
  punctuation-less segments (navigation / menu fragments). Measured
  on WCXB: `without%` improves from 56.5 → 64.3, precision +1.1pp,
  recall -1.5pp; overall F1 flat at the token level.
- `docx::extract_to_segments` / `extract_bytes_to_segments` emit one
  segment per `<w:p>`. `<w:pStyle w:val="Heading1..9">` → `Title` with
  `category_depth`; others → `NarrativeText` with `parent_id` linking
  to the preceding Title.
- `pdf_oxide::extract_to_segments` / `extract_bytes_to_segments` emit
  one `NarrativeText` per non-empty page with `metadata.page_number`.
- `Segment::Table` now carries `metadata.text_as_html` populated from
  the original `<table>...</table>` source range. Nested tables
  collapse into the outermost table's Segment.
- New `Coordinates` public type (points + system + layout dims).

## 0.9.1 — 2026-04-18

### Added
- `extract_from_bytes(bytes) -> Result<Extracted, Error>` dispatches
  over `detect::detect_bytes` to the matching per-format `extract_bytes`
  backend. One-call end-to-end over raw input.
- `Extractor::PdfOxide` variant: the `pdf_oxide` backend no longer
  mislabels itself as `PdfExtract` in the `Extracted.extractor` field.
  Non-breaking via `#[non_exhaustive]`.
- New examples: `segments.rs` (Unstructured-compatible JSON output),
  `extract_metadata.rs` (head-section scan + whichlang composition),
  `charset.rs` (Windows-1252 decode via `encoding_rs`).
- README documents the structured-output API and the additional
  feature flags (`serde`, `whichlang`, `encoding_rs`, `pdf_oxide`).
- `.github/settings.yml` adds `pptx` topic.

## 0.9.0 — 2026-04-18

### Added
- `html::strip_to_segments(html) -> Vec<Segment>` emits Unstructured.io-
  compatible typed elements (Title, NarrativeText, ListItem, Table,
  Image, FigureCaption, Header, Footer, CodeSnippet, Formula,
  PageBreak, UncategorizedText). `serde_json::to_value(&segments)` with
  the `serde` feature produces the shape that
  `langchain-community`'s `UnstructuredLoader` and Haystack's
  `UnstructuredDocumentConverter` consume directly.
- `Segment`, `SegmentData`, `SegmentMetadata` public types.
- `scripts/fetch_wcxb.py` + `examples/bench_wcxb.rs` for WCXB benchmark
  evaluation. WCXB (CC-BY-4.0) is the 2026 replacement for the 2017
  Scrapinghub article-extraction benchmark.
- README publishes real WCXB F1/P/R numbers per page type.

### Changed
- Jumped on crates.io from 0.6.0 → 0.9.0; intermediate 0.7.x / 0.8.0
  existed only in-repo. Intermediate changes are included below for
  users upgrading from 0.6.0.

## 0.8.0 (in-repo only) — 2026-04-18

### Added
- `pptx` feature + `pptx::extract_file` / `extract_bytes` for PPTX
  presentations, including speaker notes.
- `Format::Pptx` variant (non-breaking via `#[non_exhaustive]`), MIME
  and extension detection, `ppt/` ZIP-directory signature.
- `encoding_rs` feature + `detect::decode_bytes(bytes, default_label)`
  for charset-aware decoding (BOM → `<meta charset>` → default).
- `pdf_oxide` feature + `pdf_oxide::extract_file` / `extract_bytes` as
  an alternative PDF backend. Both PDF backends can coexist; callers
  pick.

## 0.7.1 (in-repo only) — 2026-04-18

### Added
- `strip_to_markdown` preserves code-fence language hints from
  `<pre class="language-X">` and `<pre><code class="language-X">`.
- `whichlang` feature + `html::detect_language(text)` returning ISO
  639-3 language codes.
- "Known limitations" section in the README honestly positioning the
  F1 gap vs Trafilatura-class extractors.

## 0.7.0 (in-repo only) — 2026-04-18

### Breaking
- Renamed `Error::PdfExtract(String)` → `Error::Parse(String)` — the
  prior variant was raised by DOCX, EPUB, RTF, and XLSX modules too,
  making the "PDF extraction failed" wording misleading. PDF messages
  are now prefixed `"PDF:"` explicitly.
- `SpanMap`'s internal tuple `(usize, usize, usize, usize)` replaced by
  a `Span` struct with an added `kind: SpanKind` field
  (`Direct | EntityDecoded | Synthetic`). `SpanMap::iter()` yields
  `&Span` instead of the tuple.
- `PathSpan` gains the same `kind` field. Both structs are
  `#[non_exhaustive]`.

### Added
- `SpanMap::source_position(pos) -> Option<usize>` — single-byte
  lookup with linear interpolation within `Direct` runs.
- `SpanMap::source_range` now uses binary search.
- `strip_to_text_with_spans` output now matches `strip_to_text`
  (whitespace collapsed via a new cleanup-with-map helper).

## 0.6.0 and earlier

See git history.
