# /qa -- Quality audit of deformat

Run a comprehensive quality pass: build, lint, test, property tests, doc coverage, benchmarks, domain-specific checks, and pre-publish hygiene. Produces a timestamped report in `.qa/reports/`.

## Execution strategy

- **Stop early on build failure**: if `cargo check` fails, the QA is blocked. Report and stop.
- **Capture exact output**: save command output to temp files (`> /tmp/deformat-qa-*.txt 2>&1`) so findings are reliable. Don't eyeball scrollback.
- **Read previous reports first**: comparison with prior runs catches regressions.
- **Full output**: read all diagnostic output. Do not truncate or pipe through head/tail.
- **Parallelize independent checks**: format, clippy, and doc-comment audit can run in parallel once the build check passes. Tests and proptests can run in parallel with lint checks.

## Report convention

Write to `.claude/reports/qa-YYYY-MM-DD.md` (globally gitignored via `~/.gitignore_global`). For same-day reruns, append `-v2`, `-v3`.

## Severity hierarchy

Classify each finding:

| Severity | Meaning |
|----------|---------|
| STRUCTURAL | Build failure, feature gate breakage, dependency direction, publish blocker |
| COHERENCE | Logic error, invariant violation, proptest failure, wrong behavior |
| SURFACE | Public API wart, missing doc comment, clippy warning |
| HYGIENE | Formatting, dead code, stale dependency, minor style issue |

Order findings by severity in the report.

## Procedure

### 0. Read prior QA reports

Check for prior reports in order: `.claude/reports/`, `qa/reports/`, `.qa/reports/`, `.claude/` root (flat files like `audit-report.md`). Read the most recent found. If reports exist in old locations, move them to `.claude/reports/` with dated names before proceeding.

```bash
eza --sort=modified -r .claude/reports/qa-*.md .qa/reports/qa-*.md 2>/dev/null | head -3
```

Read the most recent 1-2 reports if they exist. Note open issues to watch for.

### 1. Build check (compilation gate)

```bash
cargo check --all-targets > /tmp/deformat-qa-check.txt 2>&1
```

If this fails, stop and report. Everything else depends on compilation.

### 2. Format check

```bash
cargo fmt --check > /tmp/deformat-qa-fmt.txt 2>&1
```

If formatting is off, note which files. Don't auto-fix during QA -- just report.

### 3. Clippy (default features)

```bash
cargo clippy --all-targets -- -D warnings > /tmp/deformat-qa-clippy.txt 2>&1
```

### 4. Clippy (all features)

```bash
cargo clippy --all-features --all-targets -- -D warnings > /tmp/deformat-qa-clippy-all.txt 2>&1
```

Capture full output for both. Classify findings: correctness issues vs style nits.

### 5. Tests (default features)

```bash
cargo test --all-targets > /tmp/deformat-qa-test.txt 2>&1
```

Record: total tests, pass/fail count.

### 6. Doc tests

```bash
cargo test --doc > /tmp/deformat-qa-doctest.txt 2>&1
```

Doc tests verify examples in doc comments actually compile and run.

### 7. Tests (all features)

```bash
cargo test --all-features --all-targets > /tmp/deformat-qa-test-all.txt 2>&1
```

This exercises feature-gated code paths: readability, html2text, pdf. Record pass/fail count and compare against default-feature test count.

### 8. Property tests

Property tests are critical for deformat (HTML parsing invariants, entity decoding, whitespace normalization). Run with extra cases:

```bash
PROPTEST_CASES=500 cargo test --test proptest > /tmp/deformat-qa-proptest.txt 2>&1
```

If any property test fails, capture the seed and minimal failing case. The proptest regression file is at `tests/proptest.proptest-regressions`.

Current invariants tested (verify all still pass):
- Output never contains raw HTML tags
- Output never has double spaces
- Output is always trimmed
- No C0 control characters in output
- Script/style content never leaks
- Entity decoding never panics
- strip_to_text never panics on arbitrary input
- Plain text content preserved through tags
- extract() format detection consistent with detect()
- Output length never exceeds input length
- Skip tag content never leaks (nav, header, footer, aside, etc.)
- Nested skip tags don't leak inner content
- No invisible Unicode characters in output
- decode_entities preserves plain text
- decode_entities is idempotent
- Wiki ref markers stripped with option, preserved by default
- Plain text passthrough (modulo whitespace normalization)
- strip_to_text is idempotent
- Output is valid UTF-8
- No angle bracket means not HTML

### 9. Doc coverage audit

Check that all public items have doc comments:

```bash
# Public items in each module
rg '^\s*pub (fn|struct|enum|trait|type|const|mod) ' src/ --type rust -n > /tmp/deformat-qa-pub-items.txt
rg '^\s*///\s' src/ --type rust -c > /tmp/deformat-qa-doc-comments.txt
```

For each public item in `src/lib.rs`, `src/html.rs`, `src/detect.rs`, `src/error.rs`, `src/pdf.rs`, verify a `///` doc comment precedes it. Flag pub items without doc comments.

Check `#[must_use]` on public functions that return values without side effects (detection functions, `Format::mime_type()`).

### 10. Benchmark compilation

```bash
cargo bench --no-run > /tmp/deformat-qa-bench.txt 2>&1
```

Benchmarks must compile even if we don't run them during QA. If compilation fails, that's a STRUCTURAL finding.

### 11. Domain-specific checks

#### 11a. HTML entity coverage

Verify `decode_entities()` handles all major entity classes. Read `src/html.rs` and check:

- Named entities: `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&nbsp;`, `&mdash;`, `&ndash;`, `&hellip;`, `&eacute;`, `&copy;`, `&reg;`, `&euro;`, `&ldquo;`, `&rdquo;`, `&lsquo;`, `&rsquo;`, `&trade;`
- Numeric entities: `&#123;`, `&#0;` (null replacement), `&#65533;`
- Hex entities: `&#x1F4A9;`, `&#x3C;`, `&#x00;`
- Semicolon-optional named entities (HTML5 legacy)
- Edge cases: invalid names (`&notareal;`), overlong numeric (`&#99999999;`), surrogates (`&#xD800;`)

Flag any entity class that is silently passed through without decoding when it should decode.

#### 11b. extract_attr_value edge cases

The internal `extract_attr_value()` function parses HTML attributes. Verify the test suite covers:

- Double-quoted values: `class="foo"`
- Single-quoted values: `class='foo'`
- Unquoted values: `class=foo`
- No match: attribute not present
- Substring rejection: `data-class` should not match `class`
- Case insensitivity: `CLASS="foo"`
- Multiple attributes: correct one selected
- Empty value: `class=""`
- Value with spaces: `class="foo bar"`

Flag missing edge case coverage.

#### 11c. Format detection accuracy

Check the test suite covers:
- HTML: doctype, html tag, head+body, XML PI, paired tags, fragments
- Adversarial: angle brackets in math (`x < 10`), template placeholders (`<your name>`)
- PDF: magic bytes (`%PDF`), non-PDF binary
- Extensions: `.html`, `.htm`, `.xhtml`, `.pdf`, `.md`, `.txt`, case-insensitive
- Empty input, whitespace-only input, BOM prefix
- Long input (>1024 chars) where HTML markers are past the detection window

#### 11d. Wikipedia boilerplate removal

Verify that Wikipedia-specific filtering (TOC, references, navboxes, categories) is gated behind appropriate class/id matching and doesn't over-aggressively strip non-Wikipedia content.

#### 11e. Skip tag completeness

Expected skip tags: `script`, `style`, `noscript`, `template`, `svg`, `nav`, `header`, `footer`, `aside`, `head`, `menu`, `form`, `select`, `figcaption`, `textarea`, `iframe`.

### 12. Pre-publish gate

#### 12a. Version coherence

Cargo.toml version and README dependency block must match. Flag any mismatch as STRUCTURAL.

#### 12b. License files exist

Cargo.toml declares `MIT OR Apache-2.0`. Both `LICENSE-MIT` and `LICENSE-APACHE` must exist.

#### 12c. No path dependencies

Path dependencies block `cargo publish`. Must be zero hits in `[dependencies]`.

#### 12d. MSRV check

Declared MSRV is 1.80.0. If CI enforces this, note it. If not, flag the gap.

#### 12e. Crate metadata completeness

Verify all required fields are present in `Cargo.toml`: name, version, edition, license, description, repository, readme, keywords, categories.

#### 12f. Include/exclude sanity

Verify `include` in Cargo.toml covers all necessary files and excludes test fixtures, CI configs, etc.

### 13. Unsafe audit

deformat should have zero unsafe code. Flag any occurrence.

### 14. Unwrap/expect census

Library code should minimize panics. Track count across audits.

### 15. Write the report

Save to `.claude/reports/qa-YYYY-MM-DD.md`. Structure:

1. **Test conditions**: date, commit SHA, rustc version, deformat version, feature matrix
2. **Check results table**: pass/fail for each check
3. **Property test results**: all invariants listed with pass/fail status
4. **Doc coverage**: per-module pub item count vs documented count
5. **Domain findings**: entity coverage gaps, attr parsing edge cases, detection accuracy, wiki boilerplate, skip tags
6. **Pre-publish gate**: version coherence, licenses, no path deps, MSRV, metadata
7. **Bug table**: concrete issues found with file:line references, sorted by severity
8. **Comparison with prior run**: regressions, improvements, unchanged issues
9. **Actionable items**: specific things worth fixing, ordered by impact

### 16. Compare against previous runs

If prior reports exist, explicitly diff:
- Did any previously-passing check now fail? (regression)
- Are previously-reported bugs still present?
- Any new issues not seen before?
- Has the proptest invariant count grown or shrunk?
- Has the unwrap/expect count changed?

## What this is NOT

- Not a performance benchmark (that would run `cargo bench` and analyze results)
- Not an architecture review (that's `/arch-review`)
- Not an auto-fixer (report only, don't modify code)

This answers: "is deformat healthy, correct, and ready to publish?"
