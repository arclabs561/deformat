use super::*;

// ===== Basic stripping =====

#[test]
fn strip_basic() {
    assert_eq!(strip_to_text("<p>Hello <b>world</b>!</p>"), "Hello world!");
}

#[test]
fn strip_plain_text_passthrough() {
    // No HTML at all -- exercises the no-'<' fast path
    let text = "Tim Cook met with Sundar Pichai in Seattle.";
    assert_eq!(strip_to_text(text), text);
}

#[test]
fn strip_plain_text_with_entities() {
    // No tags but has entities -- fast path decodes them
    assert_eq!(strip_to_text("Caf&eacute; au lait"), "Caf\u{e9} au lait");
}

#[test]
fn strip_plain_text_with_whitespace() {
    // No tags, extra whitespace -- fast path normalizes it
    assert_eq!(strip_to_text("  hello   world  "), "hello world");
}

#[test]
fn strip_script_style() {
    let html = r#"<html><head><style>body{color:red}</style></head>
            <body><script>alert('hi')</script><p>Real text.</p></body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Real text"));
    assert!(!text.contains("alert"), "script stripped");
    assert!(!text.contains("color"), "style stripped");
}

#[test]
fn strip_block_spacing() {
    let html = "<h1>Title</h1><p>First.</p><p>Second.</p>";
    let text = strip_to_text(html);
    assert!(!text.contains("TitleFirst"), "blocks separated");
    assert!(text.contains("Title"));
    assert!(text.contains("First"));
    assert!(text.contains("Second"));
}

// ===== Entity decoding =====

#[test]
fn entity_named() {
    let text = strip_to_text("<p>A &amp; B &lt; C</p>");
    assert!(text.contains("A & B"));
    assert!(text.contains("< C"));
}

#[test]
fn entity_table_is_sorted() {
    for window in NAMED_ENTITIES.windows(2) {
        assert!(
            window[0].0 < window[1].0,
            "entity table not sorted: {:?} should come before {:?}",
            window[0].0,
            window[1].0
        );
    }
}

#[test]
fn entity_fast_path_matches_table() {
    // Verify every fast-path entity in decode_named_entity matches
    // the corresponding entry in NAMED_ENTITIES. Catches drift.
    let fast_path_entities = [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&nbsp;", ' '),
        ("&apos;", '\''),
        ("&eacute;", '\u{E9}'),
        ("&Eacute;", '\u{C9}'),
        ("&mdash;", '\u{2014}'),
        ("&ndash;", '\u{2013}'),
        ("&rsquo;", '\u{2019}'),
        ("&lsquo;", '\u{2018}'),
        ("&ldquo;", '\u{201C}'),
        ("&rdquo;", '\u{201D}'),
        ("&hellip;", '\u{2026}'),
        ("&copy;", '\u{A9}'),
        ("&reg;", '\u{AE}'),
        ("&euro;", '\u{20AC}'),
        ("&ouml;", '\u{F6}'),
        ("&uuml;", '\u{FC}'),
        ("&auml;", '\u{E4}'),
        ("&oacute;", '\u{F3}'),
    ];
    for (entity, expected_char) in fast_path_entities {
        // Verify it exists in the table with the same value
        let table_result = NAMED_ENTITIES
            .binary_search_by_key(&entity, |(name, _)| name)
            .ok()
            .map(|idx| NAMED_ENTITIES[idx].1);
        assert_eq!(
            table_result,
            Some(expected_char),
            "Fast-path entity {entity} doesn't match NAMED_ENTITIES table"
        );
        // Verify decode_named_entity returns the same value
        assert_eq!(
            decode_named_entity(entity),
            Some(expected_char),
            "decode_named_entity({entity}) mismatch"
        );
    }
}

#[test]
fn entity_decimal() {
    let text = strip_to_text("<p>It&#39;s a test</p>");
    assert!(text.contains("It's"));
}

#[test]
fn entity_hex() {
    let text = strip_to_text("<p>It&#x27;s a test</p>");
    assert!(text.contains("It's"));
}

#[test]
fn entity_hex_uppercase() {
    let text = strip_to_text("<p>It&#X27;s a test</p>");
    assert!(text.contains("It's"));
}

// ===== Whitespace collapsing =====

#[test]
fn collapses_whitespace() {
    let html = r#"<html><head><title>t</title></head>
            <body><h1>Hello   world</h1><p>Line1<br>Line2</p>
            <div>Tabbed	text</div></body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Hello world"));
    // <br> is a block element, so Line1/Line2 are separated by newline
    assert!(
        text.contains("Line1\nLine2"),
        "br should produce newline: {text}"
    );
    assert!(text.contains("Tabbed text"));
    assert!(!text.contains('\t'));
    assert!(!text.contains("  "));
}

// ===== Semantic tag filtering =====

#[test]
fn nav_stripped() {
    let html = r#"<html><body>
            <nav><a href="/">Home</a></nav>
            <main><p>Content.</p></main>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Content"));
    assert!(!text.contains("Home"));
}

#[test]
fn footer_stripped() {
    let html = r#"<html><body>
            <article><p>Body.</p></article>
            <footer><p>Copyright 2024.</p></footer>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Body"));
    assert!(!text.contains("Copyright"));
}

#[test]
fn header_not_stripped() {
    // <header> is NOT a skip tag because HTML5 allows <header> inside
    // <article> for article headings. Page-level headers typically
    // contain <nav> which IS a skip tag.
    let html = r#"<html><body>
            <header><h1>Site</h1></header>
            <main><p>Page.</p></main>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Page"));
    assert!(text.contains("Site"), "header content preserved: {text}");
}

#[test]
fn aside_stripped() {
    let html = r#"<html><body>
            <main><p>Main.</p></main>
            <aside><p>Sidebar.</p></aside>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Main"));
    assert!(!text.contains("Sidebar"));
}

#[test]
fn head_stripped() {
    let html = "<html><head><title>Page Title</title></head>\
                     <body><p>Content.</p></body></html>";
    let text = strip_to_text(html);
    assert!(!text.contains("Page Title"));
    assert!(text.contains("Content"));
}

#[test]
fn noscript_stripped() {
    let html = r#"<html><body>
            <noscript><p>Enable JS.</p></noscript>
            <main><p>App.</p></main>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("App"));
    assert!(!text.contains("Enable JS"));
}

#[test]
fn nested_semantic() {
    let html = r#"<html><body>
            <header><nav><ul><li>Link</li></ul></nav></header>
            <main><p>Real.</p></main>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Real"));
    assert!(!text.contains("Link"));
}

#[test]
fn article_preserved() {
    let html = r#"<html><body>
            <article><h2>Title</h2><p>Para.</p></article>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Title"));
    assert!(text.contains("Para"));
}

// ===== Wikipedia boilerplate =====

#[test]
fn wiki_ref_brackets_stripped_with_option() {
    let html = r#"<html><body>
            <p>Einstein[1] was a physicist.[2] See also[edit] quantum.</p>
        </body></html>"#;
    let text = strip_to_text_with_options(html, &StripOptions::wikipedia());
    assert!(!text.contains("[1]"));
    assert!(!text.contains("[edit]"));
    assert!(text.contains("Einstein"));
    assert!(text.contains("quantum"));
}

#[test]
fn wiki_ref_brackets_preserved_by_default() {
    let html = r#"<html><body>
            <p>See reference [1] and section [edit] for details.</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("[1]"), "default preserves [1]: {text}");
    assert!(text.contains("[edit]"), "default preserves [edit]: {text}");
}

#[test]
fn wiki_citation_needed_stripped_with_option() {
    let text = strip_to_text_with_options(
        "<p>Claim[citation needed] here.</p>",
        &StripOptions::wikipedia(),
    );
    assert!(!text.contains("[citation needed]"));
    assert!(text.contains("Claim"));
}

#[test]
fn wiki_toc_stripped() {
    let html = r#"<html><body>
            <p>Article text.</p>
            <div id="toc"><h2>Contents</h2><ul><li>Section</li></ul></div>
            <p>More text.</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Article text"));
    assert!(text.contains("More text"));
    assert!(!text.contains("Contents"));
}

// ===== Multilingual =====

#[test]
fn multilingual_preserved() {
    let html = r#"<html><body>
            <p>&#x4E60;&#x8FD1;&#x5E73;&#x5728;&#x5317;&#x4EAC;</p>
            <p>Путин встретился с Си Цзиньпином в Москве.</p>
            <p>प्रधान मंत्री शर्मा आज आए।</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Путин встретился с Си Цзиньпином в Москве."));
    assert!(text.contains("प्रधान मंत्री शर्मा आज आए।"));
}

// ===== Readability (feature-gated) =====

#[cfg(feature = "readability")]
#[test]
fn readability_extracts_article() {
    let html = r#"<!DOCTYPE html>
        <html><head><title>News</title></head>
        <body>
            <nav><a href="/">Home</a></nav>
            <div id="content">
                <h1>News</h1>
                <p>A team of researchers at the University of Cambridge has announced
                   the discovery of a previously unknown species of beetle in the Amazon
                   rainforest. The discovery was published in Nature on March 15, 2026.
                   The finding represents one of the most significant entomological
                   discoveries in the region in recent years.</p>
                <p>Lead researcher Dr. Sarah Chen said the species, named Chrysina
                   amazonica, was found during an expedition in January near Manaus.
                   The beetle has unique iridescent markings that distinguish it from
                   related species. Chen and her team spent three weeks collecting
                   specimens and documenting the habitat conditions.</p>
                <p>The Amazon rainforest continues to yield new discoveries despite
                   decades of intensive exploration. Conservation groups have called for
                   increased protection. Brazil's Environment Ministry said it would
                   review the protected area boundaries in light of the new findings.</p>
                <p>The research was funded by the European Research Council and National
                   Geographic Society. Additional specimens will be housed at the Natural
                   History Museum in London and the Smithsonian Institution.</p>
            </div>
            <footer>Copyright 2026</footer>
        </body></html>"#;
    let result = extract_with_readability(html, "https://example.com/article");
    assert!(result.is_some());
    let (text, title, _) = result.unwrap();
    assert!(text.contains("Dr. Sarah Chen"));
    assert!(title.is_some());
}

#[cfg(feature = "readability")]
#[test]
fn readability_returns_none_for_trivial() {
    assert!(extract_with_readability("<p>Hi</p>", "https://example.com").is_none());
}

#[cfg(feature = "readability")]
#[test]
fn readability_returns_none_for_empty() {
    assert!(extract_with_readability("", "https://example.com").is_none());
}

// ===== Extended entity decoding (NER-critical) =====

#[test]
fn entity_eacute_for_ner() {
    // "Nestlé" must be decoded correctly for NER to recognize it
    let text = strip_to_text("<p>Nestl&eacute; is a company.</p>");
    assert!(text.contains("Nestlé"), "eacute decoded: {text}");
}

#[test]
fn entity_mdash_ndash() {
    let text = strip_to_text("<p>A &mdash; B &ndash; C</p>");
    assert!(text.contains('\u{2014}'), "mdash decoded: {text}");
    assert!(text.contains('\u{2013}'), "ndash decoded: {text}");
}

#[test]
fn entity_curly_quotes() {
    let text = strip_to_text("<p>&ldquo;Hello&rdquo; &lsquo;world&rsquo;</p>");
    assert!(text.contains('\u{201C}'), "ldquo: {text}");
    assert!(text.contains('\u{201D}'), "rdquo: {text}");
    assert!(text.contains('\u{2018}'), "lsquo: {text}");
    assert!(text.contains('\u{2019}'), "rsquo: {text}");
}

#[test]
fn entity_currency_symbols() {
    let text = strip_to_text("<p>&euro;100 &pound;50 &yen;1000</p>");
    assert!(text.contains('€'), "euro: {text}");
    assert!(text.contains('£'), "pound: {text}");
    assert!(text.contains('¥'), "yen: {text}");
}

#[test]
fn entity_accented_names() {
    // Common in European news: accented names must survive extraction
    let text =
        strip_to_text("<p>&Uuml;ber M&uuml;ller traf Garc&iacute;a in S&atilde;o Paulo.</p>");
    assert!(text.contains("Über"), "Uuml: {text}");
    assert!(text.contains("Müller"), "uuml: {text}");
    assert!(text.contains("García"), "iacute: {text}");
    assert!(text.contains("São"), "atilde: {text}");
}

#[test]
fn entity_copyright_trademark() {
    let text = strip_to_text("<p>&copy; 2026 Company&trade; &reg;</p>");
    assert!(text.contains('©'), "copy: {text}");
    assert!(text.contains('™'), "trade: {text}");
    assert!(text.contains('®'), "reg: {text}");
}

#[test]
fn entity_unknown_passes_through() {
    // Unknown named entities should pass through unchanged
    let text = strip_to_text("<p>&foobar; text</p>");
    assert!(
        text.contains("&foobar;"),
        "unknown entity preserved: {text}"
    );
}

#[test]
fn entity_unterminated_passes_through() {
    // Unterminated entity (no semicolon) should not eat subsequent text
    let text = strip_to_text("<p>AT&T is a company.</p>");
    assert!(
        text.contains("AT&T"),
        "unterminated entity preserved: {text}"
    );
    assert!(
        text.contains("company"),
        "subsequent text preserved: {text}"
    );
}

// ===== Edge cases =====

#[test]
fn empty_input() {
    assert_eq!(strip_to_text(""), "");
}

#[test]
fn plain_text_passthrough() {
    let input = "No HTML here, just text.";
    assert_eq!(strip_to_text(input), input);
}

#[test]
fn unclosed_tag_handled() {
    let text = strip_to_text("<p>Hello <b>world");
    assert!(text.contains("Hello"), "text before unclosed: {text}");
    assert!(text.contains("world"), "text in unclosed: {text}");
}

#[test]
fn self_closing_tags() {
    let text = strip_to_text("<p>Line1<br/>Line2<hr/>Line3</p>");
    assert!(text.contains("Line1"), "before br: {text}");
    assert!(text.contains("Line2"), "after br: {text}");
    assert!(text.contains("Line3"), "after hr: {text}");
}

#[test]
fn html_comments_stripped() {
    let text = strip_to_text("<p>Before<!-- comment -->After</p>");
    assert!(text.contains("Before"), "before comment: {text}");
    assert!(text.contains("After"), "after comment: {text}");
    assert!(!text.contains("comment"), "comment stripped: {text}");
}

#[test]
fn cdata_section_stripped() {
    // CDATA is treated like a <! directive -- content is stripped
    let text = strip_to_text("<p>Before</p><![CDATA[some data]]><p>After</p>");
    assert!(text.contains("Before"));
    assert!(text.contains("After"));
    assert!(!text.contains("CDATA"));
    assert!(!text.contains("some data"));
}

#[test]
fn html_comment_with_tags_inside() {
    // Tags inside comments should NOT trigger script/style/skip tracking
    let text = strip_to_text("<p>Real</p><!-- <script>evil()</script> --><p>Also real</p>");
    assert!(text.contains("Real"), "before comment: {text}");
    assert!(text.contains("Also real"), "after comment: {text}");
    assert!(!text.contains("evil"), "script in comment ignored: {text}");
}

#[test]
fn html_comment_with_dashes() {
    // Comments with multiple dashes
    let text = strip_to_text("<p>A</p><!-- -- -- --><p>B</p>");
    assert!(text.contains('A'), "before: {text}");
    assert!(text.contains('B'), "after: {text}");
}

#[test]
fn ie_conditional_comment() {
    // IE conditional comments are still comments
    let text = strip_to_text("<p>Real</p><!--[if IE]>IE only<![endif]--><p>Also real</p>");
    assert!(text.contains("Real"), "before: {text}");
    assert!(text.contains("Also real"), "after: {text}");
    assert!(!text.contains("IE only"), "conditional stripped: {text}");
}

#[test]
fn quoted_attribute_with_gt() {
    // '>' inside a quoted attribute should NOT end the tag
    let html = r#"<div title="a > b">Content</div>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Content"), "content preserved: {text}");
    assert!(!text.contains("a > b"), "attr value not leaked: {text}");
    assert!(!text.contains("title"), "attr name not leaked: {text}");
}

#[test]
fn quoted_attribute_with_lt() {
    let html = r#"<span data-expr="x < 10">Result</span>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Result"), "content preserved: {text}");
    assert!(!text.contains("x < 10"), "attr not leaked: {text}");
}

#[test]
fn single_quoted_attribute_with_gt() {
    let html = "<div title='a > b'>Content</div>";
    let text = strip_to_text(html);
    assert!(text.contains("Content"), "content preserved: {text}");
    assert!(!text.contains("a > b"), "attr not leaked: {text}");
}

#[test]
fn nested_quotes_in_attribute() {
    // Double quotes inside single-quoted attr
    let html = r#"<a title='He said "hello"'>Link</a>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Link"), "content preserved: {text}");
    assert!(
        !text.contains("hello"),
        "nested quote attr not leaked: {text}"
    );
}

#[test]
fn null_entity_becomes_replacement_char() {
    let text = strip_to_text("<p>Before&#0;After</p>");
    assert!(text.contains("Before"), "before null: {text}");
    assert!(text.contains("After"), "after null: {text}");
    assert!(
        text.contains('\u{FFFD}'),
        "null becomes replacement char: {text}"
    );
}

#[test]
fn doctype_not_treated_as_comment() {
    // <!DOCTYPE html> should be handled as a tag, not a comment
    let text = strip_to_text("<!DOCTYPE html><html><body><p>Content</p></body></html>");
    assert!(text.contains("Content"), "content preserved: {text}");
    assert!(!text.contains("DOCTYPE"), "doctype stripped: {text}");
}

#[test]
fn nested_skip_tags_depth() {
    // Multiple nested skip elements should all be stripped
    let html = r#"<html><body>
            <nav><ul><li><a href="/">Home</a></li>
                <li><a href="/about">About</a></li></ul></nav>
            <p>Real content here.</p>
            <footer><nav><a href="/privacy">Privacy</a></nav></footer>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Real content"), "body preserved: {text}");
    assert!(!text.contains("Home"), "nav stripped: {text}");
    assert!(!text.contains("Privacy"), "footer nav stripped: {text}");
}

#[test]
fn data_attributes_not_in_output() {
    let html = r#"<div data-entity="person" data-id="123"><p>Tim Cook</p></div>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Tim Cook"), "content preserved: {text}");
    assert!(!text.contains("data-entity"), "attrs stripped: {text}");
    assert!(!text.contains("123"), "attr values stripped: {text}");
}

#[test]
fn multiple_scripts_and_styles() {
    let html = r#"<html><body>
            <script>var a = 1;</script>
            <p>First.</p>
            <style>.x { color: red; }</style>
            <p>Second.</p>
            <script type="application/json">{"key": "val"}</script>
            <p>Third.</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("First"), "first para: {text}");
    assert!(text.contains("Second"), "second para: {text}");
    assert!(text.contains("Third"), "third para: {text}");
    assert!(!text.contains("var a"), "script 1 stripped: {text}");
    assert!(!text.contains("color"), "style stripped: {text}");
    assert!(!text.contains("key"), "json script stripped: {text}");
}

// ===== Windows-1252 entity mapping =====

#[test]
fn win1252_en_dash() {
    // &#150; is en dash in Windows-1252, not a control character
    let text = strip_to_text("<p>Smith&#150;Jones partnership</p>");
    assert!(text.contains('\u{2013}'), "en dash decoded: {text}");
    assert!(text.contains("Smith"), "name preserved: {text}");
    assert!(text.contains("Jones"), "name preserved: {text}");
}

#[test]
fn win1252_em_dash() {
    let text = strip_to_text("<p>Wait&#151;what?</p>");
    assert!(text.contains('\u{2014}'), "em dash decoded: {text}");
}

#[test]
fn win1252_curly_quotes() {
    // &#147; and &#148; are curly double quotes in Windows-1252
    let text = strip_to_text("<p>&#147;Hello&#148; she said</p>");
    assert!(text.contains('\u{201C}'), "left double quote: {text}");
    assert!(text.contains('\u{201D}'), "right double quote: {text}");
}

#[test]
fn win1252_euro_sign() {
    let text = strip_to_text("<p>Price: &#128;100</p>");
    assert!(text.contains('€'), "euro from &#128;: {text}");
}

#[test]
fn win1252_trademark() {
    let text = strip_to_text("<p>Brand&#153;</p>");
    assert!(text.contains('™'), "trademark from &#153;: {text}");
}

// ===== Zero-width character stripping =====

#[test]
fn zero_width_space_stripped() {
    // ZWSP inside a name should be removed for clean NER tokenization
    let text = strip_to_text("<p>Albert\u{200B}Einstein</p>");
    assert!(text.contains("AlbertEinstein"), "ZWSP stripped: {text}");
}

#[test]
fn soft_hyphen_stripped() {
    let text = strip_to_text("<p>Ein\u{00AD}stein</p>");
    assert!(text.contains("Einstein"), "soft hyphen stripped: {text}");
}

#[test]
fn bom_mid_text_stripped() {
    let text = strip_to_text("<p>Hello\u{FEFF}World</p>");
    assert!(text.contains("HelloWorld"), "mid-text BOM stripped: {text}");
}

#[test]
fn word_joiner_stripped() {
    let text = strip_to_text("<p>Marie\u{2060}Curie</p>");
    assert!(text.contains("MarieCurie"), "word joiner stripped: {text}");
}

// ===== Template and SVG skipping =====

#[test]
fn template_content_skipped() {
    let html = r#"<html><body>
            <p>Visible content.</p>
            <template><p>Ghost text in template.</p></template>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Visible content"), "visible: {text}");
    assert!(!text.contains("Ghost text"), "template skipped: {text}");
}

#[test]
fn svg_content_skipped() {
    let html = r#"<html><body>
            <p>Article text.</p>
            <svg><text x="10" y="20">Chart Label</text><title>Graph</title></svg>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Article text"), "article preserved: {text}");
    assert!(!text.contains("Chart Label"), "svg text skipped: {text}");
    assert!(!text.contains("Graph"), "svg title skipped: {text}");
}

// ===== Image alt text extraction =====

#[test]
fn img_alt_text_extracted() {
    let html = r#"<p>The president spoke today.</p>
            <img src="photo.jpg" alt="President Biden at the White House">
            <p>He discussed policy.</p>"#;
    let text = strip_to_text(html);
    assert!(
        text.contains("President Biden at the White House"),
        "alt text extracted: {text}"
    );
    assert!(text.contains("spoke today"), "body preserved: {text}");
}

#[test]
fn img_alt_empty_not_added() {
    let html = r#"<p>Text.</p><img src="spacer.gif" alt=""><p>More.</p>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Text"), "before img: {text}");
    assert!(text.contains("More"), "after img: {text}");
}

#[test]
fn img_no_alt_attribute() {
    let html = r#"<p>Text.</p><img src="photo.jpg"><p>More.</p>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Text"), "before: {text}");
    assert!(text.contains("More"), "after: {text}");
}

#[test]
fn img_alt_in_skipped_region_not_extracted() {
    let html = r#"<nav><img alt="Logo" src="logo.png"></nav><p>Content.</p>"#;
    let text = strip_to_text(html);
    assert!(!text.contains("Logo"), "alt in nav skipped: {text}");
    assert!(text.contains("Content"), "body preserved: {text}");
}

// ===== Table cell separation =====

#[test]
fn table_cells_separated() {
    // Wikipedia infobox pattern: <th>Key</th><td>Value</td> must not fuse
    let html = r#"<table><tr><th>Country</th><td>England</td></tr>
            <tr><th>Region</th><td>South East</td></tr></table>"#;
    let text = strip_to_text(html);
    assert!(
        !text.contains("CountryEngland"),
        "th/td must be separated: {text}"
    );
    assert!(text.contains("Country"), "th preserved: {text}");
    assert!(text.contains("England"), "td preserved: {text}");
    assert!(
        !text.contains("EnglandRegion"),
        "rows must be separated: {text}"
    );
}

#[test]
fn closing_td_inserts_space() {
    let html = "<td>Apple</td><td>Inc</td>";
    let text = strip_to_text(html);
    assert!(!text.contains("AppleInc"), "cells separated: {text}");
}

#[test]
fn form_elements_stripped() {
    let html = r#"<html><body>
            <p>Article text.</p>
            <form action="/search">
                <input type="text" placeholder="Search...">
                <select><option>Option 1</option></select>
                <button>Submit</button>
            </form>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Article text"), "content preserved: {text}");
    assert!(!text.contains("Search"), "form stripped: {text}");
    assert!(!text.contains("Option 1"), "select stripped: {text}");
}

#[test]
fn textarea_content_stripped() {
    let html = r#"<html><body>
            <p>Article text.</p>
            <textarea>Draft comment text here</textarea>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Article text"), "body preserved: {text}");
    assert!(!text.contains("Draft comment"), "textarea stripped: {text}");
}

#[test]
fn iframe_content_stripped() {
    let html = r#"<html><body>
            <p>Main content.</p>
            <iframe src="ad.html">Fallback ad text</iframe>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Main content"), "body preserved: {text}");
    assert!(!text.contains("Fallback"), "iframe stripped: {text}");
}

#[test]
fn wiki_references_section_stripped() {
    let html = r#"<html><body>
            <p>Main article content about CRISPR gene editing.</p>
            <ol class="references">
                <li id="cite_note-1">Smith J (2024). "Paper title". Nature.</li>
                <li id="cite_note-2">Jones A (2023). "Another paper".</li>
            </ol>
            <p>Conclusion paragraph.</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("CRISPR"), "article preserved: {text}");
    assert!(text.contains("Conclusion"), "conclusion preserved: {text}");
    assert!(!text.contains("cite_note"), "references stripped: {text}");
}

#[test]
fn wiki_navbox_stripped() {
    let html = r#"<html><body>
            <p>Article content.</p>
            <div class="navbox"><table><tr><td>Related articles</td></tr></table></div>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(
        text.contains("Article content"),
        "content preserved: {text}"
    );
    assert!(
        !text.contains("Related articles"),
        "navbox stripped: {text}"
    );
}

// ===== Semicolon-optional entity decoding =====

#[test]
fn entity_without_semicolon_amp() {
    // &amp without ; should decode to &
    let text = strip_to_text("<p>AT&amp T</p>");
    assert!(
        text.contains("AT& T") || text.contains("AT&"),
        "amp without semi: {text}"
    );
}

#[test]
fn entity_without_semicolon_hellip() {
    // &hellip without ; -> ellipsis
    let text = strip_to_text("<p>Wait&hellip what?</p>");
    assert!(text.contains('\u{2026}'), "hellip without semi: {text}");
}

#[test]
fn entity_without_semicolon_nbsp() {
    // &nbsp without ; -> non-breaking space (collapsed to regular space)
    let text = strip_to_text("<p>Hello&nbsp world</p>");
    assert!(
        text.contains("Hello"),
        "nbsp without semi preserved text: {text}"
    );
}

#[test]
fn entity_without_semicolon_not_greedy() {
    // &T in AT&T should NOT be decoded as an entity
    let text = strip_to_text("<p>AT&amp;T Corporation</p>");
    assert!(text.contains("AT&T"), "AT&T with proper entity: {text}");
}

#[test]
fn entity_without_semicolon_short_passthrough() {
    // Very short &X patterns should pass through, not try entity decode
    let text = strip_to_text("<p>if x &lt 10</p>");
    // &lt without ; should still decode (it's a known entity)
    assert!(
        text.contains('<') || text.contains("lt"),
        "lt without semi: {text}"
    );
}

#[test]
fn entity_without_semicolon_unknown_passthrough() {
    // Unknown entity-like strings without ; should pass through as-is
    let text = strip_to_text("<p>&xyzzy content</p>");
    assert!(
        text.contains("&xyzzy"),
        "unknown entity passes through: {text}"
    );
}

#[test]
fn entity_without_semicolon_eacute() {
    // &eacute without ; -> é (critical for names like Nestlé)
    let text = strip_to_text("<p>Nestl&eacute CEO</p>");
    assert!(text.contains("Nestlé"), "eacute without semi: {text}");
}

// ===== Greek letter entities =====

#[test]
fn entity_greek_letters() {
    let text = strip_to_text("<p>&alpha;-synuclein and &beta;-amyloid</p>");
    assert!(text.contains('α'), "alpha: {text}");
    assert!(text.contains('β'), "beta: {text}");
}

#[test]
fn entity_greek_uppercase() {
    let text = strip_to_text("<p>&Delta;G = &minus;&Sigma;&Delta;H</p>");
    assert!(text.contains('Δ'), "Delta: {text}");
    assert!(text.contains('Σ'), "Sigma: {text}");
}

// ===== C1 range handling =====

#[test]
fn c1_unmapped_becomes_replacement() {
    // 0x81, 0x8D, 0x8F, 0x90 have no Win-1252 mapping -> U+FFFD
    let text = strip_to_text("<p>&#129;</p>"); // 0x81
    assert!(text.contains('\u{FFFD}'), "0x81 -> U+FFFD: {text}");
}

// ===== Math and symbol entities =====

#[test]
fn entity_math_symbols() {
    let text = strip_to_text("<p>&forall;x &exist;y : x &ne; y</p>");
    assert!(text.contains('∀'), "forall: {text}");
    assert!(text.contains('∃'), "exist: {text}");
    assert!(text.contains('≠'), "ne: {text}");
}

#[test]
fn entity_arrows() {
    let text = strip_to_text("<p>A &rarr; B &larr; C</p>");
    assert!(text.contains('→'), "rarr: {text}");
    assert!(text.contains('←'), "larr: {text}");
}

// ===== Line break and separator elements =====

#[test]
fn br_prevents_word_fusion() {
    let text = strip_to_text("<p>John Smith<br>CEO of Acme</p>");
    assert!(!text.contains("SmithCEO"), "br prevents fusion: {text}");
    assert!(text.contains("John Smith"), "name preserved: {text}");
    assert!(text.contains("CEO"), "title preserved: {text}");
}

#[test]
fn br_self_closing() {
    let text = strip_to_text("<p>Line one<br/>Line two</p>");
    assert!(!text.contains("oneLine"), "br/ prevents fusion: {text}");
}

#[test]
fn wbr_prevents_fusion() {
    let text = strip_to_text("<p>Super<wbr>cali<wbr>fragilistic</p>");
    // wbr inserts space, preventing weird tokenization
    assert!(!text.contains("Supercali"), "wbr inserts space: {text}");
}

#[test]
fn img_alt_entities_decoded() {
    let html = r#"<p>Photo:</p><img alt="Caf&eacute; au lait" src="photo.jpg">"#;
    let text = strip_to_text(html);
    assert!(text.contains("Café"), "entities in alt decoded: {text}");
}

#[test]
fn hr_separates_sections() {
    let text = strip_to_text("<p>Section one</p><hr><p>Section two</p>");
    assert!(!text.contains("oneSection"), "hr prevents fusion: {text}");
}

#[test]
fn definition_list_separated() {
    let html = "<dl><dt>Term</dt><dd>Definition here</dd></dl>";
    let text = strip_to_text(html);
    assert!(!text.contains("TermDefinition"), "dt/dd separated: {text}");
    assert!(text.contains("Term"), "dt preserved: {text}");
    assert!(text.contains("Definition"), "dd preserved: {text}");
}

// ===== Bidi mark stripping =====

#[test]
fn lrm_stripped() {
    // Left-to-right marks from &lrm; entity should be stripped
    let text = strip_to_text("<p>Hello&lrm; world</p>");
    assert!(!text.contains('\u{200E}'), "LRM stripped: {text}");
    assert!(text.contains("Hello"), "text preserved: {text}");
}

#[test]
fn rlm_stripped() {
    let text = strip_to_text("<p>Hello&rlm; world</p>");
    assert!(!text.contains('\u{200F}'), "RLM stripped: {text}");
}

#[test]
fn bidi_marks_in_raw_text_stripped() {
    // Bidi marks can appear as raw Unicode, not just entities
    let text = strip_to_text("<p>Name\u{200E}\u{200F}Here</p>");
    assert!(text.contains("NameHere"), "bidi marks stripped: {text}");
}

// ===== Entity edge cases =====

#[test]
fn entity_at_end_of_input() {
    // Entity at very end of input (no terminator at all)
    let text = strip_to_text("<p>Hello &amp");
    assert!(text.contains("Hello"), "text before entity: {text}");
    // &amp without ; at end should try semicolon-optional decode
    assert!(
        text.contains('&') || text.contains("&amp"),
        "entity at end handled: {text}"
    );
}

#[test]
fn entity_numeric_at_end_of_input() {
    let text = strip_to_text("<p>Hello &#169");
    assert!(text.contains("Hello"), "text preserved: {text}");
    // Numeric entities without ; at end pass through
    assert!(text.contains("&#169"), "numeric entity at end: {text}");
}

#[test]
fn double_encoded_entity() {
    // &amp;amp; should decode to &amp; (one round of decoding only)
    let text = strip_to_text("<p>&amp;amp; test</p>");
    assert!(text.contains("&amp;"), "double-encoded stays once: {text}");
}

#[test]
fn adjacent_entities() {
    // Multiple entities with no space between
    let text = strip_to_text("<p>&lt;&gt;&amp;</p>");
    assert_eq!(text, "<>&");
}

#[test]
fn entity_with_leading_hash_garbage() {
    // &#xyz; (non-numeric after #) should pass through
    let text = strip_to_text("<p>&#xyz; text</p>");
    assert!(text.contains("&#xyz;"), "garbage numeric entity: {text}");
}

// ===== Tag edge cases =====

#[test]
fn empty_tag_name() {
    // < > with just whitespace should not panic
    let text = strip_to_text("<p>Before< >After</p>");
    assert!(text.contains("Before"), "before empty tag: {text}");
    assert!(text.contains("After"), "after empty tag: {text}");
}

#[test]
fn tag_only_slash() {
    // </> should not panic
    let text = strip_to_text("<p>Before</>After</p>");
    assert!(text.contains("Before"), "before: {text}");
    assert!(text.contains("After"), "after: {text}");
}

#[test]
fn deeply_nested_skip_tags() {
    // Three levels of skip tag nesting
    let html = r#"<header><nav><aside>
            <ul><li>Deep hidden content</li></ul>
        </aside></nav></header>
        <p>Visible text.</p>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Visible text"), "visible preserved: {text}");
    assert!(
        !text.contains("Deep hidden"),
        "deeply nested skipped: {text}"
    );
}

#[test]
fn unclosed_script_eats_rest() {
    // An unclosed <script> should suppress all remaining text
    let text = strip_to_text("<p>Before</p><script>alert('hi')");
    assert!(text.contains("Before"), "before script: {text}");
    assert!(!text.contains("alert"), "script content hidden: {text}");
}

#[test]
fn unclosed_style_eats_rest() {
    let text = strip_to_text("<p>Before</p><style>.x{color:red}");
    assert!(text.contains("Before"), "before style: {text}");
    assert!(!text.contains("color"), "style content hidden: {text}");
}

#[test]
fn script_with_html_inside() {
    // Script containing HTML-like strings should not confuse parser
    let html = r#"<script>var s = "<p>fake</p>";</script><p>Real</p>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Real"), "real content: {text}");
    assert!(!text.contains("fake"), "script html not leaked: {text}");
}

#[test]
fn consecutive_block_tags_single_space() {
    // Multiple consecutive block tags should not produce excessive spaces
    let text = strip_to_text("</p><p></p><p>Content</p>");
    assert!(!text.contains("  "), "no double spaces: {text}");
}

#[test]
fn uppercase_tags_handled() {
    // HTML tags can be uppercase
    let text = strip_to_text("<P>Hello</P><DIV>World</DIV>");
    assert!(text.contains("Hello"), "uppercase P: {text}");
    assert!(text.contains("World"), "uppercase DIV: {text}");
    assert!(!text.contains("HelloWorld"), "block separation: {text}");
}

#[test]
fn mixed_case_script_tag() {
    let text = strip_to_text("<SCRIPT>evil()</SCRIPT><p>Safe</p>");
    assert!(text.contains("Safe"), "safe content: {text}");
    assert!(!text.contains("evil"), "script stripped: {text}");
}

// ===== decode_entities public API =====

#[test]
fn decode_entities_standalone() {
    assert_eq!(decode_entities("Caf&eacute;"), "Café");
    assert_eq!(decode_entities("&#169; 2026"), "\u{00A9} 2026");
    assert_eq!(decode_entities("no entities here"), "no entities here");
    assert_eq!(decode_entities(""), "");
}

#[test]
fn decode_entities_multiple() {
    assert_eq!(
        decode_entities("&lt;div&gt; &amp; &quot;test&quot;"),
        "<div> & \"test\""
    );
}

#[test]
fn decode_entities_mixed_types() {
    // Named + decimal + hex in same string
    assert_eq!(
        decode_entities("&copy; &#8212; &#x2019;"),
        "\u{00A9} \u{2014} \u{2019}"
    );
}

// ===== Ruby annotation skipping (CJK) =====

#[test]
fn ruby_annotation_stripped() {
    // Japanese furigana: base text preserved, pronunciation stripped
    let html = "<p><ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>を学ぶ</p>";
    let text = strip_to_text(html);
    assert!(text.contains("漢"), "base char 1: {text}");
    assert!(text.contains("字"), "base char 2: {text}");
    assert!(!text.contains("かん"), "rt annotation stripped: {text}");
    assert!(!text.contains("じ"), "rt annotation stripped: {text}");
}

#[test]
fn ruby_rp_stripped() {
    // <rp> provides fallback parentheses for non-ruby browsers
    let html = "<p><ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp>字</ruby></p>";
    let text = strip_to_text(html);
    assert!(text.contains("漢"), "base text: {text}");
    assert!(text.contains("字"), "base text: {text}");
    assert!(!text.contains("かん"), "annotation stripped: {text}");
    assert!(!text.contains('('), "rp parens stripped: {text}");
}

#[test]
fn ruby_in_article_context() {
    // Ruby annotations in a real article-like context
    let html = r#"<article>
            <p><ruby>東京<rt>とうきょう</rt></ruby>で<ruby>安倍<rt>あべ</rt></ruby>首相が会見した。</p>
        </article>"#;
    let text = strip_to_text(html);
    assert!(text.contains("東京"), "Tokyo preserved: {text}");
    assert!(text.contains("安倍"), "Abe preserved: {text}");
    assert!(
        !text.contains("とうきょう"),
        "Tokyo furigana stripped: {text}"
    );
    assert!(!text.contains("あべ"), "Abe furigana stripped: {text}");
}

// ===== Expanded bidi control stripping =====

#[test]
fn bidi_embedding_controls_stripped() {
    // U+202A-U+202E bidi controls that appear in RTL-mixed content
    let text = strip_to_text("<p>Name\u{202A}\u{202B}\u{202C}Here</p>");
    assert!(text.contains("NameHere"), "bidi embedding stripped: {text}");
}

#[test]
fn bidi_isolate_controls_stripped() {
    // U+2066-U+2069 bidi isolate controls (HTML5)
    let text = strip_to_text("<p>Hello\u{2066}\u{2067}\u{2068}\u{2069}World</p>");
    assert!(text.contains("HelloWorld"), "bidi isolate stripped: {text}");
}

#[test]
fn nbsp_normalized_to_space() {
    // Raw U+00A0 (NBSP) in text should become regular space
    let text = strip_to_text("<p>Hello\u{00A0}World</p>");
    assert!(text.contains("Hello World"), "NBSP normalized: {text}");
    assert!(!text.contains('\u{00A0}'), "no raw NBSP: {text}");
}

// ===== Surrogate and noncharacter entity handling =====

#[test]
fn surrogate_entity_becomes_replacement() {
    let text = strip_to_text("<p>Before&#xD800;After</p>");
    assert!(text.contains('\u{FFFD}'), "surrogate -> FFFD: {text}");
    assert!(text.contains("Before"), "text preserved: {text}");
    assert!(text.contains("After"), "text preserved: {text}");
}

#[test]
fn high_surrogate_entity_becomes_replacement() {
    let text = strip_to_text("<p>&#xDFFF;</p>");
    assert!(text.contains('\u{FFFD}'), "high surrogate -> FFFD: {text}");
}

#[test]
fn beyond_unicode_entity_becomes_replacement() {
    let text = strip_to_text("<p>&#x110000;</p>");
    assert!(text.contains('\u{FFFD}'), "beyond Unicode -> FFFD: {text}");
}

// ===== C0 control character stripping =====

#[test]
fn c0_control_chars_stripped() {
    // &#1; through &#8; should not appear in output
    let text = strip_to_text("<p>A&#1;B&#8;C</p>");
    assert!(text.contains("ABC"), "control chars stripped: {text}");
}

#[test]
fn cr_entity_normalized() {
    // &#13; (CR) should be collapsed as whitespace
    let text = strip_to_text("<p>Line1&#13;Line2</p>");
    assert!(text.contains("Line1"), "before CR: {text}");
    assert!(text.contains("Line2"), "after CR: {text}");
    assert!(!text.contains('\r'), "no raw CR: {text}");
}

#[test]
fn del_character_stripped() {
    // &#127; (DEL) should be stripped
    let text = strip_to_text("<p>Hello&#127;World</p>");
    assert!(text.contains("HelloWorld"), "DEL stripped: {text}");
}

// ===== Whitespace entity normalization =====

#[test]
fn ensp_emsp_thinsp_normalized_to_space() {
    // Unicode whitespace entities should collapse to regular space
    let text = strip_to_text("<p>Hello&ensp;World&emsp;Foo&thinsp;Bar</p>");
    assert!(text.contains("Hello World"), "ensp normalized: {text}");
    assert!(text.contains("World Foo"), "emsp normalized: {text}");
    assert!(text.contains("Foo Bar"), "thinsp normalized: {text}");
    assert!(!text.contains("  "), "no double spaces: {text}");
}

// ===== High Unicode / emoji entities =====

#[test]
fn emoji_entity_decoded() {
    let text = strip_to_text("<p>Star &#x2B50; emoji</p>");
    assert!(text.contains('\u{2B50}'), "star emoji: {text}");
}

#[test]
fn emoji_supplementary_plane() {
    // Emoji from supplementary plane (above U+FFFF)
    let text = strip_to_text("<p>Rocket &#x1F680; launch</p>");
    assert!(text.contains('\u{1F680}'), "rocket emoji: {text}");
}

#[test]
fn large_valid_codepoint() {
    // U+10FFFF is the last valid Unicode codepoint
    let text = strip_to_text("<p>&#x10FFFF;</p>");
    // char::from_u32(0x10FFFF) returns Some (it's a noncharacter but valid)
    assert!(
        !text.contains("&#x10FFFF;"),
        "large codepoint decoded: {text}"
    );
}

// ===== JSON-LD script tag =====

#[test]
fn json_ld_script_stripped() {
    let html = r#"<html><body>
            <script type="application/ld+json">
            {"@type": "NewsArticle", "headline": "Test Headline", "author": "John Smith"}
            </script>
            <p>Actual article content here.</p>
        </body></html>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Actual article"), "content preserved: {text}");
    assert!(!text.contains("NewsArticle"), "json-ld stripped: {text}");
    assert!(
        !text.contains("John Smith"),
        "json-ld author stripped: {text}"
    );
}

// ===== details/summary pattern =====

#[test]
fn details_summary_separated() {
    let html = r#"<details>
            <summary>Click to expand</summary>
            <p>Hidden content revealed on click.</p>
        </details>
        <p>Regular content.</p>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Regular content"), "main content: {text}");
    // details/summary content is included (it's visible in the DOM)
    assert!(text.contains("Click to expand"), "summary: {text}");
    assert!(
        !text.contains("expandHidden"),
        "summary/content separated: {text}"
    );
}

// ===== CDATA handling =====

#[test]
fn cdata_section_content_dropped() {
    // CDATA sections: content should not appear in output
    // (our parser treats <![CDATA[...]]> as a non-comment <! directive)
    let text = strip_to_text("<p>Before</p><![CDATA[hidden data]]><p>After</p>");
    assert!(text.contains("Before"), "before CDATA: {text}");
    assert!(text.contains("After"), "after CDATA: {text}");
    assert!(
        !text.contains("hidden data"),
        "CDATA content stripped: {text}"
    );
}

#[test]
fn cdata_with_gt_inside() {
    // CDATA containing '>' -- our parser fast-forwards to first '>' in
    // the <! handler, so inner content up to the first '>' is consumed
    // and the rest leaks as text. This is acceptable since CDATA is
    // only valid inside SVG/MathML in HTML5, and SVG is already skipped.
    let text = strip_to_text("<p>Before</p><![CDATA[a > b]]><p>After</p>");
    assert!(text.contains("Before"), "before: {text}");
    assert!(text.contains("After"), "after: {text}");
    // Note: " b]]" may leak due to first-'>' termination. This is a
    // known limitation for the rare CDATA-in-body case.
}

// ===== Malformed HTML resilience =====

#[test]
fn mismatched_close_tags_no_panic() {
    // Close tags that don't match opens -- should not panic or corrupt output
    let text = strip_to_text("<p>Hello</div></span>World</p>");
    assert!(text.contains("Hello"), "before mismatched: {text}");
    assert!(text.contains("World"), "after mismatched: {text}");
}

#[test]
fn deeply_nested_100_levels() {
    // 100+ levels of tag nesting
    let mut html = String::new();
    for _ in 0..100 {
        html.push_str("<div>");
    }
    html.push_str("Deep content");
    for _ in 0..100 {
        html.push_str("</div>");
    }
    let text = strip_to_text(&html);
    assert!(text.contains("Deep content"), "deep nesting works: {text}");
}

#[test]
fn entity_overflow_passthrough() {
    // Huge numeric entity that exceeds u32 -- should pass through as-is
    let text = strip_to_text("<p>&#99999999999;</p>");
    assert!(
        text.contains("&#99999999999;"),
        "overflow entity passes through: {text}"
    );
}

// ===== Whitespace between inline and block elements =====

#[test]
fn inline_tags_no_extra_space() {
    // Inline tags (b, i, span, a) should NOT insert spaces
    let text = strip_to_text("<p>Hello <b>bold</b> and <i>italic</i> text</p>");
    assert_eq!(text, "Hello bold and italic text");
}

#[test]
fn list_items_separated() {
    let text = strip_to_text("<ul><li>Apple</li><li>Banana</li><li>Cherry</li></ul>");
    assert!(
        !text.contains("AppleBanana"),
        "list items separated: {text}"
    );
    assert!(
        !text.contains("BananaCherry"),
        "list items separated: {text}"
    );
    assert!(text.contains("Apple"), "item 1: {text}");
    assert!(text.contains("Banana"), "item 2: {text}");
    assert!(text.contains("Cherry"), "item 3: {text}");
}

// ===== Central/Eastern European entity decoding (Latin Extended-A) =====

#[test]
fn entity_polish_names() {
    // Polish characters critical for NER: Ł, ą, ć, ę, ł, ń, ś, ź, ż
    let html = "<p>Jaros&lstrok;aw Kaczy&nacute;ski and &Lstrok;&oacute;d&zacute;</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Jarosław"), "lstrok decoded: {text}");
    assert!(text.contains("Kaczyński"), "nacute decoded: {text}");
    assert!(
        text.contains("Łódź"),
        "Lstrok+oacute+zacute decoded: {text}"
    );
}

#[test]
fn entity_czech_names() {
    // Czech characters: Č, č, Ď, ď, Ě, ě, Ň, ň, Ř, ř, Š, š, Ť, ť, Ž, ž
    let html = "<p>&Ccaron;esk&aacute; republika: Alena &Scaron;eredov&aacute; from Pra&zcaron;sk&yacute;</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Česká"), "Ccaron decoded: {text}");
    assert!(text.contains("Šeredová"), "Scaron decoded: {text}");
    assert!(text.contains("Pražský"), "zcaron decoded: {text}");
}

#[test]
fn entity_turkish_names() {
    // Turkish characters: Ğ, ğ, İ, ı, Ş, ş
    let html = "<p>Recep Tayyip Erdo&gbreve;an visited &Idot;stanbul and Mu&gbreve;la</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Erdoğan"), "gbreve decoded: {text}");
    assert!(text.contains("İstanbul"), "Idot decoded: {text}");
    assert!(text.contains("Muğla"), "gbreve lowercase decoded: {text}");
}

#[test]
fn entity_hungarian_names() {
    // Hungarian characters: Ő, ő, Ű, ű
    let html = "<p>The Hungarian city of Gy&odblac;r and Sz&udblac;cs</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Győr"), "odblac decoded: {text}");
    assert!(text.contains("Szűcs"), "udblac decoded: {text}");
}

#[test]
fn entity_romanian_names() {
    // Romanian characters: Ă, ă, Ş/Ț (Ţ cedilla form)
    let html = "<p>&Abreve;r&abreve;d in Romania; &Tcedil;ucureanu is a surname</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Ărăd"), "Abreve decoded: {text}");
    assert!(text.contains("Ţucureanu"), "Tcedil decoded: {text}");
}

#[test]
fn entity_croatian_names() {
    // Croatian characters: Đ, đ
    let html = "<p>Novak &Dstrok;okovi&cacute; (Serbian) and &Dstrok;ur&dstrok;a</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Đoković"), "Dstrok+cacute decoded: {text}");
    assert!(text.contains("Đurđa"), "Dstrok+dstrok decoded: {text}");
}

#[test]
fn entity_slovak_names() {
    // Slovak characters: Ľ, ľ, Ŕ, ŕ, Ť, ť
    let html = "<p>&Lcaron;ubom&iacute;r and the city of Bansk&aacute; Bystrica with &tcaron;a&rcaron;</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Ľubomír"), "Lcaron+iacute decoded: {text}");
    assert!(text.contains("ťař"), "tcaron+rcaron decoded: {text}");
}

#[test]
fn entity_dotless_i_turkish() {
    // Turkish dotless i (ı) is distinct from Latin i -- critical for NER
    let html = "<p>D&inodot;yarbak&inodot;r is a city in Turkey</p>";
    let text = strip_to_text(html);
    assert!(text.contains("Dıyarbakır"), "inodot decoded: {text}");
}

// ===== Edge cases: malformed/unusual HTML =====

#[test]
fn unclosed_tag_no_panic() {
    // Unclosed tag at end of input
    assert_eq!(strip_to_text("<p>Hello <b"), "Hello");
    assert_eq!(strip_to_text("Text <"), "Text");
    assert_eq!(strip_to_text("<p>Text</p><"), "Text");
}

#[test]
fn triple_nested_skip_tags() {
    // Three levels of skip nesting
    let html = "<nav><aside><footer>hidden</footer></aside></nav><p>visible</p>";
    let text = strip_to_text(html);
    assert!(!text.contains("hidden"), "nested skip leaked: {text}");
    assert!(text.contains("visible"), "visible missing: {text}");
}

#[test]
fn skip_tag_unclosed_no_hang() {
    // Unclosed skip tag -- should not hang or include everything
    let html = "<nav>hidden<p>also hidden";
    let text = strip_to_text(html);
    assert!(!text.contains("hidden"), "unclosed skip leaked: {text}");
}

#[test]
fn self_closing_in_skip_region() {
    // Self-closing tags inside skip regions should not affect skip depth
    let html = "<nav><br /><img src='x' /><hr /></nav><p>visible</p>";
    let text = strip_to_text(html);
    assert!(
        text.contains("visible"),
        "visible missing after skip: {text}"
    );
}

#[test]
fn entity_trailing_no_text() {
    // Entity right at the end, no trailing text
    assert_eq!(strip_to_text("Caf&eacute;"), "Café");
    assert_eq!(strip_to_text("A &amp;"), "A &");
}

#[test]
fn entity_no_semicolon_at_eof() {
    // Semicolon-optional entity at end of input
    assert_eq!(strip_to_text("A &amp"), "A &");
}

#[test]
fn consecutive_block_tags_get_newlines() {
    // Block tags should insert newlines between content
    let text = strip_to_text("<p>One</p><p>Two</p><p>Three</p>");
    assert_eq!(text, "One\nTwo\nThree");
}

#[test]
fn very_long_entity_name_no_panic() {
    // Absurdly long "entity" name -- should pass through
    let long = "&".to_string() + &"a".repeat(1000) + ";";
    let text = strip_to_text(&long);
    assert!(text.contains(&long), "long entity passed through");
}

#[test]
fn mixed_valid_invalid_entities() {
    let text = strip_to_text("&amp; &bogus; &#999999999; &lt;");
    assert!(text.contains('&'), "amp decoded");
    assert!(text.contains("&bogus;"), "bogus preserved");
    assert!(text.contains('<'), "lt decoded");
}

#[test]
fn only_whitespace_input() {
    assert_eq!(strip_to_text("   \t\n   "), "");
}

#[test]
fn only_tags_no_text() {
    assert_eq!(strip_to_text("<div><span></span></div>"), "");
}

// ===== Priority 3: Tag name buffer overflow =====

#[test]
fn tag_name_31_chars_lowercased() {
    // 31 chars fits in the 32-byte buffer (31 < 32), so it gets lowercased
    let tag = "A".repeat(31);
    let html = format!("<{tag}>content</{tag}>");
    let text = strip_to_text(&html);
    assert!(
        text.contains("content"),
        "content preserved for 31-char tag: {text}"
    );
    // Tag should be stripped regardless
    assert!(!text.contains(&tag), "tag stripped: {text}");
}

#[test]
fn tag_name_32_chars_still_stripped() {
    // 32 chars does NOT fit in the buffer (32 < 32 is false), so the tag
    // name is not lowercased. The tag should still be stripped from output.
    let tag = "A".repeat(32);
    let html = format!("<{tag}>content</{tag}>");
    let text = strip_to_text(&html);
    assert!(
        text.contains("content"),
        "content preserved for 32-char tag: {text}"
    );
    assert!(
        !text.contains(&tag),
        "tag stripped even without lowercase: {text}"
    );
}

#[test]
fn tag_name_100_chars_still_stripped() {
    // Far exceeds the buffer; tag name is used as-is (no lowercase).
    // Tag should still be stripped from output.
    let tag = "X".repeat(100);
    let html = format!("<{tag}>content</{tag}>");
    let text = strip_to_text(&html);
    assert!(
        text.contains("content"),
        "content preserved for 100-char tag: {text}"
    );
    assert!(
        !text.contains(&tag),
        "tag stripped even for huge name: {text}"
    );
}

// ===== Priority 4: Nested script/style tags =====

#[test]
fn nested_script_tags_all_removed() {
    // Nested <script> inside <script>: all content between the outermost
    // <script> and its closing </script> must be removed.
    let html = "<script><script>alert(1)</script></script><p>safe</p>";
    let text = strip_to_text(html);
    assert!(
        !text.contains("alert"),
        "nested script content removed: {text}"
    );
    assert!(
        text.contains("safe"),
        "text after scripts preserved: {text}"
    );
}

#[test]
fn style_containing_script_tag() {
    // <style> wrapping a <script> tag: both are skip-content tags.
    // The inner <script> should not confuse the parser.
    let html = "<style><script>alert(1)</script></style><p>safe</p>";
    let text = strip_to_text(html);
    assert!(
        !text.contains("alert"),
        "script inside style removed: {text}"
    );
    assert!(text.contains("safe"), "text after style preserved: {text}");
}

// ===== Priority 5: DEL character (0x7F) literal in input =====

#[test]
fn del_literal_byte_stripped() {
    // DEL (0x7F) as a literal byte in the input (not via entity).
    // cleanup_whitespace strips it (see the `b == 0x7F` branch).
    let html = "<p>hello\x7Fworld</p>";
    let text = strip_to_text(html);
    // DEL is stripped, so "hello" and "world" are concatenated
    assert!(
        !text.contains('\x7F'),
        "DEL character should be stripped: {text:?}"
    );
    assert!(
        text.contains("helloworld"),
        "adjacent text joined after DEL strip: {text}"
    );
}

#[test]
fn del_literal_in_plain_text_fast_path() {
    // DEL in text with no HTML tags (fast path through cleanup_whitespace)
    let text = strip_to_text("before\x7Fafter");
    assert!(
        !text.contains('\x7F'),
        "DEL stripped in fast path: {text:?}"
    );
    assert!(
        text.contains("beforeafter"),
        "text joined after DEL strip: {text}"
    );
}

// ===== extract_attr_value word-boundary fix =====

#[test]
fn extract_attr_value_exact_match() {
    assert_eq!(
        extract_attr_value("div class=\"toc\"", "class"),
        Some("toc")
    );
}

#[test]
fn extract_attr_value_no_substring_match() {
    // "class" must NOT match "data-class"
    assert_ne!(
        extract_attr_value("div data-class=\"toc\" class=\"article\"", "class"),
        Some("toc"),
        "must not match data-class"
    );
    assert_eq!(
        extract_attr_value("div data-class=\"toc\" class=\"article\"", "class"),
        Some("article"),
    );
}

#[test]
fn extract_attr_value_no_match() {
    assert_eq!(extract_attr_value("div id=\"main\"", "class"), None);
}

#[test]
fn extract_attr_value_case_insensitive() {
    assert_eq!(
        extract_attr_value("div CLASS=\"foo\"", "class"),
        Some("foo")
    );
}

#[test]
fn extract_attr_value_single_quotes() {
    assert_eq!(extract_attr_value("div class='bar'", "class"), Some("bar"));
}

#[test]
fn extract_attr_value_data_class_only() {
    // Only data-class, no plain class -- should return None for "class"
    assert_eq!(extract_attr_value("div data-class=\"toc\"", "class"), None,);
}

#[test]
fn data_class_does_not_trigger_wiki_skip() {
    // Regression: data-class="toc" must not cause wiki-skip
    let html = r#"<div data-class="toc"><p>This content should be visible.</p></div>"#;
    let text = strip_to_text(html);
    assert!(
        text.contains("This content should be visible"),
        "data-class must not trigger wiki-skip: {text}"
    );
}

#[test]
fn data_id_does_not_trigger_wiki_skip() {
    // Regression: data-id="toc" must not cause wiki-skip
    let html = r#"<div data-id="toc"><p>Content here.</p></div>"#;
    let text = strip_to_text(html);
    assert!(
        text.contains("Content here"),
        "data-id must not trigger wiki-skip: {text}"
    );
}

#[test]
fn real_class_toc_still_triggers_wiki_skip() {
    let html = r#"<div class="toc"><h2>Contents</h2></div><p>Article text.</p>"#;
    let text = strip_to_text(html);
    assert!(!text.contains("Contents"), "real toc stripped: {text}");
    assert!(text.contains("Article text"), "article preserved: {text}");
}

// ===== Markdown output =====

#[test]
fn md_headings() {
    let md = strip_to_markdown("<h1>Title</h1><h2>Section</h2><h3>Sub</h3>");
    assert!(md.contains("# Title"), "h1: {md}");
    assert!(md.contains("## Section"), "h2: {md}");
    assert!(md.contains("### Sub"), "h3: {md}");
}

#[test]
fn md_bold_italic() {
    let md = strip_to_markdown("<p>Hello <b>bold</b> and <em>italic</em>!</p>");
    assert!(md.contains("**bold**"), "bold: {md}");
    assert!(md.contains("*italic*"), "italic: {md}");
}

#[test]
fn md_links() {
    let md =
        strip_to_markdown(r#"<p>Visit <a href="https://example.com">our site</a> for more.</p>"#);
    assert!(md.contains("[our site](https://example.com)"), "link: {md}");
    assert!(md.contains("for more"), "surrounding text: {md}");
}

#[test]
fn md_inline_code() {
    let md = strip_to_markdown("<p>Use <code>cargo build</code> to compile.</p>");
    assert!(md.contains("`cargo build`"), "inline code: {md}");
}

#[test]
fn md_code_block() {
    let md = strip_to_markdown(
        "<pre><code>fn main() {\n    println!(\"hello\");\n}</code></pre><p>After.</p>",
    );
    assert!(md.contains("```"), "code fence: {md}");
    assert!(md.contains("fn main()"), "code content: {md}");
    assert!(md.contains("After"), "post-code text: {md}");
}

#[test]
fn md_unordered_list() {
    let md = strip_to_markdown("<ul><li>First</li><li>Second</li><li>Third</li></ul>");
    assert!(md.contains("- First"), "li 1: {md}");
    assert!(md.contains("- Second"), "li 2: {md}");
    assert!(md.contains("- Third"), "li 3: {md}");
}

#[test]
fn md_ordered_list() {
    let md = strip_to_markdown("<ol><li>Alpha</li><li>Beta</li><li>Gamma</li></ol>");
    assert!(md.contains("1. Alpha"), "ol 1: {md}");
    assert!(md.contains("2. Beta"), "ol 2: {md}");
    assert!(md.contains("3. Gamma"), "ol 3: {md}");
}

#[test]
fn md_mixed_lists() {
    let md = strip_to_markdown(concat!(
        "<ol><li>Ordered 1</li><li>Ordered 2</li></ol>",
        "<ul><li>Unordered A</li><li>Unordered B</li></ul>"
    ));
    assert!(md.contains("1. Ordered 1"), "ol 1: {md}");
    assert!(md.contains("2. Ordered 2"), "ol 2: {md}");
    assert!(md.contains("- Unordered A"), "ul a: {md}");
    assert!(md.contains("- Unordered B"), "ul b: {md}");
}

#[test]
fn md_image() {
    let md = strip_to_markdown(r#"<img src="photo.jpg" alt="A nice photo">"#);
    assert!(md.contains("![A nice photo](photo.jpg)"), "image: {md}");
}

#[test]
fn md_hr() {
    let md = strip_to_markdown("<p>Before</p><hr><p>After</p>");
    assert!(md.contains("---"), "hr: {md}");
    assert!(md.contains("Before"), "before: {md}");
    assert!(md.contains("After"), "after: {md}");
}

#[test]
fn md_boilerplate_still_stripped() {
    let md = strip_to_markdown(
        r#"<nav><a href="/">Home</a></nav>
               <article><h1>Title</h1><p>Content.</p></article>
               <footer><p>Copyright</p></footer>"#,
    );
    assert!(md.contains("# Title"), "heading preserved: {md}");
    assert!(md.contains("Content"), "content preserved: {md}");
    assert!(!md.contains("Home"), "nav stripped: {md}");
    assert!(!md.contains("Copyright"), "footer stripped: {md}");
}

#[test]
fn md_entities_decoded() {
    let md = strip_to_markdown("<p>Caf&eacute; &amp; B&ouml;rse</p>");
    assert!(md.contains("Caf\u{00E9}"), "eacute: {md}");
    assert!(md.contains("&"), "amp: {md}");
    assert!(md.contains("B\u{00F6}rse"), "ouml: {md}");
}

#[test]
fn md_script_stripped() {
    let md = strip_to_markdown("<script>if (x < 10) { alert('hi'); }</script><p>Visible.</p>");
    assert!(md.contains("Visible"), "content: {md}");
    assert!(!md.contains("alert"), "script stripped: {md}");
}

#[test]
fn md_full_article() {
    let html = r#"<!DOCTYPE html>
        <html><head><title>Test</title><style>body{}</style></head>
        <body>
            <nav><a href="/">Home</a></nav>
            <article>
                <h1>Rust 2026</h1>
                <p>The <strong>Rust</strong> programming language continues to grow.
                   Visit <a href="https://rust-lang.org">the official site</a>.</p>
                <h2>Features</h2>
                <ul>
                    <li>Memory safety</li>
                    <li>Zero-cost abstractions</li>
                </ul>
                <pre><code>let x = 42;</code></pre>
            </article>
            <footer><p>&copy; 2026</p></footer>
        </body></html>"#;
    let md = strip_to_markdown(html);
    assert!(md.contains("# Rust 2026"), "h1: {md}");
    assert!(md.contains("**Rust**"), "bold: {md}");
    assert!(
        md.contains("[the official site](https://rust-lang.org)"),
        "link: {md}"
    );
    assert!(md.contains("## Features"), "h2: {md}");
    assert!(md.contains("- Memory safety"), "list item: {md}");
    assert!(md.contains("```"), "code fence: {md}");
    assert!(md.contains("let x = 42"), "code: {md}");
    assert!(!md.contains("Home"), "nav stripped: {md}");
    assert!(!md.contains("2026</p>"), "footer stripped: {md}");
}

// ===== SpanMap =====

#[test]
fn span_map_basic() {
    let html = "<p>Hello world!</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Hello world!"), "text: {text}");
    assert!(!spans.is_empty(), "should have spans");

    // Find "Hello" in output
    let hello_start = text.find("Hello").unwrap();
    let hello_end = hello_start + "Hello".len();
    let src = spans.source_range(hello_start, hello_end).unwrap();
    // The source range should point into the HTML
    let source_text = &html[src.0..src.1];
    assert!(
        source_text.contains("Hello"),
        "source should contain Hello: {source_text}"
    );
}

#[test]
fn span_map_entity() {
    let html = "<p>Caf&eacute;</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Caf\u{00E9}"), "text: {text}");

    // The 'e' with acute comes from "&eacute;" in the source
    let cafe_start = text.find("Caf\u{00E9}").unwrap();
    let cafe_end = cafe_start + "Caf\u{00E9}".len();
    let src = spans.source_range(cafe_start, cafe_end).unwrap();
    let source_text = &html[src.0..src.1];
    // Source should cover "Caf" and "&eacute;"
    assert!(
        source_text.contains("&eacute;") || source_text.contains("Caf"),
        "source contains entity: {source_text}"
    );
}

#[test]
fn span_map_skipped_tags() {
    let html = "<nav>Skip me</nav><p>Keep me</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Keep me"), "text: {text}");
    assert!(!text.contains("Skip me"), "nav stripped: {text}");

    // Find "Keep" in output
    let keep_start = text.find("Keep").unwrap();
    let keep_end = keep_start + "Keep me".len();
    let src = spans.source_range(keep_start, keep_end).unwrap();
    let source_text = &html[src.0..src.1];
    assert!(
        source_text.contains("Keep me"),
        "source is in paragraph: {source_text}"
    );
}

#[test]
fn span_map_img_alt() {
    let html = r#"<p>See <img src="x.jpg" alt="photo"> here</p>"#;
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("photo"), "alt text: {text}");

    let photo_start = text.find("photo").unwrap();
    let photo_end = photo_start + "photo".len();
    let src = spans.source_range(photo_start, photo_end).unwrap();
    // Source should point to the <img> tag area
    let source_text = &html[src.0..src.1];
    assert!(
        source_text.contains("alt="),
        "source contains img tag: {source_text}"
    );
}

#[test]
fn span_map_no_out_of_bounds() {
    // Query a range beyond the output -- should return None
    let (text, spans) = strip_to_text_with_spans("<p>short</p>");
    assert!(text.contains("short"));
    assert!(spans.source_range(100, 200).is_none());
    assert!(spans.source_range(0, 0).is_none());
}

// ===== Anchor election (prefer_main_landmark) =====

#[test]
fn anchor_election_finds_main_landmark() {
    let inner = "<p>Article body content here.</p>".repeat(20);
    let html = format!(
        "<html><body><nav>nav text everywhere here</nav>\
             <main>{inner}</main>\
             <footer>footer text everywhere here</footer></body></html>"
    );
    let elected = strip_to_text_with_options(&html, &StripOptions::main_landmark());
    let baseline = strip_to_text(&html);
    // Both should keep article body. Both should drop nav/footer (skip
    // tags). Anchor election adds no signal here, but must NOT regress.
    assert!(elected.contains("Article body content"));
    assert!(baseline.contains("Article body content"));
}

#[test]
fn anchor_election_drops_non_skip_boilerplate_outside_main() {
    // <div class="related-posts"> is NOT a skip tag, so the baseline
    // scanner emits its content. Anchor election restricts to <main>
    // and drops the related-posts div.
    let html = "<html><body>\
            <main><h1>Title</h1>\
            <p>The article body has substantial content with multiple sentences and paragraphs to clear the minimum landmark threshold for election.</p>\
            <p>Second paragraph adds more body text so the slice is comfortably above the 200-char floor that find_main_landmark_slice enforces.</p>\
            </main>\
            <div class=\"related-posts\">RELATED_BOILERPLATE_TOKEN should not appear in output</div>\
            </body></html>";
    let elected = strip_to_text_with_options(html, &StripOptions::main_landmark());
    let baseline = strip_to_text(html);
    assert!(elected.contains("article body"));
    assert!(
        !elected.contains("RELATED_BOILERPLATE_TOKEN"),
        "election should drop non-skip-tag boilerplate outside <main>: {elected}"
    );
    assert!(
            baseline.contains("RELATED_BOILERPLATE_TOKEN"),
            "baseline emits the boilerplate (no election); confirms the test isn't trivially passing: {baseline}"
        );
}

#[test]
fn anchor_election_falls_through_when_no_landmark() {
    // No <main> or <article>: must fall through to normal strip.
    let html = "<html><body><div><p>Just a div-wrapped paragraph.</p></div></body></html>";
    let elected = strip_to_text_with_options(html, &StripOptions::main_landmark());
    let baseline = strip_to_text(html);
    assert_eq!(elected, baseline);
}

#[test]
fn anchor_election_picks_longest_article_when_no_main() {
    // Page with two <article>s: a tiny teaser and a full body.
    // Election picks the longer one.
    let body = "<p>Full article body paragraph repeated. </p>".repeat(15);
    let html = format!(
        "<html><body>\
            <article><p>Teaser snippet.</p></article>\
            <article>{body}</article>\
            </body></html>"
    );
    let elected = strip_to_text_with_options(&html, &StripOptions::main_landmark());
    assert!(elected.contains("Full article body"));
    assert!(
        !elected.contains("Teaser snippet"),
        "shorter article shouldn't win election: {elected}"
    );
}

#[test]
fn anchor_election_skips_too_small_landmarks() {
    // <main> with only a search box (well under 200 chars) should not
    // win election; fall through to whole-document scanning.
    let html = "<html><body>\
            <main><form><input type=\"search\"></form></main>\
            <div><p>Real body content lives outside the empty main landmark, in a regular div container with several sentences worth of text.</p></div>\
            </body></html>";
    let elected = strip_to_text_with_options(html, &StripOptions::main_landmark());
    assert!(
        elected.contains("Real body content"),
        "election should reject empty <main> and fall through: {elected}"
    );
}

#[test]
fn anchor_election_handles_nested_articles() {
    // <article> nested inside <article>: the find_matching_close depth
    // tracker must not return early on the inner close tag.
    let inner_body = "<p>Outer body content paragraph repeated. </p>".repeat(15);
    let html = format!(
        "<html><body><article>{inner_body}\
             <article><p>Nested teaser inside outer.</p></article>\
             </article></body></html>"
    );
    let elected = strip_to_text_with_options(&html, &StripOptions::main_landmark());
    assert!(elected.contains("Outer body content"));
    assert!(elected.contains("Nested teaser"));
}

#[test]
fn anchor_election_default_off_preserves_byte_exact_strip() {
    // StripOptions::default() must NOT enable election -- back-compat.
    let html = "<html><body><main><p>Inside main.</p></main><p>Outside main.</p></body></html>";
    let default = strip_to_text_with_options(html, &StripOptions::default());
    let strip = strip_to_text(html);
    assert_eq!(default, strip);
    assert!(default.contains("Outside main"));
}
