# Contributing to deformat

Thanks for your interest. deformat extracts plain text from HTML, PDF, DOCX, EPUB, and other document formats. The default build depends only on `memchr`; per-format extractors enable behind feature flags.

## Before you start

For non-trivial work (new format, scanner algorithm changes, API additions), open an issue first to align on scope. Drive-by bug fixes and doc patches don't need an issue.

## Setup

- Rust toolchain: stable, MSRV `1.80`. Use `rustup` to manage.
- Optional: `cargo-nextest` for faster test runs (`cargo install cargo-nextest`).

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Style

- Direct, lowercase prose in commits. No marketing words ("powerful", "robust", "elegant"). No em-dashes in prose.
- Commit messages: `deformat: short lowercase description`. One commit per logical change.
- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` must pass before `git add`.

## Testing

- `cargo test --all-features` runs the full matrix including per-format extractors.
- WCXB regression: `tests/fixtures/regression/` pins per-fixture F1 floors so filter or scanner changes that drop F1 fail CI before the next benchmark run.
- For new HTML scanner behavior, add a fixture under `tests/fixtures/regression/` (gold output + minimum F1).
- Per-format extractor tests live in `src/<format>.rs`. Round-trip and edge-case coverage for malformed input is expected.

## Evaluation

The full WCXB benchmark runs via `cargo run --release --example bench_wcxb -- --split dev`. Numbers in the README correspond to 1,495 scored pages from the 1,497-page dev corpus; IDs 4802 and 4893 lack `main_content`. Don't ship a metric change without re-running the bench and updating the README.

## Pull requests

- Keep PRs scoped to one concern.
- Show before/after for behavior changes (especially F1 deltas on WCXB).
- Link the related issue.
- CI must be green before requesting review.

## License

Dual-licensed under MIT or Apache-2.0 at your option. By contributing you agree your contributions are licensed under both.
