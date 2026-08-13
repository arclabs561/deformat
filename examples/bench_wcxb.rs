//! Score deformat against the WCXB benchmark.
//!
//! WCXB is CC-BY-4.0 by Murrough Foley. Download the fixtures with
//! `scripts/fetch_wcxb.py` (uses the HuggingFace Hub client) before
//! running this example.
//!
//! Usage:
//!
//! ```sh
//! scripts/fetch_wcxb.py --split dev
//! cargo run --release --example bench_wcxb -- --split dev
//! cargo run --release --features readability --example bench_wcxb -- --split dev --extractor readable
//! ```
//!
//! Metric: word-level F1 against the `ground_truth.main_content` field
//! in each per-page JSON (matches WCXB's reference `evaluate.py`).
//! Reports overall F1/P/R and per-page-type breakdown.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[path = "../tests/support/wcxb_score.rs"]
mod wcxb_score;

#[derive(Debug)]
struct Args {
    split: String,
    extractor: String,
}

#[derive(Debug)]
struct GroundTruth {
    main_content: String,
    page_type: String,
    with_req: Vec<String>,
    without_req: Vec<String>,
}

#[derive(Default, Clone)]
struct Stats {
    pages: usize,
    f1_sum: f64,
    p_sum: f64,
    r_sum: f64,
    with_rate_sum: f64,
    without_rate_sum: f64,
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let Args { split, extractor } = parse_args(args)?;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let split_dir = root.join("target/bench-fixtures/wcxb").join(&split);
    let gt_dir = split_dir.join("ground-truth");
    let html_dir = split_dir.join("html");

    if !gt_dir.is_dir() || !html_dir.is_dir() {
        return Err(format!(
            "WCXB {split} fixtures not found at {}\nRun: scripts/fetch_wcxb.py --split {split}",
            split_dir.display()
        ));
    }

    let gt_entries = fixture_entries(&gt_dir, "json")?;
    let html_entries = fixture_entries(&html_dir, "html")?;
    let gt_ids: BTreeSet<&str> = gt_entries.keys().map(String::as_str).collect();
    let html_ids: BTreeSet<&str> = html_entries.keys().map(String::as_str).collect();
    validate_fixture_ids(&gt_ids, &html_ids)?;

    let mut overall = Stats::default();
    let mut by_type: HashMap<String, Stats> = HashMap::new();
    for (id, gt_path) in &gt_entries {
        let raw = fs::read_to_string(gt_path)
            .map_err(|error| format!("read {}: {error}", gt_path.display()))?;
        let gt = parse_gt(&raw).map_err(|error| format!("parse ground truth {id}: {error}"))?;
        let html_path = &html_entries[id];
        let html = fs::read_to_string(html_path)
            .map_err(|error| format!("read {}: {error}", html_path.display()))?;
        let extracted = extract(&extractor, &html);

        let (p, r, f1) = wcxb_score::word_prf(&extracted, &gt.main_content);
        let stats = by_type.entry(gt.page_type.clone()).or_default();
        stats.pages += 1;
        stats.f1_sum += f1;
        stats.p_sum += p;
        stats.r_sum += r;
        overall.pages += 1;
        overall.f1_sum += f1;
        overall.p_sum += p;
        overall.r_sum += r;

        let with_rate = snippet_rate(&extracted, &gt.with_req);
        let without_rate = snippet_rate(&extracted, &gt.without_req);
        stats.with_rate_sum += with_rate;
        stats.without_rate_sum += without_rate;
        overall.with_rate_sum += with_rate;
        overall.without_rate_sum += without_rate;
    }

    println!(
        "Extractor: {extractor} | WCXB {split} split | scored={}",
        overall.pages
    );

    println!();
    print_row("page_type", "N", "F1", "P", "R", "with%", "without%");
    print_row("--------", "---", "----", "----", "----", "-----", "-----");
    let mut types: Vec<&String> = by_type.keys().collect();
    types.sort();
    for t in types {
        let s = &by_type[t];
        print_stats(t, s);
    }
    println!();
    print_stats("ALL", &overall);

    Ok(())
}

fn validate_fixture_ids(gt_ids: &BTreeSet<&str>, html_ids: &BTreeSet<&str>) -> Result<(), String> {
    if gt_ids != html_ids {
        let missing_html: Vec<_> = gt_ids.difference(html_ids).copied().collect();
        let missing_gt: Vec<_> = html_ids.difference(gt_ids).copied().collect();
        return Err(format!(
            "ground-truth/HTML ID mismatch: missing HTML {missing_html:?}; missing ground truth {missing_gt:?}"
        ));
    }
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let mut split = None;
    let mut extractor = "strip".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--split" => split = Some(args.next().ok_or("--split requires dev or test")?),
            "--extractor" => {
                extractor = args.next().ok_or("--extractor requires a value")?;
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let split = split.ok_or("--split is required (dev or test)")?;
    if !matches!(split.as_str(), "dev" | "test") {
        return Err(format!("unknown split {split:?}; expected dev or test"));
    }
    Ok(Args { split, extractor })
}

fn fixture_entries(dir: &Path, extension: &str) -> Result<HashMap<String, PathBuf>, String> {
    let mut entries = HashMap::new();
    let read_dir = fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in read_dir {
        let path = entry
            .map_err(|error| format!("read entry in {}: {error}", dir.display()))?
            .path();
        if path.extension().is_some_and(|value| value == extension) {
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| format!("non-UTF-8 fixture name: {}", path.display()))?
                .to_string();
            if entries.insert(id.clone(), path).is_some() {
                return Err(format!("duplicate fixture ID {id} in {}", dir.display()));
            }
        }
    }
    Ok(entries)
}

fn extract(kind: &str, html: &str) -> String {
    match kind {
        "strip" => deformat::html::strip_to_text(html),
        "anchor" => deformat::html::strip_to_text_with_options(
            html,
            &deformat::html::StripOptions::main_landmark(),
        ),
        "segments" => deformat::html::strip_to_segments(html)
            .iter()
            .map(|s| s.data().text.clone())
            .collect::<Vec<_>>()
            .join("\n\n"),
        "filtered" => {
            let segs = deformat::html::strip_to_segments(html);
            deformat::html::filter_boilerplate(segs, 40)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "density" => {
            // Link-density filter at Trafilatura-style 0.45 ratio cap.
            deformat::html::strip_to_segments_filtered(html, 0.45)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        k if k.starts_with("density-") => {
            // e.g. `density-0.7`
            let cap: f32 = k.trim_start_matches("density-").parse().unwrap_or(0.45);
            deformat::html::strip_to_segments_filtered(html, cap)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "both" => {
            let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
            deformat::html::filter_boilerplate(segs, 40)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "sentence" => {
            // Sentence-density post-filter alone.
            let segs = deformat::html::strip_to_segments(html);
            deformat::html::filter_low_sentence_density(segs, 1.0)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        k if k.starts_with("sentence-") => {
            // e.g. `sentence-2.0`
            let cap: f32 = k.trim_start_matches("sentence-").parse().unwrap_or(1.0);
            let segs = deformat::html::strip_to_segments(html);
            deformat::html::filter_low_sentence_density(segs, cap)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        "triple" => {
            // Full pipeline: link-density, then sentence-density, then boilerplate.
            let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
            let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
            deformat::html::filter_boilerplate(segs, 40)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        k if k.starts_with("cetd") => {
            // Four-filter pipeline: link-density -> sentence-density ->
            // boilerplate -> CETD (Sun et al. SIGIR 2011). `cetd` uses
            // floor 0.4; `cetd-0.5` overrides.
            let floor: f32 = k
                .strip_prefix("cetd-")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.4);
            let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
            let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
            let segs = deformat::html::filter_boilerplate(segs, 40);
            deformat::html::filter_low_cetd_density(segs, floor)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        k if k.starts_with("page-typed") => {
            // Page-type-aware routing: detect the page type first, then
            // select a pipeline tailored to it. Sketched here as a
            // proof of concept; production users would customize per
            // type based on their corpus.
            use deformat::page_type::{detect_page_type, PageType};
            let pt = detect_page_type(html);
            let segs = match pt {
                PageType::Listing | PageType::Forum => {
                    // These page types are legitimately link/list heavy;
                    // don't over-filter. Skip link-density.
                    let segs = deformat::html::strip_to_segments(html);
                    deformat::html::filter_boilerplate(segs, 30)
                }
                _ => {
                    // Article / Documentation / Product / default: full
                    // four-filter pipeline.
                    let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
                    let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
                    let segs = deformat::html::filter_boilerplate(segs, 40);
                    deformat::html::filter_low_cetd_density(segs, 0.4)
                }
            };
            segs.iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        k if k.starts_with("triple-") => {
            // e.g. `triple-2.0` sets sentence-density cap at 2.0.
            let cap: f32 = k.trim_start_matches("triple-").parse().unwrap_or(1.0);
            let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
            let segs = deformat::html::filter_low_sentence_density(segs, cap);
            deformat::html::filter_boilerplate(segs, 40)
                .iter()
                .map(|s| s.data().text.clone())
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        #[cfg(feature = "readability")]
        "readable" => deformat::extract_readable(html, None).text,
        #[cfg(feature = "readability")]
        "cascade" => deformat::extract_html_cascade(html).text,
        other => panic!("unknown extractor: {other}"),
    }
}

fn parse_gt(raw: &str) -> Result<GroundTruth, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| format!("invalid JSON: {error}"))?;
    let ground_truth = value
        .get("ground_truth")
        .and_then(serde_json::Value::as_object)
        .ok_or("missing object ground_truth")?;
    let main_content = match ground_truth.get("main_content") {
        Some(serde_json::Value::Null) | None => String::new(),
        Some(value) => value
            .as_str()
            .ok_or("ground_truth.main_content is not a string")?
            .to_string(),
    };
    let page_type_value = value.pointer("/_internal/page_type");
    let page_type = match page_type_value {
        Some(serde_json::Value::Object(object)) => object
            .get("primary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("article"),
        Some(serde_json::Value::String(value)) => value,
        _ => "article",
    };
    let page_type = if page_type == "category" {
        "collection"
    } else {
        page_type
    }
    .to_string();
    let string_array = |key: &str| -> Result<Vec<String>, String> {
        ground_truth
            .get(key)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("missing array ground_truth.{key}"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("ground_truth.{key} contains a non-string"))
            })
            .collect()
    };
    Ok(GroundTruth {
        main_content,
        page_type,
        with_req: string_array("with")?,
        without_req: string_array("without")?,
    })
}

fn snippet_rate(text: &str, snippets: &[String]) -> f64 {
    if snippets.is_empty() {
        return 1.0;
    }
    let text = text.to_lowercase();
    let found = snippets
        .iter()
        .filter(|snippet| text.contains(&snippet.to_lowercase()))
        .count();
    found as f64 / snippets.len() as f64
}

fn print_row(a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, g: &str) {
    println!("{a:<14} {b:>5} {c:>6} {d:>6} {e:>6} {f:>7} {g:>7}");
}

fn print_stats(label: &str, s: &Stats) {
    let n = s.pages.max(1) as f64;
    let f1 = s.f1_sum / n;
    let p = s.p_sum / n;
    let r = s.r_sum / n;
    let with_pct = 100.0 * s.with_rate_sum / n;
    let without_pct = 100.0 * s.without_rate_sum / n;
    println!(
        "{label:<14} {:>5} {:>6.3} {:>6.3} {:>6.3} {:>6.1}% {:>6.1}%",
        s.pages, f1, p, r, with_pct, without_pct,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<Args, String> {
        parse_args(values.iter().map(|value| (*value).to_string()))
    }

    #[test]
    fn split_is_required_and_bounded() {
        assert!(args(&[]).unwrap_err().contains("--split is required"));
        assert!(args(&["--split"])
            .unwrap_err()
            .contains("requires dev or test"));
        assert!(args(&["--split", "train"])
            .unwrap_err()
            .contains("expected dev or test"));
        assert_eq!(args(&["--split", "test"]).unwrap().split, "test");
    }

    #[test]
    fn ground_truth_parser_matches_reference_defaults() {
        let raw = r#"{
            "_internal":{"page_type":{"primary":"article"}},
            "ground_truth":{"with":[],"without":[]}
        }"#;
        let parsed = parse_gt(raw).unwrap();
        assert!(parsed.main_content.is_empty());
        assert_eq!(parsed.page_type, "article");

        let category = r#"{
            "_internal":{"page_type":"category"},
            "ground_truth":{"main_content":"x","with":[],"without":[]}
        }"#;
        assert_eq!(parse_gt(category).unwrap().page_type, "collection");

        let malformed_type = r#"{
            "_internal":{"page_type":7},
            "ground_truth":{"main_content":"x","with":[],"without":[]}
        }"#;
        assert_eq!(parse_gt(malformed_type).unwrap().page_type, "article");

        let malformed = r#"{
            "_internal":{"page_type":{"primary":"article"}},
            "ground_truth":{"main_content":7,"with":[],"without":[]}
        }"#;
        assert!(parse_gt(malformed).unwrap_err().contains("not a string"));
    }

    #[test]
    fn snippet_rates_are_macro_ready_and_not_complemented() {
        assert_eq!(snippet_rate("anything", &[]), 1.0);
        assert_eq!(
            snippet_rate("Alpha beta", &["alpha".into(), "missing".into()]),
            0.5
        );
        assert_eq!(snippet_rate("forbidden", &["forbidden".into()]), 1.0);
    }

    #[test]
    fn fixture_id_mismatch_is_an_error() {
        let gt_ids = BTreeSet::from(["0001", "0002"]);
        let html_ids = BTreeSet::from(["0001", "0003"]);
        let error = validate_fixture_ids(&gt_ids, &html_ids).unwrap_err();
        assert!(error.contains("missing HTML [\"0002\"]"));
        assert!(error.contains("missing ground truth [\"0003\"]"));
    }
}
