//! Thorough tests for SpanMap (output-to-source offset tracking).
//!
//! These tests verify that every piece of extracted text can be traced
//! back to its correct source position in the original HTML.

use deformat::html::{strip_to_text_with_paths, strip_to_text_with_spans, SpanKind};

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
    for span in spans.iter() {
        let ss = span.source_start;
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
    for span in spans.iter() {
        let ss = span.source_start;
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
    assert!(!spans.is_empty());

    let (_, empty_spans) = strip_to_text_with_spans("");
    assert!(empty_spans.is_empty());
    assert_eq!(empty_spans.len(), 0);
}

#[test]
fn span_map_iter() {
    let html = "<p>Hello</p><p>World</p>";
    let (text, spans) = strip_to_text_with_spans(html);

    // Iterate and verify all spans are valid
    for span in spans.iter() {
        let (os, oe, ss, se) = (
            span.output_start,
            span.output_end,
            span.source_start,
            span.source_end,
        );
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
    // Plain text (no '<') goes through the fast path. As of v0.11.1 the fast
    // path emits a single whole-input span so callers get a consistent API.
    let html = "Just plain text.";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "Just plain text.");
    assert_eq!(spans.len(), 1, "fast path emits single whole-input span");
    let src = spans.source_range(0, text.len()).unwrap();
    assert_eq!(src, (0, html.len()));
    // No entities or collapse → Direct; byte counts match.
    assert_eq!(spans.iter().next().unwrap().kind, SpanKind::Direct);
}

#[test]
fn plain_text_with_entities_fast_path_span_is_entity_decoded() {
    let html = "AT&amp;T";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "AT&T");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans.iter().next().unwrap().kind, SpanKind::EntityDecoded);
    let src = spans.source_range(0, text.len()).unwrap();
    assert_eq!(src, (0, html.len()));
}

#[test]
fn plain_text_with_whitespace_collapse_fast_path_span_is_entity_decoded() {
    let html = "  lots   of   spaces  ";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "lots of spaces");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans.iter().next().unwrap().kind, SpanKind::EntityDecoded);
}

#[test]
fn plain_text_all_whitespace_fast_path_emits_no_spans() {
    let (text, spans) = strip_to_text_with_spans("   \t\n  ");
    assert!(text.is_empty());
    assert!(spans.is_empty());
}

#[test]
fn only_tags_no_text() {
    let html = "<div><span></span></div>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.trim().is_empty());
    assert!(spans.is_empty() || spans.iter().all(|s| s.output_start == s.output_end));
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

// =============================================================================
// Structural path tests
// =============================================================================

#[test]
fn path_basic_structure() {
    let html = "<html><body><p>Hello</p></body></html>";
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("Hello"));

    let hello_span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Hello"));
    assert!(hello_span.is_some(), "Hello should have a path span");
    let path = &hello_span.unwrap().path;
    assert!(path.contains("html"), "path has html: {path}");
    assert!(path.contains("body"), "path has body: {path}");
    assert!(path.contains("p"), "path has p: {path}");
}

#[test]
fn path_nested_elements() {
    let html = "<article><section><p>Deep content</p></section></article>";
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("Deep content"));

    let span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Deep"))
        .unwrap();
    assert!(
        span.path.contains("article"),
        "path has article: {}",
        span.path
    );
    assert!(
        span.path.contains("section"),
        "path has section: {}",
        span.path
    );
    assert!(span.path.contains("p"), "path has p: {}", span.path);
}

#[test]
fn path_skipped_elements_not_in_path() {
    let html = "<body><nav>Skip</nav><article><p>Content</p></article></body>";
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("Content"));
    assert!(!text.contains("Skip"));

    let content_span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Content"))
        .unwrap();
    assert!(
        !content_span.path.contains("nav"),
        "nav not in path: {}",
        content_span.path
    );
    assert!(
        content_span.path.contains("article"),
        "article in path: {}",
        content_span.path
    );
}

#[test]
fn path_entity_inherits_parent_path() {
    let html = "<div><p>Caf&eacute;</p></div>";
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("Caf\u{00E9}"));

    // The entity-decoded char should have a path too
    let eacute_span = spans.iter().find(|s| {
        let chunk = &text[s.output_start..s.output_end];
        chunk.contains('\u{00E9}')
    });
    assert!(eacute_span.is_some(), "entity has a path span");
    let path = &eacute_span.unwrap().path;
    assert!(path.contains("div"), "path has div: {path}");
    assert!(path.contains("p"), "path has p: {path}");
}

#[test]
fn path_multiple_children_same_parent() {
    let html = "<div><p>First</p><p>Second</p><p>Third</p></div>";
    let (text, spans) = strip_to_text_with_paths(html);

    // All three paragraphs should have paths containing "div" and "p"
    for word in ["First", "Second", "Third"] {
        let span = spans
            .iter()
            .find(|s| text[s.output_start..s.output_end].contains(word));
        assert!(span.is_some(), "{word} has a path span");
        let path = &span.unwrap().path;
        assert!(path.contains("div"), "{word}: path has div: {path}");
        assert!(path.contains("p"), "{word}: path has p: {path}");
    }
}

#[test]
fn path_img_alt_text() {
    let html = r#"<div><img src="x.jpg" alt="Photo description"></div>"#;
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("Photo description"));

    let span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Photo"))
        .unwrap();
    assert!(
        span.path.contains("div"),
        "img alt path has parent: {}",
        span.path
    );
}

#[test]
fn path_fast_path_emits_whole_input_span_with_empty_path() {
    // As of v0.11.1 the plain-text fast path in strip_to_text_with_paths
    // emits a single whole-input span with an empty path (no surrounding tags).
    let (text, spans) = strip_to_text_with_paths("No tags here.");
    assert_eq!(text, "No tags here.");
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.output_start, 0);
    assert_eq!(span.output_end, text.len());
    assert!(
        span.path.is_empty(),
        "path should be empty: {:?}",
        span.path
    );
}

#[test]
fn path_fast_path_empty_for_all_whitespace() {
    let (text, spans) = strip_to_text_with_paths("   ");
    assert!(text.is_empty());
    assert!(spans.is_empty());
}

#[test]
fn path_realistic_article() {
    let html = r#"<html><body>
        <article>
            <h1>Title Here</h1>
            <p>Introduction paragraph.</p>
            <div class="content">
                <p>Main body text.</p>
            </div>
        </article>
    </body></html>"#;

    let (_text, spans) = strip_to_text_with_paths(html);

    // Check that spans with h1 path exist
    let h1_spans: Vec<_> = spans.iter().filter(|s| s.path.contains("h1")).collect();
    assert!(
        !h1_spans.is_empty(),
        "should have h1 path spans, all paths: {:?}",
        spans.iter().map(|s| &s.path).collect::<Vec<_>>()
    );

    // Check that spans with div/p path exist
    let div_p_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.path.contains("div") && s.path.contains("p"))
        .collect();
    assert!(
        !div_p_spans.is_empty(),
        "should have div/p path spans, all paths: {:?}",
        spans.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
}

// =============================================================================
// source_position: byte-level mapping with intra-run interpolation
// =============================================================================

#[test]
fn source_position_direct_run_interpolates() {
    let html = "<p>ABCDEFGHIJ</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "ABCDEFGHIJ");

    // Each output byte should map to its exact source byte.
    let abc_src = html.find("ABCDEFGHIJ").unwrap();
    for i in 0..text.len() {
        let src = spans.source_position(i).expect("every byte has a source");
        assert_eq!(src, abc_src + i, "byte {i} should map to {}", abc_src + i);
    }
}

#[test]
fn source_position_out_of_bounds_returns_none() {
    let (text, spans) = strip_to_text_with_spans("<p>hi</p>");
    assert!(spans.source_position(text.len() + 5).is_none());
}

#[test]
fn source_position_on_entity_returns_entity_start() {
    let html = "<p>X &amp; Y</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    // Text is "X & Y"; the '&' byte is at position 2.
    let amp_pos = text.find('&').unwrap();
    let src = spans.source_position(amp_pos).unwrap();
    // Source should point at the '&' that starts "&amp;".
    assert_eq!(&html[src..src + 5], "&amp;");
}

// =============================================================================
// SpanKind classification
// =============================================================================

#[test]
fn span_kinds_are_tagged() {
    let html = r#"<p>Direct &amp; <img alt="photo"> text</p>"#;
    let (_text, spans) = strip_to_text_with_spans(html);

    let has_direct = spans.iter().any(|s| s.kind == SpanKind::Direct);
    let has_entity = spans.iter().any(|s| s.kind == SpanKind::EntityDecoded);
    let has_synthetic = spans.iter().any(|s| s.kind == SpanKind::Synthetic);
    assert!(has_direct, "expected a Direct span");
    assert!(has_entity, "expected an EntityDecoded span");
    assert!(has_synthetic, "expected a Synthetic (img alt) span");
}

// =============================================================================
// Output parity: strip_to_text_with_spans matches strip_to_text
// =============================================================================

#[test]
fn spans_output_matches_strip_to_text() {
    // Prior behavior: the spans path skipped cleanup_whitespace, producing
    // different text than strip_to_text. Now they should agree byte-for-byte.
    let cases = [
        "<p>Hello    world</p>",
        "<article><p>One.</p>\n\n\n<p>Two.</p></article>",
        "<p>  leading  and  trailing  </p>",
        "<p>Mix\t\t\ntabs</p>",
    ];
    for html in cases {
        let plain = deformat::html::strip_to_text(html);
        let (spanned, _) = strip_to_text_with_spans(html);
        assert_eq!(spanned, plain, "mismatch for {html:?}");
    }
}

#[test]
fn spans_survive_whitespace_collapse() {
    // "Hello" and "world" are separated by multiple spaces in source; after
    // cleanup the output contains a single space. The span covering the
    // whole run is demoted from Direct to EntityDecoded (byte counts
    // differ), so we verify via source_range rather than per-byte
    // interpolation.
    let html = "<p>Hello    world</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "Hello world");

    let src = spans.source_range(0, text.len()).unwrap();
    let source = &html[src.0..src.1];
    assert!(
        source.contains("Hello"),
        "source contains Hello: {source:?}"
    );
    assert!(
        source.contains("world"),
        "source contains world: {source:?}"
    );
}

// =============================================================================
// Regression: path-attribution on closing inline tags (v0.11.0 bug)
//
// Before 0.11.0, `</a>` produced `effective = "/a"` due to a chained
// `unwrap_or(&tag_lower)` footgun, so `path_stack.truncate` never popped the
// anchor. Text emitted after `</a>` carried a stale `a` component in its
// PathSpan.path. The fix at src/html.rs:509 uses `tag_lower.trim_matches('/')`.
// These tests guard against regression.
// =============================================================================

#[test]
fn closing_anchor_does_not_leak_into_path() {
    // The bytes after </a> must NOT carry the anchor in their path.
    let html = r#"<article><p>Before <a href="/">link</a> after.</p></article>"#;
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("after."));

    // Find the span covering "after." (must be after the </a>)
    let after_span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("after"))
        .expect("span for 'after' exists");
    assert!(
        !after_span.path.ends_with("/a") && !after_span.path.contains("/a/"),
        "path must not carry closed anchor: {:?}",
        after_span.path
    );
    // It should still carry the surrounding article/p path.
    assert!(
        after_span.path.contains("p"),
        "path has p: {:?}",
        after_span.path
    );
    assert!(
        after_span.path.contains("article"),
        "path has article: {:?}",
        after_span.path
    );
}

#[test]
fn closing_inline_does_not_leak_for_various_tags() {
    // Same regression for other inline elements that can get mis-truncated.
    for tag in ["b", "i", "em", "strong", "span", "u", "s", "code"] {
        let html = format!("<div><p>Before <{tag}>X</{tag}> after.</p></div>");
        let (text, spans) = strip_to_text_with_paths(&html);
        let after_span = spans
            .iter()
            .find(|s| text[s.output_start..s.output_end].contains("after"))
            .unwrap_or_else(|| panic!("no span for 'after' with tag {tag}"));
        let leaked = after_span.path.contains(&format!("/{tag}")) || after_span.path.ends_with(tag);
        // The /tag check may match /strong inside /strong; check more carefully:
        // the path must NOT have <tag> as a *leaf* component after close.
        let components: Vec<&str> = after_span.path.split('/').collect();
        let leaf_is_tag = components
            .last()
            .map(|s| {
                s.trim_end_matches(|c: char| c == ']' || c.is_ascii_digit() || c == '[') == tag
            })
            .unwrap_or(false);
        assert!(
            !leaf_is_tag,
            "tag {tag} leaked as leaf in path {:?} (components {:?}, heuristic leaked={leaked})",
            after_span.path, components
        );
    }
}

// =============================================================================
// Sibling indexing in paths
// =============================================================================

#[test]
fn sibling_indexing_in_path() {
    // When siblings of the same tag repeat, later siblings get [n] index.
    let html = "<div><p>First</p><p>Second</p><p>Third</p></div>";
    let (text, spans) = strip_to_text_with_paths(html);

    let second = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Second"))
        .unwrap();
    let third = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Third"))
        .unwrap();

    // At minimum Second/Third must carry *some* index bracket in their path.
    assert!(
        second.path.contains("p[2]") || second.path.contains("p[1]"),
        "second p path should carry index: {:?}",
        second.path
    );
    assert!(
        third.path.contains("p[3]") || third.path.contains("p[2]"),
        "third p path should carry index: {:?}",
        third.path
    );
}

#[test]
fn sibling_indexing_only_when_siblings_repeat() {
    // With one p (no repetition), no [n] should appear.
    let html = "<div><p>Solo</p></div>";
    let (text, spans) = strip_to_text_with_paths(html);
    let span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("Solo"))
        .unwrap();
    assert!(
        !span.path.contains('['),
        "single sibling should not get [n]: {:?}",
        span.path
    );
}

// =============================================================================
// Span-map structural invariants
// =============================================================================

#[test]
fn spans_are_monotonic_and_non_overlapping_in_output() {
    let html = r#"<article>
        <h1>Title</h1>
        <p>Paragraph with <b>bold</b> and <i>italic</i> text and an
           <img src="x" alt="pic"> inline.</p>
        <p>Entity: &amp; &lt; &#8364;.</p>
    </article>"#;
    let (_text, spans) = strip_to_text_with_spans(html);

    let mut prev_end: usize = 0;
    for (i, s) in spans.iter().enumerate() {
        assert!(
            s.output_start <= s.output_end,
            "span {i} has inverted output range: {}..{}",
            s.output_start,
            s.output_end
        );
        assert!(
            s.source_start <= s.source_end,
            "span {i} has inverted source range: {}..{}",
            s.source_start,
            s.source_end
        );
        assert!(
            s.output_start >= prev_end,
            "span {i} output {} overlaps prior end {prev_end}",
            s.output_start
        );
        prev_end = s.output_end;
    }
}

#[test]
fn source_ranges_are_utf8_boundaries() {
    // Source ranges must point at UTF-8 char boundaries so `html[src.0..src.1]`
    // never panics.
    let html = "<p>Café with &eacute; and <b>ü</b> text.</p>";
    let (_text, spans) = strip_to_text_with_spans(html);
    for (i, s) in spans.iter().enumerate() {
        assert!(
            html.is_char_boundary(s.source_start),
            "span {i} source_start {} not on char boundary",
            s.source_start
        );
        assert!(
            html.is_char_boundary(s.source_end),
            "span {i} source_end {} not on char boundary",
            s.source_end
        );
    }
}

#[test]
fn output_positions_are_utf8_boundaries() {
    let html = "<p>Café with &eacute; and <b>ü</b> text.</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    for (i, s) in spans.iter().enumerate() {
        assert!(
            text.is_char_boundary(s.output_start),
            "span {i} output_start {} not on char boundary",
            s.output_start
        );
        assert!(
            text.is_char_boundary(s.output_end),
            "span {i} output_end {} not on char boundary",
            s.output_end
        );
    }
}

// =============================================================================
// source_position semantics per SpanKind
// =============================================================================

#[test]
fn source_position_on_synthetic_span_returns_tag_start() {
    // <img alt="..."> produces a Synthetic span. source_position on any byte
    // inside that run should return the span's source_start (the start of the
    // img tag).
    let html = r#"<p><img src="a.jpg" alt="sunset"></p>"#;
    let (text, spans) = strip_to_text_with_spans(html);
    let alt_start = text.find("sunset").expect("alt text present");
    let alt_end = alt_start + "sunset".len();

    let synth = spans
        .iter()
        .find(|s| s.kind == SpanKind::Synthetic)
        .expect("Synthetic span exists");

    // source_position at any interior point should equal synth.source_start.
    for p in alt_start..alt_end {
        let sp = spans.source_position(p).expect("alt byte has a span");
        assert_eq!(
            sp, synth.source_start,
            "synthetic interior byte {p} should map to tag start"
        );
    }
    // The source range should bracket the <img ...> tag.
    let src_slice = &html[synth.source_start..synth.source_end];
    assert!(
        src_slice.contains("<img") && src_slice.contains("alt="),
        "synthetic source range covers img tag: {src_slice}"
    );
}

#[test]
fn source_position_on_entity_span_returns_entity_start() {
    let html = "<p>&eacute;</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    let accent_pos = text.find('\u{00E9}').unwrap();
    let accent_end = accent_pos + '\u{00E9}'.len_utf8();

    // Every byte inside the decoded entity should map to the same entity start.
    let p0 = spans.source_position(accent_pos).unwrap();
    for p in accent_pos..accent_end {
        let sp = spans.source_position(p).unwrap();
        assert_eq!(
            sp, p0,
            "entity interior byte {p} should collapse to entity start"
        );
    }
    assert_eq!(&html[p0..p0 + "&eacute;".len()], "&eacute;");
}

// =============================================================================
// source_range across boundaries
// =============================================================================

#[test]
fn source_range_straddling_multiple_spans_returns_union() {
    // Output is "AB&CD"; query spanning "B&C" should return a source range
    // covering from 'B' through '&amp;' through 'C'.
    let html = "<p>AB&amp;CD</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "AB&CD");

    let src = spans.source_range(1, 4).expect("range has coverage"); // "B&C"
    let src_slice = &html[src.0..src.1];
    assert!(src_slice.contains('B'), "covers B: {src_slice}");
    assert!(src_slice.contains("&amp;"), "covers entity: {src_slice}");
    assert!(src_slice.contains('C'), "covers C: {src_slice}");
}

#[test]
fn source_range_across_entity_boundary() {
    let html = "<p>a&amp;b&amp;c</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "a&b&c");

    // Full-output query covers everything from 'a' through 'c'.
    let full = spans.source_range(0, text.len()).unwrap();
    let slice = &html[full.0..full.1];
    assert!(slice.contains('a'));
    assert!(slice.contains('c'));
    assert_eq!(slice.matches("&amp;").count(), 2);
}

// =============================================================================
// Whitespace-collapse kind demotion
// =============================================================================

#[test]
fn whitespace_collapse_demotes_direct_to_entity_decoded() {
    // "hello    world" pre-cleanup (14 bytes) collapses to "hello world"
    // post-cleanup (11 bytes). The span is Direct in the scanner but its byte
    // counts don't match after remap → demoted to EntityDecoded.
    let html = "<p>hello    world</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "hello world");

    // At least one span should be EntityDecoded purely because of collapse
    // (there are no actual entities in the input).
    let demoted = spans.iter().any(|s| s.kind == SpanKind::EntityDecoded);
    assert!(
        demoted,
        "expected collapse to produce at least one EntityDecoded span"
    );
}

// =============================================================================
// Self-closing tags and unclosed tags
// =============================================================================

#[test]
fn self_closing_tags_do_not_nest_path() {
    // <br/> and <hr/> are self-closing; they must not push onto path_stack.
    let html = "<div><p>A<br/>B<hr/>C</p></div>";
    let (text, spans) = strip_to_text_with_paths(html);

    for word in ["A", "B", "C"] {
        let span = spans
            .iter()
            .find(|s| text[s.output_start..s.output_end].contains(word));
        if let Some(span) = span {
            assert!(
                !span.path.contains("br"),
                "{word}: br should not appear in path: {:?}",
                span.path
            );
            assert!(
                !span.path.contains("hr"),
                "{word}: hr should not appear in path: {:?}",
                span.path
            );
            assert!(
                span.path.contains("p"),
                "{word}: p should appear in path: {:?}",
                span.path
            );
        }
    }
}

#[test]
fn unclosed_tag_does_not_corrupt_path() {
    // Pathological: tag never closes. Must not panic; spans for later text
    // may include the unclosed tag in their path, but path components must
    // remain well-formed (no ".." or empty components from bad indexing).
    let html = "<article><p>Open <span>nested text here";
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("nested text"));
    for s in spans.iter() {
        for component in s.path.split('/') {
            assert!(
                !component.is_empty() || s.path.is_empty(),
                "empty path component in {:?}",
                s.path
            );
        }
    }
}

// =============================================================================
// Multibyte text
// =============================================================================

#[test]
fn multibyte_text_span_source_matches() {
    // Cyrillic + CJK both use >1 byte per char. Source ranges must still be
    // valid UTF-8 boundaries and the slice must contain the multibyte text.
    let html = "<p>Привет, 世界!</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("Привет"));
    assert!(text.contains("世界"));

    // The whole-output source range should include the multibyte content.
    let src = spans.source_range(0, text.len()).unwrap();
    let slice = &html[src.0..src.1];
    assert!(slice.contains("Привет"));
    assert!(slice.contains("世界"));
}

#[test]
fn multibyte_source_position_is_byte_exact_for_direct() {
    // For a plain Direct run, a multibyte char's bytes should all map into
    // the source at byte-exact offsets (the source chars are the same bytes).
    let html = "<p>Hello 世界</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    // Find the start of '世' in output.
    let cjk_out = text.find('世').unwrap();
    let cjk_src = spans.source_position(cjk_out).unwrap();
    // Confirm html[cjk_src..] starts with '世'.
    assert!(
        html[cjk_src..].starts_with('世'),
        "Direct multibyte byte should map byte-exact: got {:?}",
        &html[cjk_src..cjk_src + 3]
    );
}

// =============================================================================
// Consecutive entities
// =============================================================================

#[test]
fn consecutive_entities_each_get_span() {
    let html = "<p>&amp;&lt;&gt;</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert_eq!(text, "&<>");

    // Each of the three entities should have its own EntityDecoded span.
    let entity_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.kind == SpanKind::EntityDecoded)
        .collect();
    assert!(
        entity_spans.len() >= 3,
        "expected >=3 entity spans, got {} ({:?})",
        entity_spans.len(),
        entity_spans
            .iter()
            .map(|s| (s.output_start, s.output_end))
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// Property: every byte in a covered output range has a source position
// =============================================================================

#[test]
fn every_byte_in_a_span_has_source_position() {
    let html = "<p>Hello, <b>world</b>! &amp; more.</p>";
    let (_text, spans) = strip_to_text_with_spans(html);
    for (i, s) in spans.iter().enumerate() {
        for p in s.output_start..s.output_end {
            let sp = spans.source_position(p);
            assert!(
                sp.is_some(),
                "byte {p} in span {i} ({}..{}) should have a source position",
                s.output_start,
                s.output_end
            );
        }
    }
}

// =============================================================================
// Monotonicity: widening the query cannot shrink the source range
// =============================================================================

#[test]
fn source_range_union_is_monotone_under_widening() {
    let html = "<p>ABCDEFGHIJ</p>";
    let (text, spans) = strip_to_text_with_spans(html);

    // For nested output ranges (2..4) ⊆ (1..5) ⊆ (0..text.len()), source
    // ranges must satisfy: inner.0 >= outer.0 and inner.1 <= outer.1 is NOT
    // required (the union covers all overlapping spans), but wider query
    // must never yield a *strictly narrower* source range.
    let narrow = spans.source_range(2, 4).unwrap();
    let mid = spans.source_range(1, 5).unwrap();
    let wide = spans.source_range(0, text.len()).unwrap();

    assert!(mid.0 <= narrow.0 && mid.1 >= narrow.1);
    assert!(wide.0 <= mid.0 && wide.1 >= mid.1);
}

// =============================================================================
// Trim behavior
// =============================================================================

#[test]
fn trim_does_not_produce_zero_length_spans() {
    // Leading/trailing whitespace gets trimmed. No span should survive with
    // output_start == output_end.
    let html = "<p>   hello   </p>";
    let (_text, spans) = strip_to_text_with_spans(html);
    for s in spans.iter() {
        assert!(
            s.output_start < s.output_end,
            "span collapsed to zero length: {}..{}",
            s.output_start,
            s.output_end
        );
    }
}

#[test]
fn trim_does_not_produce_zero_length_path_spans() {
    let html = "<p>   hello world   </p>";
    let (_text, spans) = strip_to_text_with_paths(html);
    for s in &spans {
        assert!(
            s.output_start < s.output_end,
            "path span collapsed to zero length: {}..{}",
            s.output_start,
            s.output_end
        );
    }
}

#[test]
fn path_spans_indexable_after_trailing_whitespace_trim() {
    // Regression: strip_to_text_with_paths used to only rebase spans for
    // leading-whitespace trimming, not trailing. With trailing ws in the
    // source, spans could end up with output_end > trimmed_text.len(),
    // causing caller panics on text[os..oe]. Fixed at src/html.rs ~606
    // by computing `text.trim()` once and clamping span bounds with .min().
    let cases = [
        "<article><p>Before <a>link</a> after </p></article>",
        "<p>   hello world   </p>",
        "<div><p>Leading trailing  </p></div>",
        "<p>x  </p>",
    ];
    for html in cases {
        let (text, spans) = strip_to_text_with_paths(html);
        for s in &spans {
            assert!(
                s.output_end <= text.len(),
                "span out {}..{} escapes trimmed text (len {}) for html {:?}",
                s.output_start,
                s.output_end,
                text.len(),
                html
            );
            // And indexing must not panic.
            let _ = &text[s.output_start..s.output_end];
        }
    }
}

// =============================================================================
// Nested img inside container: path must reflect container, not just <img>
// =============================================================================

#[test]
fn path_for_img_alt_in_deeply_nested_container() {
    // `<img>` without a trailing slash is treated as open-tag form by the
    // scanner, so it DOES appear on the path stack when the alt span is
    // emitted — the path leaf is the img itself. The important assertion is
    // that all ancestor containers appear.
    let html =
        r#"<article><section><figure><img src="x.jpg" alt="diagram"></figure></section></article>"#;
    let (text, spans) = strip_to_text_with_paths(html);
    assert!(text.contains("diagram"));

    let span = spans
        .iter()
        .find(|s| text[s.output_start..s.output_end].contains("diagram"))
        .unwrap();
    assert_eq!(span.kind, SpanKind::Synthetic);
    for expected in ["article", "section", "figure"] {
        assert!(
            span.path.contains(expected),
            "path must contain {expected}: {:?}",
            span.path
        );
    }
}

// =============================================================================
// source_range edge cases
// =============================================================================

#[test]
fn source_range_at_exact_span_boundary() {
    let html = "<p>AAA</p><p>BBB</p>";
    let (text, spans) = strip_to_text_with_spans(html);
    assert!(text.contains("AAA"));
    assert!(text.contains("BBB"));

    // Query exactly at "AAA" boundary.
    let aaa_start = text.find("AAA").unwrap();
    let aaa_end = aaa_start + "AAA".len();
    let src = spans.source_range(aaa_start, aaa_end).unwrap();
    assert!(html[src.0..src.1].contains("AAA"));
}

#[test]
fn source_range_empty_query_past_end_returns_none() {
    let (text, spans) = strip_to_text_with_spans("<p>hello</p>");
    // Empty range past the end of the span coverage yields None.
    assert!(spans.source_range(text.len(), text.len()).is_none());
    assert!(spans.source_range(text.len() + 1, text.len() + 1).is_none());
}

// =============================================================================
// Mixed Direct + EntityDecoded kind distribution
// =============================================================================

#[test]
fn mixed_kinds_coexist_in_one_document() {
    let html = r#"<p>Direct text &amp; entity and <img alt="alt">.</p>"#;
    let (_text, spans) = strip_to_text_with_spans(html);

    let kinds: std::collections::HashSet<SpanKind> = spans.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&SpanKind::Direct), "Direct present");
    assert!(
        kinds.contains(&SpanKind::EntityDecoded),
        "EntityDecoded present"
    );
    assert!(kinds.contains(&SpanKind::Synthetic), "Synthetic present");
}

// =============================================================================
// PathSpan kind tagging
// =============================================================================

#[test]
fn path_span_kinds_match_origin() {
    let html = r#"<p>Direct &amp; <img alt="x"> text</p>"#;
    let (_text, spans) = strip_to_text_with_paths(html);
    let kinds: std::collections::HashSet<SpanKind> = spans.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&SpanKind::Direct));
    assert!(kinds.contains(&SpanKind::EntityDecoded));
    assert!(kinds.contains(&SpanKind::Synthetic));
}
