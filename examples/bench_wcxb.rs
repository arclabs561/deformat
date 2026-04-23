//! Score deformat against the WCXB benchmark.
//!
//! WCXB is CC-BY-4.0 by Murrough Foley. Download the fixtures with
//! `scripts/fetch_wcxb.py` (uses the HuggingFace Hub client) before
//! running this example.
//!
//! Usage:
//!
//! ```sh
//! scripts/fetch_wcxb.py                          # dev split
//! cargo run --release --example bench_wcxb       # strip_to_text baseline
//! cargo run --release --example bench_wcxb -- --extractor readable --features readability
//! ```
//!
//! Metric: word-level F1 against the `ground_truth.main_content` field
//! in each per-page JSON (matches WCXB's reference `evaluate.py`).
//! Reports overall F1/P/R and per-page-type breakdown.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Default, Clone)]
struct Stats {
    pages: usize,
    f1_sum: f64,
    p_sum: f64,
    r_sum: f64,
    with_ok: usize,
    with_total: usize,
    without_ok: usize,
    without_total: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let extractor = args
        .iter()
        .position(|a| a == "--extractor")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "strip".to_string());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_dir = root.join("target/bench-fixtures/wcxb/dev");
    let gt_dir = dev_dir.join("ground-truth");
    let html_dir = dev_dir.join("html");

    if !gt_dir.exists() {
        eprintln!(
            "WCXB fixtures not found at {:?}\nRun: scripts/fetch_wcxb.py",
            dev_dir
        );
        return ExitCode::from(2);
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(&gt_dir)
        .expect("read ground-truth dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    println!(
        "Extractor: {extractor} | WCXB dev split | {} pages",
        entries.len()
    );

    let mut overall = Stats::default();
    let mut by_type: HashMap<String, Stats> = HashMap::new();

    for gt_path in &entries {
        let Ok(raw) = fs::read_to_string(gt_path) else {
            continue;
        };
        let (gold, page_type, with_req, without_req) = match parse_gt(&raw) {
            Some(parts) => parts,
            None => continue,
        };
        let stem = gt_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let html_path = html_dir.join(format!("{stem}.html"));
        let Ok(html) = fs::read_to_string(&html_path) else {
            continue;
        };
        let extracted = extract(&extractor, &html);

        let (p, r, f1) = word_prf(&extracted, &gold);
        let stats = by_type.entry(page_type.clone()).or_default();
        stats.pages += 1;
        stats.f1_sum += f1;
        stats.p_sum += p;
        stats.r_sum += r;
        overall.pages += 1;
        overall.f1_sum += f1;
        overall.p_sum += p;
        overall.r_sum += r;

        let extracted_lower = extracted.to_lowercase();
        for snippet in &with_req {
            stats.with_total += 1;
            overall.with_total += 1;
            if extracted_lower.contains(&snippet.to_lowercase()) {
                stats.with_ok += 1;
                overall.with_ok += 1;
            }
        }
        for snippet in &without_req {
            stats.without_total += 1;
            overall.without_total += 1;
            if !extracted_lower.contains(&snippet.to_lowercase()) {
                stats.without_ok += 1;
                overall.without_ok += 1;
            }
        }
    }

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

    ExitCode::SUCCESS
}

fn extract(kind: &str, html: &str) -> String {
    match kind {
        "strip" => deformat::html::strip_to_text(html),
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
        other => panic!("unknown extractor: {other}"),
    }
}

fn parse_gt(raw: &str) -> Option<(String, String, Vec<String>, Vec<String>)> {
    let main = extract_json_str(raw, "\"main_content\"")?;
    let page_type = extract_json_str(raw, "\"primary\"").unwrap_or_else(|| "unknown".to_string());
    let with_req = extract_json_string_array(raw, "\"with\"").unwrap_or_default();
    let without_req = extract_json_string_array(raw, "\"without\"").unwrap_or_default();
    Some((main, page_type, with_req, without_req))
}

/// Tiny dependency-free JSON string-value fetcher scoped to this
/// script. Assumes each key occurs once in the file; WCXB ground-truth
/// files satisfy that. Unescapes `\"`, `\\`, `\n`, `\t`, `\/`.
fn extract_json_str(raw: &str, key: &str) -> Option<String> {
    let pos = raw.find(key)?;
    let rest = &raw[pos + key.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let bytes = after.as_bytes();
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            return Some(out);
        }
        if b == b'\\' && i + 1 < bytes.len() {
            let esc = bytes[i + 1];
            match esc {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'u' => {
                    if i + 5 < bytes.len() {
                        let hex = &after[i + 2..i + 6];
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(c) = char::from_u32(n) {
                                out.push(c);
                            }
                        }
                        i += 6;
                        continue;
                    }
                }
                _ => out.push(esc as char),
            }
            i += 2;
            continue;
        }
        // Advance by whole UTF-8 char to avoid splitting a multibyte boundary.
        let ch = after[i..].chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    None
}

fn extract_json_string_array(raw: &str, key: &str) -> Option<Vec<String>> {
    let pos = raw.find(key)?;
    let rest = &raw[pos + key.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if !after.starts_with('[') {
        return None;
    }
    let end = after.find(']')?;
    let body = &after[1..end];
    let mut out = Vec::new();
    // Parse quoted strings inside the array. Good enough for WCXB's
    // with/without which contain short snippets with occasional quotes
    // or commas -- if a future version tightens, swap in serde_json.
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut s = String::new();
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    let esc = bytes[i + 1];
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'n' => s.push('\n'),
                        _ => s.push(esc as char),
                    }
                    i += 2;
                    continue;
                }
                let ch = body[i..].chars().next()?;
                s.push(ch);
                i += ch.len_utf8();
            }
            out.push(s);
            i += 1;
        } else {
            i += 1;
        }
    }
    Some(out)
}

fn word_prf(predicted: &str, reference: &str) -> (f64, f64, f64) {
    let pred = word_counter(predicted);
    let refe = word_counter(reference);
    if refe.is_empty() {
        return if pred.is_empty() {
            (1.0, 1.0, 1.0)
        } else {
            (0.0, 1.0, 0.0)
        };
    }
    if pred.is_empty() {
        return (1.0, 0.0, 0.0);
    }
    let mut overlap = 0usize;
    for (w, &pc) in &pred {
        if let Some(&rc) = refe.get(w) {
            overlap += pc.min(rc);
        }
    }
    let pred_total: usize = pred.values().sum();
    let ref_total: usize = refe.values().sum();
    let p = overlap as f64 / pred_total as f64;
    let r = overlap as f64 / ref_total as f64;
    let f1 = if p + r > 0.0 {
        2.0 * p * r / (p + r)
    } else {
        0.0
    };
    (p, r, f1)
}

fn word_counter(s: &str) -> HashMap<String, usize> {
    let mut c = HashMap::new();
    let mut word = String::new();
    for ch in s.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            for lc in ch.to_lowercase() {
                word.push(lc);
            }
        } else if !word.is_empty() {
            *c.entry(std::mem::take(&mut word)).or_insert(0) += 1;
        }
    }
    if !word.is_empty() {
        *c.entry(word).or_insert(0) += 1;
    }
    c
}

fn print_row(a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, g: &str) {
    println!("{a:<14} {b:>5} {c:>6} {d:>6} {e:>6} {f:>7} {g:>7}");
}

fn print_stats(label: &str, s: &Stats) {
    let n = s.pages.max(1) as f64;
    let f1 = s.f1_sum / n;
    let p = s.p_sum / n;
    let r = s.r_sum / n;
    let with_pct = if s.with_total > 0 {
        100.0 * s.with_ok as f64 / s.with_total as f64
    } else {
        f64::NAN
    };
    let without_pct = if s.without_total > 0 {
        100.0 * s.without_ok as f64 / s.without_total as f64
    } else {
        f64::NAN
    };
    println!(
        "{label:<14} {:>5} {:>6.3} {:>6.3} {:>6.3} {:>6.1}% {:>6.1}%",
        s.pages, f1, p, r, with_pct, without_pct,
    );
}

#[allow(dead_code)]
fn _unused(_p: &Path) {}
