//! Adversarial edge case tests.
//!
//! These test deformat's robustness against malformed, pathological,
//! and deliberately tricky HTML inputs.

use deformat::html::strip_to_text;

#[test]
fn unclosed_script_swallows_remaining() {
    // An unclosed script tag means everything after it is inside the script.
    let text = strip_to_text("<script>var x = 1;<p>After.</p>");
    assert!(!text.contains("var x"), "script content stripped");
    // Content after unclosed script is lost -- expected behavior
}

#[test]
fn deep_nesting_1000_levels() {
    let html = "<div>".repeat(1000) + "<p>Deep</p>" + &"</div>".repeat(1000);
    let text = strip_to_text(&html);
    assert!(text.contains("Deep"), "deep content: {text}");
}

#[test]
fn long_tag_name_100_chars() {
    let tag = "a".repeat(100);
    let html = format!("<{tag}>text</{tag}>");
    let text = strip_to_text(&html);
    assert!(text.contains("text"), "long tag: {text}");
}

#[test]
fn tag_with_200_attributes() {
    let attrs: String = (0..200)
        .map(|i| format!("data-x{i}=\"v{i}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let html = format!("<div {attrs}><p>Content.</p></div>");
    let text = strip_to_text(&html);
    assert!(text.contains("Content"), "many attrs: {text}");
    assert!(!text.contains("data-x"), "no attr leak: {text}");
}

#[test]
fn entity_at_eof_without_semicolon() {
    let text = strip_to_text("<p>Price: &euro");
    assert!(text.contains("Price"), "text: {text}");
    assert!(text.contains('\u{20AC}'), "euro decoded: {text}");
}

#[test]
fn null_bytes_in_input() {
    let text = strip_to_text("<p>Before\x00After</p>");
    assert!(text.contains("Before"), "before null: {text}");
    assert!(text.contains("After"), "after null: {text}");
}

#[test]
fn unicode_bom_prefix() {
    let text = strip_to_text("\u{FEFF}<html><body><p>BOM.</p></body></html>");
    assert!(text.contains("BOM"), "bom content: {text}");
}

#[test]
fn empty_tags_everywhere() {
    let text = strip_to_text("<p></p><div></div><span></span><p>Real.</p>");
    assert!(text.contains("Real"), "real content: {text}");
}

#[test]
fn interleaved_script_style_content() {
    let text = strip_to_text("<p>A</p><script>x</script><p>B</p><style>y</style><p>C</p>");
    assert!(
        text.contains("A") && text.contains("B") && text.contains("C"),
        "all visible: {text}"
    );
    assert!(
        !text.contains("x") && !text.contains("y"),
        "no script/style: {text}"
    );
}

#[test]
fn cdata_section() {
    let text = strip_to_text("<p>Before</p><![CDATA[raw]]><p>After</p>");
    assert!(
        text.contains("Before") && text.contains("After"),
        "text: {text}"
    );
}

#[test]
fn processing_instruction() {
    let text = strip_to_text("<?xml version='1.0'?><p>XML.</p>");
    assert!(text.contains("XML"), "text: {text}");
}

#[test]
fn ten_consecutive_amp_entities() {
    let html = "<p>".to_string() + &"&amp;".repeat(10) + "</p>";
    let text = strip_to_text(&html);
    assert_eq!(text.matches('&').count(), 10, "10 amps: {text}");
}

#[test]
fn triple_br_sequence() {
    let text = strip_to_text("<p>A<br/><br/><br/>B</p>");
    assert!(text.contains("A") && text.contains("B"), "content: {text}");
}

#[test]
fn mismatched_close_tags() {
    let text = strip_to_text("<p>Open<div>Mis</p>matched</div><p>After.</p>");
    assert!(text.contains("After"), "post-mismatch content: {text}");
}

#[test]
fn style_comment_with_close_tag() {
    // Known limitation: </style> inside a CSS comment ends the style block early.
    // Real browsers don't, but our scanner doesn't parse CSS comments.
    let text = strip_to_text("<style>/* </style> trick */body{}</style><p>Visible.</p>");
    assert!(text.contains("Visible"), "post-style content: {text}");
    // The CSS comment content leaks -- documented limitation
}

#[test]
fn data_uri_not_leaked() {
    let text =
        strip_to_text(r#"<img src="data:image/png;base64,iVBOR..." alt="Photo"><p>Text.</p>"#);
    assert!(text.contains("Photo"), "alt text: {text}");
    assert!(!text.contains("base64"), "data uri stripped: {text}");
}

#[test]
fn svg_text_elements_stripped() {
    let text = strip_to_text(r#"<svg><text x="10">SVG Text</text></svg><p>After.</p>"#);
    assert!(text.contains("After"), "post-svg: {text}");
    assert!(!text.contains("SVG Text"), "svg text stripped: {text}");
}

#[test]
fn hundred_kb_attribute_value() {
    let long = "x".repeat(100_000);
    let html = format!(r#"<div data-big="{long}"><p>Survives.</p></div>"#);
    let text = strip_to_text(&html);
    assert!(text.contains("Survives"), "content: {text}");
    assert!(!text.contains("xxxxx"), "no attr leak: {text}");
}

#[test]
fn whitespace_only_paragraphs() {
    let text = strip_to_text("<p>  \n\t  </p>");
    assert!(text.is_empty(), "whitespace-only is empty: {text:?}");
}

#[test]
fn textarea_skipped() {
    let text = strip_to_text("<textarea>User input</textarea><p>Visible.</p>");
    assert!(text.contains("Visible"), "visible: {text}");
    assert!(!text.contains("User input"), "textarea skipped: {text}");
}

#[test]
fn select_options_skipped() {
    let text = strip_to_text("<select><option>Choose</option></select><p>Visible.</p>");
    assert!(text.contains("Visible"), "visible: {text}");
    assert!(!text.contains("Choose"), "select skipped: {text}");
}

#[test]
fn iframe_skipped() {
    let text = strip_to_text("<iframe>Fallback</iframe><p>Visible.</p>");
    assert!(text.contains("Visible"), "visible: {text}");
    assert!(!text.contains("Fallback"), "iframe skipped: {text}");
}

#[test]
fn template_skipped() {
    let text = strip_to_text("<template><p>Template</p></template><p>Visible.</p>");
    assert!(text.contains("Visible"), "visible: {text}");
    assert!(!text.contains("Template"), "template skipped: {text}");
}

#[test]
fn tag_like_content_in_attribute() {
    let text = strip_to_text(r#"<div title="<script>alert(1)</script>"><p>Safe.</p></div>"#);
    assert!(text.contains("Safe"), "content: {text}");
    assert!(!text.contains("alert"), "no attr leak: {text}");
}

#[test]
fn five_mb_html_completes() {
    let html = "<html><body>".to_string() + &"<p>Paragraph.</p>".repeat(100_000) + "</body></html>";
    let start = std::time::Instant::now();
    let text = strip_to_text(&html);
    let elapsed = start.elapsed();
    assert!(text.contains("Paragraph"), "content present");
    assert!(elapsed.as_secs() < 5, "completed in {elapsed:?}");
}

#[test]
fn script_with_lt_operator() {
    let text = strip_to_text("<script>if (x < 10 && y > 5) { foo(); }</script><p>After.</p>");
    assert!(text.contains("After"), "post-script: {text}");
    assert!(!text.contains("foo"), "script stripped: {text}");
}

#[test]
fn style_with_lt_in_selector() {
    let text = strip_to_text("<style>div[data-x<y]{color:red}</style><p>After.</p>");
    assert!(text.contains("After"), "post-style: {text}");
    assert!(!text.contains("color"), "style stripped: {text}");
}

#[test]
fn multiple_scripts_with_operators() {
    let text = strip_to_text(concat!(
        "<script>var a = 1 < 2;</script>",
        "<p>Between scripts.</p>",
        "<script>var b = 3 > 1;</script>",
        "<p>After all scripts.</p>"
    ));
    assert!(text.contains("Between scripts"), "between: {text}");
    assert!(text.contains("After all scripts"), "after: {text}");
    assert!(!text.contains("var a"), "script 1 stripped: {text}");
    assert!(!text.contains("var b"), "script 2 stripped: {text}");
}

#[test]
fn form_content_not_skipped() {
    // Regression: <form> was previously a skip tag
    let text = strip_to_text("<form><div><p>Form content visible.</p></div></form>");
    assert!(
        text.contains("Form content visible"),
        "form content: {text}"
    );
}

#[test]
fn header_content_not_skipped() {
    // Regression: <header> was previously a skip tag
    let text = strip_to_text("<header><h1>Article Title</h1></header>");
    assert!(text.contains("Article Title"), "header content: {text}");
}

// =============================================================================
// CSS-hidden content stripping
// =============================================================================

#[test]
fn display_none_stripped() {
    let text = strip_to_text(r#"<div style="display:none">Hidden content</div><p>Visible.</p>"#);
    assert!(text.contains("Visible"), "visible: {text}");
    assert!(
        !text.contains("Hidden content"),
        "display:none stripped: {text}"
    );
}

#[test]
fn display_none_with_spaces() {
    let text =
        strip_to_text(r#"<div style="display: none !important;">Hidden</div><p>Visible.</p>"#);
    assert!(!text.contains("Hidden"), "display:none with spaces: {text}");
}

#[test]
fn visibility_hidden_stripped() {
    let text = strip_to_text(r#"<span style="visibility:hidden">Ghost</span><p>Visible.</p>"#);
    assert!(!text.contains("Ghost"), "visibility:hidden: {text}");
}

#[test]
fn hidden_attribute_stripped() {
    let text = strip_to_text("<div hidden><p>Hidden by attr</p></div><p>Visible.</p>");
    assert!(!text.contains("Hidden by attr"), "hidden attr: {text}");
    assert!(text.contains("Visible"), "visible: {text}");
}

#[test]
fn aria_hidden_stripped() {
    let text =
        strip_to_text(r#"<div aria-hidden="true">Screen reader hidden</div><p>Visible.</p>"#);
    assert!(!text.contains("Screen reader"), "aria-hidden: {text}");
}

#[test]
fn nested_hidden_elements() {
    let text = strip_to_text(
        r#"<div style="display:none"><div><p>Deep hidden</p></div></div><p>Visible.</p>"#,
    );
    assert!(!text.contains("Deep hidden"), "nested hidden: {text}");
    assert!(text.contains("Visible"), "visible after nested: {text}");
}

#[test]
fn hidden_then_visible_sibling() {
    let text = strip_to_text(concat!(
        r#"<div style="display:none"><p>Hidden section</p></div>"#,
        r#"<div><p>Visible section after hidden.</p></div>"#,
    ));
    assert!(!text.contains("Hidden section"), "hidden: {text}");
    assert!(
        text.contains("Visible section after hidden"),
        "visible: {text}"
    );
}

#[test]
fn hidden_mobile_menu() {
    // Common pattern: mobile nav hidden with display:none on desktop
    let text = strip_to_text(
        r#"<html><body>
        <div class="mobile-menu" style="display:none">
            <ul><li>Link 1</li><li>Link 2</li></ul>
        </div>
        <article><p>Article content.</p></article>
    </body></html>"#,
    );
    assert!(text.contains("Article content"), "article: {text}");
    assert!(!text.contains("Link 1"), "mobile menu hidden: {text}");
}
