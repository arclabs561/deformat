//! Property tests for deformat.
//!
//! These verify structural invariants that must hold for *any* input,
//! not just specific test cases.

use proptest::prelude::*;

// =============================================================================
// Strategies
// =============================================================================

/// Generate arbitrary HTML-like strings with tags, entities, and text.
fn arb_html_fragment() -> impl Strategy<Value = String> {
    let tag_names = prop::sample::select(vec![
        "p",
        "div",
        "span",
        "b",
        "i",
        "a",
        "h1",
        "h2",
        "h3",
        "li",
        "ul",
        "ol",
        "td",
        "th",
        "tr",
        "table",
        "article",
        "section",
        "main",
        "blockquote",
        "em",
        "strong",
        "code",
        "pre",
    ]);
    let skip_tag_names = prop::sample::select(vec![
        "script", "style", "nav", "footer", "aside", "noscript", "template", "svg",
    ]);
    let entities = prop::sample::select(vec![
        "&amp;",
        "&lt;",
        "&gt;",
        "&quot;",
        "&apos;",
        "&nbsp;",
        "&eacute;",
        "&mdash;",
        "&ndash;",
        "&copy;",
        "&reg;",
        "&euro;",
        "&hellip;",
        "&ldquo;",
        "&rdquo;",
        "&#169;",
        "&#x1F4A9;",
        "&#0;",
        "&#8212;",
    ]);

    prop::collection::vec(
        prop_oneof![
            // Plain text words
            "[a-zA-Z0-9 .,!?'-]{1,30}".prop_map(|s| s),
            // Opening + closing tag around text
            (tag_names.clone(), "[a-zA-Z0-9 .,]{0,20}")
                .prop_map(|(tag, text)| { format!("<{tag}>{text}</{tag}>") }),
            // Skip tag with hidden content
            (skip_tag_names, "[a-zA-Z0-9 .,]{0,20}")
                .prop_map(|(tag, text)| { format!("<{tag}>{text}</{tag}>") }),
            // Entity
            entities.prop_map(|s| s.to_string()),
            // Self-closing tag
            tag_names.prop_map(|tag| format!("<{tag} />")),
            // HTML comment
            "[a-zA-Z0-9 .,!?-]{0,20}".prop_map(|text| format!("<!--{text}-->")),
        ],
        1..20,
    )
    .prop_map(|parts: Vec<String>| parts.join(""))
}

/// Generate strings that look like entity references (for entity decoding tests).
fn arb_entity_like() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid named entities
        prop::sample::select(vec![
            "&amp;", "&lt;", "&gt;", "&nbsp;", "&eacute;", "&copy;", "&euro;", "&mdash;",
        ])
        .prop_map(|s| s.to_string()),
        // Valid numeric entities
        (1u32..0x10FFFF).prop_map(|n| format!("&#{n};")),
        // Valid hex entities
        (1u32..0x10FFFF).prop_map(|n| format!("&#x{n:X};")),
        // Semicolon-optional named entities
        prop::sample::select(vec!["&amp", "&lt", "&gt", "&nbsp", "&eacute", "&copy"])
            .prop_map(|s| s.to_string()),
        // Random &-prefixed strings
        "[a-zA-Z]{1,10}".prop_map(|s| format!("&{s}")),
    ]
}

// =============================================================================
// Invariant: output never contains raw HTML tags
// =============================================================================

proptest! {
    #[test]
    fn output_never_contains_html_tags(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        // Decoded entities like &lt; legitimately produce '<' in output.
        // Only flag tags that were NOT produced by entity decoding.
        // We check: if the input didn't contain the literal entity encodings
        // that produce '<' and '>', then any tag in the output is a real bug.
        //
        // Skip this check when the input contains &lt; or &#60; or &#x3C;
        // (which decode to '<') since the resulting '<' is correct behavior.
        if html.contains("&lt") || html.contains("&#60") || html.contains("&#x3C")
            || html.contains("&#x3c")
        {
            // Entity-decoded '<' can form tag-like patterns -- not a bug
            return Ok(());
        }
        const TAG_NAMES: &[&str] = &[
            "<script", "<style", "<div", "<span", "<p ", "<p>",
            "<a ", "<a>", "<b>", "<b ", "<i>", "<i ",
            "<em>", "<em ", "<strong", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
            "<table", "<tr", "<td", "<th", "<ul", "<ol", "<li",
            "<nav", "<header", "<footer", "<aside", "<form", "<img",
            "<br", "<hr", "<section", "<article", "<main", "<blockquote",
            "<code", "<pre",
        ];
        let text_lower = text.to_lowercase();
        for tag in TAG_NAMES {
            prop_assert!(
                !text_lower.contains(tag),
                "HTML tag {:?} found in output: {:?}\nInput: {:?}",
                tag,
                text,
                html
            );
        }
    }
}

// =============================================================================
// Invariant: output never contains double spaces
// =============================================================================

proptest! {
    #[test]
    fn output_never_has_double_spaces(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        prop_assert!(
            !text.contains("  "),
            "Double spaces found in output: {:?}\nInput: {:?}",
            text,
            html
        );
    }
}

// =============================================================================
// Invariant: output is always trimmed
// =============================================================================

proptest! {
    #[test]
    fn output_is_always_trimmed(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        let trimmed = text.trim().to_string();
        prop_assert_eq!(
            text,
            trimmed,
            "Output not trimmed for input: {:?}",
            html
        );
    }
}

// =============================================================================
// Invariant: no C0 control characters in output (except \n, \r, \t)
// =============================================================================

proptest! {
    #[test]
    fn output_has_no_control_chars(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        let bad_chars: Vec<_> = text
            .chars()
            .filter(|&c| (c as u32) < 0x20 && c != '\n' && c != '\r' && c != '\t')
            .collect();
        prop_assert!(
            bad_chars.is_empty(),
            "Control characters {:?} found in output: {:?}\nInput: {:?}",
            bad_chars.iter().map(|c| format!("U+{:04X}", *c as u32)).collect::<Vec<_>>(),
            text,
            html
        );
    }
}

// =============================================================================
// Invariant: script/style content never leaks into output
// =============================================================================

proptest! {
    #[test]
    fn script_content_never_leaks(
        content in "[a-zA-Z]{5,15}",
        wrapper in prop::sample::select(vec!["script", "style"]).prop_map(|s| s.to_string()),
    ) {
        let html = format!("<{wrapper}>{content}</{wrapper}><p>visible</p>");
        let text = deformat::html::strip_to_text(&html);
        prop_assert!(
            !text.contains(&content),
            "{wrapper} content leaked: {:?}\nInput: {:?}",
            text,
            html
        );
        prop_assert!(
            text.contains("visible"),
            "visible content missing: {:?}",
            text
        );
    }
}

// =============================================================================
// Panic-freedom: entity decoding tolerates any entity-like input
// =============================================================================

proptest! {
    // No assertion: the test passes iff strip_to_text returns without panicking.
    #[test]
    fn entity_decoding_does_not_panic(entity in arb_entity_like()) {
        let html = format!("<p>{entity}</p>");
        let _text = deformat::html::strip_to_text(&html);
    }
}

// =============================================================================
// Panic-freedom: strip_to_text tolerates arbitrary input
// =============================================================================

proptest! {
    // No assertion: the test passes iff strip_to_text returns without panicking.
    #[test]
    fn strip_does_not_panic(input in ".*") {
        let _text = deformat::html::strip_to_text(&input);
    }
}

// =============================================================================
// Invariant: plain text content preserved through tags
// =============================================================================

proptest! {
    #[test]
    fn plain_text_content_preserved(text in "[a-zA-Z0-9]{1,50}") {
        let html = format!("<p>{text}</p>");
        let result = deformat::html::strip_to_text(&html);
        prop_assert!(
            result.contains(&text),
            "Plain text not preserved: input={text:?}, output={result:?}"
        );
    }
}

// =============================================================================
// Invariant: extract() format detection is consistent with detect()
// =============================================================================

proptest! {
    #[test]
    fn extract_format_consistent(html in arb_html_fragment()) {
        let result = deformat::extract(&html).unwrap();
        let detected = deformat::detect::detect_str(&html);
        prop_assert_eq!(
            result.format,
            detected,
            "Format mismatch: extract={:?}, detect={:?}\nInput: {:?}",
            result.format,
            detected,
            &html[..html.len().min(80)]
        );
    }
}

// =============================================================================
// Invariant: output length never exceeds input length
// =============================================================================

proptest! {
    #[test]
    fn output_never_longer_than_input(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        prop_assert!(
            text.len() <= html.len(),
            "Output longer than input: output={} bytes, input={} bytes\nInput: {:?}",
            text.len(),
            html.len(),
            &html[..html.len().min(80)]
        );
    }
}

// =============================================================================
// Invariant: skip tag content never leaks (all skip tag types)
// =============================================================================

proptest! {
    #[test]
    fn skip_tag_content_never_leaks(
        content in "[a-zA-Z]{5,15}",
        tag in prop::sample::select(vec![
            "nav", "footer", "aside", "noscript",
            "template", "svg", "textarea", "iframe",
        ]).prop_map(|s| s.to_string()),
    ) {
        let html = format!("<{tag}>{content}</{tag}><p>visible</p>");
        let text = deformat::html::strip_to_text(&html);
        prop_assert!(
            !text.contains(&content),
            "{tag} content leaked: {:?}\nInput: {:?}",
            text,
            html
        );
    }
}

// =============================================================================
// Invariant: nested skip tags don't leak inner content
// =============================================================================

proptest! {
    #[test]
    fn nested_skip_tags_no_leak(
        content in "[a-zA-Z]{5,15}",
        outer in prop::sample::select(vec!["footer", "nav", "aside", "noscript"])
            .prop_map(|s| s.to_string()),
        inner in prop::sample::select(vec!["nav", "aside", "noscript", "template"])
            .prop_map(|s| s.to_string()),
    ) {
        let html = format!(
            "<{outer}><{inner}>{content}</{inner}></{outer}><p>visible</p>"
        );
        let text = deformat::html::strip_to_text(&html);
        prop_assert!(
            !text.contains(&content),
            "nested {outer}>{inner} content leaked: {:?}",
            text
        );
        prop_assert!(
            text.contains("visible"),
            "visible content missing after nested skip: {:?}",
            text
        );
    }
}

// =============================================================================
// Invariant: no invisible Unicode characters in output
// =============================================================================

proptest! {
    #[test]
    fn output_has_no_invisible_chars(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        let invisible: Vec<_> = text
            .chars()
            .filter(|&c| matches!(c,
                '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{200E}' | '\u{200F}'
                | '\u{00AD}' | '\u{2060}' | '\u{FEFF}'
                | '\u{202A}' | '\u{202B}' | '\u{202C}' | '\u{202D}' | '\u{202E}'
                | '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
                | '\u{180E}' | '\u{FE0F}' | '\u{00A0}'
            ))
            .collect();
        prop_assert!(
            invisible.is_empty(),
            "Invisible chars {:?} found in output: {:?}",
            invisible.iter().map(|c| format!("U+{:04X}", *c as u32)).collect::<Vec<_>>(),
            text
        );
    }
}

// =============================================================================
// Invariant: decode_entities is idempotent for plain text
// =============================================================================

proptest! {
    #[test]
    fn decode_entities_preserves_plain_text(text in "[a-zA-Z0-9 .,!?'-]{0,100}") {
        let result = deformat::html::decode_entities(&text);
        prop_assert_eq!(
            result,
            text,
            "Plain text was modified by decode_entities"
        );
    }
}

// =============================================================================
// Invariant: decode_entities is idempotent (single pass = double pass)
// =============================================================================

proptest! {
    #[test]
    fn decode_entities_idempotent(html in arb_html_fragment()) {
        let once = deformat::html::decode_entities(&html);
        let twice = deformat::html::decode_entities(&once);
        prop_assert_eq!(
            once,
            twice,
            "decode_entities not idempotent on input: {:?}",
            &html[..html.len().min(80)]
        );
    }
}

// =============================================================================
// Invariant: wiki ref markers stripped when StripOptions::wikipedia() is used
// =============================================================================

proptest! {
    #[test]
    fn wiki_ref_markers_stripped_with_option(
        num in 1u32..999,
        text_before in "[a-zA-Z]{3,10}",
        text_after in "[a-zA-Z]{3,10}",
    ) {
        use deformat::html::{strip_to_text_with_options, StripOptions};
        // Numeric refs [N]
        let html = format!("<p>{text_before}[{num}]{text_after}</p>");
        let result = strip_to_text_with_options(&html, &StripOptions::wikipedia());
        let marker = format!("[{num}]");
        prop_assert!(
            !result.contains(&marker),
            "Wiki ref marker {:?} found in output: {:?}",
            marker,
            result
        );
        prop_assert!(result.contains(&text_before));
        prop_assert!(result.contains(&text_after));
    }

    #[test]
    fn wiki_edit_markers_stripped_with_option(
        text_before in "[a-zA-Z]{3,10}",
        text_after in "[a-zA-Z]{3,10}",
    ) {
        use deformat::html::{strip_to_text_with_options, StripOptions};
        let html = format!("<p>{text_before} [edit] {text_after}</p>");
        let result = strip_to_text_with_options(&html, &StripOptions::wikipedia());
        prop_assert!(
            !result.contains("[edit]"),
            "Wiki edit marker found in output: {:?}",
            result
        );
        prop_assert!(result.contains(&text_before));
        prop_assert!(result.contains(&text_after));
    }

    #[test]
    fn wiki_citation_needed_stripped_with_option(
        text_before in "[a-zA-Z]{3,10}",
        text_after in "[a-zA-Z]{3,10}",
    ) {
        use deformat::html::{strip_to_text_with_options, StripOptions};
        let html = format!("<p>{text_before} [citation needed] {text_after}</p>");
        let result = strip_to_text_with_options(&html, &StripOptions::wikipedia());
        prop_assert!(
            !result.contains("[citation needed]"),
            "Citation needed marker found in output: {:?}",
            result
        );
        prop_assert!(result.contains(&text_before));
        prop_assert!(result.contains(&text_after));
    }
}

// =============================================================================
// Invariant: wiki ref markers preserved by default
// =============================================================================

proptest! {
    #[test]
    fn wiki_ref_markers_preserved_by_default(
        num in 1u32..999,
        text_before in "[a-zA-Z]{3,10}",
        text_after in "[a-zA-Z]{3,10}",
    ) {
        let html = format!("<p>{text_before}[{num}]{text_after}</p>");
        let result = deformat::html::strip_to_text(&html);
        let marker = format!("[{num}]");
        prop_assert!(
            result.contains(&marker),
            "Wiki ref marker {:?} should be preserved by default: {:?}",
            marker,
            result
        );
    }
}

// =============================================================================
// Invariant: plain text without HTML passes through strip_to_text unchanged
// (modulo whitespace normalization)
// =============================================================================

proptest! {
    #[test]
    fn plain_text_passthrough(text in "[a-zA-Z0-9,.!? ]{1,100}") {
        let result = deformat::html::strip_to_text(&text);
        // The text has no HTML markers, so content should be preserved
        // (only whitespace normalization and trimming may differ)
        let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        prop_assert_eq!(
            result,
            normalized,
            "Plain text not preserved through strip_to_text"
        );
    }
}

// =============================================================================
// Invariant: strip_to_text is idempotent (output is already clean text)
// =============================================================================

proptest! {
    #[test]
    fn strip_is_idempotent(html in arb_html_fragment()) {
        let once = deformat::html::strip_to_text(&html);
        // Entity decoding can produce '<' / '>' from &lt; / &gt;, which the
        // second pass would then treat as tag markers. Skip check in that case.
        if once.contains('<') || once.contains('>') {
            return Ok(());
        }
        let twice = deformat::html::strip_to_text(&once);
        prop_assert_eq!(
            once,
            twice,
            "strip_to_text not idempotent on input: {:?}",
            &html[..html.len().min(80)]
        );
    }
}

// =============================================================================
// Invariant: detect_str agrees with memchr('<', ...) heuristic
// =============================================================================

proptest! {
    #[test]
    fn no_angle_bracket_means_not_html(text in "[a-zA-Z0-9 .,!?'-]{1,200}") {
        // Text with no '<' should never be detected as HTML
        prop_assert_eq!(
            deformat::detect::detect_str(&text),
            deformat::detect::Format::PlainText,
            "Text without '<' detected as HTML: {:?}",
            &text[..text.len().min(80)]
        );
    }
}

// =============================================================================
// SpanMap structural invariants
// =============================================================================

proptest! {
    /// Every span's [output_start, output_end) is a valid byte range in text.
    /// Every span's [source_start, source_end) is a valid byte range in html.
    /// Both pairs sit on UTF-8 char boundaries.
    #[test]
    fn spans_have_valid_bounds_and_utf8_boundaries(html in arb_html_fragment()) {
        let (text, spans) = deformat::html::strip_to_text_with_spans(&html);
        for (i, s) in spans.iter().enumerate() {
            prop_assert!(s.output_start <= s.output_end, "span {i} output inverted");
            prop_assert!(s.source_start <= s.source_end, "span {i} source inverted");
            prop_assert!(s.output_end <= text.len(), "span {i} output OOB");
            prop_assert!(s.source_end <= html.len(), "span {i} source OOB");
            prop_assert!(text.is_char_boundary(s.output_start), "span {i} output_start {} not on char boundary", s.output_start);
            prop_assert!(text.is_char_boundary(s.output_end), "span {i} output_end {} not on char boundary", s.output_end);
            prop_assert!(html.is_char_boundary(s.source_start), "span {i} source_start {} not on char boundary", s.source_start);
            prop_assert!(html.is_char_boundary(s.source_end), "span {i} source_end {} not on char boundary", s.source_end);
        }
    }
}

proptest! {
    /// Spans are in output order and do not overlap each other in output space.
    #[test]
    fn spans_are_sorted_and_non_overlapping(html in arb_html_fragment()) {
        let (_text, spans) = deformat::html::strip_to_text_with_spans(&html);
        let mut prev_end: usize = 0;
        for (i, s) in spans.iter().enumerate() {
            prop_assert!(s.output_start >= prev_end, "span {i} output_start {} < prev_end {prev_end}", s.output_start);
            prev_end = s.output_end;
        }
    }
}

proptest! {
    /// Widening an output-range query can never yield a strictly narrower
    /// source range (union is monotone under widening).
    #[test]
    fn source_range_is_monotone_under_widening(html in arb_html_fragment()) {
        let (text, spans) = deformat::html::strip_to_text_with_spans(&html);
        if text.len() < 4 {
            return Ok(());
        }
        let mid = text.len() / 2;
        let narrow = spans.source_range(mid, mid + 1);
        let wide = spans.source_range(mid.saturating_sub(1), mid + 2);
        if let (Some(n), Some(w)) = (narrow, wide) {
            prop_assert!(w.0 <= n.0, "wide.0 {} > narrow.0 {}", w.0, n.0);
            prop_assert!(w.1 >= n.1, "wide.1 {} < narrow.1 {}", w.1, n.1);
        }
    }
}

proptest! {
    /// source_position on any byte in a span's output range returns Some.
    #[test]
    fn every_covered_output_byte_has_source_position(html in arb_html_fragment()) {
        let (_text, spans) = deformat::html::strip_to_text_with_spans(&html);
        // Sample at most 32 spans to keep proptest fast.
        for s in spans.iter().take(32) {
            if s.output_end - s.output_start > 64 {
                continue;
            }
            for p in s.output_start..s.output_end {
                prop_assert!(
                    spans.source_position(p).is_some(),
                    "byte {p} in span {}..{} has no source position",
                    s.output_start,
                    s.output_end
                );
            }
        }
    }
}

proptest! {
    /// For Direct spans, byte-level interpolation maps output byte to the
    /// corresponding source byte. In particular, the first byte of the output
    /// run must equal the first byte of the source run.
    #[test]
    fn direct_spans_first_byte_matches_source(html in arb_html_fragment()) {
        use deformat::html::SpanKind;
        let (text, spans) = deformat::html::strip_to_text_with_spans(&html);
        for s in spans.iter().take(32) {
            if s.kind != SpanKind::Direct {
                continue;
            }
            if s.output_end <= s.output_start || s.source_end <= s.source_start {
                continue;
            }
            if s.output_start >= text.len() || s.source_start >= html.len() {
                continue;
            }
            let text_byte = text.as_bytes()[s.output_start];
            let html_byte = html.as_bytes()[s.source_start];
            prop_assert_eq!(
                text_byte, html_byte,
                "Direct span output byte {} != source byte {} at span {}..{} / src {}..{}",
                text_byte, html_byte,
                s.output_start, s.output_end,
                s.source_start, s.source_end
            );
        }
    }
}

proptest! {
    /// strip_to_text_with_spans produces the same text as strip_to_text.
    #[test]
    fn spans_output_matches_plain_strip(html in arb_html_fragment()) {
        let plain = deformat::html::strip_to_text(&html);
        let (spanned, _) = deformat::html::strip_to_text_with_spans(&html);
        prop_assert_eq!(plain, spanned);
    }
}

proptest! {
    /// source_range(0, text.len()) returns either None (empty) or a range
    /// that lies within [0, html.len()].
    #[test]
    fn full_output_range_maps_into_source(html in arb_html_fragment()) {
        let (text, spans) = deformat::html::strip_to_text_with_spans(&html);
        if text.is_empty() {
            return Ok(());
        }
        if let Some((ss, se)) = spans.source_range(0, text.len()) {
            prop_assert!(se <= html.len(), "source_end {se} OOB ({} bytes html)", html.len());
            prop_assert!(ss <= se, "inverted source range {ss}..{se}");
        }
    }
}

proptest! {
    /// Every PathSpan from strip_to_text_with_paths has a non-empty path when
    /// there is any surrounding tag in the source. Paths never contain empty
    /// components (e.g., "//").
    #[test]
    fn path_spans_have_well_formed_paths(html in arb_html_fragment()) {
        let (_text, spans) = deformat::html::strip_to_text_with_paths(&html);
        for s in &spans {
            prop_assert!(!s.path.contains("//"), "path contains empty component: {:?}", s.path);
            for component in s.path.split('/').filter(|c| !c.is_empty()) {
                // Components are either a bare tag name or tag[N].
                let tag_part = component.split('[').next().unwrap();
                prop_assert!(
                    !tag_part.is_empty() && tag_part.bytes().all(|b| b.is_ascii_alphanumeric()),
                    "malformed path component {:?} in path {:?}",
                    component, s.path
                );
            }
        }
    }
}

proptest! {
    /// Closing inline tags never leave their own name as the leaf of the next
    /// text span's path. Regression guard for the 0.11.0 bug at src/html.rs:509.
    /// Uses a distinctive `TAIL_` prefix so the find cannot collide with the
    /// inner content.
    #[test]
    fn closing_inline_does_not_leak_into_following_path(
        inner in "[a-z]{1,10}",
        tail_suffix in "[a-z]{1,10}",
        tag in prop::sample::select(vec!["a", "b", "i", "em", "strong", "span", "code", "u"])
    ) {
        let marker = format!("TAIL{tail_suffix}");
        let html = format!("<article><p>Before <{tag}>{inner}</{tag}> {marker}</p></article>");
        let (text, spans) = deformat::html::strip_to_text_with_paths(&html);
        let span = spans
            .iter()
            .find(|s| text[s.output_start..s.output_end].contains(&marker));
        if let Some(span) = span {
            let leaf = span.path.rsplit('/').next().unwrap_or("");
            let leaf_tag = leaf.split('[').next().unwrap_or("");
            prop_assert_ne!(
                leaf_tag, tag,
                "closing <{}> leaked as leaf in path {:?} for marker {:?}",
                tag, span.path, marker
            );
        }
    }
}

proptest! {
    /// Multilang safety: CJK / Arabic / Hindi / Armenian paragraphs
    /// containing a non-ASCII sentence terminator and enough chars to
    /// clear the density floor must NOT be dropped by
    /// filter_low_sentence_density. Guards the pre-2026-04-23 bug where
    /// the ASCII-only terminator check silently dropped all non-English
    /// prose.
    #[test]
    fn non_ascii_sentences_survive_sentence_density_filter(
        (terminator, filler_char) in prop::sample::select(vec![
            ('\u{3002}', '测'),       // Chinese/Japanese full stop  。
            ('\u{FF01}', '試'),       // fullwidth exclamation        ！
            ('\u{FF1F}', 'あ'),       // fullwidth question           ？
            ('\u{061F}', 'ا'),       // Arabic question mark         ؟
            ('\u{0964}', 'त'),       // Devanagari danda             ।
            ('\u{0589}', 'ա'),       // Armenian full stop           ։
            ('\u{1362}', 'አ'),       // Ethiopic full stop           ።
        ]),
        sentences in 3usize..=8,
        chars_per_sentence in 20usize..=60,
    ) {
        let mut body = String::new();
        for _ in 0..sentences {
            for _ in 0..chars_per_sentence {
                body.push(filler_char);
            }
            body.push(terminator);
        }
        let html = format!("<article><p>{body}</p></article>");
        let segs = deformat::html::strip_to_segments(&html);
        let filtered = deformat::html::filter_low_sentence_density(segs.clone(), 1.0);
        prop_assert!(
            filtered.len() == segs.len(),
            "non-ASCII-terminated prose was dropped: {} -> {} (terminator U+{:04X})",
            segs.len(),
            filtered.len(),
            terminator as u32,
        );
    }
}

proptest! {
    /// filter_low_cetd_density must never drop a structural segment
    /// (Title/Header/Footer/ListItem/Table/CodeSnippet/FigureCaption)
    /// even when its char density is an outlier. This is the same
    /// structural-preservation guarantee the link-density filter has.
    #[test]
    fn cetd_preserves_structural_segments_under_all_floors(
        floor in 0.0f32..2.0,
    ) {
        let html = r#"<article>
            <h1>Title</h1>
            <header>Masthead</header>
            <ul><li>a</li><li>b</li><li>c</li></ul>
            <pre><code>code block</code></pre>
            <table><tr><td>cell</td></tr></table>
            <p>Long narrative paragraph that carries the mean density for
               the batch. It has multiple sentences. The structural
               elements above are all short but must survive regardless.</p>
            <p>Second long narrative paragraph to give the smoothing
               meaningful neighbours. Another sentence here. And one more.</p>
            <p>A third narrative paragraph so n >= 3 for the filter to fire.</p>
        </article>"#;
        let segs = deformat::html::strip_to_segments(html);
        let kinds_before: std::collections::HashSet<&str> =
            segs.iter().map(|s| s.type_name()).collect();
        let kept = deformat::html::filter_low_cetd_density(segs, floor);
        let kinds_after: std::collections::HashSet<&str> =
            kept.iter().map(|s| s.type_name()).collect();
        for structural in ["Title", "ListItem", "Table", "CodeSnippet"] {
            if kinds_before.contains(structural) {
                prop_assert!(
                    kinds_after.contains(structural),
                    "{structural} dropped at floor {floor}",
                );
            }
        }
    }
}

proptest! {
    /// CETD passes the multilang safety bar: non-ASCII text with
    /// non-ASCII sentence terminators must clear the CETD filter when
    /// it's flanked by peers of similar length. Guards against the
    /// char-count-based density treating CJK blocks differently.
    #[test]
    fn cetd_multilang_body_not_dropped_among_peers(
        (terminator, filler) in prop::sample::select(vec![
            ('\u{3002}', '测'),
            ('\u{FF01}', '試'),
            ('\u{061F}', 'ا'),
            ('\u{0964}', 'त'),
        ]),
        sentences in 3usize..=6,
        chars_per_sentence in 20usize..=40,
    ) {
        let mut body = String::new();
        for _ in 0..sentences {
            for _ in 0..chars_per_sentence {
                body.push(filler);
            }
            body.push(terminator);
        }
        // Three copies of the same multilang body -- equal peers so
        // CETD's smoothing cannot declare any of them an outlier.
        let html = format!(
            "<article><p>{body}</p><p>{body}</p><p>{body}</p></article>"
        );
        let segs = deformat::html::strip_to_segments(&html);
        let n_before = segs.len();
        let kept = deformat::html::filter_low_cetd_density(segs, 0.5);
        prop_assert!(
            kept.len() == n_before,
            "multilang peers dropped by CETD: {} -> {}",
            n_before,
            kept.len(),
        );
    }
}

proptest! {
    /// detect_page_type must not panic on arbitrary byte sequences.
    /// Returning PageType::Unknown is always a valid outcome; we just
    /// guard against crashes on malformed HTML and non-ASCII content.
    /// This test specifically includes multi-byte UTF-8 sequences to
    /// exercise char-boundary handling in any internal slicing.
    #[test]
    fn page_type_detection_never_panics(
        html in prop::collection::vec(prop::sample::select(vec![
            // ASCII structural bytes
            '<', '>', '"', '\'', '/', '\\', '!', '?', '-', '_', '=',
            ' ', '\n', '\t',
            'a', 'b', 'c', 'd', 'e', 'g', 'h', 'i', 'l', 'm', 'o',
            'p', 'r', 's', 't', 'y',
            // 2-byte UTF-8: Latin-1 Supplement (accented)
            'é', 'ü', 'ö', 'ñ', 'ç', 'å', '\u{2019}', // right single quote (often mid-slice)
            // 3-byte UTF-8: CJK, Arabic
            '中', 'あ', 'ア', '가', 'ع',
            // 4-byte UTF-8: emoji + supplementary plane
            '😀', '📄', '\u{1F4A9}',
        ]), 0..500).prop_map(|chars: Vec<char>| chars.into_iter().collect::<String>()),
    ) {
        // Call and discard -- any PageType value is acceptable; we only
        // assert the function doesn't panic on char boundaries.
        let _ = deformat::page_type::detect_page_type(&html);
    }
}

proptest! {
    /// Targeted panic-guard: build HTML that contains real non-ASCII
    /// characters positioned near the keywords detect_page_type greps
    /// for (og:type, rel="canonical", @type). This is the real-world
    /// failure mode the ASCII-only strategy above can miss.
    #[test]
    fn page_type_detection_safe_near_og_type_keyword(
        prefix in 0usize..500,
        suffix in 0usize..500,
        filler in prop::sample::select(vec!['中', 'あ', 'é', '\u{2019}', '😀']),
    ) {
        let mut html = String::new();
        for _ in 0..prefix { html.push(filler); }
        html.push_str("og:type");
        for _ in 0..suffix { html.push(filler); }
        // Must not panic on char-boundary slicing near the match.
        let _ = deformat::page_type::detect_page_type(&html);
    }
}

proptest! {
    /// For plain ASCII text inside a single <p>, the concatenated Direct
    /// source slices of all spans must contain every word from the input.
    /// This is a soft round-trip property.
    #[test]
    fn direct_spans_preserve_ascii_words(word in "[a-zA-Z]{3,12}") {
        let html = format!("<p>{word}</p>");
        let (_text, spans) = deformat::html::strip_to_text_with_spans(&html);
        let concat: String = spans
            .iter()
            .map(|s| &html[s.source_start..s.source_end])
            .collect();
        prop_assert!(
            concat.contains(&word),
            "concatenated source slices must contain input word"
        );
    }
}

// =============================================================================
// Panic guards on arbitrary bytes (complement cargo-fuzz's coverage
// with a smaller in-CI safety net)
// =============================================================================

proptest! {
    /// strip_to_text must not panic on any string, valid HTML or not.
    /// Proptest isn't as thorough as a libfuzzer run, but catches the
    /// common class of char-boundary / unclosed-tag panics in CI.
    #[test]
    fn strip_to_text_never_panics_on_arbitrary_bytes(
        s in prop::collection::vec(any::<u8>(), 0..2000)
            .prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
    ) {
        let _ = deformat::html::strip_to_text(&s);
    }
}

proptest! {
    /// strip_to_text_with_paths must not panic, and every span must
    /// satisfy the char-boundary / in-bounds / non-overlap invariants
    /// regardless of input.
    #[test]
    fn strip_to_text_with_paths_invariants_hold_on_arbitrary_bytes(
        s in prop::collection::vec(any::<u8>(), 0..1500)
            .prop_map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
    ) {
        let (text, spans) = deformat::html::strip_to_text_with_paths(&s);
        let mut prev_end = 0usize;
        for span in &spans {
            prop_assert!(text.is_char_boundary(span.output_start));
            prop_assert!(text.is_char_boundary(span.output_end));
            prop_assert!(s.is_char_boundary(span.source_start));
            prop_assert!(s.is_char_boundary(span.source_end));
            prop_assert!(span.output_start <= span.output_end);
            prop_assert!(span.source_start <= span.source_end);
            prop_assert!(span.output_end <= text.len());
            prop_assert!(span.source_end <= s.len());
            prop_assert!(span.output_start >= prev_end);
            prev_end = span.output_end;
        }
    }
}

proptest! {
    /// The full four-filter pipeline must not panic over the entire
    /// threshold parameter space, including the edge values (0.0, 1.0)
    /// and a tiny integer boilerplate minimum.
    #[test]
    fn filter_pipeline_never_panics_on_arbitrary_thresholds(
        html in arb_html_fragment(),
        link_cap in 0.0f32..=1.0,
        sent_cap in 0.0f32..=10.0,
        boiler_min in 0usize..=200,
        cetd_frac in 0.0f32..=2.0,
    ) {
        let segs = deformat::html::strip_to_segments_filtered(&html, link_cap);
        let segs = deformat::html::filter_low_sentence_density(segs, sent_cap);
        let segs = deformat::html::filter_boilerplate(segs, boiler_min);
        let _ = deformat::html::filter_low_cetd_density(segs, cetd_frac);
    }
}

proptest! {
    /// Per-segment filters are idempotent: running them a second time
    /// does nothing because each segment is judged independently.
    /// (Exception: filter_low_cetd_density uses batch-relative mean
    /// and is NOT idempotent by design -- dropping outliers shifts
    /// the mean upward. It satisfies monotonicity instead, tested
    /// separately below.)
    #[test]
    fn per_segment_filters_are_idempotent_under_repetition(
        html in arb_html_fragment(),
    ) {
        let base = deformat::html::strip_to_segments(&html);

        let once = deformat::html::filter_boilerplate(base.clone(), 40);
        let twice = deformat::html::filter_boilerplate(once.clone(), 40);
        prop_assert_eq!(once.len(), twice.len(), "filter_boilerplate idempotent");

        let once = deformat::html::filter_low_sentence_density(base.clone(), 1.0);
        let twice = deformat::html::filter_low_sentence_density(once.clone(), 1.0);
        prop_assert_eq!(once.len(), twice.len(), "filter_low_sentence_density idempotent");
    }
}

proptest! {
    /// CETD is batch-relative, not per-segment: repeated application
    /// can drop more segments (because the mean shifts after the
    /// first pass removed the outliers). But it must stay monotone:
    /// the second pass's output length <= first pass's output length.
    /// This property is what callers need to rely on in practice.
    #[test]
    fn filter_low_cetd_density_is_monotone_under_repetition(
        html in arb_html_fragment(),
        frac in 0.0f32..=1.0,
    ) {
        let base = deformat::html::strip_to_segments(&html);
        let once = deformat::html::filter_low_cetd_density(base, frac);
        let n1 = once.len();
        let twice = deformat::html::filter_low_cetd_density(once, frac);
        prop_assert!(twice.len() <= n1, "CETD second pass grew segments");
    }
}

proptest! {
    /// Filter-chain monotonicity: applying any filter can only reduce
    /// the segment count or keep it stable, never add segments.
    /// Guards against filters accidentally creating synthetic
    /// outputs.
    #[test]
    fn filter_never_grows_the_segment_count(
        html in arb_html_fragment(),
        link_cap in 0.0f32..=1.0,
        sent_cap in 0.0f32..=5.0,
        boiler_min in 0usize..=80,
        cetd_frac in 0.0f32..=1.5,
    ) {
        let base = deformat::html::strip_to_segments_filtered(&html, link_cap);
        let n0 = base.len();
        let after_sent = deformat::html::filter_low_sentence_density(base, sent_cap);
        prop_assert!(after_sent.len() <= n0);
        let n1 = after_sent.len();
        let after_boiler = deformat::html::filter_boilerplate(after_sent, boiler_min);
        prop_assert!(after_boiler.len() <= n1);
        let n2 = after_boiler.len();
        let after_cetd = deformat::html::filter_low_cetd_density(after_boiler, cetd_frac);
        prop_assert!(after_cetd.len() <= n2);
    }
}

proptest! {
    /// Segment element_ids are deterministic: the same input produces
    /// the same ids across independent parses. Guards against any
    /// future hashing / ordering change silently breaking the
    /// "reproducible pipelines" claim in the SegmentData doc comment.
    #[test]
    fn segment_element_ids_are_deterministic(html in arb_html_fragment()) {
        let a = deformat::html::strip_to_segments(&html);
        let b = deformat::html::strip_to_segments(&html);
        prop_assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            prop_assert_eq!(&sa.data().element_id, &sb.data().element_id);
        }
    }
}

proptest! {
    /// detect_bytes must not panic on any byte sequence. Guards the
    /// format-discrimination path that classifies DOCX/XLSX/PPTX/EPUB
    /// via ZIP central-directory inspection.
    #[test]
    fn detect_bytes_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1000)) {
        let _ = deformat::detect::detect_bytes(&bytes);
    }
}

proptest! {
    /// Three-filter pipeline produces text that never contains a raw
    /// `<script` or `<style` prefix, regardless of the input. Guards
    /// against any future filter accidentally re-admitting content
    /// the scanner already discarded.
    #[test]
    fn filter_output_never_contains_script_or_style(html in arb_html_fragment()) {
        let segs = deformat::html::strip_to_segments_filtered(&html, 0.45);
        let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
        let segs = deformat::html::filter_boilerplate(segs, 40);
        let segs = deformat::html::filter_low_cetd_density(segs, 0.4);
        for seg in &segs {
            let t = seg.data().text.to_lowercase();
            prop_assert!(!t.contains("<script"), "script tag leaked through filters");
            prop_assert!(!t.contains("<style"), "style tag leaked through filters");
        }
    }
}
