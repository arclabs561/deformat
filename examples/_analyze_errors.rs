//! Temporary analysis tool. Delete after session.
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = root.join("target/bench-fixtures/wcxb/dev");
    let gt_dir = dev.join("ground-truth");
    let html_dir = dev.join("html");
    let mut scores: Vec<(f64, String, String, String, usize)> = Vec::new();
    for entry in fs::read_dir(&gt_dir).expect("read gt").flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(gold) = extract_json_str(&raw, "\"main_content\"") else {
            continue;
        };
        let Some(page_type) = extract_json_str(&raw, "\"primary\"") else {
            continue;
        };
        if page_type != "article" {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let html_path = html_dir.join(format!("{stem}.html"));
        let Ok(html) = fs::read_to_string(&html_path) else {
            continue;
        };
        let segs = deformat::html::strip_to_segments_filtered(&html, 0.45);
        let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
        let segs = deformat::html::filter_boilerplate(segs, 40);
        let extracted: String = segs
            .iter()
            .map(|s| s.data().text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        let (_, _, f1) = word_prf(&extracted, &gold);
        scores.push((f1, stem.to_string(), gold, extracted, html.len()));
    }
    scores.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("Worst 10 articles now:\n");
    for (f1, stem, gold, extracted, html_len) in scores.iter().take(10) {
        println!(
            "=== {stem} F1={f1:.3} html={html_len}b gold={}c extr={}c ===",
            gold.len(),
            extracted.len()
        );
        let gold_preview: String = gold.chars().take(150).collect();
        let ex_preview: String = extracted.chars().take(150).collect();
        println!("  gold[:150]: {gold_preview:?}");
        println!("  extr[:150]: {ex_preview:?}");
    }
    // Also report histogram
    println!("\nF1 histogram:");
    let mut bins = [0usize; 10];
    for (f1, _, _, _, _) in &scores {
        let idx = ((f1 * 10.0) as usize).min(9);
        bins[idx] += 1;
    }
    for (i, c) in bins.iter().enumerate() {
        println!(
            "  [{:.1}-{:.1}): {} {}",
            i as f64 / 10.0,
            (i + 1) as f64 / 10.0,
            c,
            "#".repeat(c / 10)
        );
    }
    let zero = scores.iter().filter(|(f1, _, _, _, _)| *f1 < 0.01).count();
    println!("\n{zero} articles still score F1 < 0.01");
}

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
        let ch = after[i..].chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    None
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
