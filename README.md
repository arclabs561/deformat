# deformat

Extract plain text from HTML, PDF, and other document formats.

NER engines, LLM pipelines, and search indexers need plain text. `deformat`
sits upstream: it takes formatted documents and returns clean text. No network
I/O -- it operates on `&str` and `&[u8]` inputs.

## Quick start

```rust
use deformat::{extract, Format};

// Auto-detect format and extract text
let result = extract("<p>Hello <b>world</b>!</p>");
assert_eq!(result.text, "Hello world!");
assert_eq!(result.format, Format::Html);

// Plain text passes through unchanged
let result = extract("Just plain text.");
assert_eq!(result.text, "Just plain text.");
```

## Feature flags

All features are opt-in. The default build has one dependency: `memchr`.

| Feature | Crate | What it adds |
|---------|-------|-------------|
| `readability` | `dom_smoothie` | Mozilla Readability article extraction |
| `html2text` | `html2text` | DOM-based HTML-to-text with layout awareness |
| `pdf` | `pdf-extract` | PDF text extraction |

```toml
[dependencies]
deformat = { version = "0.3", features = ["readability", "html2text"] }
```

## HTML extraction

Three strategies, from simplest to most capable:

1. **`html::strip_to_text`** (always available) -- fast byte-level tag stripping
   with ~300 named HTML entities (ISO-8859-1, Latin Extended-A for Central/Eastern
   European names, Greek, math, typography), Windows-1252 C1 range mapping,
   CJK ruby annotation stripping, semantic element filtering, image alt text
   extraction, and Wikipedia boilerplate removal.

2. **`extract_html2text`** (feature `html2text`) -- DOM-based conversion that
   preserves layout structure (tables, lists, indentation).

3. **`extract_readable`** (feature `readability`) -- Mozilla Readability
   algorithm that extracts the main article content, stripping navigation,
   sidebars, and boilerplate. Falls back to `strip_to_text` if extraction
   produces insufficient content.

### Entity decoding

```rust
// Standalone entity decoding (useful for attribute values, etc.)
assert_eq!(deformat::html::decode_entities("Caf&eacute;"), "Café");
assert_eq!(deformat::html::decode_entities("&#169; 2026"), "\u{00A9} 2026");
```

## Format detection

```rust
use deformat::detect::{is_html, is_pdf, detect_str, detect_bytes, detect_path};
use deformat::Format;

assert!(is_html("<!DOCTYPE html><html>..."));
assert_eq!(detect_str("<html><body>Hello</body></html>"), Format::Html);
assert_eq!(detect_path("report.pdf"), Format::Pdf);
```

## License

MIT OR Apache-2.0
