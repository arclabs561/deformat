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
fn docx_segments_include_tables_with_text_as_html() {
    use std::io::Write;
    let xml = r#"<?xml version="1.0"?>
    <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
      <w:body>
        <w:p><w:r><w:t>Intro paragraph.</w:t></w:r></w:p>
        <w:tbl>
          <w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr>
          <w:tr>
            <w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc>
          </w:tr>
          <w:tr>
            <w:tc><w:p><w:r><w:t>Alpha</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc>
          </w:tr>
          <w:tr>
            <w:tc><w:p><w:r><w:t>Beta</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>
        <w:p><w:r><w:t>After the table.</w:t></w:r></w:p>
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
    // Expect: intro, table, after.
    let kinds: Vec<&str> = segs.iter().map(|s| s.type_name()).collect();
    assert_eq!(kinds, vec!["NarrativeText", "Table", "NarrativeText"]);

    let table = segs.iter().find(|s| s.type_name() == "Table").unwrap();
    let data = table.data();
    assert!(data.text.contains("Name"), "text: {:?}", data.text);
    assert!(data.text.contains("Alpha"), "text: {:?}", data.text);
    assert!(data.text.contains("Beta"), "text: {:?}", data.text);

    let html = data
        .metadata
        .text_as_html
        .as_deref()
        .expect("table carries text_as_html");
    assert!(html.starts_with("<table>"), "html: {html}");
    assert!(html.ends_with("</table>"), "html: {html}");
    assert_eq!(
        html.matches("<tr>").count(),
        3,
        "three rows in html: {html}"
    );
    assert_eq!(html.matches("<td>").count(), 6, "six cells in html: {html}");
    for expected in ["Name", "Value", "Alpha", "Beta"] {
        assert!(
            html.contains(expected),
            "html must contain {expected}: {html}"
        );
    }
}

#[cfg(feature = "docx")]
#[test]
fn docx_table_with_special_chars_escapes_html() {
    use std::io::Write;
    let xml = r#"<?xml version="1.0"?>
    <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
      <w:body>
        <w:tbl>
          <w:tr>
            <w:tc><w:p><w:r><w:t>&lt;b&gt;bold&lt;/b&gt;</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>a &amp; b</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>
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
    let table = segs.iter().find(|s| s.type_name() == "Table").unwrap();
    let html = table.data().metadata.text_as_html.as_deref().unwrap();
    // Cell text was <b>bold</b> -- must be escaped in text_as_html.
    assert!(
        html.contains("&lt;b&gt;"),
        "angle brackets must be escaped: {html}"
    );
    assert!(html.contains("&amp;"), "ampersands escaped: {html}");
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

// =============================================================================
// strip_to_segments_filtered -- link-density cap
// =============================================================================

#[test]
fn density_filter_drops_pure_link_block() {
    // A <p> full of links vs a <p> full of prose.
    let html = r#"
        <p><a href="/">Home</a> <a href="/a">About</a> <a href="/c">Contact</a></p>
        <p>This paragraph has real prose that is not a link.</p>
    "#;
    // Cap at 0.5: first paragraph (100% link text) dropped, second kept.
    let segs = deformat::html::strip_to_segments_filtered(html, 0.5);
    assert_eq!(segs.len(), 1);
    assert!(segs[0].data().text.contains("real prose"));
}

#[test]
fn density_filter_preserves_prose_with_one_link() {
    let html = "<p>Read <a href='/'>the paper</a> for details on this topic.</p>";
    let segs = deformat::html::strip_to_segments_filtered(html, 0.5);
    assert_eq!(segs.len(), 1);
    assert!(segs[0].data().text.contains("Read"));
}

#[test]
fn density_filter_preserves_titles() {
    // A Title composed entirely of an <a> still survives.
    let html = "<h1><a href='/'>Home</a></h1><nav><a>x</a><a>y</a></nav>";
    let segs = deformat::html::strip_to_segments_filtered(html, 0.5);
    assert!(segs.iter().any(|s| s.type_name() == "Title"));
}

#[test]
fn density_filter_preserves_table_segments() {
    // A Table composed entirely of link-text cells still survives —
    // surviving tables past the scanner-level nav/footer skip are
    // legitimate content (product specs, comparison grids).
    let html = "<article><table>\
                  <tr><td><a href='/a'>Spec A</a></td><td><a href='/b'>Spec B</a></td></tr>\
                </table></article>";
    let segs = deformat::html::strip_to_segments_filtered(html, 0.3);
    assert!(
        segs.iter().any(|s| s.type_name() == "Table"),
        "table preserved under aggressive link-density cap: {:?}",
        segs.iter().map(|s| s.type_name()).collect::<Vec<_>>()
    );
}

// =============================================================================
// Sentence-density filter
// =============================================================================

#[test]
fn sentence_density_drops_punctuationless_prose_block() {
    // A long block of words with zero sentence punctuation — the textual
    // shape of a tag-cloud-as-paragraph that link-density misses when the
    // words aren't wrapped in <a>.
    let html = r#"
        <article>
            <p>ruby python javascript rust golang kotlin scala haskell typescript elixir clojure ocaml fsharp erlang zig nim crystal dart</p>
            <p>The article begins with an actual paragraph containing multiple sentences. It develops ideas over time. The writing has proper punctuation.</p>
        </article>
    "#;
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_low_sentence_density(segs, 1.0);
    // The tag list should be dropped; the prose paragraph kept.
    let texts: Vec<&str> = filtered.iter().map(|s| s.data().text.as_str()).collect();
    assert!(
        !texts.iter().any(|t| t.contains("python javascript")),
        "tag-cloud paragraph kept: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("article begins")),
        "prose paragraph dropped: {texts:?}"
    );
}

#[test]
fn sentence_density_preserves_short_blocks() {
    // Short blocks (< MIN_WORDS_FOR_DENSITY words) are not gated on density
    // because the ratio is noisy. Wrap in <article> so the two <p>s get
    // distinct block keys via sibling indexing.
    let html = "<article><p>Intro paragraph here</p><p>Proper sentence follows. And another.</p></article>";
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_low_sentence_density(segs, 2.0);
    assert_eq!(
        filtered.len(),
        2,
        "both blocks preserved: {:?}",
        filtered.iter().map(|s| &s.data().text).collect::<Vec<_>>()
    );
}

#[test]
fn sentence_density_preserves_titles_headers_lists_tables() {
    // Note: <footer> is in deformat's scanner-level skip list so Footer
    // segments are never emitted from HTML. This test covers the kinds
    // that DO emerge from HTML.
    let html = r#"
        <header>Site name goes here in header block with many more words</header>
        <h1>Title with many words but zero real sentences in it overall</h1>
        <ul>
            <li>First list item word word word word word word word word word word word word</li>
        </ul>
        <table><tr><td>many words in table cell without punctuation at all in this table</td></tr></table>
    "#;
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_low_sentence_density(segs, 10.0); // very aggressive
    let kinds: Vec<&str> = filtered.iter().map(|s| s.type_name()).collect();
    for expected in ["Title", "Header", "ListItem", "Table"] {
        assert!(
            kinds.contains(&expected),
            "expected {expected} in {kinds:?}"
        );
    }
}

#[test]
fn sentence_density_composes_with_link_filter_and_boilerplate() {
    let html = r#"
        <nav><a href="/">Home</a> <a href="/a">About</a> <a href="/b">Contact</a></nav>
        <article>
            <h1>The Headline</h1>
            <p>ruby python rust go kotlin scala clojure ocaml haskell elixir zig nim crystal dart swift lua perl bash</p>
            <p>This is the actual article prose. It has several sentences. Each terminator counts toward the density score.</p>
        </article>
    "#;
    let pipeline = deformat::html::filter_boilerplate(
        deformat::html::filter_low_sentence_density(
            deformat::html::strip_to_segments_filtered(html, 0.45),
            1.0,
        ),
        20,
    );
    let texts: Vec<&str> = pipeline.iter().map(|s| s.data().text.as_str()).collect();
    assert!(
        !texts.iter().any(|t| t.contains("Home")),
        "nav dropped by link-density: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("python rust")),
        "tag-cloud dropped by sentence-density: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("actual article prose")),
        "prose kept: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("The Headline")),
        "title kept: {texts:?}"
    );
}

// =============================================================================
// CETD (sibling-smoothed density) filter
// =============================================================================

#[test]
fn cetd_drops_consecutive_short_boilerplate_run() {
    // Realistic pattern: 10 long article paragraphs, then a RUN of 3
    // short boilerplate labels (like a footer nav row), then more
    // content. CETD smooths across neighbours; an isolated outlier
    // between two long blocks is SHELTERED (by design: reduces
    // false-positives on short-but-legit content between paragraphs).
    // A run of 3+ short siblings produces a sustained low density the
    // smoothing cannot mask.
    let mut html = String::from("<article>");
    for i in 0..10 {
        html.push_str(&format!(
            "<p>Content paragraph {i} with a substantial amount of real body \
             text that represents genuine article content, long enough to \
             clearly exceed the boilerplate noise floor. Each sentence ends \
             with a proper period.</p>"
        ));
    }
    html.push_str("<p>home</p><p>about</p><p>contact</p>");
    for i in 10..20 {
        html.push_str(&format!(
            "<p>Content paragraph {i} continues the article with more body \
             text of similar length.</p>"
        ));
    }
    html.push_str("</article>");

    let segs = deformat::html::strip_to_segments(&html);
    let kept = deformat::html::filter_low_cetd_density(segs, 0.4);
    // At least two of the three short labels should drop -- the middle
    // one is neighbored on both sides by short peers, so its smoothed
    // density is unambiguously low.
    let kept_labels = kept
        .iter()
        .filter(|s| ["home", "about", "contact"].contains(&s.data().text.as_str()))
        .count();
    assert!(
        kept_labels <= 1,
        "short-boilerplate run not dropped by CETD: {kept_labels} of 3 survived"
    );
}

#[test]
fn cetd_shelters_isolated_short_block_between_long_siblings() {
    // The dual to the test above: an isolated short block between long
    // ones should NOT be dropped. CETD's smoothing exists precisely to
    // prevent this false-positive mode.
    let mut html = String::from("<article>");
    for i in 0..5 {
        html.push_str(&format!(
            "<p>Long paragraph {i} with real content. Multiple sentences. \
             Another sentence here. Filler to exceed 40 chars for \
             filter_boilerplate's minimum.</p>"
        ));
    }
    html.push_str("<p>brief interlude</p>");
    for i in 5..10 {
        html.push_str(&format!(
            "<p>Long paragraph {i} resuming the article. More body text. \
             Filler to exceed the minimum. Another sentence.</p>"
        ));
    }
    html.push_str("</article>");

    let segs = deformat::html::strip_to_segments(&html);
    let kept = deformat::html::filter_low_cetd_density(segs, 0.4);
    assert!(
        kept.iter().any(|s| s.data().text == "brief interlude"),
        "isolated short block dropped despite smoothing"
    );
}

#[test]
fn cetd_preserves_structural_types_regardless_of_length() {
    // A short Title / ListItem / Table / CodeSnippet must survive
    // CETD filtering even if their char count is far below the mean.
    let html = r#"<article>
        <h1>X</h1>
        <ul><li>a</li></ul>
        <pre><code>42</code></pre>
        <table><tr><td>y</td></tr></table>
        <p>Long narrative paragraph with enough text that the short structural
           elements above would be flagged as outliers if the filter didn't
           preserve them. Two more sentences. And a third.</p>
        <p>Another long narrative paragraph to drive the mean density. It has
           several sentences as well. Content keeps coming.</p>
        <p>Yet another sibling carrying mean density for the smoothing.</p>
    </article>"#;
    let segs = deformat::html::strip_to_segments(html);
    let kept = deformat::html::filter_low_cetd_density(segs, 0.8);
    let kinds: Vec<&str> = kept.iter().map(|s| s.type_name()).collect();
    for expected in ["Title", "ListItem", "Table", "CodeSnippet"] {
        assert!(
            kinds.contains(&expected),
            "{expected} preserved under aggressive CETD cap 0.8: {kinds:?}"
        );
    }
}

#[test]
fn cetd_passes_through_when_fewer_than_three_narrative_segments() {
    // Too few samples for smoothing to mean anything -- passthrough.
    let html = "<p>one</p><p>two</p>";
    let segs = deformat::html::strip_to_segments(html);
    let n_before = segs.len();
    let kept = deformat::html::filter_low_cetd_density(segs, 0.99);
    assert_eq!(kept.len(), n_before);
}

#[test]
fn cetd_zero_floor_keeps_everything() {
    let html = "<article><p>A A A A A A A A A A A A A A A A.</p>\
                <p>b</p>\
                <p>B B B B B B B B B B B B B B B B.</p></article>";
    let segs = deformat::html::strip_to_segments(html);
    let kept = deformat::html::filter_low_cetd_density(segs.clone(), 0.0);
    assert_eq!(kept.len(), segs.len());
}

#[test]
fn cetd_composes_with_other_filters_in_four_filter_pipeline() {
    // Four-filter pipeline: link-density -> sentence-density ->
    // boilerplate -> CETD. Verify composition doesn't drop a clearly
    // content-shaped article.
    let html = r#"<article>
        <h1>Research findings</h1>
        <p>Researchers found something interesting. The effect was strong.
           The data supported the hypothesis. More details follow below.</p>
        <p>The second paragraph expands on the result. Control experiments
           confirmed the baseline. Replication across three sites.</p>
        <p>Discussion of limitations. The sample size was modest. Future
           work should expand to a larger cohort.</p>
    </article>"#;
    let segs = deformat::html::strip_to_segments_filtered(html, 0.45);
    let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
    let segs = deformat::html::filter_boilerplate(segs, 40);
    let final_segs = deformat::html::filter_low_cetd_density(segs, 0.4);
    assert!(!final_segs.is_empty());
    // Ensure the research content survives.
    let joined: String = final_segs
        .iter()
        .map(|s| s.data().text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(joined.contains("Researchers found"));
    assert!(joined.contains("second paragraph"));
    assert!(joined.contains("limitations"));
}

// =============================================================================
// Page-type classifier (heuristic)
// =============================================================================

#[test]
fn page_type_real_world_article_with_og_type() {
    let html = r#"<html><head>
        <meta property="og:type" content="article">
        <meta property="og:title" content="Real story">
    </head><body><article><p>Body.</p></article></body></html>"#;
    assert_eq!(
        deformat::page_type::detect_page_type(html),
        deformat::page_type::PageType::Article
    );
}

#[test]
fn page_type_json_ld_blog_posting() {
    let html = r#"<html><head>
        <script type="application/ld+json">
        { "@context": "https://schema.org", "@type": "BlogPosting", "headline": "y" }
        </script>
    </head></html>"#;
    assert_eq!(
        deformat::page_type::detect_page_type(html),
        deformat::page_type::PageType::Article
    );
}

#[test]
fn page_type_conflicting_signals_og_wins() {
    // og:type says article; <article> says article; page looks like product
    // via price class -- og:type wins (highest priority).
    let html = r#"<html><head>
        <meta property="og:type" content="article">
    </head><body>
        <span class="price">$10</span>
        <span class="price">$20</span>
        <span class="price">$30</span>
    </body></html>"#;
    assert_eq!(
        deformat::page_type::detect_page_type(html),
        deformat::page_type::PageType::Article
    );
}

#[test]
fn page_type_defaults_to_unknown_when_no_signal() {
    assert_eq!(
        deformat::page_type::detect_page_type("<p>plain</p>"),
        deformat::page_type::PageType::Unknown
    );
}

#[test]
fn sentence_density_zero_cap_keeps_everything() {
    let html = "<p>ruby python rust go kotlin scala clojure ocaml haskell elixir zig nim crystal dart swift lua perl bash awk sed tcl vim emacs</p>";
    let segs = deformat::html::strip_to_segments(html);
    let filtered = deformat::html::filter_low_sentence_density(segs, 0.0);
    assert_eq!(filtered.len(), 1);
}

// =============================================================================
// Segment::Image emission
// =============================================================================

#[test]
fn standalone_figure_img_emits_image_segment() {
    let html = r#"<article>
        <h1>Paper</h1>
        <p>Some intro text that has real sentences here.</p>
        <figure><img src="diagram.png" alt="Experimental setup diagram"></figure>
        <p>Closing text with another complete sentence.</p>
    </article>"#;
    let segs = deformat::html::strip_to_segments(html);
    let image = segs.iter().find(|s| s.type_name() == "Image");
    assert!(
        image.is_some(),
        "expected an Image segment, got {:?}",
        segs.iter().map(|s| s.type_name()).collect::<Vec<_>>()
    );
    let data = image.unwrap().data();
    assert_eq!(data.text, "Experimental setup diagram");
    // Image segments under a title should carry parent_id.
    assert!(data.metadata.parent_id.is_some());
}

#[test]
fn inline_img_does_not_emit_image_segment() {
    // An <img> inside a paragraph must NOT split out as a separate Image
    // segment -- the alt text is part of the surrounding NarrativeText.
    let html = r#"<article><p>Look at <img alt="pic"> inline.</p></article>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert!(
        segs.iter().all(|s| s.type_name() != "Image"),
        "inline img should not produce Image segment: {:?}",
        segs.iter()
            .map(|s| (s.type_name(), s.data().text.clone()))
            .collect::<Vec<_>>()
    );
    let narrative = segs
        .iter()
        .find(|s| s.type_name() == "NarrativeText")
        .expect("narrative segment present");
    assert!(narrative.data().text.contains("pic"));
    assert!(narrative.data().text.contains("Look at"));
}

#[test]
fn bare_img_outside_any_block_emits_image() {
    let html = r#"<img alt="standalone image">"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 1, "got {:?}", segs);
    assert_eq!(segs[0].type_name(), "Image");
    assert_eq!(segs[0].data().text, "standalone image");
}

#[test]
fn multiple_figures_each_emit_image() {
    let html = r#"<article>
        <h1>Gallery</h1>
        <figure><img alt="first image caption"></figure>
        <figure><img alt="second image caption"></figure>
    </article>"#;
    let segs = deformat::html::strip_to_segments(html);
    let images: Vec<_> = segs.iter().filter(|s| s.type_name() == "Image").collect();
    assert_eq!(images.len(), 2, "expected two Image segments");
    let texts: Vec<&str> = images.iter().map(|s| s.data().text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("first image caption")));
    assert!(texts.iter().any(|t| t.contains("second image caption")));
}

#[test]
fn img_inside_heading_stays_title() {
    // <h1><img alt="Logo"></h1> — logo image in a heading. The structural
    // Title role wins over Image.
    let html = r#"<h1><img alt="Logo Text"></h1>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].type_name(), "Title");
    assert_eq!(segs[0].data().text, "Logo Text");
}

#[test]
fn img_inside_table_cell_stays_table() {
    let html = r#"<article><table><tr><td><img alt="chart pic"></td></tr></table></article>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert!(
        segs.iter().any(|s| s.type_name() == "Table"),
        "img inside td -> Table: {:?}",
        segs.iter().map(|s| s.type_name()).collect::<Vec<_>>()
    );
    assert!(!segs.iter().any(|s| s.type_name() == "Image"));
}

#[test]
fn img_inside_list_item_stays_listitem() {
    let html = r#"<ul><li><img alt="bullet image"></li></ul>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].type_name(), "ListItem");
}

#[test]
fn img_inside_pre_stays_codesnippet() {
    let html = r#"<pre><img alt="code image"></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].type_name(), "CodeSnippet");
}

// =============================================================================
// <details>/<summary> classification
// =============================================================================

#[test]
fn summary_emits_as_title_with_children_under_it() {
    let html = r#"<article>
        <details>
            <summary>Click to expand</summary>
            <p>Hidden paragraph one.</p>
            <p>Hidden paragraph two.</p>
        </details>
    </article>"#;
    let segs = deformat::html::strip_to_segments(html);
    // Summary is a Title; following paragraphs are NarrativeText with
    // parent_id = summary's element_id.
    let summary = segs
        .iter()
        .find(|s| s.data().text.contains("Click to expand"))
        .expect("summary segment present");
    assert_eq!(summary.type_name(), "Title");
    let summary_id = &summary.data().element_id;

    let para_one = segs
        .iter()
        .find(|s| s.data().text.contains("Hidden paragraph one"))
        .unwrap();
    assert_eq!(para_one.type_name(), "NarrativeText");
    assert_eq!(
        para_one.data().metadata.parent_id.as_deref(),
        Some(summary_id.as_str())
    );
}

#[test]
fn summary_has_no_category_depth() {
    // Summary is title-like but not an h1-h6, so category_depth stays unset.
    let html = r#"<details><summary>Toggle</summary><p>Body.</p></details>"#;
    let segs = deformat::html::strip_to_segments(html);
    let summary = segs
        .iter()
        .find(|s| s.data().text.contains("Toggle"))
        .unwrap();
    assert_eq!(summary.type_name(), "Title");
    assert_eq!(summary.data().metadata.category_depth, None);
}

// =============================================================================
// CodeSnippet language hints
// =============================================================================

#[test]
fn code_block_with_language_class_populates_languages() {
    let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].type_name(), "CodeSnippet");
    assert_eq!(
        segs[0].data().metadata.languages.as_deref(),
        Some(["rust".to_string()].as_slice())
    );
}

#[test]
fn code_block_with_lang_prefix_class_populates_languages() {
    // Some highlighters use `lang-X` instead of `language-X`.
    let html = r#"<pre><code class="lang-python">print(1)</code></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(
        segs[0].data().metadata.languages.as_deref(),
        Some(["python".to_string()].as_slice())
    );
}

#[test]
fn code_block_with_mixed_class_finds_language_token() {
    // highlight.js wraps in `hljs language-X`. Language must be found
    // among whitespace-separated class tokens.
    let html = r#"<pre><code class="hljs language-go">package main</code></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(
        segs[0].data().metadata.languages.as_deref(),
        Some(["go".to_string()].as_slice())
    );
}

#[test]
fn code_block_without_language_class_has_no_languages() {
    let html = r#"<pre><code>plain code</code></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(segs[0].type_name(), "CodeSnippet");
    assert!(segs[0].data().metadata.languages.is_none());
}

#[test]
fn code_block_language_is_lowercased() {
    // Language identifiers normalize to lowercase so `language-Rust`
    // matches `language-rust` in downstream tooling.
    let html = r#"<pre><code class="language-Rust">code</code></pre>"#;
    let segs = deformat::html::strip_to_segments(html);
    assert_eq!(
        segs[0].data().metadata.languages.as_deref(),
        Some(["rust".to_string()].as_slice())
    );
}
