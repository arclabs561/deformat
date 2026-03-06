//! Integration tests for deformat.
//!
//! These test cross-module interactions and realistic content scenarios
//! that go beyond individual unit tests.

use deformat::{extract, extract_as, Format};

// =============================================================================
// Extract + detect consistency
// =============================================================================

#[test]
fn extract_format_matches_detect() {
    let cases = [
        ("<p>Hello</p>", Format::Html),
        ("<!DOCTYPE html><html><body>Hi</body></html>", Format::Html),
        ("Just text.", Format::PlainText),
        ("", Format::PlainText),
        ("# Heading", Format::PlainText),
    ];
    for (input, expected) in &cases {
        let result = extract(input);
        assert_eq!(
            result.format, *expected,
            "format mismatch for {:?}",
            &input[..input.len().min(40)]
        );
    }
}

// =============================================================================
// extract_as covers all Format variants
// =============================================================================

#[test]
fn extract_as_html_strips_tags() {
    let r = extract_as("<b>bold</b>", Format::Html);
    assert_eq!(r.text, "bold");
    assert_eq!(r.format, Format::Html);
}

#[test]
fn extract_as_plaintext_passthrough() {
    let r = extract_as("<b>not stripped</b>", Format::PlainText);
    assert_eq!(r.text, "<b>not stripped</b>");
}

#[test]
fn extract_as_markdown_passthrough() {
    let r = extract_as("# Title\n\nParagraph.", Format::Markdown);
    assert_eq!(r.text, "# Title\n\nParagraph.");
    assert_eq!(r.format, Format::Markdown);
}

#[test]
fn extract_as_unknown_passthrough() {
    let r = extract_as("mystery content", Format::Unknown);
    assert_eq!(r.text, "mystery content");
}

#[test]
fn extract_as_pdf_returns_error_metadata() {
    let r = extract_as("fake pdf content", Format::Pdf);
    assert!(r.text.is_empty(), "PDF str extraction should be empty");
    assert!(r.metadata.contains_key("error"), "should have error metadata");
}

// =============================================================================
// Realistic article: Wikipedia-style
// =============================================================================

#[test]
fn realistic_wikipedia_article() {
    let html = r#"<!DOCTYPE html>
    <html><head>
        <title>CRISPR - Wikipedia</title>
        <link rel="stylesheet" href="/style.css">
        <script>window.__tracking = true;</script>
    </head>
    <body>
        <nav id="mw-navigation">
            <ul><li><a href="/">Main page</a></li>
                <li><a href="/random">Random article</a></li></ul>
        </nav>
        <div id="content">
            <h1>CRISPR</h1>
            <div id="toc"><h2>Contents</h2>
                <ul><li>1 History</li><li>2 Mechanism</li></ul>
            </div>
            <p>CRISPR (Clustered Regularly Interspaced Short Palindromic Repeats)
               is a family of DNA sequences found in prokaryotic organisms.[1]
               Jennifer Doudna and Emmanuelle Charpentier won the Nobel Prize
               in Chemistry in 2020 for developing CRISPR-Cas9.[2]</p>
            <p>The technology enables precise editing of genomes and has
               applications in medicine, agriculture, and biotechnology.
               Feng Zhang at the Broad Institute also made key contributions.[3]</p>
            <ol class="references">
                <li id="cite_note-1">Mojica FJ (2005). "Intervening sequences".</li>
                <li id="cite_note-2">Doudna JA (2012). "RNA-guided endonuclease".</li>
                <li id="cite_note-3">Cong L, Zhang F (2013). "Multiplex engineering".</li>
            </ol>
            <div class="navbox"><table><tr><td>Gene editing topics</td></tr></table></div>
        </div>
        <footer><p>Wikipedia &copy; 2026. Content under CC BY-SA.</p></footer>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);

    // Article content preserved
    assert!(text.contains("CRISPR"), "article title: {text}");
    assert!(text.contains("Jennifer Doudna"), "person name: {text}");
    assert!(text.contains("Emmanuelle Charpentier"), "person name: {text}");
    assert!(text.contains("Feng Zhang"), "person name: {text}");
    assert!(text.contains("Nobel Prize"), "event: {text}");
    assert!(text.contains("Broad Institute"), "org: {text}");

    // Boilerplate stripped
    assert!(!text.contains("Main page"), "nav stripped: {text}");
    assert!(!text.contains("Random article"), "nav stripped: {text}");
    assert!(!text.contains("Contents"), "TOC stripped: {text}");
    assert!(!text.contains("Mojica"), "references stripped: {text}");
    assert!(!text.contains("Gene editing topics"), "navbox stripped: {text}");
    assert!(!text.contains("Wikipedia"), "footer stripped: {text}");
    assert!(!text.contains("tracking"), "script stripped: {text}");
    assert!(!text.contains("style.css"), "head stripped: {text}");

    // Reference markers stripped
    assert!(!text.contains("[1]"), "ref markers stripped: {text}");
    assert!(!text.contains("[2]"), "ref markers stripped: {text}");

    // Copyright entity decoded
    let full_result = extract(html);
    assert_eq!(full_result.format, Format::Html);
}

// =============================================================================
// Realistic article: news page
// =============================================================================

#[test]
fn realistic_news_article() {
    let html = r#"<!DOCTYPE html>
    <html><head><title>Breaking News - Reuters</title></head>
    <body>
        <header>
            <nav><a href="/">Reuters</a> | <a href="/world">World</a></nav>
        </header>
        <article>
            <h1>EU Summit Reaches Climate Agreement</h1>
            <p>European leaders including Emmanuel Macron, Olaf Scholz, and
               Giorgia Meloni agreed on new carbon emission targets at the
               Brussels summit on March 15, 2026.</p>
            <p>The agreement commits EU member states to reducing emissions
               by 65% from 1990 levels by 2035. European Commission President
               Ursula von der Leyen called it &ldquo;a historic moment for
               Europe&rsquo;s climate ambition.&rdquo;</p>
        </article>
        <aside>
            <h3>Related Stories</h3>
            <ul><li>Climate protests in Berlin</li>
                <li>US rejoins Paris Agreement</li></ul>
        </aside>
        <footer><p>&copy; 2026 Reuters. All rights reserved.</p></footer>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);

    // Article content
    assert!(text.contains("Emmanuel Macron"), "person: {text}");
    assert!(text.contains("Olaf Scholz"), "person: {text}");
    assert!(text.contains("Giorgia Meloni"), "person: {text}");
    assert!(text.contains("Ursula von der Leyen"), "person: {text}");
    assert!(text.contains("Brussels"), "location: {text}");
    assert!(text.contains("65%"), "data point: {text}");

    // Curly quotes decoded
    assert!(text.contains('\u{201C}'), "ldquo decoded: {text}");

    // Boilerplate stripped
    assert!(!text.contains("Reuters"), "header nav stripped: {text}");
    assert!(!text.contains("Related Stories"), "aside stripped: {text}");
    assert!(!text.contains("Climate protests"), "aside stripped: {text}");
    assert!(!text.contains("All rights reserved"), "footer stripped: {text}");
}

// =============================================================================
// Entity decoding for NER-critical names
// =============================================================================

#[test]
fn ner_critical_entity_decoding() {
    let html = r#"<html><body>
        <p>Nestl&eacute; CEO Mark Schneider met with S&atilde;o Paulo
           Governor Tarc&iacute;sio de Freitas. The &euro;5 billion deal
           was signed at the B&ouml;rse Frankfurt.</p>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);

    assert!(text.contains("Nestlé"), "eacute in company name: {text}");
    assert!(text.contains("São Paulo"), "atilde in location: {text}");
    assert!(text.contains("Tarcísio"), "iacute in person name: {text}");
    assert!(text.contains("€"), "euro symbol: {text}");
    assert!(text.contains("Börse"), "ouml in org name: {text}");
}

// =============================================================================
// Format detection edge cases
// =============================================================================

#[test]
fn detect_not_fooled_by_emoticons() {
    assert!(!deformat::detect::is_html("<3 my cat"));
    assert!(!deformat::detect::is_html("a < b and c > d"));
    assert!(!deformat::detect::is_html("if x < 10 then stop"));
}

#[test]
fn detect_json_not_html() {
    assert!(!deformat::detect::is_html(r#"{"key": "value", "num": 42}"#));
    assert_eq!(
        deformat::detect::detect_str(r#"{"key": "value"}"#),
        Format::PlainText
    );
}

// =============================================================================
// Whitespace invariants
// =============================================================================

#[test]
fn no_double_spaces_in_output() {
    let html = r#"<html><body>
        <h1>Title</h1>
        <p>First   paragraph   with    spaces.</p>
        <p>Second     paragraph.</p>
        <div>  <span>  nested  </span>  </div>
    </body></html>"#;
    let text = deformat::html::strip_to_text(html);
    assert!(
        !text.contains("  "),
        "no double spaces in output: {:?}",
        text
    );
}

#[test]
fn output_is_trimmed() {
    let html = "  <p>  Content  </p>  ";
    let text = deformat::html::strip_to_text(html);
    assert_eq!(text, text.trim(), "output should be trimmed");
}

// =============================================================================
// Table / infobox extraction
// =============================================================================

#[test]
fn wikipedia_infobox_cells_not_fused() {
    let html = r#"<html><body>
        <table class="infobox">
            <tr><th>Born</th><td>June 1, 1955</td></tr>
            <tr><th>Country</th><td>England</td></tr>
            <tr><th>Died</th><td>March 9, 2020</td></tr>
            <tr><th>Nationality</th><td>British</td></tr>
        </table>
        <p>Article content about this person.</p>
    </body></html>"#;
    let text = deformat::html::strip_to_text(html);

    // Key invariant: no cell fusion
    assert!(
        !text.contains("BornJune"),
        "th-td fusion: {text}"
    );
    assert!(
        !text.contains("EnglandDied"),
        "cross-row fusion: {text}"
    );
    assert!(
        !text.contains("BritishArticle"),
        "table-paragraph fusion: {text}"
    );
    assert!(text.contains("Article content"), "body preserved: {text}");
}

// =============================================================================
// Attribute value isolation
// =============================================================================

#[test]
fn attribute_gt_does_not_break_extraction() {
    // Real-world: data attributes with comparison operators
    let html = r#"<html><body>
        <div data-filter="age > 18" data-sort="name < desc">
            <p>Visible content here.</p>
        </div>
    </body></html>"#;
    let text = deformat::html::strip_to_text(html);
    assert!(text.contains("Visible content"), "content preserved: {text}");
    assert!(!text.contains("age > 18"), "attr not leaked: {text}");
    assert!(!text.contains("data-filter"), "attr name not leaked: {text}");
}

// =============================================================================
// Extracted struct
// =============================================================================

#[test]
fn extracted_clone_and_debug() {
    let result = extract("<p>Hello</p>");
    let cloned = result.clone();
    assert_eq!(result.text, cloned.text);
    assert_eq!(result.format, cloned.format);
    let debug = format!("{:?}", result);
    assert!(debug.contains("Extracted"), "debug impl works");
}

// =============================================================================
// CJK ruby annotation handling
// =============================================================================

#[test]
fn japanese_article_with_furigana() {
    // Realistic Japanese Wikipedia-style article with ruby annotations
    let html = r#"<!DOCTYPE html>
    <html><head><title>東京 - Wikipedia</title></head>
    <body>
        <nav><a href="/">メインページ</a></nav>
        <article>
            <h1><ruby>東京<rt>とうきょう</rt></ruby></h1>
            <p><ruby>東京都<rt>とうきょうと</rt></ruby>は<ruby>日本<rt>にほん</rt></ruby>の
               <ruby>首都<rt>しゅと</rt></ruby>であり、
               <ruby>人口<rt>じんこう</rt></ruby>は約1400<ruby>万人<rt>まんにん</rt></ruby>。
               <ruby>安倍<rt>あべ</rt></ruby><ruby>晋三<rt>しんぞう</rt></ruby>
               <ruby>元首相<rt>もとしゅしょう</rt></ruby>は<ruby>東京<rt>とうきょう</rt></ruby>で
               <ruby>記者会見<rt>きしゃかいけん</rt></ruby>を<ruby>行<rt>おこな</rt></ruby>った。</p>
        </article>
        <footer><p>&copy; Wikipedia</p></footer>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);

    // Base text preserved
    assert!(text.contains("東京都"), "Tokyo-to: {text}");
    assert!(text.contains("日本"), "Japan: {text}");
    assert!(text.contains("安倍"), "Abe: {text}");
    assert!(text.contains("晋三"), "Shinzo: {text}");

    // Furigana stripped
    assert!(!text.contains("とうきょう"), "Tokyo reading stripped: {text}");
    assert!(!text.contains("にほん"), "Japan reading stripped: {text}");
    assert!(!text.contains("あべ"), "Abe reading stripped: {text}");
    assert!(!text.contains("しんぞう"), "Shinzo reading stripped: {text}");

    // Boilerplate stripped
    assert!(!text.contains("メインページ"), "nav stripped: {text}");
    assert!(!text.contains("Wikipedia"), "footer stripped: {text}");
}

// =============================================================================
// Unicode cleanup in realistic content
// =============================================================================

#[test]
fn rtl_mixed_text_bidi_controls_stripped() {
    // Arabic/Hebrew mixed with Latin text containing bidi marks
    let html = "<p>\u{202B}محمد\u{202C} met \u{202B}David\u{202C} in \u{202B}القاهرة\u{202C}.</p>";
    let text = deformat::html::strip_to_text(html);
    assert!(text.contains("محمد"), "Arabic name: {text}");
    assert!(text.contains("David"), "Latin name: {text}");
    assert!(text.contains("القاهرة"), "Arabic location: {text}");
    // No bidi controls
    assert!(!text.contains('\u{202B}'), "no RLE: {text}");
    assert!(!text.contains('\u{202C}'), "no PDF: {text}");
}

#[test]
fn nbsp_in_entity_names() {
    // French names with NBSP (common in French typography: space before colon/semicolon)
    let html = "<p>Le pr\u{00E9}sident Macron\u{00A0}: discours \u{00E0} l'Elys\u{00E9}e.</p>";
    let text = deformat::html::strip_to_text(html);
    assert!(text.contains("Macron"), "name preserved: {text}");
    assert!(!text.contains('\u{00A0}'), "NBSP normalized: {text}");
}

#[test]
fn extract_preserves_format_for_cjk_html() {
    let html = "<p><ruby>東京<rt>とうきょう</rt></ruby></p>";
    let result = extract(html);
    assert_eq!(result.format, Format::Html);
    assert!(result.text.contains("東京"), "base text in extract: {}", result.text);
    assert!(!result.text.contains("とうきょう"), "furigana stripped in extract: {}", result.text);
}
