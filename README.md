# deformat

[![crates.io](https://img.shields.io/crates/v/deformat.svg)](https://crates.io/crates/deformat)
[![docs.rs](https://docs.rs/deformat/badge.svg)](https://docs.rs/deformat)

Extracts plain text from HTML, PDF, and other document formats. Operates on
`&str` and `&[u8]` inputs -- no network I/O, no filesystem access (except
PDF file extraction).

## Supported formats

| Format | Input | Feature flag | Extractor |
|--------|-------|--------------|-----------|
| HTML (tag strip) | `&str` | *(none -- always available)* | `html::strip_to_text` |
| HTML (layout-aware) | `&str` | `html2text` | `extract_html2text` |
| HTML (article) | `&str` | `readability` | `extract_readable` |
| PDF | `&Path` or `&[u8]` | `pdf` | `pdf::extract_file`, `pdf::extract_bytes` |
| Plain text / Markdown | `&str` | *(none)* | passthrough |

The default build depends only on [`memchr`](https://crates.io/crates/memchr).

## Install

```sh
cargo add deformat                                        # minimal
cargo add deformat --features readability,html2text,pdf   # all extractors
```

```toml
[dependencies]
deformat = { version = "0.4.1", features = ["readability", "html2text"] }
```

## Usage

### Auto-detect and extract

```rust
use deformat::{extract, Format};

let result = extract("<p>Hello <b>world</b>!</p>");
assert_eq!(result.text, "Hello world!");
assert_eq!(result.format, Format::Html);

// Plain text passes through unchanged
let result = extract("Just plain text.");
assert_eq!(result.text, "Just plain text.");
assert_eq!(result.format, Format::PlainText);
```

All extraction functions return an `Extracted` struct:

```rust
pub struct Extracted {
    pub text: String,
    pub format: Format,
    pub metadata: HashMap<String, String>,  // e.g. "extractor", "title", "excerpt"
}
```

### HTML strategies

```rust
// 1. Tag stripping (always available, fast)
let text = deformat::html::strip_to_text("<p>Hello <b>world</b>!</p>");
assert_eq!(text, "Hello world!");

// Standalone entity decoding
assert_eq!(deformat::html::decode_entities("Caf&eacute;"), "Cafe\u{0301}");
```

```rust
// 2. Layout-aware DOM conversion (feature: html2text)
let result = deformat::extract_html2text("<table><tr><td>A</td></tr></table>", 80);
```

```rust
// 3. Article extraction via Mozilla Readability (feature: readability)
//    Falls back to tag stripping if content is too short (< 50 chars).
let result = deformat::extract_readable(html, Some("https://example.com/article"));
```

### PDF extraction

```rust
// From file path (feature: pdf)
let result = deformat::pdf::extract_file(std::path::Path::new("report.pdf"))?;

// From bytes in memory
let result = deformat::pdf::extract_bytes(&pdf_bytes)?;
```

### Format detection

```rust
use deformat::detect::{is_html, is_pdf, detect_str, detect_bytes, detect_path};
use deformat::Format;

assert!(is_html("<!DOCTYPE html><html>..."));
assert_eq!(detect_str("<html><body>Hello</body></html>"), Format::Html);
assert_eq!(detect_bytes(b"%PDF-1.4 ..."), Format::Pdf);
assert_eq!(detect_path("report.pdf"), Format::Pdf);
```

## HTML tag stripping details

`html::strip_to_text` handles: tag removal, script/style/noscript content removal,
semantic element filtering (`<nav>`, `<header>`, `<footer>`, `<aside>`, `<form>`,
etc.), ~300 named HTML entities (Latin, Greek, math, typography), numeric/hex character
references, Windows-1252 C1 range mapping, CJK ruby annotation stripping, Wikipedia
boilerplate removal, reference marker stripping (`[1]`, `[edit]`), image alt text
extraction, and whitespace collapsing.

## License

MIT OR Apache-2.0
