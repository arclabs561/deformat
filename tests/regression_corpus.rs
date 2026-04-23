//! Regression corpus: per-fixture word-level F1 floors.
//!
//! Pinned article-class fixtures with hand-authored expected text. The
//! test asserts that each extractor strategy clears its per-fixture F1
//! floor; future filter or scanner changes that drop F1 fail CI before
//! they reach a release.
//!
//! Why this exists: WCXB benchmark fixtures aren't committed (1495
//! pages, fetched separately). Without an in-repo corpus, the only F1
//! signal CI could observe was test counts -- a filter change that
//! quietly regresses article F1 by 2pp would land green. The committed
//! corpus is small (4 fixtures, ~7KB) so it can ride in every test
//! run, including under `--all-features` matrix jobs.
//!
//! Floors are calibrated below the *measured* F1 with a small headroom
//! so legitimate scanner improvements don't have to update floors on
//! every commit. When a floor needs to move (intentional redesign,
//! filter retuning), update both the floor and the rationale comment.

use std::collections::HashMap;
use std::path::PathBuf;

/// Word-level F1: tokenize on whitespace, lowercase, compute multiset
/// overlap. Matches the metric used by `examples/bench_wcxb.rs`.
fn word_f1(extracted: &str, expected: &str) -> f32 {
    let ex_words = tokenize(extracted);
    let gt_words = tokenize(expected);
    if ex_words.is_empty() && gt_words.is_empty() {
        return 1.0;
    }
    if ex_words.is_empty() || gt_words.is_empty() {
        return 0.0;
    }

    let mut ex_counts: HashMap<&str, usize> = HashMap::new();
    for w in &ex_words {
        *ex_counts.entry(w.as_str()).or_insert(0) += 1;
    }
    let mut gt_counts: HashMap<&str, usize> = HashMap::new();
    for w in &gt_words {
        *gt_counts.entry(w.as_str()).or_insert(0) += 1;
    }

    let mut overlap: usize = 0;
    for (w, gt_n) in &gt_counts {
        if let Some(ex_n) = ex_counts.get(w) {
            overlap += (*ex_n).min(*gt_n);
        }
    }

    let precision = overlap as f32 / ex_words.len() as f32;
    let recall = overlap as f32 / gt_words.len() as f32;
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn fixture_pair(name: &str) -> (String, String) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/regression");
    let html = std::fs::read_to_string(dir.join(format!("{name}.html")))
        .unwrap_or_else(|e| panic!("read {name}.html: {e}"));
    let expected = std::fs::read_to_string(dir.join(format!("{name}.expected.txt")))
        .unwrap_or_else(|e| panic!("read {name}.expected.txt: {e}"));
    (html, expected)
}

/// Pinned fixtures with their per-extractor F1 floors. The floors are
/// set ~2-5pp below the measured F1 to absorb whitespace/cleanup
/// variance without papering over real regressions.
struct FixtureCase {
    name: &'static str,
    /// Floor for `strip_to_text`. The default extractor every user gets.
    strip_floor: f32,
    /// Floor for the triple-filter pipeline (link + sentence + boilerplate).
    triple_floor: f32,
    /// Floor for anchor election.
    anchor_floor: f32,
}

const CASES: &[FixtureCase] = &[
    FixtureCase {
        name: "clean_news_article",
        // Measured: strip=1.000, triple=0.981, anchor=1.000.
        // Floors set ~3pp below measured to absorb byline-trimming
        // whitespace variance from cleanup_whitespace().
        strip_floor: 0.97,
        // Triple drops the short "By Jane Doe, March 14, 2026" byline
        // (filter_boilerplate floor 40 chars). Floor at 0.96 catches
        // additional unintentional drops.
        triple_floor: 0.95,
        anchor_floor: 0.97,
    },
    FixtureCase {
        name: "blog_with_sidebar",
        // Measured: strip=1.000, triple=1.000, anchor=1.000.
        // Sidebar is in <aside> (skip tag), so strip already drops it.
        strip_floor: 0.97,
        triple_floor: 0.97,
        anchor_floor: 0.97,
    },
    FixtureCase {
        name: "documentation_page",
        // Measured: strip=1.000, triple=1.000, anchor=1.000.
        // Has nav, code blocks, lists. Strip keeps content outside <nav>;
        // anchor restricts to <main>.
        strip_floor: 0.97,
        triple_floor: 0.95,
        anchor_floor: 0.97,
    },
];

#[test]
fn regression_corpus_strip_floors() {
    let mut failures = Vec::new();
    for case in CASES {
        let (html, expected) = fixture_pair(case.name);
        let extracted = deformat::html::strip_to_text(&html);
        let f1 = word_f1(&extracted, &expected);
        if f1 < case.strip_floor {
            failures.push(format!(
                "{}: strip F1 {f1:.3} < floor {:.3}\n  extracted: {:?}",
                case.name,
                case.strip_floor,
                &extracted[..extracted.len().min(200)]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "regression: strip dropped below per-fixture floors:\n{}",
        failures.join("\n")
    );
}

#[test]
fn regression_corpus_triple_floors() {
    let mut failures = Vec::new();
    for case in CASES {
        let (html, expected) = fixture_pair(case.name);
        let segs = deformat::html::strip_to_segments_filtered(&html, 0.45);
        let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
        let segs = deformat::html::filter_boilerplate(segs, 40);
        let extracted = segs
            .iter()
            .map(|s| s.data().text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        let f1 = word_f1(&extracted, &expected);
        if f1 < case.triple_floor {
            failures.push(format!(
                "{}: triple F1 {f1:.3} < floor {:.3}",
                case.name, case.triple_floor
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "regression: triple pipeline dropped below per-fixture floors:\n{}",
        failures.join("\n")
    );
}

#[test]
fn regression_corpus_anchor_floors() {
    let mut failures = Vec::new();
    for case in CASES {
        let (html, expected) = fixture_pair(case.name);
        let extracted = deformat::html::strip_to_text_with_options(
            &html,
            &deformat::html::StripOptions::main_landmark(),
        );
        let f1 = word_f1(&extracted, &expected);
        if f1 < case.anchor_floor {
            failures.push(format!(
                "{}: anchor F1 {f1:.3} < floor {:.3}",
                case.name, case.anchor_floor
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "regression: anchor election dropped below per-fixture floors:\n{}",
        failures.join("\n")
    );
}

#[cfg(feature = "readability")]
#[test]
fn regression_corpus_cascade_never_regresses_strip() {
    // The cascade must never produce LOWER F1 than strip alone for any
    // fixture. If it does, the cascade's ratio condition is buggy.
    for case in CASES {
        let (html, expected) = fixture_pair(case.name);
        let strip = deformat::html::strip_to_text(&html);
        let cascade = deformat::extract_html_cascade(&html);
        let strip_f1 = word_f1(&strip, &expected);
        let cascade_f1 = word_f1(&cascade.text, &expected);
        // Allow a 1pp slack for ratio-condition false positives where
        // readability's slightly-different output is preferred.
        assert!(
            cascade_f1 >= strip_f1 - 0.01,
            "{}: cascade F1 {cascade_f1:.3} regressed below strip F1 {strip_f1:.3} \
             (cascade chose {:?})",
            case.name,
            cascade.extractor
        );
    }
}

#[cfg(feature = "readability")]
#[test]
fn regression_corpus_cascade_rescues_in_aside_fixture() {
    // Pair test with the article_in_aside_misnesting adversarial fixture:
    // strip gets 0 chars, cascade rescues to ~2200 chars via readability.
    // This is the single most important falsifying assertion for the
    // cascade -- if it drops, the cascade is silently no-op'ing.
    let html = include_str!("fixtures/adversarial/article_in_aside_misnesting.html");
    let strip_chars = deformat::html::strip_to_text(html).chars().count();
    let cascade_chars = deformat::extract_html_cascade(html).text.chars().count();
    assert!(
        strip_chars < 100,
        "scanner expected to drop content: {strip_chars}"
    );
    assert!(
        cascade_chars > 1000,
        "cascade expected to rescue content via readability: {cascade_chars}"
    );
}
