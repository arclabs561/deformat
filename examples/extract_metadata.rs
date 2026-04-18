//! Pull structured metadata from an HTML document.
//!
//! ```sh
//! cargo run --example extract_metadata
//! # with language detection
//! cargo run --example extract_metadata --features whichlang
//! ```
//!
//! `html::extract_metadata` scans the `<head>` for title, author,
//! description, publication date, canonical URL, and the `<html lang>`
//! attribute. The `whichlang` feature adds a fallback that detects the
//! language from extracted body text when `<html lang>` is absent or
//! wrong.

fn main() {
    let html = r#"<!DOCTYPE html>
    <html lang="en">
      <head>
        <title>CRISPR editing milestones — Nature</title>
        <meta name="author" content="Dr. Sarah Chen">
        <meta name="description" content="A 2026 review of notable CRISPR-Cas9 results.">
        <meta property="article:published_time" content="2026-04-18T09:30:00Z">
        <link rel="canonical" href="https://example.com/crispr-2026">
        <meta property="og:title" content="CRISPR editing milestones — Nature">
      </head>
      <body>
        <article>
          <h1>CRISPR editing milestones</h1>
          <p>CRISPR-Cas9 enables precise editing of genomes. Over the
             past decade, its applications have expanded from basic
             research into medicine and agriculture.</p>
          <p>This review surveys the most cited results of the 2020s.</p>
        </article>
      </body>
    </html>"#;

    let meta = deformat::html::extract_metadata(html);
    println!("title         : {:?}", meta.title);
    println!("author        : {:?}", meta.author);
    println!("description   : {:?}", meta.description);
    println!("date_published: {:?}", meta.date_published);
    println!("language      : {:?}", meta.language);
    println!("canonical_url : {:?}", meta.canonical_url);

    #[cfg(feature = "whichlang")]
    {
        // Fallback path: detect language from the body when <html lang>
        // was missing or unreliable.
        let body_text = deformat::html::strip_to_text(html);
        let detected = deformat::html::detect_language(&body_text);
        println!("detected lang : {detected:?}");
    }
}
