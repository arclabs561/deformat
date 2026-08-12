# deformat examples

## Where to start

| I want to... | Run |
|---|---|
| Strip HTML from stdin to plain text | `strip` |
| See the boilerplate filter pipeline step by step | `filter_pipeline` |
| Inspect typed HTML segments | `segments` |
| Convert extracted segments into retrieval spans | `segments_to_slabs` |
| Emit segment JSON for downstream tools | `segments_json` |
| Extract HTML metadata | `extract_metadata` |
| Decode non-UTF-8 HTML before extraction | `charset` |
| Run a readability-style article extractor | `clean_html` |
| Score extractors on WCXB fixtures | `bench_wcxb` |

## No-feature examples

```sh
cargo run --example filter_pipeline
cargo run --example segments
cargo run --example segments_to_slabs
cargo run --example segments_json
cargo run --example extract_metadata
printf '<article><h1>Hello</h1><p>world</p></article>' | cargo run --example strip
```

`segments_to_slabs` shows the extraction-to-retrieval boundary:

```text
=== slabs ===
#00 Title            bytes=0..15 chars=Some(0..15) text="Span Boundaries"
#01 NarrativeText    bytes=17..79 chars=Some(17..79) text="deformat extracts typed document segments from source formats."
#02 NarrativeText    bytes=81..148 chars=Some(81..148) text="slabs records those selected spans with byte and character offsets."
```

## Feature-gated examples

```sh
cargo run --features serde --example segments
cargo run --features serde --example segments_json
cargo run --features whichlang --example extract_metadata
cargo run --features encoding_rs --example charset
cargo run --features readability --example clean_html
```

## Benchmark runner

`bench_wcxb` requires an explicit `dev` or `test` split:

```sh
scripts/fetch_wcxb.py --split dev
cargo run --release --example bench_wcxb -- --split dev --extractor strip
cargo run --release --example bench_wcxb -- --split dev --extractor triple
cargo run --release --features readability --example bench_wcxb -- --split dev --extractor readable
cargo run --release --features readability --example bench_wcxb -- --split dev --extractor cascade
```

The output reports word-level precision, recall, and F1 overall and by WCXB page type.
