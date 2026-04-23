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
fn main_landmark_recovers_from_unclosed_nav_drawer() {
    // Malformed HTML: several <nav> tags never close, leaving skip_depth
    // pegged. <main> is the HTML5 primary-content landmark and never
    // legitimately nests inside <nav>/<aside>/<footer>, so its opening
    // resets skip_depth to 0. Without this recovery, the article body
    // would never be emitted.
    //
    // Minimal repro of a real-world case (Go blog article with
    // malformed navigation-drawer markup): 5 <nav> opens, 2 </nav>
    // closes.
    let html = r#"<html><body>
        <header>
            <nav>Main header nav</nav>
        </header>
        <aside>
            <nav>Drawer nav 1</nav>
            <nav>Drawer nav 2 (UNCLOSED)
            <nav>Drawer nav 3 (UNCLOSED)
            <nav>Drawer nav 4 (UNCLOSED)
        </aside>
        <main>
            <article>
                <h1>Article title</h1>
                <p>Article body paragraph here.</p>
            </article>
        </main>
    </body></html>"#;
    let text = strip_to_text(html);
    assert!(
        text.contains("Article title"),
        "main landmark must recover extraction: {text:?}"
    );
    assert!(
        text.contains("Article body paragraph"),
        "article body must be emitted: {text:?}"
    );
    assert!(
        !text.contains("Drawer nav"),
        "nav content still skipped: {text:?}"
    );
}

#[test]
fn unclosed_attribute_quote_does_not_swallow_rest_of_document() {
    // Real-world malformed <img> tag from the WCXB dev split (file 4563):
    // a stray unclosed `"` after `wp-image-NUMBER`. Without recovery, the
    // scanner would treat everything up to the next `"` as an attribute
    // value -- swallowing thousands of bytes of article content.
    //
    // After 256+ bytes of quoted-attribute scanning, seeing `<` followed
    // by a tag-like character kicks the recovery path.
    let filler = "x".repeat(260); // push past the 256-byte recovery threshold
    let html = format!(
        r#"<img src="x.jpg" alt="caption" width="170" wp-image-3737" />
        <p>{filler}</p>
        <a href="/path">Link text here</a>
        <p>Article body that must not be swallowed by the malformed img tag.</p>"#
    );
    let text = strip_to_text(&html);
    assert!(
        text.contains("Article body that must not be swallowed"),
        "recovery kept article body: {text:?}"
    );
}

#[test]
fn short_attribute_with_tag_like_content_is_preserved() {
    // Short attribute values (<256 bytes) with tag-like syntax inside
    // remain inside the quote -- the recovery only fires once the
    // quote is suspiciously long.
    let html = r#"<div title="<script>alert(1)</script>"><p>Safe.</p></div>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Safe"));
    assert!(!text.contains("alert"));
}

#[test]
fn main_landmark_does_not_reset_when_skip_depth_is_zero() {
    // Well-formed HTML should be unchanged by the recovery.
    let well_formed = r#"<html><body>
        <header><nav><a>Home</a></nav></header>
        <main><article><p>Body.</p></article></main>
    </body></html>"#;
    let text = strip_to_text(well_formed);
    assert!(text.contains("Body."));
    assert!(!text.contains("Home"));
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
    // Test hangs (e.g. quadratic blowup) are caught by the test runner's
    // default timeout; asserting wall-clock time is load-sensitive on CI.
    let html = "<html><body>".to_string() + &"<p>Paragraph.</p>".repeat(100_000) + "</body></html>";
    let text = strip_to_text(&html);
    assert!(text.contains("Paragraph"), "content present");
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
