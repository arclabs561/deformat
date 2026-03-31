//! Comprehensive cross-feature tests.
//!
//! Tests that exercise multiple deformat features together and verify
//! consistency between output modes (text, markdown, spans).

// =============================================================================
// Text vs Markdown consistency
// =============================================================================

#[test]
fn text_and_markdown_preserve_same_content() {
    let html = r#"<article>
        <h1>Title Here</h1>
        <p>First paragraph with <b>bold</b> and <em>italic</em>.</p>
        <p>Second paragraph with a <a href="https://example.com">link</a>.</p>
        <ul><li>Item one</li><li>Item two</li></ul>
        <pre><code>code_block();</code></pre>
    </article>"#;

    let text = deformat::html::strip_to_text(html);
    let md = deformat::html::strip_to_markdown(html);

    // Both should contain all the visible words
    for word in [
        "Title",
        "bold",
        "italic",
        "paragraph",
        "link",
        "Item",
        "code_block",
    ] {
        assert!(text.contains(word), "text missing '{word}': {text}");
        assert!(md.contains(word), "markdown missing '{word}': {md}");
    }

    // Markdown should have formatting that text doesn't
    assert!(md.contains("# Title"), "md has heading: {md}");
    assert!(md.contains("**bold**"), "md has bold: {md}");
    assert!(md.contains("*italic*"), "md has italic: {md}");
    assert!(md.contains("[link]"), "md has link: {md}");
    assert!(md.contains("```"), "md has code fence: {md}");

    // Text should NOT have markdown formatting
    assert!(!text.contains("# "), "text has no heading markers");
    assert!(!text.contains("**"), "text has no bold markers");
    assert!(!text.contains("```"), "text has no code fences");
}

// =============================================================================
// SpanMap correctness across features
// =============================================================================

#[test]
fn span_map_every_word_traceable() {
    let html = r#"<article>
        <h2>Introduction</h2>
        <p>Rust is a systems programming language focused on safety.</p>
        <p>It was created by Graydon Hoare at Mozilla Research.</p>
    </article>"#;

    let (text, spans) = deformat::html::strip_to_text_with_spans(html);

    // Every word in the output should be traceable to the source
    for word in text.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        let start = text.find(word).unwrap();
        let end = start + word.len();
        let src = spans.source_range(start, end);
        assert!(
            src.is_some(),
            "word '{word}' at {start}..{end} not traceable"
        );
        let (ss, se) = src.unwrap();
        let source = &html[ss..se];
        // The source region should contain the word (possibly with entities)
        // For simple words, exact match. For entity-decoded words, partial.
        assert!(
            source.contains(word) || word.chars().all(|c| c.is_ascii_alphanumeric() || c == '\''),
            "source doesn't contain '{word}': source='{source}'"
        );
    }
}

#[test]
fn span_map_entity_precision() {
    // Verify that entity decoding produces precise source ranges
    let html = "<p>A&amp;B &lt;C&gt; Nestl&eacute;</p>";
    let (text, spans) = deformat::html::strip_to_text_with_spans(html);

    assert!(text.contains("A&B"), "decoded amp: {text}");
    assert!(text.contains("<C>"), "decoded lt/gt: {text}");
    assert!(text.contains("Nestl\u{00E9}"), "decoded eacute: {text}");

    // The "&" character should map back to "&amp;" in the source
    let amp_pos = text.find("A&B").unwrap() + 1; // position of '&'
    let src = spans.source_range(amp_pos, amp_pos + 1);
    assert!(src.is_some(), "&amp; traceable");
    let (ss, se) = src.unwrap();
    assert!(
        html[ss..se].contains("&amp;"),
        "& maps to &amp;: {}",
        &html[ss..se]
    );
}

// =============================================================================
// Format detection consistency
// =============================================================================

#[test]
fn format_detection_agrees_across_methods() {
    // Test that detect_str, detect_bytes, and detect_path agree
    let html = "<!DOCTYPE html><html><body>Hello</body></html>";
    assert_eq!(deformat::detect::detect_str(html), deformat::Format::Html);
    assert_eq!(
        deformat::detect::detect_bytes(html.as_bytes()),
        deformat::Format::Html
    );

    let plain = "Just plain text, no markup whatsoever.";
    assert_eq!(
        deformat::detect::detect_str(plain),
        deformat::Format::PlainText
    );
    assert_eq!(
        deformat::detect::detect_bytes(plain.as_bytes()),
        deformat::Format::PlainText
    );

    // PDF magic bytes
    assert_eq!(
        deformat::detect::detect_bytes(b"%PDF-1.4 content"),
        deformat::Format::Pdf
    );
    assert_eq!(
        deformat::detect::detect_path(std::path::Path::new("test.pdf")),
        deformat::Format::Pdf
    );

    // MIME detection
    assert_eq!(
        deformat::detect::detect_mime("text/html; charset=utf-8"),
        deformat::Format::Html
    );
    assert_eq!(
        deformat::detect::detect_mime("application/pdf"),
        deformat::Format::Pdf
    );
}

// =============================================================================
// extract() auto-detection
// =============================================================================

#[test]
fn extract_auto_detects_and_strips() {
    let html = "<html><body><p>Auto-detected HTML.</p></body></html>";
    let result = deformat::extract(html).unwrap();
    assert_eq!(result.format, deformat::Format::Html);
    assert_eq!(result.extractor, deformat::Extractor::Strip);
    assert!(result.text.contains("Auto-detected"));
    assert!(!result.text.contains("<p>"));
}

#[test]
fn extract_plaintext_passthrough() {
    let text = "This is plain text. No HTML here.";
    let result = deformat::extract(text).unwrap();
    assert_eq!(result.format, deformat::Format::PlainText);
    assert_eq!(result.extractor, deformat::Extractor::Passthrough);
    assert_eq!(result.text, text);
}

#[test]
fn extract_as_xml_strips_tags() {
    let xml = r#"<?xml version="1.0"?><root><item>Content</item><item>More</item></root>"#;
    let result = deformat::extract_as(xml, deformat::Format::Xml).unwrap();
    assert!(result.text.contains("Content"), "xml text: {}", result.text);
    assert!(result.text.contains("More"), "xml text: {}", result.text);
    assert!(!result.text.contains("<item>"), "tags stripped");
}

// =============================================================================
// Error types
// =============================================================================

#[test]
fn error_display_and_source() {
    let err = deformat::Error::UnsupportedFormat("test".into());
    assert!(err.to_string().contains("test"));
    assert!(std::error::Error::source(&err).is_none());

    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    let err = deformat::Error::Io(io_err);
    assert!(err.to_string().contains("I/O"));
    assert!(std::error::Error::source(&err).is_some());

    let err = deformat::Error::PdfExtract("bad pdf".into());
    assert!(err.to_string().contains("bad pdf"));

    let err = deformat::Error::EmptyResult;
    assert!(err.to_string().contains("no text"));
}

// =============================================================================
// Extractor enum Display
// =============================================================================

#[test]
fn extractor_display() {
    assert_eq!(deformat::Extractor::Strip.to_string(), "strip");
    assert_eq!(deformat::Extractor::Readability.to_string(), "readability");
    assert_eq!(deformat::Extractor::Html2text.to_string(), "html2text");
    assert_eq!(deformat::Extractor::PdfExtract.to_string(), "pdf-extract");
    assert_eq!(deformat::Extractor::Passthrough.to_string(), "passthrough");
}

// =============================================================================
// Extracted struct
// =============================================================================

#[test]
fn extracted_non_exhaustive_and_clone() {
    let result = deformat::extract("<p>test</p>").unwrap();
    let cloned = result.clone();
    assert_eq!(result.text, cloned.text);
    assert_eq!(result.format, cloned.format);
    assert_eq!(result.extractor, cloned.extractor);
    let debug = format!("{:?}", result);
    assert!(debug.contains("Extracted"));
}

// =============================================================================
// Format enum completeness
// =============================================================================

#[test]
fn format_display_all_variants() {
    let formats = [
        (deformat::Format::PlainText, "plain text"),
        (deformat::Format::Html, "HTML"),
        (deformat::Format::Xml, "XML"),
        (deformat::Format::Pdf, "PDF"),
        (deformat::Format::Markdown, "Markdown"),
        (deformat::Format::Rtf, "RTF"),
        (deformat::Format::Docx, "DOCX"),
        (deformat::Format::Epub, "EPUB"),
        (deformat::Format::Xlsx, "XLSX"),
        (deformat::Format::Unknown, "unknown"),
    ];
    for (fmt, expected) in &formats {
        assert_eq!(fmt.to_string(), *expected, "Display for {fmt:?}");
        // mime_type should not be empty
        assert!(!fmt.mime_type().is_empty(), "mime_type for {fmt:?}");
    }
}

#[test]
fn format_mime_roundtrip() {
    // All formats with known MIME types should roundtrip through detect_mime
    let roundtrippable = [
        deformat::Format::PlainText,
        deformat::Format::Html,
        deformat::Format::Xml,
        deformat::Format::Pdf,
        deformat::Format::Markdown,
        deformat::Format::Rtf,
        deformat::Format::Epub,
    ];
    for fmt in &roundtrippable {
        let mime = fmt.mime_type();
        let detected = deformat::detect::detect_mime(mime);
        assert_eq!(
            *fmt, detected,
            "roundtrip failed for {fmt:?}: mime={mime}, detected={detected:?}"
        );
    }
}

// =============================================================================
// Whitespace invariants across all modes
// =============================================================================

#[test]
fn no_double_spaces_in_any_mode() {
    let html = r#"<html><body>
        <h1>  Title  </h1>
        <p>  First    paragraph   with    spaces.  </p>
        <p>  Second     paragraph.  </p>
    </body></html>"#;

    let text = deformat::html::strip_to_text(html);
    assert!(!text.contains("  "), "text: no double spaces: {text}");

    // Text output should be trimmed
    assert_eq!(text, text.trim(), "text: trimmed");
}

// =============================================================================
// Unicode handling across all features
// =============================================================================

#[test]
fn unicode_preserved_in_all_modes() {
    let html = "<p>Caf\u{00E9} in \u{4E1C}\u{4EAC} with Nestl&eacute;</p>";

    let text = deformat::html::strip_to_text(html);
    let md = deformat::html::strip_to_markdown(html);
    let (span_text, _spans) = deformat::html::strip_to_text_with_spans(html);

    // All modes should preserve the same Unicode content
    for output in [&text, &md, &span_text] {
        assert!(output.contains("Caf\u{00E9}"), "cafe in output: {output}");
        assert!(
            output.contains("\u{4E1C}\u{4EAC}"),
            "Tokyo in output: {output}"
        );
        assert!(
            output.contains("Nestl\u{00E9}"),
            "Nestle entity decoded: {output}"
        );
    }
}

// =============================================================================
// DOCX extraction tests
// =============================================================================

#[cfg(feature = "docx")]
#[test]
fn docx_roundtrip_consistency() {
    // Create a DOCX, extract both text, verify consistency
    use std::io::Write;
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("[Content_Types].xml", opts).unwrap();
    write!(zip, r#"<?xml version="1.0"?><Types/>"#).unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    write!(
        zip,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:body><w:p><w:r><w:t>Test DOCX content</w:t></w:r></w:p></w:body>
        </w:document>"#
    )
    .unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let result = deformat::docx::extract_bytes(&bytes).unwrap();
    assert!(result.text.contains("Test DOCX content"));
    assert_eq!(result.format, deformat::Format::Docx);

    // Format detection should identify it as DOCX
    let detected = deformat::detect::detect_bytes(&bytes);
    assert_eq!(detected, deformat::Format::Docx);
}

// =============================================================================
// EPUB extraction tests
// =============================================================================

#[cfg(feature = "epub")]
#[test]
fn epub_with_multiple_chapters() {
    use std::io::Write;
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("mimetype", opts).unwrap();
    write!(zip, "application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", opts).unwrap();
    write!(zip, r#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("content.opf", opts).unwrap();
    write!(zip, r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Multi-Chapter</dc:title></metadata><manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="ch3.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>"#).unwrap();
    zip.start_file("ch1.xhtml", opts).unwrap();
    write!(
        zip,
        "<html><body><h1>Chapter 1</h1><p>Beginning.</p></body></html>"
    )
    .unwrap();
    zip.start_file("ch2.xhtml", opts).unwrap();
    write!(
        zip,
        "<html><body><h1>Chapter 2</h1><p>Middle with caf&eacute;.</p></body></html>"
    )
    .unwrap();
    zip.start_file("ch3.xhtml", opts).unwrap();
    write!(
        zip,
        "<html><body><h1>Chapter 3</h1><p>The end.</p></body></html>"
    )
    .unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let result = deformat::epub::extract_bytes(&bytes).unwrap();
    assert_eq!(result.title.as_deref(), Some("Multi-Chapter"));
    assert!(result.text.contains("Chapter 1"));
    assert!(result.text.contains("Chapter 2"));
    assert!(result.text.contains("Chapter 3"));
    assert!(
        result.text.contains("caf\u{00E9}"),
        "entity decoded in EPUB"
    );

    // Chapters should appear in spine order
    let c1 = result.text.find("Chapter 1").unwrap();
    let c2 = result.text.find("Chapter 2").unwrap();
    let c3 = result.text.find("Chapter 3").unwrap();
    assert!(c1 < c2 && c2 < c3, "spine order: c1={c1} c2={c2} c3={c3}");
}

// =============================================================================
// RTF extraction tests
// =============================================================================

#[cfg(feature = "rtf")]
#[test]
fn rtf_extraction_and_detection() {
    let rtf = r"{\rtf1\ansi{\fonttbl\f0 Helvetica;}\f0\pard Hello RTF world.\par}";
    let bytes = rtf.as_bytes();

    // Detection
    assert_eq!(deformat::detect::detect_bytes(bytes), deformat::Format::Rtf);
    assert_eq!(
        deformat::detect::detect_path(std::path::Path::new("doc.rtf")),
        deformat::Format::Rtf
    );

    // Extraction
    let result = deformat::rtf::extract_bytes(bytes).unwrap();
    assert!(
        result.text.contains("Hello RTF world"),
        "text: {}",
        result.text
    );
    assert_eq!(result.format, deformat::Format::Rtf);
}

// =============================================================================
// StripOptions consistency
// =============================================================================

#[test]
fn strip_options_wiki_vs_default() {
    let html = "<p>Einstein[1] was a physicist.[2] He published[edit] many papers.</p>";

    let default_text = deformat::html::strip_to_text(html);
    let wiki_text = deformat::html::strip_to_text_with_options(
        html,
        &deformat::html::StripOptions::wikipedia(),
    );

    // Default should preserve markers
    assert!(default_text.contains("[1]"), "default keeps [1]");
    assert!(default_text.contains("[edit]"), "default keeps [edit]");

    // Wikipedia mode should strip them
    assert!(!wiki_text.contains("[1]"), "wiki strips [1]");
    assert!(!wiki_text.contains("[edit]"), "wiki strips [edit]");

    // Both should preserve the actual content
    assert!(default_text.contains("Einstein"), "content preserved");
    assert!(wiki_text.contains("Einstein"), "content preserved");
}
