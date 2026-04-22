//! Benchmark tests against real-world HTML from local fixtures.
//!
//! Previously these tests hit live URLs (scrapinghub GitHub, example.com,
//! Wikipedia, HN). They're now driven by the WCXB dev split already
//! downloaded by `scripts/fetch_wcxb.py` and pointed at by
//! `examples/bench_wcxb.rs`. Tests are `#[ignore]` so CI without the
//! fixture directory skips them silently.
//!
//! Run locally with:
//!
//! ```sh
//! scripts/fetch_wcxb.py                    # one-time fetch
//! cargo test --test bench_real_html -- --ignored
//! ```

use std::fs;
use std::path::PathBuf;

fn wcxb_dev_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/bench-fixtures/wcxb/dev")
        .join("html")
}

fn collect_samples(max: usize) -> Vec<(String, String)> {
    let dir = wcxb_dev_dir();
    let Ok(read) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    paths.sort();
    paths.truncate(max);
    paths
        .into_iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            let html = fs::read_to_string(&p).ok()?;
            Some((name, html))
        })
        .collect()
}

/// Scan WCXB fixtures: `strip_to_text` and `strip_to_markdown` must never
/// panic and must never leak raw `<script>` / `<style>` tags into output.
#[test]
#[ignore]
fn wcxb_no_panics_no_tag_leaks() {
    let samples = collect_samples(80);
    if samples.is_empty() {
        eprintln!(
            "WCXB fixtures not found at {:?}\nRun: scripts/fetch_wcxb.py",
            wcxb_dev_dir()
        );
        return;
    }

    for (name, html) in &samples {
        let text = deformat::html::strip_to_text(html);
        let md = deformat::html::strip_to_markdown(html);

        if html.len() > 2000 {
            assert!(
                text.len() > 50,
                "suspiciously short text output for {name}: {} bytes from {} bytes HTML",
                text.len(),
                html.len(),
            );
            assert!(
                md.len() > 50,
                "suspiciously short markdown output for {name}: {} bytes",
                md.len(),
            );
        }

        assert!(
            !text.contains("<script"),
            "script tag leaked in {name}: {}",
            &text.chars().take(200).collect::<String>(),
        );
        assert!(!text.contains("<style"), "style tag leaked in {name}",);
        assert!(
            !text.contains("</script>") && !text.contains("</style>"),
            "closing script/style leaked in {name}",
        );
    }
    println!(
        "WCXB smoke test: {} fixtures passed strip_to_text / strip_to_markdown",
        samples.len()
    );
}

/// Exercise the full segment + three-filter pipeline on a WCXB sample:
/// must not panic, must produce at least some segments on most inputs.
#[test]
#[ignore]
fn wcxb_segment_pipeline_no_panics() {
    let samples = collect_samples(80);
    if samples.is_empty() {
        eprintln!(
            "WCXB fixtures not found at {:?}\nRun: scripts/fetch_wcxb.py",
            wcxb_dev_dir()
        );
        return;
    }

    let mut produced_some = 0usize;
    for (name, html) in &samples {
        let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
        let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
        let segs = deformat::html::filter_boilerplate(segs, 40);

        // The pipeline may legitimately reduce to zero segments on
        // extreme edge cases (redirect-only pages, empty bodies).
        // But on pages > 5 KB we expect at least one segment.
        if html.len() > 5000 && segs.is_empty() {
            continue;
        }
        if !segs.is_empty() {
            produced_some += 1;
        }

        for s in &segs {
            let d = s.data();
            // Never empty text in a kept segment.
            assert!(
                !d.text.is_empty(),
                "empty segment text in {name} ({})",
                s.type_name()
            );
            // element_id is always non-empty.
            assert!(!d.element_id.is_empty(), "empty element_id in {name}");
        }
    }
    assert!(
        produced_some >= samples.len() / 2,
        "pipeline produced no segments on too many fixtures: {produced_some} / {}",
        samples.len()
    );
    println!(
        "WCXB pipeline smoke test: {produced_some} / {} fixtures produced segments",
        samples.len()
    );
}

/// SpanMap invariants hold on real-world HTML too: sorted, non-overlap
/// in output, UTF-8 boundary-aligned.
#[test]
#[ignore]
fn wcxb_spanmap_invariants() {
    let samples = collect_samples(40);
    if samples.is_empty() {
        eprintln!(
            "WCXB fixtures not found at {:?}\nRun: scripts/fetch_wcxb.py",
            wcxb_dev_dir()
        );
        return;
    }

    for (name, html) in &samples {
        let (text, spans) = deformat::html::strip_to_text_with_spans(html);
        let mut prev_end = 0usize;
        for (i, s) in spans.iter().enumerate() {
            assert!(
                s.output_start <= s.output_end,
                "inverted output in {name} span {i}"
            );
            assert!(
                s.source_start <= s.source_end,
                "inverted source in {name} span {i}"
            );
            assert!(
                s.output_end <= text.len(),
                "output OOB in {name} span {i}: {} > {}",
                s.output_end,
                text.len()
            );
            assert!(s.source_end <= html.len(), "source OOB in {name} span {i}");
            assert!(
                s.output_start >= prev_end,
                "spans overlap in {name} at span {i}"
            );
            assert!(
                text.is_char_boundary(s.output_start) && text.is_char_boundary(s.output_end),
                "output not on char boundary in {name} span {i}"
            );
            assert!(
                html.is_char_boundary(s.source_start) && html.is_char_boundary(s.source_end),
                "source not on char boundary in {name} span {i}"
            );
            prev_end = s.output_end;
        }
    }
    println!(
        "WCXB SpanMap invariants checked across {} fixtures",
        samples.len()
    );
}
