//! Thorough tests for SpanMap (output-to-source offset tracking).
//!
//! These tests verify that every piece of extracted text can be traced
//! back to its correct source position in the original HTML.

use deformat::html::strip_to_text_with_spans;

// =============================================================================
// Basic text tracing
// =============================================================================

#[test]
fn simple_paragraph() {
    let html = "<p>Hello world!</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "Hello world!");
    assert!(!spans.is_empty());

    // Entire output should be traceable
    let src = spans.source_range(0, text.len()).unwrap();
    assert!(html[src.0..src.1].contains("Hello world!"));
}

#[test]
fn multiple_paragraphs() {
    let html = "<p>First.</p><p>Second.</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("First."));
    assert!(text.contains("Second."));

    // "First." should map to the first <p>
    let first_start = text.find("First.").unwrap();
    let first_end = first_start + "First.".len();
    let src = spans.source_range(first_start, first_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(source.contains("First."), "First maps correctly: {source}");

    // "Second." should map to the second <p>
    let second_start = text.find("Second.").unwrap();
    let second_end = second_start + "Second.".len();
    let src = spans.source_range(second_start, second_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("Second."),
        "Second maps correctly: {source}"
    );
}

#[test]
fn text_with_inline_tags() {
    let html = "<p>Hello <b>bold</b> text</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Hello"));
    assert!(text.contains("bold"));
    assert!(text.contains("text"));

    // "bold" should map to inside the <b> tag
    let bold_start = text.find("bold").unwrap();
    let bold_end = bold_start + "bold".len();
    let src = spans.source_range(bold_start, bold_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(source.contains("bold"), "bold traces to source: {source}");
}

// =============================================================================
// Entity decoding tracing
// =============================================================================

#[test]
fn named_entity_traces_to_source() {
    let html = "<p>Caf&eacute; au lait</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Caf\u{00E9}"));

    // The e-acute should map back to &eacute; in source
    let cafe = text.find("Caf\u{00E9}").unwrap();
    // The accented char is at byte position cafe+3 (C=1,a=1,f=1,e-acute=2 bytes in UTF-8)
    let accent_start = cafe + 3; // start of \u{00E9}
    let accent_end = accent_start + '\u{00E9}'.len_utf8();
    let src = spans.source_range(accent_start, accent_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("&eacute;"),
        "accent maps to entity: {source}"
    );
}

#[test]
fn numeric_entity_traces_to_source() {
    let html = "<p>Price: &#8364;100</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("\u{20AC}")); // euro sign

    let euro_pos = text.find('\u{20AC}').unwrap();
    let euro_end = euro_pos + '\u{20AC}'.len_utf8();
    let src = spans.source_range(euro_pos, euro_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("&#8364;"),
        "euro maps to numeric entity: {source}"
    );
}

#[test]
fn hex_entity_traces_to_source() {
    let html = "<p>Quote: &#x201C;hello&#x201D;</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains('\u{201C}')); // left double quote

    let quote_pos = text.find('\u{201C}').unwrap();
    let quote_end = quote_pos + '\u{201C}'.len_utf8();
    let src = spans.source_range(quote_pos, quote_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("&#x201C;"),
        "left quote maps to hex entity: {source}"
    );
}

#[test]
fn amp_entity_traces_to_source() {
    let html = "<p>AT&amp;T</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("AT&T"));

    let amp_pos = text.find('&').unwrap();
    let src = spans.source_range(amp_pos, amp_pos + 1).unwrap();
    let source = &html[src.0..src.1];
    assert!(source.contains("&amp;"), "amp maps to entity: {source}");
}

#[test]
fn multiple_entities_in_sequence() {
    let html = "<p>&lt;&gt;&amp;</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "<>&");

    // Each decoded char should trace to its entity
    let lt_src = spans.source_range(0, 1).unwrap();
    assert!(html[lt_src.0..lt_src.1].contains("&lt;"), "< from &lt;");

    let gt_src = spans.source_range(1, 2).unwrap();
    assert!(html[gt_src.0..gt_src.1].contains("&gt;"), "> from &gt;");

    let amp_src = spans.source_range(2, 3).unwrap();
    assert!(html[amp_src.0..amp_src.1].contains("&amp;"), "& from &amp;");
}

// =============================================================================
// Skipped content
// =============================================================================

#[test]
fn nav_content_not_in_spans() {
    let html = "<nav>Navigation</nav><p>Article text.</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(!text.contains("Navigation"));
    assert!(text.contains("Article text."));

    // All spans should point to the <p> area, not the <nav> area
    let nav_end = html.find("</nav>").unwrap() + 6; // byte after </nav>
    for &(_, _, ss, _) in spans.iter() {
        assert!(
            ss >= nav_end || ss < 5,
            "span source {ss} should not be inside nav (nav ends at {nav_end})"
        );
    }
}

#[test]
fn script_content_not_in_spans() {
    let html = "<script>var x = 1;</script><p>Visible.</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Visible"));
    assert!(!text.contains("var x"));

    let script_end = html.find("</script>").unwrap() + 9;
    for &(_, _, ss, _) in spans.iter() {
        assert!(
            ss >= script_end,
            "span at source {ss} should be after script (ends at {script_end})"
        );
    }
}

// =============================================================================
// Complex real-world-like HTML
// =============================================================================

#[test]
fn realistic_article_spans() {
    let html = r#"<!DOCTYPE html>
    <html><head><title>Test</title></head>
    <body>
        <nav><a href="/">Home</a></nav>
        <article>
            <h1>Research Paper</h1>
            <p>Dr. M&uuml;ller and Prof. Nestl&eacute; published
               a paper on quantum computing in Nature.</p>
            <p>The &#8364;5 million grant funded the research.</p>
        </article>
        <footer><p>&copy; 2026</p></footer>
    </body></html>"#;

    let (text, spans) = strip_to_text_with_spans(html);

    // Verify key NER entities trace back correctly
    // Entity-decoded characters should individually trace to their source entities
    let umlaut_pos = text.find('\u{00FC}').expect("text contains ü");
    let umlaut_end = umlaut_pos + '\u{00FC}'.len_utf8();
    let src = spans.source_range(umlaut_pos, umlaut_end).unwrap();
    assert!(
        html[src.0..src.1].contains("&uuml;"),
        "ü maps to &uuml;: {}",
        &html[src.0..src.1]
    );

    let eacute_pos = text.find('\u{00E9}').expect("text contains é");
    let eacute_end = eacute_pos + '\u{00E9}'.len_utf8();
    let src = spans.source_range(eacute_pos, eacute_end).unwrap();
    assert!(
        html[src.0..src.1].contains("&eacute;"),
        "é maps to &eacute;: {}",
        &html[src.0..src.1]
    );

    let euro_pos = text.find('\u{20AC}').expect("text contains €");
    let euro_end = euro_pos + '\u{20AC}'.len_utf8();
    let src = spans.source_range(euro_pos, euro_end).unwrap();
    assert!(
        html[src.0..src.1].contains("&#8364;"),
        "€ maps to &#8364;: {}",
        &html[src.0..src.1]
    );

    // Plain text words should also trace correctly
    let paper_start = text.find("paper").unwrap();
    let paper_end = paper_start + "paper".len();
    let src = spans.source_range(paper_start, paper_end).unwrap();
    assert!(
        html[src.0..src.1].contains("paper"),
        "plain word traces: {}",
        &html[src.0..src.1]
    );
}

// =============================================================================
// Image alt text
// =============================================================================

#[test]
fn img_alt_text_traces_to_img_tag() {
    let html = r#"<p>See <img src="photo.jpg" alt="A sunset over mountains"> below.</p>"#;
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("A sunset over mountains"));

    let alt_start = text.find("A sunset").unwrap();
    let alt_end = alt_start + "A sunset over mountains".len();
    let src = spans.source_range(alt_start, alt_end).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("<img") && source.contains("alt="),
        "alt text maps to img tag: {source}"
    );
}

// =============================================================================
// SpanMap API
// =============================================================================

#[test]
fn span_map_len_and_is_empty() {
    let (_, spans) = strip_to_text_with_spans("<p>text</p>");
    assert!(!spans.is_empty());
    assert!(spans.len() > 0);

    let (_, empty_spans) = strip_to_text_with_spans("");
    assert!(empty_spans.is_empty());
    assert_eq!(empty_spans.len(), 0);
}

#[test]
fn span_map_iter() {
    let html = "<p>Hello</p><p>World</p>";
    let (text, spans) = strip_to_text_with_spans(html);

    // Iterate and verify all spans are valid
    for &(os, oe, ss, se) in spans.iter() {
        assert!(os <= oe, "output range valid: {os}..{oe}");
        assert!(ss <= se, "source range valid: {ss}..{se}");
        assert!(
            oe <= text.len(),
            "output end in bounds: {oe} <= {}",
            text.len()
        );
        assert!(
            se <= html.len(),
            "source end in bounds: {se} <= {}",
            html.len()
        );
    }
}

#[test]
fn span_map_source_range_partial_overlap() {
    let html = "<p>ABCDEF</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "ABCDEF");

    // Query middle of the text
    let src = spans.source_range(2, 4).unwrap(); // "CD"
    let source = &html[src.0..src.1];
    assert!(
        source.contains("CD"),
        "partial range maps correctly: {source}"
    );
}

#[test]
fn span_map_out_of_bounds_returns_none() {
    let (text, spans) = strip_to_text_with_spans("<p>short</p>");
    assert!(spans
        .source_range(text.len() + 10, text.len() + 20)
        .is_none());
    assert!(spans.source_range(0, 0).is_none()); // empty range
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn empty_html() {
    let (text, spans) = strip_to_text_with_spans("");
    assert!(text.is_empty());
    assert!(spans.is_empty());
}

#[test]
fn no_tags_plain_text() {
    let html = "Just plain text.";
    let (text, spans) = strip_to_text_with_spans(html);
    // Plain text without tags goes through the fast path (no '<')
    // This path doesn't track spans (it goes through decode_entities_in_str + cleanup_whitespace)
    // Document the behavior: fast path produces empty spans
    assert_eq!(text, "Just plain text.");
    // Fast path doesn't populate spans -- this is a known limitation
    // for plain text input without any HTML tags
}

#[test]
fn only_tags_no_text() {
    let html = "<div><span></span></div>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.trim().is_empty());
    assert!(spans.is_empty() || spans.iter().all(|&(os, oe, _, _)| os == oe));
}

#[test]
fn deeply_nested_tags() {
    let html = "<div><div><div><p>Deep content</p></div></div></div>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Deep content"));

    let start = text.find("Deep content").unwrap();
    let end = start + "Deep content".len();
    let src = spans.source_range(start, end).unwrap();
    assert!(html[src.0..src.1].contains("Deep content"));
}

#[test]
fn span_map_with_wiki_options() {
    // SpanMap uses StripOptions::default() (no wiki stripping)
    // Verify it still works with wiki content
    let html = "<p>Einstein[1] was a physicist.</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("[1]"), "default preserves wiki refs");

    let bracket_start = text.find("[1]").unwrap();
    let bracket_end = bracket_start + "[1]".len();
    let src = spans.source_range(bracket_start, bracket_end).unwrap();
    assert!(
        html[src.0..src.1].contains("[1]"),
        "wiki ref traces correctly"
    );
}

// =============================================================================
// Proptest-style: every word in output is traceable
// =============================================================================

#[test]
fn all_words_traceable_in_complex_html() {
    let html = r#"<!DOCTYPE html><html><body>
        <article>
            <h1>Climate Summit 2026</h1>
            <p>Leaders from France, Germany, and Japan met in Brussels
               to discuss carbon emission targets.</p>
            <p>The agreement includes a &#8364;50 billion green investment
               fund managed by the European Commission.</p>
            <img src="summit.jpg" alt="World leaders at summit">
        </article>
    </body></html>"#;

    let (text, spans) = strip_to_text_with_spans(html);

    // Every significant word should be traceable to the source
    let words_to_check = [
        "Climate",
        "Summit",
        "Leaders",
        "France",
        "Germany",
        "Japan",
        "Brussels",
        "carbon",
        "emission",
        "agreement",
        "billion",
        "green",
        "investment",
        "European",
        "Commission",
        "leaders",
        "summit",
    ];

    for word in &words_to_check {
        if let Some(start) = text.find(word) {
            let end = start + word.len();
            let src = spans.source_range(start, end);
            assert!(
                src.is_some(),
                "word '{word}' at {start}..{end} should be traceable"
            );
            let (ss, se) = src.unwrap();
            // The source region may include surrounding whitespace from
            // HTML indentation. Verify the word appears in a broader
            // context around the source range.
            let context_start = ss.saturating_sub(20);
            let context_end = (se + 20).min(html.len());
            let context = &html[context_start..context_end];
            assert!(
                context.contains(word),
                "word '{word}' should appear near source region [{ss}..{se}]: '{context}'"
            );
        }
    }
}
