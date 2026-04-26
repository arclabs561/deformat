/// Extract readable text from messy HTML.
///
/// HTML from the web is full of navigation, ads, and boilerplate.
/// `extract_readable` uses readability heuristics to pull out the
/// main article content. Useful as a preprocessing step before
/// NER, summarization, or embedding.
///
/// ```sh
/// cargo run --example clean_html --features readability
/// ```
///
/// Output prints the raw and extracted character counts (raw drops
/// from a few hundred down to the article body) followed by the
/// recovered text, which begins with the headline plus the first
/// `<article>` paragraph.
fn main() {
    let html = r#"
    <html>
    <head><title>Famous Scientists</title></head>
    <body>
      <nav><a href="/">Home</a> | <a href="/about">About</a></nav>
      <div id="sidebar">
        <h3>Popular Articles</h3>
        <ul><li>Top 10 Discoveries</li><li>Nobel Prize Winners</li></ul>
      </div>
      <article>
        <h1>Marie Curie</h1>
        <p>Marie Curie was a physicist and chemist who conducted
        pioneering research on radioactivity. She was the first woman
        to win a Nobel Prize and remains the only person to win Nobel
        Prizes in two different sciences.</p>
        <p>Born in Warsaw in 1867, she moved to Paris to study at the
        Sorbonne.</p>
      </article>
      <footer>Copyright 2024 Example Corp</footer>
    </body>
    </html>
    "#;

    println!("=== Raw HTML: {} chars ===", html.len());

    let result = deformat::extract_readable(html, None);
    println!("=== Extracted text: ~{} chars ===\n", result.text.len());
    println!("{}", result.text.trim());
}
