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
deformat = { version = "0.7.1", features = ["readability", "html2text"] }
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
    pub extractor: Extractor,    // Strip, Readability, Html2text, PdfExtract, Passthrough
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

## HTML tag stripping details

`html::strip_to_text` handles: tag removal, script/style/noscript content removal,
semantic element filtering (`<nav>`, `<header>`, `<footer>`, `<aside>`,
etc.), ~300 named HTML entities (Latin, Greek, math, typography), numeric/hex character
references, Windows-1252 C1 range mapping, CJK ruby annotation stripping, Wikipedia
boilerplate removal, reference marker stripping (`[1]`, `[edit]`), image alt text
extraction, and whitespace collapsing.

## Known limitations

Worth calling out so you can pick the right tool for the job:

- **Article-extraction accuracy on crawled HTML**: on the Scrapinghub benchmark
  `deformat` scores F1 ≈ 0.79 (recall 0.997, precision 0.68). Trafilatura-class
  Python extractors reach F1 ≈ 0.94; Rust ports (`trafilatura`, `rs-trafilatura`,
  `justext`) are now available if highest precision matters more than multi-format
  coverage. Text-density boilerplate detection is not yet implemented here.
- **Table structure in PDF and DOCX is flattened to text**. Row/column
  relationships are lost. No Rust extractor currently reconstructs table structure
  from PDF line drawings; DOCX tables are emitted as tab-separated rows.
- **Charset detection assumes UTF-8**. Legacy Windows-1252 / CJK-encoded
  documents must be decoded upstream before calling extractors that take `&str`.
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
