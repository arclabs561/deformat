# deformat

[![crates.io](https://img.shields.io/crates/v/deformat.svg)](https://crates.io/crates/deformat)
[![docs.rs](https://docs.rs/deformat/badge.svg)](https://docs.rs/deformat)

Extract plain text from HTML, PDF, and other document formats.

## Supported formats

| Format | Input | Feature flag | Extractor |
|--------|-------|--------------|-----------|
| HTML (tag strip) | `&str` | *(none -- always available)* | `html::strip_to_text` |
| HTML (markdown) | `&str` | *(none)* | `html::strip_to_markdown` |
| HTML (layout-aware) | `&str` | `html2text` | `extract_html2text` |
| HTML (article) | `&str` | `readability` | `extract_readable` |
| PDF | `&Path` or `&[u8]` | `pdf` | `pdf::extract_file`, `pdf::extract_bytes` |
| DOCX | `&Path` or `&[u8]` | `docx` | `docx::extract_file`, `docx::extract_bytes` |
| EPUB | `&Path` or `&[u8]` | `epub` | `epub::extract_file`, `epub::extract_bytes` |
| RTF | `&Path` or `&[u8]` | `rtf` | `rtf::extract_file`, `rtf::extract_bytes` |
| XLSX/XLS/ODS | `&Path` or `&[u8]` | `xlsx` | `xlsx::extract_file`, `xlsx::extract_bytes` |
| PPTX | `&Path` or `&[u8]` | `pptx` | `pptx::extract_file`, `pptx::extract_bytes` |
| XML | `&str` | *(none)* | `html::strip_to_text` (tag strip) |
| Plain text / Markdown | `&str` | *(none)* | passthrough |

The default build depends only on [`memchr`](https://crates.io/crates/memchr).

## Install

```sh
cargo add deformat                                        # minimal
cargo add deformat --features readability,html2text,pdf   # all extractors
```

```toml
[dependencies]
deformat = { version = "0.11.0", features = ["readability", "html2text"] }
```

## Usage

### Auto-detect and extract

```rust
use deformat::{extract, Format};

let result = extract("<p>Hello <b>world</b>!</p>").unwrap();
assert_eq!(result.text, "Hello world!");
assert_eq!(result.format, Format::Html);

// Plain text passes through unchanged
let result = extract("Just plain text.").unwrap();
assert_eq!(result.text, "Just plain text.");
assert_eq!(result.format, Format::PlainText);
```

All extraction functions return an `Extracted` struct:

```rust
pub struct Extracted {
    pub text: String,
    pub format: Format,
    pub extractor: Extractor,    // Strip, Readability, Html2text, PdfExtract, PdfOxide, Passthrough
    pub title: Option<String>,   // article title (readability only)
    pub excerpt: Option<String>, // article excerpt (readability only)
    pub fallback: bool,          // true if a richer extractor failed
}
```

### HTML strategies

Three HTML extractors: `html::strip_to_text` (tag stripping, always available), `extract_html2text` (layout-aware DOM, feature: `html2text`), and `extract_readable` (article extraction via Mozilla Readability, feature: `readability` -- falls back to tag stripping if content < 50 chars). Entity decoding available via `html::decode_entities`.

`html::extract_metadata` returns an `HtmlMetadata` struct with title, author, description, date, language, and canonical URL extracted from `<head>`. No feature flag required.

```rust
let meta = deformat::html::extract_metadata(html);
// meta.title, meta.author, meta.description, meta.date_published,
// meta.language, meta.canonical_url
```

### PDF extraction

```rust
let result = deformat::pdf::extract_file(std::path::Path::new("report.pdf"))?;
let result = deformat::pdf::extract_bytes(&pdf_bytes)?;
```

### Format detection

`detect_str`, `detect_bytes`, `detect_path` return `Format`. Helpers: `is_html`, `is_pdf`.

### Structured output (Unstructured.io-compatible)

`html::strip_to_segments` returns a `Vec<Segment>` with typed variants
(Title, NarrativeText, ListItem, Table, CodeSnippet, ...). With the
`serde` feature, the JSON form matches the shape that
`langchain-community`'s `UnstructuredLoader` and Haystack's
`UnstructuredDocumentConverter` consume directly -- no adapter needed.

```rust
let segs = deformat::html::strip_to_segments(
    "<article><h1>Greeting</h1><p>Hello, world!</p></article>",
);
assert_eq!(segs[0].type_name(), "Title");
assert_eq!(segs[0].data().metadata.category_depth, Some(1));
```

With `serde`:

```rust
let json = serde_json::to_string(&segs)?;
// Ships as: [{"type":"Title","element_id":"...","text":"Greeting","metadata":{"category_depth":1}}, ...]
```

### Additional feature flags

| Feature | Adds |
|---------|------|
| `serde` | `Serialize`/`Deserialize` on `Extracted`, `Format`, `Extractor`, `Segment`, `HtmlMetadata` |
| `whichlang` | `html::detect_language` — ISO 639-3 language detection |
| `encoding_rs` | `detect::decode_bytes` — charset-aware decoding of non-UTF-8 HTML |
| `pdf_oxide` | Alternative PDF backend (faster per vendor; unaudited here) |

## HTML tag stripping details

`html::strip_to_text` handles: tag removal, script/style/noscript content removal,
semantic element filtering (`<nav>`, `<header>`, `<footer>`, `<aside>`,
etc.), ~300 named HTML entities (Latin, Greek, math, typography), numeric/hex character
references, Windows-1252 C1 range mapping, CJK ruby annotation stripping, Wikipedia
boilerplate removal, reference marker stripping (`[1]`, `[edit]`), image alt text
extraction, and whitespace collapsing.

## Benchmark (WCXB dev split, 1,497 pages)

`cargo run --release --example bench_wcxb` — word-level F1 against the
`ground_truth.main_content` field from the
[WCXB](https://webcontentextraction.org) benchmark (CC-BY-4.0).

| page_type      |    N |    F1 |     P |     R |
|----------------|-----:|------:|------:|------:|
| article        |  792 | 0.851 | 0.778 | 0.986 |
| documentation  |   91 | 0.891 | 0.855 | 0.964 |
| service        |  165 | 0.730 | 0.648 | 0.951 |
| listing        |   99 | 0.602 | 0.524 | 0.942 |
| collection     |  117 | 0.532 | 0.415 | 0.966 |
| forum          |  112 | 0.504 | 0.610 | 0.755 |
| product        |  119 | 0.438 | 0.330 | 0.958 |
| **overall**    | 1495 | 0.740 | 0.675 | 0.957 |

Recall is strong across all page types; precision is the gap. Articles
and documentation are competitive; commerce / forum / listing pages
over-include boilerplate. Reproduce with `scripts/fetch_wcxb.py` +
the `bench_wcxb` example.

`html::filter_boilerplate` drops short label-like segments from
`strip_to_segments` output. Measured delta on the same WCXB dev split:
overall `without%` 56.5 → 64.3 (+7.8pp boilerplate removal), precision
+1.1pp, recall −1.5pp. Opt in by calling
`filter_boilerplate(segs, 40)` after `strip_to_segments`.

`html::strip_to_segments_filtered(html, link_ratio_cap)` applies a
Trafilatura-style link-density pass: blocks whose output text is
mostly inside `<a>` elements are dropped. Measured at `cap=0.45`:
overall F1 0.740 → **0.748**, precision +2.2pp, recall −1.4pp.
Per-type: article +1.2pp, forum +1.4pp, service +1.6pp; listing
regresses (−3.4pp) because legitimate listing pages are link-heavy.
Choose the threshold that fits your corpus — the helper sweeps in
`examples/bench_wcxb.rs`.

## Known limitations

Worth calling out so you can pick the right tool for the job:

- **Article-extraction precision**: see the WCXB table above. Trafilatura-class
  Python extractors reach F1 ≈ 0.94 on articles via text-density / link-density
  scoring; deformat has not implemented that yet. If you need highest precision
  for HTML-only pipelines, see `trafilatura`, `rs-trafilatura`, or `justext`
  on crates.io.
- **Table structure in PDF and DOCX is flattened to text**. Row/column
  relationships are lost. No Rust extractor currently reconstructs table structure
  from PDF line drawings; DOCX tables are emitted as tab-separated rows.
- **Default charset is UTF-8**. Non-UTF-8 HTML (legacy Windows-1252, CJK)
  needs the `encoding_rs` feature — call `detect::decode_bytes(bytes, "utf-8")`
  before handing the result to `strip_to_text`. BOM and `<meta charset>`
  are honored; caller's `default_label` is the fallback.
- **No OCR**. Scanned PDFs yield empty text. Compose with `tesseract-sys`.
- **No layout analysis**. Multi-column PDFs may read columns in presentation
  order rather than reading order. For typeset papers consider a vision-model
  pipeline (Python-side Marker, Docling) instead.
- **Output positions in `strip_to_text_with_spans` are post-cleanup**. Spans
  whose source bytes span whitespace collapsed by cleanup are tagged
  `SpanKind::EntityDecoded` rather than `Direct` — byte-level interpolation
  via `source_position` is only exact for `Direct` spans.

## License

MIT OR Apache-2.0
