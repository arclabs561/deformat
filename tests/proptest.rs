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
