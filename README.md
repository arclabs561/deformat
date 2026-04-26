# deformat

[![crates.io](https://img.shields.io/crates/v/deformat.svg)](https://crates.io/crates/deformat)
[![docs.rs](https://docs.rs/deformat/badge.svg)](https://docs.rs/deformat)

Extract plain text from HTML, PDF, DOCX, EPUB, and other document formats.

```rust
let result = deformat::extract("<p>Hello <b>world</b>!</p>").unwrap();
assert_eq!(result.text, "Hello world!");
```

Default build depends only on [`memchr`](https://crates.io/crates/memchr);
per-format extractors enable behind feature flags. Auto-detection covers
HTML / XML / plain text / markdown; binary formats use explicit
`extract_file` / `extract_bytes` entry points.

On the [Web Content Extraction Benchmark](https://webcontentextraction.org)
(WCXB dev split, 1,495 pages across 7 page types), the triple-filter
pipeline reaches **F1 = 0.774 across all page types** and **0.881 on
articles** (see [Evaluation](#evaluation)).

## Install

```sh
cargo add deformat                                        # minimal
cargo add deformat --features readability,html2text,pdf   # all extractors
```

Full API on [docs.rs](https://docs.rs/deformat).

## Supported formats

| Format | Input | Feature flag | Extractor |
|--------|-------|--------------|-----------|
| HTML (tag strip) | `&str` | *(none)* | `html::strip_to_text` |
| HTML (markdown) | `&str` | *(none)* | `html::strip_to_markdown` |
| HTML (layout-aware) | `&str` | `html2text` | `extract_html2text` |
| HTML (article) | `&str` | `readability` | `extract_readable` |
| PDF | `&Path` or `&[u8]` | `pdf` | `pdf::extract_file`, `pdf::extract_bytes` |
| PDF (faster, coords) | `&Path` or `&[u8]` | `pdf_oxide` | `pdf_oxide::extract_file`, `pdf_oxide::extract_bytes`, `pdf_oxide::extract_bytes_to_segments_with_coords` |
| DOCX | `&Path` or `&[u8]` | `docx` | `docx::extract_file`, `docx::extract_bytes` |
| EPUB | `&Path` or `&[u8]` | `epub` | `epub::extract_file`, `epub::extract_bytes` |
| RTF | `&Path` or `&[u8]` | `rtf` | `rtf::extract_file`, `rtf::extract_bytes` |
| XLSX/XLS/ODS | `&Path` or `&[u8]` | `xlsx` | `xlsx::extract_file`, `xlsx::extract_bytes` |
| PPTX | `&Path` or `&[u8]` | `pptx` | `pptx::extract_file`, `pptx::extract_bytes` |
| XML | `&str` | *(none)* | `html::strip_to_text` |
| Plain text / Markdown | `&str` | *(none)* | passthrough |

## Evaluation

Quality is measured against the
[Web Content Extraction Benchmark](https://webcontentextraction.org)
(WCXB, CC-BY-4.0): 2,008 manually annotated pages across seven page
types (article, documentation, service, listing, collection, forum,
product), 1,495 in the dev split. Larger and more category-balanced
than the older 2017 ScrapingHub benchmark, which is article-only.

The metric is word-level F1 against `ground_truth.main_content`:
tokenize on whitespace, lowercase, strip non-alphanumeric edges,
compute multiset overlap, then `F1 = 2·P·R / (P+R)`. Same shape as
ScrapingHub's metric and as the published comparative-extraction
literature, so numbers are cross-comparable.

Reporting per page type is the point — a single ALL number hides
that one category carries the result. The `bench_wcxb` runner emits
the per-type breakdown alongside the aggregate.

```sh
scripts/fetch_wcxb.py                                            # pulls dev split from HuggingFace
cargo run --release --example bench_wcxb -- --extractor strip    # baseline
cargo run --release --example bench_wcxb -- --extractor triple   # link + sentence + boilerplate filter pipeline
cargo run --release --features readability --example bench_wcxb -- --extractor cascade  # strip + readability fallback
cargo run --release --example bench_wcxb -- --extractor anchor   # <main>/<article> election
```

WCXB fixtures aren't committed (~200 MB); the fetch script downloads
them on first use. To swap in your own corpus, point the runner at
a directory with the same `{html,ground-truth}/<id>.{html,json}`
shape — the F1 scorer in `examples/bench_wcxb.rs` is ~50 lines and
backend-agnostic.

A separate committed regression corpus
(`tests/fixtures/regression/`, ~7 KB) pins per-fixture F1 floors so
filter or scanner changes that drop F1 fail CI before the next
release. Adversarial scanner repros live in
`tests/fixtures/adversarial/`. See `tests/fixtures/PROVENANCE.md`.

### Results (WCXB dev split, 1,495 pages)

| Strategy | ALL F1 | Article F1 | When |
|---|---:|---:|---|
| `strip_to_text` (baseline) | 0.746 | 0.855 | Default; recall-first |
| triple filter pipeline | **0.774** | 0.881 | Mixed corpora; best heuristic |
| `extract_html_cascade` | 0.748 | 0.859 | Wild HTML; rescues content the scanner drops |
| `StripOptions::main_landmark` | 0.748 | **0.867** | Article corpora with `<main>`/`<article>` |

Per-type triple-pipeline F1 deltas from baseline: article +2.6pp,
service +4.0pp, forum +4.9pp, product +5.0pp, listing +0.8pp,
collection +2.9pp, documentation −0.7pp.

## Known limitations

- **Article-extraction F1 ceiling.** WCXB article F1 tops out at
  0.881 (triple pipeline) / 0.867 (anchor election); the published
  heuristic-extractor ceiling is around 0.91. The 3pp gap is open
  work. The path forward is DOM-aware block scoring (paragraph
  position, heading proximity, per-block text-to-tag ratio) — what
  Trafilatura uses to reach ≈ 0.94 article F1. Adopting that here
  in Rust without inheriting a DOM parse is the design problem.
- **Vision-heavy PDFs**: multi-column papers, scans, and figure-rich
  documents need layout analysis. Out of scope for this crate;
  pair with [Marker](https://github.com/VikParuchuri/marker) or
  [Docling](https://github.com/DS4SD/docling) for those.
- **No OCR.** Scanned PDFs yield empty text. Compose with
  `tesseract-sys`.
- **Default charset is UTF-8.** Non-UTF-8 HTML needs the
  `encoding_rs` feature: call `detect::decode_bytes(bytes, "utf-8")`
  before `strip_to_text`.
- **Tables**: PDF tables flatten to text. DOCX tables via
  `extract_to_segments` come through as `Segment::Table` with
  `metadata.text_as_html` preserving the grid; the plain
  `extract_file` path still flattens.
- **Span positions are post-cleanup.** Spans whose source bytes span
  collapsed whitespace are tagged `SpanKind::EntityDecoded` rather
  than `Direct`; byte-level interpolation is exact only for
  `Direct`.

## License

MIT OR Apache-2.0
