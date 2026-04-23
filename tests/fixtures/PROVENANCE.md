# Fixture provenance

Every file in this directory has one of three origins. Files from
third parties are only committed if the third party's license permits
redistribution and the file is small enough to justify the repo cost.

## Licensing summary

- **Authored-by-this-crate** fixtures (all `synthetic/` and `adversarial/`):
  dual-licensed under `MIT OR Apache-2.0`, identical to the rest of the
  crate. No separate LICENSE file is needed — the repo-root `LICENSE-MIT`
  and `LICENSE-APACHE` cover these artifacts.
- **Public-domain third-party samples**: committed with per-file
  attribution in this document.
- **Other third-party samples** (copyrighted, restricted): NOT committed.
  Fetched at run-time by `scripts/fetch_fixtures.sh` (Project Gutenberg
  EPUBs) or `scripts/fetch_wcxb.py` (WCXB benchmark, CC-BY-4.0). Tests
  that depend on them are marked `#[ignore]` so CI passes without them.

## Manifest

### `synthetic/` — authored for this crate

Generated deterministically by `scripts/generate_synthetic_fixtures.py`.
Rerun that script to reproduce byte-identical outputs (Python stdlib
only; no external deps). Dual-licensed MIT-OR-Apache-2.0.

| File | Bytes | Purpose |
|---|---|---|
| `minimal.docx` | ~1 KB | Minimal valid OOXML word document, ASCII prose |
| `unicode.docx` | ~1 KB | DOCX with CJK + accented + Cyrillic runs |
| `table.docx` | ~1 KB | DOCX containing a `<w:tbl>` for segment-level table extraction |
| `minimal.xlsx` | ~2 KB | Single-sheet XLSX with header row + two data rows |
| `unicode.xlsx` | ~2 KB | Multi-sheet XLSX with CJK and Cyrillic shared strings |
| `minimal.pptx` | ~2 KB | Single-slide PPTX with title + body runs |
| `minimal.epub` | ~1 KB | Two-chapter EPUB with OPF + XHTML content files |
| `minimal.rtf` | 62 B | RTF with ASCII content |
| `unicode.rtf` | 99 B | RTF with `\uN?` unicode escapes and Windows-1252 ANSI codepage |

Total: ~10 KB. All exercised by `tests/real_formats.rs`.

### `adversarial/` — minimized regression repros

Hand-authored minimal HTML snippets that reproduce scanner bugs found
via the WCXB benchmark. Each file's comment header points at the real
pattern and the regression guard it backs. Dual-licensed
MIT-OR-Apache-2.0.

| File | Repro of |
|---|---|
| `unclosed_nav_drawer.html` | 5 `<nav>` opens with 2 `</nav>` closes (go.dev/blog pattern). Guards the `<main>` landmark skip_depth reset. |
| `unclosed_attr_quote.html` | `<img … wp-image-3737" />` with a stray unpaired quote (goloadup.com pattern). Guards the 256-byte attribute-quote recovery. |
| `cms_toc_section_ids.html` | Drupal `<section id="toc_12345">` sections (dos.ny.gov pattern). Guards the hyphen-or-exact ID match in `is_wiki_skip_tag`. |
| `nested_void_elements.html` | Inline `<img>` / `<br>` / `<hr>` inside a paragraph. Guards the void-element path-stack exclusion. |

## Not committed (fetched on demand)

| Source | Fetched by | License | Why not committed |
|---|---|---|---|
| Project Gutenberg EPUBs (Alice, Metamorphosis) | `scripts/fetch_fixtures.sh` | Public domain | Larger than needed — 200-500 KB each. Minimal.epub covers the core code paths. Gutenberg redistribution is legally fine but size is the deciding factor. |
| WCXB benchmark (1,497 dev + held-out test pages) | `scripts/fetch_wcxb.py` | CC-BY-4.0 (Murrough Foley) | ~200 MB total. Attribution required on redistribution; script handles it by fetching from HuggingFace on demand. The WCXB F1 numbers in the README are reproducible by running the script. |

## Design rationale

The cut between "commit" and "fetch" is driven by three constraints:

1. **CI must exercise real-format code paths without a fetch step.**
   Everything in `synthetic/` and `adversarial/` is loaded by
   non-`#[ignore]` tests, so CI catches DOCX/XLSX/EPUB/PPTX/RTF
   regressions the moment they land. Previously these tests were
   `#[ignore]`-gated behind the fetch script, so CI never ran them.
2. **Repo size must stay small.** Total `tests/fixtures/` is under 20 KB.
   Anything larger goes behind a fetch script.
3. **License clarity.** Only files we author (authoratively MIT-OR-Apache)
   or confirmed public-domain get committed. Third-party samples with
   attribution requirements or copyright restrictions stay in
   `target/bench-fixtures/` (gitignored).

When adding a new fixture, ask:
- Is it authored here? → `tests/fixtures/` + generator script if applicable.
- Is it third-party, small, and public-domain? → `tests/fixtures/` with a row in this file.
- Is it third-party and copyrighted / CC-BY / large? → `scripts/` fetcher + `#[ignore]` test.
