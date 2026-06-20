# deformat examples

## Where to start

| I want to... | Run |
|---|---|
| Strip HTML from stdin to plain text | `strip` |
| See the boilerplate filter pipeline step by step | `filter_pipeline` |
| Inspect typed HTML segments | `segments` |
| Emit segment JSON for downstream tools | `segments_json` |
| Extract HTML metadata | `extract_metadata` |
| Decode non-UTF-8 HTML before extraction | `charset` |
| Run a readability-style article extractor | `clean_html` |
| Score extractors on WCXB fixtures | `bench_wcxb` |

## No-feature examples

```sh
cargo run --example filter_pipeline
cargo run --example segments
cargo run --example segments_json
cargo run --example extract_metadata
printf '<article><h1>Hello</h1><p>world</p></article>' | cargo run --example strip
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

`bench_wcxb` expects the WCXB dev fixtures under `target/bench-fixtures/wcxb/dev`.
Fetch them before running the benchmark:

```sh
scripts/fetch_wcxb.py
cargo run --release --example bench_wcxb -- --extractor strip
cargo run --release --example bench_wcxb -- --extractor triple
cargo run --release --features readability --example bench_wcxb -- --extractor readable
cargo run --release --features readability --example bench_wcxb -- --extractor cascade
```

The output reports word-level precision, recall, and F1 overall and by WCXB page type.
