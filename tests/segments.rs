//! Tests for typed [`deformat::Segment`] output (Unstructured.io-compatible).

use deformat::html::{strip_to_segments, Segment};

#[test]
fn title_and_paragraph() {
    let html = "<article><h1>Greeting</h1><p>Hello, world!</p></article>";
    let segs = strip_to_segments(html);
    assert_eq!(segs.len(), 2, "got: {segs:?}");
    assert_eq!(segs[0].type_name(), "Title");
    assert_eq!(segs[0].data().text, "Greeting");
    assert_eq!(segs[0].data().metadata.category_depth, Some(1));
    assert_eq!(segs[1].type_name(), "NarrativeText");
    assert_eq!(segs[1].data().text, "Hello, world!");
    // Paragraph under a Title -> parent_id linked
    assert_eq!(
        segs[1].data().metadata.parent_id.as_ref(),
        Some(&segs[0].data().element_id)
    );
}

#[test]
fn heading_levels() {
    let html = "<h1>A</h1><h2>B</h2><h3>C</h3><h6>F</h6>";
    let segs = strip_to_segments(html);
    let depths: Vec<Option<u32>> = segs
        .iter()
        .map(|s| s.data().metadata.category_depth)
        .collect();
    assert_eq!(depths, vec![Some(1), Some(2), Some(3), Some(6)]);
}

#[test]
fn list_items_separate_segments() {
    let html = "<ul><li>First</li><li>Second</li><li>Third</li></ul>";
    let segs = strip_to_segments(html);
    assert!(segs.iter().all(|s| s.type_name() == "ListItem"));
    assert_eq!(segs.len(), 3);
    let texts: Vec<&str> = segs.iter().map(|s| s.data().text.as_str()).collect();
    assert_eq!(texts, vec!["First", "Second", "Third"]);
}

#[test]
fn pre_block_is_code_snippet() {
    let html = "<pre><code>fn main() {}</code></pre>";
    let segs = strip_to_segments(html);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].type_name(), "CodeSnippet");
    assert!(segs[0].data().text.contains("fn main"));
}

#[test]
fn header_is_tagged_footer_is_stripped() {
    // Note: `<footer>` is in deformat's skip-tag list (boilerplate removal),
    // so its text never reaches strip_to_segments. `<header>` is kept.
    let html =
        "<body><header>Masthead here</header><p>Body.</p><footer>Copyright 2026</footer></body>";
    let segs = strip_to_segments(html);
    let kinds: Vec<&str> = segs.iter().map(|s| s.type_name()).collect();
    assert!(kinds.contains(&"Header"), "got {kinds:?}");
    assert!(kinds.contains(&"NarrativeText"), "got {kinds:?}");
    assert!(
        !kinds.contains(&"Footer"),
        "<footer> content should be skipped"
    );
    assert!(
        !segs.iter().any(|s| s.data().text.contains("Copyright")),
        "footer text leaked"
    );
}

#[test]
fn parent_id_links_to_nearest_title() {
    let html = "<article><h1>One</h1><p>P1</p><p>P2</p><h2>Two</h2><p>P3</p></article>";
    let segs = strip_to_segments(html);
    let h1_id = segs[0].data().element_id.clone();
    let h2_id = &segs[3].data().element_id;
    assert_eq!(
        segs[1].data().metadata.parent_id.as_deref(),
        Some(h1_id.as_str())
    );
    assert_eq!(
        segs[2].data().metadata.parent_id.as_deref(),
        Some(h1_id.as_str())
    );
    assert_eq!(
        segs[4].data().metadata.parent_id.as_deref(),
        Some(h2_id.as_str())
    );
}

#[test]
fn empty_input_returns_empty() {
    assert!(strip_to_segments("").is_empty());
    assert!(strip_to_segments("<div></div>").is_empty());
}

#[test]
fn element_id_is_stable_across_runs() {
    let html = "<h1>Stable</h1><p>Content</p>";
    let a = strip_to_segments(html);
    let b = strip_to_segments(html);
    assert_eq!(a[0].data().element_id, b[0].data().element_id);
    assert_eq!(a[1].data().element_id, b[1].data().element_id);
}

#[test]
fn narrative_text_inside_article() {
    let html = r#"<article>
        <p>First paragraph of prose.</p>
        <p>Second paragraph with <em>emphasis</em> and <strong>strong</strong>.</p>
    </article>"#;
    let segs = strip_to_segments(html);
    // Both paragraphs should be NarrativeText; inline tags collapse into the text
    let narrative: Vec<&Segment> = segs
        .iter()
        .filter(|s| s.type_name() == "NarrativeText")
        .collect();
    assert_eq!(narrative.len(), 2);
    assert!(narrative[1].data().text.contains("emphasis"));
    assert!(narrative[1].data().text.contains("strong"));
}

#[cfg(feature = "serde")]
#[test]
fn serde_wire_format_matches_unstructured() {
    let html = "<h1>Title</h1><p>Body.</p>";
    let segs = strip_to_segments(html);
    let json = serde_json::to_value(&segs).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // First: Title with category_depth=1
    let first = &arr[0];
    assert_eq!(first["type"], "Title");
    assert!(first["element_id"].is_string());
    assert_eq!(first["text"], "Title");
    assert_eq!(first["metadata"]["category_depth"], 1);

    // Second: NarrativeText with parent_id
    let second = &arr[1];
    assert_eq!(second["type"], "NarrativeText");
    assert_eq!(second["text"], "Body.");
    assert_eq!(
        second["metadata"]["parent_id"],
        first["element_id"].as_str().unwrap()
    );

    // Confirm optional fields are omitted when None (Unstructured wire shape)
    let second_meta = second["metadata"].as_object().unwrap();
    assert!(!second_meta.contains_key("page_number"));
    assert!(!second_meta.contains_key("text_as_html"));
    assert!(!second_meta.contains_key("languages"));
}

#[cfg(feature = "serde")]
#[test]
fn serde_segments_are_deserializable() {
    // Roundtrip: serialize then deserialize should preserve shape.
    let html = "<h1>A</h1><p>B</p>";
    let segs = strip_to_segments(html);
    let json = serde_json::to_string(&segs).unwrap();
    let back: Vec<Segment> = serde_json::from_str(&json).unwrap();
    assert_eq!(back.len(), segs.len());
    assert_eq!(back[0].type_name(), segs[0].type_name());
    assert_eq!(back[0].data().text, segs[0].data().text);
}
