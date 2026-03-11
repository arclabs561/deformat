# /arch-review -- Architectural coherence audit of deformat

Audit deformat for structural integrity: module boundaries, type system coherence, feature gate correctness, API surface hygiene, and error handling. This is not a functional test -- it answers "is this codebase well-factored and safe to evolve?"

## Execution strategy

- **Read before judging**: read actual source for every claim. Don't infer from module names.
- **Parallelize independent checks**: sections 2-7 are independent; run them concurrently where possible.
- **Capture evidence**: for every finding, cite `file:line` and quote the relevant code. No vibes.
- **Severity levels**: STRUCTURAL (module boundary / dep violation), COHERENCE (type misuse / invariant gap), SURFACE (public API wart), HYGIENE (dead code / feature gate issue), STYLE (convention drift). Order findings by severity.
- **Previous reports**: read existing reports from `.qa/reports/arch-*.md` before running. Diff against prior findings.

## Report convention

Reports go in `.qa/reports/arch-YYYY-MM-DD.md` (gitignored). Append a `-suffix` for multiple same-day reports.

## Module layout

```
src/lib.rs      (L0: entry point, Extracted struct, extract(), extract_as(),
                 feature-gated extract_readable(), extract_html2text())
src/detect.rs   (L0: Format enum, detect_str(), detect_bytes(), detect_path(), is_html(), is_pdf())
src/error.rs    (L0: Error enum, Display, std::error::Error, From<io::Error>)
src/html.rs     (L0: strip_to_text(), strip_to_text_with_options(), StripOptions,
                 decode_entities(), extract_with_readability() [feature-gated],
                 internal: tag parser, entity decoder, skip tag logic, wiki boilerplate filter)
src/pdf.rs      (L0, feature-gated: extract_file(), extract_bytes())
```

This is a single-crate library. There are no layering levels -- all modules are peers at L0. The architecture questions are about module boundaries, not dependency direction.

## Procedure

### 1. Establish baseline

```bash
cd <repo-root>
cargo metadata --format-version=1 --no-deps | python3 -c "
import json, sys
meta = json.load(sys.stdin)
for p in meta['packages']:
    if p['name'] == 'deformat':
        deps = [d['name'] for d in p['dependencies']]
        feats = list(p['features'].keys())
        print(f\"{p['name']} {p['version']}  deps={deps}  features={sorted(feats)}\")
"
```

Record: commit SHA, version, dependency list, feature list. This is the audit's scope.

### 2. Module boundary audit

#### 2a. Cross-module coupling

Each module should have a clear responsibility. Check for inappropriate coupling:

```bash
rg 'use (crate|super)::' src/lib.rs src/html.rs src/pdf.rs src/detect.rs --type rust
```

Expected:
- `lib.rs` imports from `detect`, `error`, `html`, `pdf` (feature-gated)
- `html.rs` imports from `memchr` only (no crate-internal deps)
- `pdf.rs` imports from `crate::{Error, Extracted, Format}`
- `detect.rs` has no crate-internal imports

Flag: circular dependencies, `html.rs` depending on `detect.rs` or vice versa.

#### 2b. Module size proportionality

```bash
wc -l src/*.rs
```

`html.rs` is ~3000 lines -- the bulk of the crate. Assess whether it should be split:
- Is entity decoding self-contained enough to be `src/entity.rs`?
- Is the Wikipedia boilerplate filter a separable concern?
- Is the tag parser (skip logic, attribute extraction) a separable concern?

Flag only if the monolith is causing practical problems (hard to navigate, test, or maintain).

### 3. Type system coherence

#### 3a. Extracted struct design

`Extracted` is `#[non_exhaustive]` with 6 fields. Check:

- Is `#[non_exhaustive]` appropriate? (Yes -- allows adding fields without breaking consumers.)
- Can `Extracted` be constructed outside this crate? (`#[non_exhaustive]` prevents struct literal syntax.)
- Are all fields `pub`? Should any be private with accessors?
- Does `title` and `excerpt` being `Option<String>` make sense for non-readability extractors?

#### 3b. Format enum completeness

`Format` is `#[non_exhaustive]` with 5 variants. Check:

- Is `Unknown` used as a catch-all? Where is it produced?
- Is there a path where `Unknown` is passed to `extract_as()` and silently treated as plain text?
- Should `Markdown` be detected from content (currently only from path extension)?
- Are all variants reachable from public detection functions?

#### 3c. Error enum design

`Error` is `#[non_exhaustive]` with 3 variants. Check:

- Does `Error` implement `std::error::Error` with proper `source()` delegation?
- Is `UnsupportedFormat(String)` the right type? Should it carry `Format` instead?
- Is `EmptyResult` ever returned from non-PDF paths?
- PDF errors wrapped as `Error::Io` -- semantically correct?

### 4. Feature gate correctness

#### 4a. Feature isolation

Each optional feature should be fully gated. Check all `#[cfg(feature` annotations.

#### 4b. Default feature build

The default build should compile with only `memchr` as a dependency:

```bash
cargo tree --no-default-features --depth 1
cargo tree --all-features --depth 1
```

#### 4c. Feature combinations

```bash
cargo check --features readability 2>&1 | tail -1
cargo check --features html2text 2>&1 | tail -1
cargo check --features pdf 2>&1 | tail -1
cargo check --all-features 2>&1 | tail -1
cargo check --no-default-features 2>&1 | tail -1
```

All five must succeed. Flag any compile failure.

### 5. API surface review

#### 5a. Public API inventory

For each pub item, assess: should it be public? Is it part of the intended API or an internal helper that leaked?

#### 5b. Re-export audit

Currently re-exports: `detect::Format`, `error::Error`. Check:
- Should `html::StripOptions` be re-exported?
- Should `html::strip_to_text` be re-exported at crate root?
- Is the re-export set consistent with "what most users need from `use deformat::*`"?

#### 5c. Must-use annotations

Check: are all public functions that return values without side effects marked `#[must_use]`?

#### 5d. Derive coverage

Check:
- Does `Format` derive `Copy`? (It should -- fieldless enum.)
- Does `Extracted` derive `PartialEq` for testing convenience?
- Does `StripOptions` derive `Default`?

### 6. Error handling audit

#### 6a. Unwrap/expect census

Count occurrences in non-test library code. Each one is a potential panic.

#### 6b. Error message quality

Check each `Error` construction site for actionable messages.

#### 6c. Fallback behavior

`extract_readable()` and `extract_html2text()` fall back to `strip_to_text()` on error. Check:
- Is the fallback documented?
- Is the original error logged or silently swallowed?
- Should the caller be able to opt out of fallback?

### 7. Internal architecture of html.rs

#### 7a. Parser state machine

Is there a clear state machine for tag parsing, or ad-hoc conditionals?

#### 7b. Entity decoder architecture

How is the named entity lookup implemented? How many entities are covered?

#### 7c. Skip tag tracking

How does the parser track nested skip tags? Can malformed close tags cause incorrect depth tracking?

#### 7d. Whitespace normalization

Is whitespace collapsed to single spaces? Are block elements treated as line breaks? Does the model match browser rendering behavior?

#### 7e. Wikipedia-specific logic

Is Wikipedia boilerplate removal gated behind class/id matching? Can it false-positive on non-Wikipedia content?

### 8. Serde and serialization

Check whether `Extracted`, `Format`, or `Error` implement serde traits. Should serde be behind a feature flag?

### 9. Dead code and hygiene

#### 9a. Dead code

Flag any `#[allow(dead_code)]` or `#[allow(unused` suppression.

#### 9b. Clippy suppressions

For each `#[allow(clippy::`, verify the suppression is still necessary.

#### 9c. TODO/FIXME/HACK

Flag any unresolved items. Track count across audits.

#### 9d. Test-only code in library

Verify test modules are properly gated.

### 10. Dependency audit

#### 10a. Dependency tree

```bash
cargo tree --all-features --depth 2
```

Check for duplicate crate versions.

#### 10b. Dependency version currency

Note any significantly outdated dependencies.

### 11. Write the report

Save to `.qa/reports/arch-YYYY-MM-DD.md`. Structure:

1. **Audit scope**: commit SHA, version, deps, features
2. **Module boundaries**: coupling analysis, size proportionality, split candidates
3. **Type system**: Extracted design, Format completeness, Error design
4. **Feature gates**: isolation, compilation matrix, default build
5. **API surface**: pub inventory, re-exports, must-use, derives
6. **Error handling**: unwrap census, message quality, fallback behavior
7. **html.rs internals**: parser architecture, entity decoder, skip tags, whitespace, Wikipedia logic
8. **Serialization**: serde status and recommendations
9. **Hygiene**: dead code, clippy suppressions, TODOs, test isolation
10. **Dependencies**: tree, versions, duplicates
11. **Finding table**: all findings sorted by severity
12. **Comparison**: diff against previous arch reviews

### 12. Standing context

#### Accepted trade-offs

(None yet -- establish baseline on first run.)

#### Metrics to track

| Metric | Baseline | Notes |
|--------|----------|-------|
| `html.rs` line count | ~3000 | Monitor for uncontrolled growth |
| Unwrap/expect in lib code | ? | Establish on first run |
| `#[allow(clippy::...)]` count | ? | Verify each is still necessary |
| Unsafe blocks | 0 expected | Must stay zero |
| TODO/FIXME count | ? | Track resolution |
| Public API item count | ? | Detect accidental API growth |

## What this is NOT

- Not a functional quality audit (that's `/qa`)
- Not a performance benchmark (that's `cargo bench`)
- Not a code style review (that's `cargo fmt` + `cargo clippy`)

This answers: "if someone needed to extend, depend on, or contribute to deformat, would the architecture help or hinder them?"
