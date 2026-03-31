//! Benchmark tests against real-world HTML from public datasets.
//!
//! These tests are `#[ignore]` by default -- they download data from the
//! internet and take significant time. Run with:
//!
//! ```sh
//! cargo test --test bench_real_html -- --ignored
//! ```
//!
//! For the full Scrapinghub article extraction benchmark with F1 scoring,
//! use the Python script instead:
//!
//! ```sh
//! uv run scripts/bench_extraction.py --limit 20
//! ```

use std::io::Read;
use std::path::PathBuf;

fn cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/bench-fixtures")
}

fn download_if_missing(url: &str, dest: &std::path::Path) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    dest.parent().map(|p| std::fs::create_dir_all(p).ok());
    let output = std::process::Command::new("curl")
        .args(["-sL", "-o"])
        .arg(dest)
        .arg(url)
        .output()
        .map_err(|e| format!("curl failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl returned {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Download a few Scrapinghub HTML samples and verify deformat doesn't panic,
/// produces non-empty output, and strips obvious boilerplate.
#[test]
#[ignore]
fn scrapinghub_no_panics() {
    let base = "https://raw.githubusercontent.com/scrapinghub/article-extraction-benchmark/refs/heads/master/html";
    // Pick 10 diverse samples by hash prefix
    let samples = [
        "042bb7b5fedab6eac7db576522b89b93904c237d344bcbe14a6a5ab7f7335856",
        "04a6711caa7c687592777718866e781e976e0fe684faebe8b3cedcef8cd0ea34",
        "06e5123e4ef7cfb4533250dc45d1e03d0838fc66223f45c583c4d12f48b4da85",
        "098bb3e96c0acdf36efdcde45fb9cca3f8c82c7cb2071b76097a1b96155f1eb2",
        "0d46122928b6f468cc4bbc694051d0dbae5702bc75a16dab82a99b58daf150a0",
        "08f793762792bd252c75fb57544cdf506ffcc04785136cb87503f02364b82b56",
        "076f4f33bf75059db581bedf36e76fb65e89a8f7752db3339aa3ea11c5122f32",
        "05844573ca7e1fba714d715bb11ca08c26e25328999c74a1cb3bc8a0e4399f0f",
        "06ee193de4bd611f7fafbab0c59b0f6fe3495093516720632cd093b24c7a0e98",
        "0dd1357045727799a447563fd8851f4ebe79f042073ea16991a9b67aa595f81a",
    ];

    let html_dir = cache_dir().join("scrapinghub/html");
    let mut success = 0;
    let mut skipped = 0;

    for hash in &samples {
        let url = format!("{base}/{hash}.html.gz");
        let dest = html_dir.join(format!("{hash}.html.gz"));

        if download_if_missing(&url, &dest).is_err() {
            skipped += 1;
            continue;
        }

        let gz_bytes = match std::fs::read(&dest) {
            Ok(b) => b,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let mut decoder = flate2::read::GzDecoder::new(&gz_bytes[..]);
        let mut html = String::new();
        if decoder.read_to_string(&mut html).is_err() {
            // Not valid UTF-8 gzip -- skip
            skipped += 1;
            continue;
        }

        // This must not panic
        let text = deformat::html::strip_to_text(&html);
        let md = deformat::html::strip_to_markdown(&html);

        // Basic sanity: output should be non-trivial for real articles
        if html.len() > 1000 {
            assert!(
                text.len() > 50,
                "suspiciously short text output for {}: {} bytes from {} bytes HTML",
                &hash[..12],
                text.len(),
                html.len()
            );
            assert!(
                md.len() > 50,
                "suspiciously short markdown output for {}: {} bytes",
                &hash[..12],
                md.len()
            );
        }

        // No HTML tags in text output
        assert!(
            !text.contains("<script"),
            "script tag leaked in {}: {}...",
            &hash[..12],
            &text[..text.len().min(100)]
        );
        assert!(
            !text.contains("<style"),
            "style tag leaked in {}",
            &hash[..12]
        );

        success += 1;
    }

    assert!(
        success >= 3,
        "too few successful extractions: {success} success, {skipped} skipped"
    );
    println!(
        "Scrapinghub smoke test: {success} success, {skipped} skipped out of {}",
        samples.len()
    );
}

/// Fetch a small sample from Common Crawl and verify no panics on real
/// web content. Downloads ~1MB of WARC data.
#[test]
#[ignore]
fn common_crawl_no_panics() {
    // Common Crawl wet file (pre-extracted text) -- smaller than full WARC.
    // We use a WAT (metadata) file listing to find a small WARC segment.
    // For simplicity, we'll download a single small WARC and parse it manually.
    //
    // Alternative: fetch random pages from a curated list.

    let urls = [
        "https://example.com",
        "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "https://news.ycombinator.com",
    ];

    let mut success = 0;
    for url in &urls {
        let dest = cache_dir().join(format!(
            "crawl/{}.html",
            url.replace("://", "_").replace('/', "_")
        ));

        if download_if_missing(url, &dest).is_err() {
            continue;
        }

        let html = match std::fs::read_to_string(&dest) {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Must not panic
        let text = deformat::html::strip_to_text(&html);
        let md = deformat::html::strip_to_markdown(&html);

        assert!(!text.is_empty(), "empty text from {url}");
        assert!(!md.is_empty(), "empty markdown from {url}");
        assert!(!text.contains("<script"), "script leaked from {url}");

        success += 1;
    }

    assert!(
        success >= 1,
        "no successful downloads -- network may be unavailable"
    );
    println!("Common Crawl smoke test: {success} / {} URLs", urls.len());
}
