//! Regression tests for EPUB href resolution, PDF font decoding across
//! documents, and HTML segment text joining.
//!
//! Run with `cargo test --features epub,pdf --test extraction_regressions`.
#![allow(dead_code)]

// ---------------------------------------------------------------- EPUB

#[cfg(feature = "epub")]
fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[cfg(feature = "epub")]
fn epub(opf_path: &str, opf: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let container = format!(
        r#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="{opf_path}" media-type="application/oebps-package+xml"/></rootfiles></container>"#
    );
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        (opf_path, opf.as_bytes()),
    ];
    for (name, body) in files {
        entries.push((name, body.as_bytes()));
    }
    make_zip(&entries)
}

/// OPF manifest hrefs are URLs (EPUB 3.3: item@href is a valid URL
/// string), so a file named "Chapter 1.xhtml" is referenced as
/// "Chapter%201.xhtml". Looking up the raw href as a ZIP entry name
/// silently drops the chapter.
#[cfg(feature = "epub")]
#[test]
fn epub_percent_encoded_href_is_resolved() {
    let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest><item id="c1" href="Text/Chapter%201.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="Text/ch2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#;
    let bytes = epub(
        "OEBPS/content.opf",
        opf,
        &[
            (
                "OEBPS/Text/Chapter 1.xhtml",
                "<html><body><p>Call me Ishmael.</p></body></html>",
            ),
            (
                "OEBPS/Text/ch2.xhtml",
                "<html><body><p>Second chapter.</p></body></html>",
            ),
        ],
    );
    let out = deformat::epub::extract_bytes(&bytes).unwrap();
    assert!(
        out.text.contains("Call me Ishmael"),
        "chapter 1 dropped: {:?}",
        out.text
    );
}

/// Hrefs are relative to the OPF: "../Text/ch1.xhtml" from
/// "OEBPS/pkg/package.opf" names "OEBPS/Text/ch1.xhtml". The raw join
/// "OEBPS/pkg/../Text/ch1.xhtml" is not a ZIP entry name, so every chapter
/// was dropped and the book failed with EmptyResult (the name-scan
/// fallback does not run because the spine was non-empty).
#[cfg(feature = "epub")]
#[test]
fn epub_dotdot_href_is_resolved() {
    let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest><item id="c1" href="../Text/ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let bytes = epub(
        "OEBPS/pkg/package.opf",
        opf,
        &[(
            "OEBPS/Text/ch1.xhtml",
            "<html><body><p>Call me Ishmael.</p></body></html>",
        )],
    );
    let out = deformat::epub::extract_bytes(&bytes);
    assert!(
        out.as_ref()
            .is_ok_and(|o| o.text.contains("Call me Ishmael")),
        "{out:?}"
    );
}

// ---------------------------------------------------------------- PDF

/// A one-page PDF whose only font is a subset-tagged TrueType font with a
/// `/ToUnicode` CMap mapping codes 1..=3 to `chars`. Every document built
/// here has the same object numbers and the same `/BaseFont`, as PDFs from
/// one producer template do; only the CMap content differs.
#[cfg(feature = "pdf")]
fn one_font_pdf(chars: [char; 3]) -> Vec<u8> {
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         3 beginbfchar\n<01> <{:04X}>\n<02> <{:04X}>\n<03> <{:04X}>\nendbfchar\n\
         endcmap\nCMapName currentdict /CMapResource defineresource pop\nend\nend\n",
        chars[0] as u32, chars[1] as u32, chars[2] as u32
    );
    let content = "BT /F1 12 Tf 72 700 Td <010203> Tj ET\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /AAAAAA+DejaVuSans \
         /FirstChar 1 /LastChar 3 /Widths [600 600 600] /ToUnicode 6 0 R >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
        format!("<< /Length {} >>\nstream\n{cmap}endstream", cmap.len()),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// Font decoding must depend only on the document being extracted. Each of
/// these PDFs decodes correctly on its own; extracting them in sequence in
/// one process must not reuse an earlier document's `/ToUnicode` mapping
/// for a later document whose font merely has the same name and object
/// numbers.
#[cfg(feature = "pdf")]
#[test]
fn pdf_text_does_not_depend_on_previously_extracted_documents() {
    let cases = [['C', 'a', 't'], ['D', 'o', 'g'], ['é', 'à', 'ü']];
    let mut bad = Vec::new();
    // Twice through, so every document is extracted both before and after
    // the others.
    for round in 0..2 {
        for chars in cases {
            let want: String = chars.iter().collect();
            let got = deformat::pdf::extract_bytes(&one_font_pdf(chars))
                .map(|e| e.text)
                .unwrap_or_else(|e| format!("error: {e}"));
            if !got.contains(&want) {
                bad.push(format!("round {round}: want {want:?}, got {got:?}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "wrong text after earlier extractions: {bad:#?}"
    );
}

// ---------------------------------------------------------------- HTML segments

/// Segment text is documented as the block's text with whitespace
/// normalized and entities decoded, so inline tags and entity boundaries
/// inside a block must not become spaces.
#[test]
fn segment_text_does_not_split_words_at_inline_boundaries() {
    let cases = [
        ("<p>Caf&eacute; au lait</p>", "Café au lait"),
        (
            "<p>don&rsquo;t <b>stop</b> now.</p>",
            "don\u{2019}t stop now.",
        ),
        (
            "<p>See <a href=\"/x\">the docs</a>, then go.</p>",
            "See the docs, then go.",
        ),
        ("<p>un<b>believ</b>able</p>", "unbelievable"),
    ];
    for (html, want) in cases {
        let segs = deformat::html::strip_to_segments(html);
        let got: Vec<String> = segs.iter().map(|s| s.data().text.clone()).collect();
        let extracted = deformat::extract(html).unwrap().text;
        assert_eq!(extracted, want, "extract() baseline");
        assert_eq!(got, vec![want.to_string()], "segment text for {html:?}");
    }
}

fn assert_segments_are_substrings_of_strip_to_text(html: &str) {
    let text = deformat::html::strip_to_text(html);
    let segs = deformat::html::strip_to_segments(html);
    let mut missing = vec![];
    for s in &segs {
        let t = s.data().text.trim();
        if !t.is_empty() && !text.contains(t) {
            missing.push(t.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "segments not found in strip_to_text output {text:?}: {missing:?}"
    );
}

/// Segment text uses the same whitespace normalization and entity
/// decoding as `strip_to_text`, so within a block it is locatable in that
/// output: collapsed runs, CRLF, NBSP and entities must not differ.
#[test]
fn segments_within_one_block_are_substrings_of_strip_to_text() {
    assert_segments_are_substrings_of_strip_to_text(
        "<html><body>\r\n<h1>Caf&eacute; r\u{e9}sum\u{e9}</h1>\r\n<p>First  sentence\r\nwraps here. Second\u{a0}one has \u{1F680} emoji.</p>\r\n<ul><li>Item\u{a0}one</li><li>Item &amp; two</li></ul>\r\n<p>Zo\u{eb} na\u{ef}ve &lt;tag&gt; \u{6771}\u{4eac}.</p>\r\n</body></html>",
    );
}

/// Bug-hunt expectation kept verbatim: every segment's text, including
/// table rows and paragraphs containing `<br>`, should be a substring of
/// `strip_to_text`.
///
/// Ignored because it conflicts with the pinned segment contract in
/// `tests/segments.rs::structural_newline_spans_do_not_leak_into_segment_text`
/// (`<td>a</td><td>b</td>` -> "a b", `a<br>b` -> "a b"), while
/// `strip_to_text` emits "a\nb" for both. The two remaining mismatches are
/// exactly those joins: "Zoë naïve <tag> 東京. After break" and "a1 b1".
/// Making segments keep the newline is a contract change, not a bug fix.
#[test]
#[ignore = "conflicts with the pinned block-boundary join (\"a b\"); see doc comment"]
fn segments_are_substrings_of_strip_to_text() {
    assert_segments_are_substrings_of_strip_to_text(
        "<html><body>\r\n<h1>Caf&eacute; r\u{e9}sum\u{e9}</h1>\r\n<p>First  sentence\r\nwraps here. Second\u{a0}one has \u{1F680} emoji.</p>\r\n<ul><li>Item\u{a0}one</li><li>Item &amp; two</li></ul>\r\n<p>Zo\u{eb} na\u{ef}ve &lt;tag&gt; \u{6771}\u{4eac}.<br>After break</p>\r\n<table><tr><td>a1</td><td>b1</td></tr></table>\r\n</body></html>",
    );
}
