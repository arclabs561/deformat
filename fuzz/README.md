# deformat fuzzing

[cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) targets
for deformat's panic-guarded surfaces. Run any target with:

```sh
cargo +nightly fuzz run <target>           # indefinite
cargo +nightly fuzz run <target> -- -runs=10000   # bounded
cargo +nightly fuzz run <target> -- -max_total_time=60   # 60s smoke
```

## Targets

| Target | What it guards |
|---|---|
| `strip_to_text` | Core scanner; no-panic on any input |
| `strip_to_text_with_paths` | Span invariants: UTF-8 char boundaries, non-overlap, in-bounds |
| `strip_to_segments` | Full segment pipeline + every filter |
| `filter_pipeline` | Filter threshold parameter edges (NaN, ±∞, 0.0, 1.0) |
| `detect_page_type` | Heuristic classifier char-boundary safety |
| `detect_bytes` | Magic-byte + ZIP central-directory classifier |
| `docx_extract` | DOCX ZIP+XML parser (via `zip` crate) |
| `epub_extract` | EPUB ZIP+OPF+XHTML |
| `xlsx_extract` | XLSX via `calamine` |

## First-run setup

```sh
cargo install cargo-fuzz          # one-time
cd fuzz
cargo +nightly fuzz list           # verify targets compile
cargo +nightly fuzz run strip_to_text -- -max_total_time=60
```

## Corpus seeds

Seed the corpus from committed fixtures so the fuzzer starts with
known-valid inputs and mutates outward:

```sh
cp ../tests/fixtures/synthetic/*.docx fuzz/corpus/docx_extract/
cp ../tests/fixtures/synthetic/*.epub fuzz/corpus/epub_extract/
cp ../tests/fixtures/synthetic/*.xlsx fuzz/corpus/xlsx_extract/
cp ../tests/fixtures/adversarial/*.html fuzz/corpus/strip_to_text/
cp ../tests/fixtures/adversarial/*.html fuzz/corpus/strip_to_segments/
cp ../tests/fixtures/adversarial/*.html fuzz/corpus/detect_page_type/
```

(`fuzz/corpus/<target>/` is cargo-fuzz's conventional corpus dir; it
auto-creates on first run.)

## CI

Fuzz targets are NOT run in CI by default -- fuzz runs are long and
require nightly Rust. When adding a regression repro, save the
minimized input to `fuzz/corpus/<target>/` and add a deterministic
unit test in `tests/adversarial.rs` so CI catches it without needing
the fuzzer. `cargo +nightly fuzz tmin` produces the minimized input.
