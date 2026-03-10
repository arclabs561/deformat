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
        "script", "style", "nav", "header", "footer", "aside", "noscript", "template", "svg",
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
// Invariant: entity decoding never panics
// =============================================================================

proptest! {
    #[test]
    fn entity_decoding_never_panics(entity in arb_entity_like()) {
        let html = format!("<p>{entity}</p>");
        let _text = deformat::html::strip_to_text(&html);
    }
}

// =============================================================================
// Invariant: strip_to_text never panics on arbitrary input
// =============================================================================

proptest! {
    #[test]
    fn strip_never_panics(input in ".*") {
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
            "nav", "header", "footer", "aside", "noscript",
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
        outer in prop::sample::select(vec!["header", "footer", "nav", "aside"])
            .prop_map(|s| s.to_string()),
        inner in prop::sample::select(vec!["nav", "aside", "form", "noscript"])
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
// Invariant: output is valid UTF-8 (should be guaranteed by Rust, but verify)
// =============================================================================

proptest! {
    #[test]
    fn output_is_valid_utf8(html in arb_html_fragment()) {
        let text = deformat::html::strip_to_text(&html);
        // If we got here, the String was valid UTF-8.
        // Double-check by round-tripping through bytes.
        let bytes = text.as_bytes();
        let roundtrip = std::str::from_utf8(bytes);
        prop_assert!(
            roundtrip.is_ok(),
            "Output is not valid UTF-8: {:?}",
            text
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
