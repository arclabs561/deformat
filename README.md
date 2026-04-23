# deformat

[![crates.io](https://img.shields.io/crates/v/deformat.svg)](https://crates.io/crates/deformat)
[![docs.rs](https://docs.rs/deformat/badge.svg)](https://docs.rs/deformat)

Extract plain text from HTML, PDF, and other document formats. Single
Rust crate, twelve formats, source-byte tracking, no DOM parse on the
default HTML path.

## When to reach for deformat

- Running a RAG / NER / search pipeline that needs clean text from
  many input formats with one dependency.
- Wanting source-position tracking (`SpanMap`, `PathSpan`) so a
  highlighted snippet can be pointed back at its byte range in the
  original HTML / DOCX / PDF.
- Needing throughput on a Rust ingestion pipeline; the default HTML
  scanner is `&[u8]`-walking with `memchr`, no allocator pressure
  from a DOM tree.
- Wanting Unstructured.io-shape `Segment` JSON without a Python
  process boundary.

## When to reach for something else

- **Pretraining-scale article extraction** where every F1 point
  matters: use [`Trafilatura`](https://github.com/adbar/trafilatura)
  (Python, F1 ≈ 0.94 on news/article corpora vs deformat's 0.87 on
  the same page class). The published comparative-extraction
  literature places the heuristic-extractor ceiling at ≈ 0.91; closing
  that last gap is what Trafilatura's DOM-aware block scoring and
  multi-pass cascade buys.
- Document understanding for vision-heavy PDFs (multi-column papers,
  scanned scans): use [`Marker`](https://github.com/VikParuchuri/marker)
  or [`Docling`](https://github.com/DS4SD/docling).
- HTML to Markdown for an LLM prompt where layout matters: use
  the `html2text` feature, or
  [`html-to-markdown`](https://github.com/JohannesKaufmann/html-to-markdown).

deformat trades the last 5–7 F1 points on article pages for speed +
spans + 12-format coverage in a single Rust crate. Pick accordingly.

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
deformat = { version = "0.15.0", features = ["readability", "html2text"] }
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

On the Python side, the same JSON deserializes directly into LangChain
`Document`s without an adapter:

```sh
cargo run --example segments_json --features serde > /tmp/segments.json
python3 scripts/langchain_interop.py /tmp/segments.json
```

Inside `scripts/langchain_interop.py`:

```python
import json
from langchain_core.documents import Document  # or the stdlib stand-in

with open("segments.json") as f:
    segments = json.load(f)

docs = [
    Document(
        page_content=s["text"],
        metadata={
            "category": s["type"],          # Title / NarrativeText / ListItem / Table / Image / ...
            "element_id": s["element_id"],
            **{k: s["metadata"][k]
               for k in ("parent_id", "category_depth", "text_as_html",
                         "page_number", "filename", "filetype",
                         "languages", "coordinates")
               if k in s["metadata"] and s["metadata"][k] is not None},
        },
    )
    for s in segments
]
# Feed `docs` into any LangChain retriever / splitter / VectorStore.
```

The `type` field is the Unstructured element category, so code written
against `langchain-community.UnstructuredLoader` (element mode) ports
over by swapping the loader for this snippet — same
`(page_content, metadata)` shape comes out the other end.

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

## Test fixtures

Small, authored-here fixtures live under `tests/fixtures/synthetic/`
(~10 KB total: DOCX, XLSX, PPTX, EPUB, RTF — each with Unicode and
structure variants). They're generated deterministically by
`scripts/generate_synthetic_fixtures.py` (Python stdlib only) and
committed so `cargo test --all-features` exercises real-format code
paths in CI without a fetch step.

Minimized regression repros for WCXB-surfaced scanner bugs live under
`tests/fixtures/adversarial/`. See `tests/fixtures/PROVENANCE.md` for
the per-file manifest and the commit-vs-fetch decision rationale.

The WCXB benchmark (1,495 pages, CC-BY-4.0) is NOT committed — fetch
it with `scripts/fetch_wcxb.py` to reproduce the F1 numbers below.

## Benchmark (WCXB dev split, 1,495 pages)

`cargo run --release --example bench_wcxb` — word-level F1 against
the `ground_truth.main_content` field from the
[WCXB](https://webcontentextraction.org) benchmark (CC-BY-4.0).
Comparisons across all extractor strategies on a fixed dev split:

| Strategy | ALL F1 | Article F1 | P | R | When |
|---|---:|---:|---:|---:|---|
| `strip_to_text` (baseline) | 0.746 | 0.855 | 0.675 | 0.967 | Default; recall-first |
| + triple filter pipeline | **0.774** | 0.881 | 0.738 | 0.919 | Mixed corpora; best heuristic |
| `extract_html_cascade` | 0.748 | 0.859 | 0.675 | 0.970 | Unknown/wild HTML; falls back to readability when scanner drops content |
| `StripOptions::main_landmark` (anchor election) | 0.748 | **0.867** | 0.723 | 0.915 | Article corpora with `<main>`/`<article>` |
| Trafilatura (Python; published) | ~0.91 | ~0.94 | — | — | Pretraining-scale article extraction |

Per-page-type F1 with the triple filter pipeline:

| page_type | N | strip F1 | triple F1 | Δ |
|---|---:|---:|---:|---:|
| article | 792 | 0.855 | 0.881 | +2.6pp |
| documentation | 91 | 0.911 | 0.904 | −0.7pp |
| service | 165 | 0.748 | 0.788 | +4.0pp |
| forum | 112 | 0.508 | 0.557 | +4.9pp |
| listing | 99 | 0.612 | 0.620 | +0.8pp |
| collection | 117 | 0.522 | 0.551 | +2.9pp |
| product | 119 | 0.450 | 0.500 | +5.0pp |
| **overall** | 1495 | 0.746 | 0.774 | +2.8pp |

Recall is strong across all page types; precision is the gap.
Reproduce with `scripts/fetch_wcxb.py` + the `bench_wcxb` example;
the runner takes `--extractor strip|triple|cascade|anchor|...`.

### Filter composition

Three composable filters sit on top of `strip_to_segments`:

- `html::strip_to_segments_filtered(html, link_ratio_cap)` — drops
  blocks whose output text is mostly inside `<a>` elements
  (Trafilatura-style link density).
- `html::filter_low_sentence_density(segments, min_per_100_words)` —
  drops `NarrativeText` / `UncategorizedText` whose sentence count
  per 100 words falls below the floor. Catches tag-cloud paragraphs
  that link-density misses because they aren't wrapped in anchors.
  Preserves structural kinds (Title, Header, ListItem, Table, …) and
  short blocks (<15 words).
- `html::filter_boilerplate(segments, min_chars)` — drops short
  label-like fragments.

Compose them — link-density (structural) → sentence-density
(content-shape) → boilerplate (char-count). Choose thresholds that
fit your corpus; `examples/bench_wcxb.rs` sweeps caps.

### Anchor election

`StripOptions::main_landmark()` enables Trafilatura-style anchor
election: pre-scan for the first `<main>` or longest `<article>`
landmark and restrict extraction to that subtree. On article-class
pages this lifts F1 by ~1.2pp (0.855 → 0.867) by dropping
related-content sidebars and footer blurbs that don't live inside
`<nav>`/`<footer>` skip tags. On forum/product/listing pages where
content lives outside `<main>` (replies, reviews, specs), election
regresses F1 — this is why it's opt-in. Pages without either
landmark fall through to whole-document scanning.

```rust
use deformat::html::{strip_to_text_with_options, StripOptions};

let text = strip_to_text_with_options(html, &StripOptions::main_landmark());
```

### Cascade

`extract_html_cascade(html)` (feature `readability`) runs
`strip_to_text` first, falls back to `extract_readable` only when
readability finds more than `2×` the scanner's output. This rescues
pages where the scanner's heuristic skip set bites (article body
inside `<aside>`, content inside non-semantic divs that defeat tag
priors). The `2×` ratio is borrowed from Trafilatura's
`compare_extraction` step — the only published cascade form with
measured F1 gains. On WCXB dev the cascade adds +0.2pp ALL F1; the
real win is robustness on wild HTML, not benchmark movement.

The link-density filter preserves structural segments (`Title`,
`Header`, `Table`) regardless of ratio — they reach the segmenter only
after the scanner-level `<nav>`/`<footer>`/`<aside>` skip, so
surviving tables are content (product specs, comparison grids), not
navigation.

### DOCX tables

`docx::extract_to_segments` emits one segment per paragraph; `<w:tbl>`
tables become `Segment::Table` with `metadata.text_as_html` populated
from a normalized `<table><tr><td>…</td></tr></table>` shape. Cell
text is HTML-escaped so `<`, `>`, `&`, `"` round-trip safely through
JSON.

### Source offset tracking

`html::strip_to_text_with_spans` returns the cleaned text plus a
`SpanMap` that maps output byte ranges back to source byte ranges.
`strip_to_text_with_paths` adds an XPath-like path per span
(`article/p[2]`). Each span carries a `SpanKind`: `Direct` when output
bytes equal source bytes, `EntityDecoded` when entities or whitespace
cleanup changed them, `Synthetic` when the text has no literal
counterpart in source (e.g. `<img alt>`).

## Known limitations

Worth calling out so you can pick the right tool for the job:

- **Article-extraction precision ceiling**: baseline `strip_to_text`
  hits F1 ≈ 0.855 on articles; the triple filter pipeline pushes it
  to 0.881; anchor election gets to 0.867. Trafilatura-class Python
  extractors reach F1 ≈ 0.94 via DOM-aware block scoring and
  multi-pass cascades. The literature's heuristic-extractor ceiling
  is around 0.91; closing the rest needs ML (`Web2Text`, `Dripper`,
  …). If pretraining-scale article quality is the goal, use
  Trafilatura.
- **Table structure**: PDF tables are flattened to text (no row/column
  reconstruction from line drawings). DOCX tables via
  `extract_to_segments` come through as `Segment::Table` with
  `metadata.text_as_html` preserving the grid shape; the plain
  `extract_file` / `extract_bytes` path still flattens to text.
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
