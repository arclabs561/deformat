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
        "p", "div", "span", "b", "i", "a", "h1", "h2", "h3", "li", "ul", "ol", "td", "th", "tr",
        "table", "article", "section", "main", "blockquote", "em", "strong", "code", "pre",
    ]);
    let skip_tag_names = prop::sample::select(vec![
        "script", "style", "nav", "header", "footer", "aside", "noscript", "template", "svg",
    ]);
    let entities = prop::sample::select(vec![
        "&amp;", "&lt;", "&gt;", "&quot;", "&apos;", "&nbsp;", "&eacute;", "&mdash;", "&ndash;",
        "&copy;", "&reg;", "&euro;", "&hellip;", "&ldquo;", "&rdquo;", "&#169;", "&#x1F4A9;",
        "&#0;", "&#8212;",
    ]);

    prop::collection::vec(
        prop_oneof![
            // Plain text words
            "[a-zA-Z0-9 .,!?'-]{1,30}".prop_map(|s| s),
            // Opening + closing tag around text
            (tag_names.clone(), "[a-zA-Z0-9 .,]{0,20}").prop_map(|(tag, text)| {
                format!("<{tag}>{text}</{tag}>")
            }),
            // Skip tag with hidden content
            (skip_tag_names, "[a-zA-Z0-9 .,]{0,20}").prop_map(|(tag, text)| {
                format!("<{tag}>{text}</{tag}>")
            }),
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
        // Check for complete HTML tags (with closing >).
        // Decoded entities like &lt; legitimately produce '<' in output,
        // so we only flag actual complete tags that should have been stripped.
        let tag_re = regex::Regex::new(
            r"<(script|style|div|span|p|a|b|i|em|strong|h[1-6]|table|tr|td|th|ul|ol|li|nav|header|footer|aside|form|img|br|hr|section|article|main|blockquote|code|pre)\b[^>]*>"
        ).unwrap();
        prop_assert!(
            !tag_re.is_match(&text),
            "HTML tag found in output: {:?}\nInput: {:?}",
            text,
            html
        );
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
        let result = deformat::extract(&html);
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
