//! Integration tests for deformat.
//!
//! These test cross-module interactions and realistic content scenarios
//! that go beyond individual unit tests.

use deformat::{extract, extract_as, Error, Format};

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
        let result = extract(input).unwrap();
        assert_eq!(
            result.format,
            *expected,
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
    let r = extract_as("<b>bold</b>", Format::Html).unwrap();
    assert_eq!(r.text, "bold");
    assert_eq!(r.format, Format::Html);
}

#[test]
fn extract_as_plaintext_passthrough() {
    let r = extract_as("<b>not stripped</b>", Format::PlainText).unwrap();
    assert_eq!(r.text, "<b>not stripped</b>");
}

#[test]
fn extract_as_markdown_passthrough() {
    let r = extract_as("# Title\n\nParagraph.", Format::Markdown).unwrap();
    assert_eq!(r.text, "# Title\n\nParagraph.");
    assert_eq!(r.format, Format::Markdown);
}

#[test]
fn extract_as_unknown_passthrough() {
    let r = extract_as("mystery content", Format::Unknown).unwrap();
    assert_eq!(r.text, "mystery content");
}

#[test]
fn extract_as_pdf_returns_error() {
    let r = extract_as("fake pdf content", Format::Pdf);
    assert!(r.is_err(), "PDF str extraction should return Err");
    match r.unwrap_err() {
        Error::UnsupportedFormat(msg) => {
            assert!(msg.contains("PDF"), "error mentions PDF: {msg}");
        }
        other => panic!("expected UnsupportedFormat, got: {other}"),
    }
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

    use deformat::html::{strip_to_text_with_options, StripOptions};
    let text = strip_to_text_with_options(html, &StripOptions::wikipedia());

    // Article content preserved
    assert!(text.contains("CRISPR"), "article title: {text}");
    assert!(text.contains("Jennifer Doudna"), "person name: {text}");
    assert!(
        text.contains("Emmanuelle Charpentier"),
        "person name: {text}"
    );
    assert!(text.contains("Feng Zhang"), "person name: {text}");
    assert!(text.contains("Nobel Prize"), "event: {text}");
    assert!(text.contains("Broad Institute"), "org: {text}");

    // Boilerplate stripped
    assert!(!text.contains("Main page"), "nav stripped: {text}");
    assert!(!text.contains("Random article"), "nav stripped: {text}");
    assert!(!text.contains("Contents"), "TOC stripped: {text}");
    assert!(!text.contains("Mojica"), "references stripped: {text}");
    assert!(
        !text.contains("Gene editing topics"),
        "navbox stripped: {text}"
    );
    assert!(!text.contains("Wikipedia"), "footer stripped: {text}");
    assert!(!text.contains("tracking"), "script stripped: {text}");
    assert!(!text.contains("style.css"), "head stripped: {text}");

    // Reference markers stripped (with wikipedia options)
    assert!(!text.contains("[1]"), "ref markers stripped: {text}");
    assert!(!text.contains("[2]"), "ref markers stripped: {text}");

    // Copyright entity decoded
    let full_result = extract(html).unwrap();
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
    assert!(
        !text.contains("All rights reserved"),
        "footer stripped: {text}"
    );
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
    assert!(!text.contains("BornJune"), "th-td fusion: {text}");
    assert!(!text.contains("EnglandDied"), "cross-row fusion: {text}");
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
    assert!(
        text.contains("Visible content"),
        "content preserved: {text}"
    );
    assert!(!text.contains("age > 18"), "attr not leaked: {text}");
    assert!(
        !text.contains("data-filter"),
        "attr name not leaked: {text}"
    );
}

// =============================================================================
// Extracted struct
// =============================================================================

#[test]
fn extracted_clone_and_debug() {
    let result = extract("<p>Hello</p>").unwrap();
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
    assert!(
        !text.contains("とうきょう"),
        "Tokyo reading stripped: {text}"
    );
    assert!(!text.contains("にほん"), "Japan reading stripped: {text}");
    assert!(!text.contains("あべ"), "Abe reading stripped: {text}");
    assert!(
        !text.contains("しんぞう"),
        "Shinzo reading stripped: {text}"
    );

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
    let result = extract(html).unwrap();
    assert_eq!(result.format, Format::Html);
    assert!(
        result.text.contains("東京"),
        "base text in extract: {}",
        result.text
    );
    assert!(
        !result.text.contains("とうきょう"),
        "furigana stripped in extract: {}",
        result.text
    );
}

// =============================================================================
// Central/Eastern European entity decoding
// =============================================================================

#[test]
fn polish_wikipedia_article() {
    let html = r#"<!DOCTYPE html>
    <html><head><title>Łódź - Wikipedia</title></head>
    <body>
        <nav><a href="/">Strona główna</a></nav>
        <article>
            <h1>&Lstrok;&oacute;d&zacute;</h1>
            <p>&Lstrok;&oacute;d&zacute; jest trzecim co do wielko&sacute;ci miastem
               w Polsce. Prezydent miasta, Hanna Zdanowska (ur. 1957),
               kieruje miastem od 2010 roku. W 2023 roku Jaros&lstrok;aw
               Kaczy&nacute;ski odwiedzi&lstrok; &Lstrok;&oacute;d&zacute;.</p>
            <p>Miasto le&zdot;y nad rzek&aogonek; &Lstrok;&oacute;dk&aogonek;
               i jest wa&zdot;nym o&sacute;rodkiem przemys&lstrok;owym.</p>
        </article>
        <footer><p>&copy; Wikipedia</p></footer>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);

    // Polish entities decoded correctly
    assert!(text.contains("Łódź"), "Lstrok+oacute+zacute: {text}");
    assert!(text.contains("Jarosław"), "lstrok: {text}");
    assert!(text.contains("Kaczyński"), "nacute: {text}");
    assert!(text.contains("wielkości"), "sacute: {text}");
    assert!(text.contains("leży"), "zdot: {text}");
    assert!(text.contains("rzeką"), "aogonek: {text}");
    assert!(text.contains("ośrodkiem"), "sacute in word: {text}");
    assert!(text.contains("przemysłowym"), "lstrok in word: {text}");

    // Boilerplate stripped
    assert!(!text.contains("Strona główna"), "nav stripped: {text}");
    assert!(!text.contains("Wikipedia"), "footer stripped: {text}");
}

#[test]
fn turkish_entity_decoding() {
    let html = r#"<article>
        <p>Recep Tayyip Erdo&gbreve;an, T&uuml;rkiye Cumhurba&scedil;kan&inodot;,
           &Idot;stanbul'da bir toplant&inodot;ya kat&inodot;ld&inodot;.
           Mu&gbreve;la ve Antalya'dan delegeler de vard&inodot;.</p>
    </article>"#;

    let text = deformat::html::strip_to_text(html);
    assert!(text.contains("Erdoğan"), "gbreve: {text}");
    assert!(text.contains("İstanbul"), "Idot: {text}");
    assert!(text.contains("Cumhurbaşkanı"), "scedil+inodot: {text}");
    assert!(text.contains("Muğla"), "gbreve in city name: {text}");
    assert!(text.contains("toplantıya"), "inodot in word: {text}");
}

#[test]
fn czech_entity_decoding() {
    let html = r#"<article>
        <p>V &Ccaron;esk&eacute; republice se kon&aacute; summit.
           Premi&eacute;r Petr Fiala a prezident Petr Pavel se setkali
           v Pra&zcaron;sk&eacute;m hrad&ecaron;.</p>
    </article>"#;

    let text = deformat::html::strip_to_text(html);
    assert!(text.contains("České"), "Ccaron: {text}");
    assert!(text.contains("Pražském"), "zcaron: {text}");
    assert!(text.contains("hradě"), "ecaron: {text}");
}

// =============================================================================
// decode_entities standalone function
// =============================================================================

#[test]
fn decode_entities_multilingual() {
    assert_eq!(
        deformat::html::decode_entities("Caf&eacute; in &Lstrok;&oacute;d&zacute;"),
        "Café in Łódź"
    );
    assert_eq!(
        deformat::html::decode_entities("Erdo&gbreve;an visited &Idot;stanbul"),
        "Erdoğan visited İstanbul"
    );
    assert_eq!(
        deformat::html::decode_entities("&Ccaron;esk&aacute; &amp; Slovensk&aacute;"),
        "Česká & Slovenská"
    );
}

// =============================================================================
// Complex kitchen-sink document
// =============================================================================

#[test]
fn kitchen_sink_all_features() {
    // Tests: skip tags, entities (named + numeric + Win1252), ruby annotations,
    // tables, img alt, wiki boilerplate, bidi controls, and whitespace handling
    let html = r#"<!DOCTYPE html>
    <html><head>
        <title>Test Article</title>
        <style>body { color: black; }</style>
        <script>var x = 1;</script>
    </head>
    <body>
        <nav><a href="/">Home</a> | <a href="/about">About</a></nav>
        <header><div id="sitesub">From TestWiki</div></header>
        <article>
            <h1><ruby>東京<rt>とうきょう</rt></ruby> Summit 2026</h1>
            <div id="toc"><h2>Contents</h2><ul><li>1 Overview</li></ul></div>
            <p>Nestl&eacute; CEO Laurent Freixe met with
               <ruby>安倍<rt>あべ</rt></ruby><ruby>晋三<rt>しんぞう</rt></ruby>
               in S&atilde;o Paulo. The &#8364;5 billion deal[1] was
               signed at the B&ouml;rse Frankfurt&#146;s main hall.</p>
            <table class="infobox">
                <tr><th>Location</th><td>&Lstrok;&oacute;d&zacute;, Poland</td></tr>
                <tr><th>Date</th><td>March 15, 2026</td></tr>
            </table>
            <p>Erdo&gbreve;an and &Ccaron;esk&yacute; delegates attended.
               <img src="photo.jpg" alt="Laurent Freixe at podium">
               The atmosphere was &#x201C;historic&#x201D;.</p>
            <ol class="references">
                <li id="cite_note-1">Source: Reuters (2026).</li>
            </ol>
            <div class="navbox"><table><tr><td>Related articles</td></tr></table></div>
        </article>
        <aside><h3>Trending</h3><ul><li>Other news</li></ul></aside>
        <footer><p>&copy; 2026 TestWiki. Licensed under CC.</p></footer>
    </body></html>"#;

    use deformat::html::{strip_to_text_with_options, StripOptions};
    let text = strip_to_text_with_options(html, &StripOptions::wikipedia());

    // Content preserved
    assert!(text.contains("東京"), "CJK base text: {text}");
    assert!(text.contains("Summit 2026"), "title: {text}");
    assert!(text.contains("Nestlé"), "eacute entity: {text}");
    assert!(text.contains("Laurent Freixe"), "name: {text}");
    assert!(text.contains("安倍"), "CJK name 1: {text}");
    assert!(text.contains("晋三"), "CJK name 2: {text}");
    assert!(text.contains("São Paulo"), "atilde entity: {text}");
    assert!(text.contains("€"), "numeric entity (euro): {text}");
    assert!(text.contains("Börse"), "ouml entity: {text}");
    assert!(text.contains("Łódź"), "Latin Extended-A entities: {text}");
    assert!(text.contains("Erdoğan"), "Turkish gbreve: {text}");
    assert!(text.contains("Český"), "Czech Ccaron: {text}");
    assert!(
        text.contains("Laurent Freixe at podium"),
        "img alt text: {text}"
    );

    // Win-1252 entity: &#146; = right single quote
    assert!(text.contains('\u{2019}'), "Win1252 right quote: {text}");
    // Hex entity: curly quotes
    assert!(text.contains('\u{201C}'), "hex left curly quote: {text}");
    assert!(text.contains('\u{201D}'), "hex right curly quote: {text}");

    // Furigana stripped
    assert!(
        !text.contains("とうきょう"),
        "Tokyo furigana stripped: {text}"
    );
    assert!(!text.contains("あべ"), "Abe furigana stripped: {text}");
    assert!(
        !text.contains("しんぞう"),
        "Shinzo furigana stripped: {text}"
    );

    // Boilerplate stripped
    assert!(!text.contains("Home"), "nav stripped: {text}");
    assert!(!text.contains("About"), "nav stripped: {text}");
    assert!(!text.contains("TestWiki"), "footer stripped: {text}");
    assert!(!text.contains("Contents"), "TOC stripped: {text}");
    assert!(!text.contains("Reuters"), "references stripped: {text}");
    assert!(
        !text.contains("Related articles"),
        "navbox stripped: {text}"
    );
    assert!(!text.contains("Trending"), "aside stripped: {text}");
    assert!(!text.contains("Other news"), "aside stripped: {text}");
    assert!(!text.contains("color: black"), "style stripped: {text}");
    assert!(!text.contains("var x"), "script stripped: {text}");

    // Structural invariants
    assert!(!text.contains("  "), "no double spaces: {text}");
    assert_eq!(text, text.trim(), "output trimmed");
    assert!(!text.contains("[1]"), "wiki ref markers stripped: {text}");

    // Table cells not fused
    assert!(!text.contains("LocationŁódź"), "th-td not fused: {text}");
}

// =============================================================================
// extract_attr_value word-boundary regression
// =============================================================================

#[test]
fn data_class_does_not_hide_content() {
    // data-class="toc" must NOT trigger wiki-skip detection.
    // Only a bare class="toc" should match.
    let html = r#"<!DOCTYPE html>
    <html><body>
        <div data-class="toc" data-id="references">
            <p>This paragraph has data-class but not class. It should be visible.</p>
        </div>
        <div class="article-content">
            <p>Article body text.</p>
        </div>
    </body></html>"#;
    let text = deformat::html::strip_to_text(html);
    assert!(
        text.contains("This paragraph has data-class"),
        "data-class must not trigger wiki-skip: {text}"
    );
    assert!(text.contains("Article body"), "article preserved: {text}");
}

#[test]
fn aria_class_does_not_hide_content() {
    let html = r#"<div aria-class="navbox"><p>Visible content here.</p></div>"#;
    let text = deformat::html::strip_to_text(html);
    assert!(
        text.contains("Visible content here"),
        "aria-class must not trigger wiki-skip: {text}"
    );
}

// =============================================================================
// Extracted struct field access (non-HashMap)
// =============================================================================

#[test]
fn extracted_fields_accessible() {
    let result = extract("<p>Hello</p>").unwrap();
    assert_eq!(result.extractor, "strip");
    assert_eq!(result.format, Format::Html);
    assert!(result.title.is_none());
    assert!(result.excerpt.is_none());
    assert!(!result.fallback);
}

#[test]
fn extracted_passthrough_fields() {
    let result = extract("plain text").unwrap();
    assert_eq!(result.extractor, "passthrough");
    assert_eq!(result.format, Format::PlainText);
    assert!(!result.fallback);
}
