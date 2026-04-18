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

// =============================================================================
// filter_boilerplate
// =============================================================================

#[test]
fn filter_drops_short_labelless_narratives() {
    // Three NarrativeTexts: real prose, nav label, menu item.
    let html = r#"
        <p>This is a real paragraph of prose ending with a period.</p>
        <p>About</p>
        <p>Contact Us</p>
    "#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 3);
    let filtered = deformat::html::filter_boilerplate(segs, 40);
    // Real prose survives; short labels dropped.
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].data().text.starts_with("This is a real"));
}

#[test]
fn filter_preserves_titles_always() {
    // Short title-like content survives even without sentence punctuation.
    let html = "<h1>About</h1><p>Text.</p>";
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_boilerplate(segs, 40);
    assert!(filtered.iter().any(|s| s.type_name() == "Title"));
}

#[test]
fn filter_keeps_multi_word_list_items() {
    let html = r#"<ul>
        <li>Buy groceries and cook dinner</li>
        <li>X</li>
    </ul>"#;
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_boilerplate(segs, 40);
    // Multi-word list item survives; single-token one is dropped.
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].data().text.contains("groceries"));
}

// =============================================================================
// DOCX segments (feature-gated)
// =============================================================================

#[cfg(feature = "docx")]
#[test]
fn docx_segments_one_per_paragraph() {
    use std::io::Write;
    let xml = r#"<?xml version="1.0"?>
    <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
      <w:body>
        <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>
        <w:p><w:r><w:t>Deformat extracts text from document formats.</w:t></w:r></w:p>
        <w:p><w:r><w:t>It supports PDF, DOCX, EPUB, and more.</w:t></w:r></w:p>
      </w:body>
    </w:document>"#;
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("[Content_Types].xml", opts).unwrap();
    write!(zip, r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#).unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    write!(zip, "{xml}").unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let segs = deformat::docx::extract_bytes_to_segments(&bytes).unwrap();
    assert_eq!(segs.len(), 3, "got: {segs:?}");
    assert_eq!(segs[0].type_name(), "Title");
    assert_eq!(segs[0].data().metadata.category_depth, Some(1));
    assert_eq!(segs[0].data().text, "Introduction");
    assert_eq!(segs[1].type_name(), "NarrativeText");
    assert!(segs[1].data().text.contains("Deformat extracts"));
    // Non-title paragraphs carry parent_id = title id.
    assert_eq!(
        segs[1].data().metadata.parent_id.as_deref(),
        Some(segs[0].data().element_id.as_str())
    );
}

#[cfg(feature = "docx")]
#[test]
fn docx_segments_empty_returns_empty_result() {
    use std::io::Write;
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("word/document.xml", opts).unwrap();
    write!(zip, r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body></w:body></w:document>"#).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let r = deformat::docx::extract_bytes_to_segments(&bytes);
    assert!(matches!(r, Err(deformat::Error::EmptyResult)));
}

// =============================================================================
// PDF segments (feature-gated)
// =============================================================================

#[cfg(feature = "pdf_oxide")]
#[test]
fn pdf_oxide_segments_invalid_bytes_is_parse_err() {
    let r = deformat::pdf_oxide::extract_bytes_to_segments(b"not a pdf");
    assert!(matches!(r, Err(deformat::Error::Parse(_))));
}

// =============================================================================
// Table -> text_as_html
// =============================================================================

#[test]
fn table_segment_carries_text_as_html() {
    let html = r#"<table>
        <tr><th>Name</th><th>Score</th></tr>
        <tr><td>Alice</td><td>95</td></tr>
    </table>"#;
    let segs = deformat::html::strip_to_segments(html);
    // Expect one Table segment, not multiple cell segments.
    let tables: Vec<&Segment> = segs.iter().filter(|s| s.type_name() == "Table").collect();
    assert_eq!(tables.len(), 1, "got segments: {segs:?}");
    let tab = tables[0];
    // Table plain text contains the cell values.
    assert!(tab.data().text.contains("Alice"));
    assert!(tab.data().text.contains("95"));
    // text_as_html carries the original `<table>...</table>` slice.
    let as_html = tab
        .data()
        .metadata
        .text_as_html
        .as_ref()
        .expect("text_as_html populated");
    assert!(as_html.starts_with("<table"), "got: {as_html:?}");
    assert!(as_html.ends_with("</table>"), "got: {as_html:?}");
    assert!(as_html.contains("Alice"));
}

#[test]
fn nested_tables_use_outer_html() {
    let html = r#"<table id="outer"><tr><td>
        <table id="inner"><tr><td>InnerCell</td></tr></table>
    </td></tr></table>"#;
    let segs = deformat::html::strip_to_segments(html);
    let tables: Vec<&Segment> = segs.iter().filter(|s| s.type_name() == "Table").collect();
    // One Table segment spanning the outer table.
    assert_eq!(tables.len(), 1, "got: {segs:?}");
    let as_html = tables[0]
        .data()
        .metadata
        .text_as_html
        .as_ref()
        .expect("text_as_html");
    assert!(as_html.contains(r#"id="outer""#));
    assert!(as_html.contains(r#"id="inner""#));
}
