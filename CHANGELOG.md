# Changelog

All notable changes land here. The crate uses SemVer: breaking changes
bump the minor version while in 0.x.

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
