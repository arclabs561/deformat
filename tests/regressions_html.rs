//! Regression tests for reported HTML, entity and charset defects. Each
//! asserts the expected behavior; open ones are #[ignore = "open: ..."].

use deformat::html::{decode_entities, strip_to_text};

/// `extract_meta_charset` advances `pos = tag_end + 1`; when a `<meta` tag has
/// no `>` inside the 1 KiB prescan window (and no charset), `pos` exceeds the
/// string length and the next `lower[pos..]` slice panics.
#[cfg(feature = "encoding_rs")]
#[test]
fn decode_bytes_meta_without_close_does_not_panic() {
    let r = std::panic::catch_unwind(|| {
        deformat::detect::decode_bytes(b"<meta name=x", "utf-8").into_owned()
    });
    assert!(r.is_ok(), "decode_bytes panicked on a truncated <meta>");
}

/// Same root cause on an ordinary page: a long description <meta> that
/// straddles byte 1024 of the prescan window.
#[cfg(feature = "encoding_rs")]
#[test]
fn decode_bytes_meta_straddling_prescan_window_does_not_panic() {
    let mut html = String::from("<!doctype html><html><head><meta name=\"description\" content=\"");
    html.push_str(&"x".repeat(1100));
    html.push_str("\"><title>t</title></head><body><p>hello</p></body></html>");
    let bytes = html.into_bytes();
    let r =
        std::panic::catch_unwind(|| deformat::detect::decode_bytes(&bytes, "utf-8").into_owned());
    assert!(
        r.is_ok(),
        "decode_bytes panicked on a long description meta"
    );
}

/// WHATWG prescan: a meta charset of UTF-16BE/LE in an ASCII-compatible byte
/// stream is replaced by UTF-8 (https://html.spec.whatwg.org/multipage/parsing.html#prescan-a-byte-stream-to-determine-its-encoding).
#[cfg(feature = "encoding_rs")]
#[test]
fn decode_bytes_meta_utf16_is_treated_as_utf8() {
    let bytes = b"<html><head><meta charset=\"utf-16\"></head><body><p>hello</p></body></html>";
    let s = deformat::detect::decode_bytes(bytes, "windows-1252");
    assert!(
        s.contains("hello"),
        "decoded as UTF-16: {:?}",
        s.chars().take(12).collect::<String>()
    );
}

/// A literal U+00A0 (or any non-ASCII whitespace) right before a block
/// boundary sets `last_was_space`, so the block newline that follows is
/// treated as a dropped control character and the paragraphs merge.
#[test]
fn nbsp_before_block_boundary_keeps_newline() {
    assert_eq!(strip_to_text("<p>Hello</p><p>World</p>"), "Hello\nWorld");
    assert_eq!(
        strip_to_text("<p>Hello\u{a0}</p><p>World</p>"),
        "Hello\nWorld"
    );
    assert_eq!(
        strip_to_text("<p>\u{6771}\u{4EAC}\u{3000}</p><p>x</p>"),
        "\u{6771}\u{4EAC}\nx"
    );
}

/// XML goes through strip_to_text (README: "XML | &str | html::strip_to_text"),
/// but CDATA is treated as a bogus `<!` directive and skipped to the first `>`.
#[test]
fn xml_cdata_text_is_kept() {
    let r = deformat::extract_as(
        "<rss><item><description><![CDATA[Profit > 5% in Q3]]></description></item></rss>",
        deformat::Format::Xml,
    )
    .unwrap();
    assert_eq!(r.text, "Profit > 5% in Q3");
}

/// HTML5 tokenizer: `<!-->` is an (abruptly closed) empty comment, so the
/// following text is visible.
#[test]
#[ignore = "open: <!--> swallows following text (fixed in #22)"]
fn abrupt_empty_comment_does_not_swallow_text() {
    assert_eq!(strip_to_text("<p><!-->visible<!-- c --></p>"), "visible");
}

/// `&#+65;` is not a numeric character reference (digits only);
/// `str::parse::<u32>` accepts a leading '+'.
#[test]
#[ignore = "open: signed numeric references decode (fixed in #21)"]
fn numeric_reference_rejects_plus_sign() {
    assert_eq!(decode_entities("&#+65;"), "&#+65;");
}

/// Uppercase legacy references (&AMP; &LT; &GT; &QUOT; &COPY; &REG;) are
/// part of the HTML5 named character reference table.
#[test]
#[ignore = "open: uppercase legacy entities missing (fixed in #21)"]
fn uppercase_legacy_entities_decode() {
    assert_eq!(
        decode_entities("&AMP;&LT;&GT;&QUOT;&COPY;&REG;"),
        "&<>\"\u{a9}\u{ae}"
    );
}

/// Only the legacy subset (amp, lt, copy, eacute, ...) may omit the
/// semicolon; `&Beta` / `&mdash` without `;` are literal text in browsers.
#[test]
#[ignore = "open: non-legacy entities decode without ';' (fixed in #21)"]
fn non_legacy_entity_without_semicolon_is_literal() {
    assert_eq!(decode_entities("Alpha&Beta test"), "Alpha&Beta test");
    assert_eq!(decode_entities("a&mdash b"), "a&mdash b");
}

/// strip_to_text documents "Whitespace collapsing (HTML rendering
/// semantics)", but <pre> content is collapsed too.
#[test]
#[ignore = "open: <pre> whitespace is collapsed"]
fn pre_preserves_whitespace() {
    let t = strip_to_text("<pre>fn main() {\n    let x  =  1;\n}</pre>");
    assert_eq!(t, "fn main() {\n    let x  =  1;\n}");
}

#[cfg(any(feature = "docx", feature = "pptx"))]
fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Word writes document.xml without whitespace between elements. The plain
/// DOCX path runs strip_to_text over the raw XML, where `w:p` is not a block
/// tag, so consecutive paragraphs are glued together.
#[cfg(feature = "docx")]
#[test]
fn docx_paragraphs_are_separated_without_source_whitespace() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>World</w:t></w:r></w:p></w:body></w:document>"#;
    let bytes = make_zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", xml),
    ]);
    let r = deformat::docx::extract_bytes(&bytes).unwrap();
    assert!(!r.text.contains("HelloWorld"), "glued: {:?}", r.text);
}

/// Same root cause for PPTX `<a:p>` paragraphs on one slide.
#[cfg(feature = "pptx")]
#[test]
fn pptx_paragraphs_are_separated_without_source_whitespace() {
    let xml = r#"<?xml version="1.0"?><p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Quarterly results</a:t></a:r></a:p><a:p><a:r><a:t>Revenue up</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
    let bytes = make_zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("ppt/slides/slide1.xml", xml),
    ]);
    let r = deformat::pptx::extract_bytes(&bytes).unwrap();
    assert!(!r.text.contains("resultsRevenue"), "glued: {:?}", r.text);
}

/// Corrupt / truncated PDFs must return Err, not panic.
#[cfg(feature = "pdf")]
#[test]
fn pdf_corrupt_inputs_do_not_panic() {
    let cases: Vec<Vec<u8>> = vec![
        b"%PDF-1.4\n".to_vec(),
        b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF".to_vec(),
        b"%PDF-1.7\nxref\n0 99999999\n0000000000 65535 f \ntrailer<</Size 99999999>>\nstartxref\n9\n%%EOF".to_vec(),
        b"%PDF-1.4\n1 0 obj<</Length 999999>>stream\nabc\nendstream endobj\n%%EOF".to_vec(),
    ];
    for (i, c) in cases.iter().enumerate() {
        let r = std::panic::catch_unwind(|| deformat::pdf::extract_bytes(c).map(|e| e.text));
        assert!(r.is_ok(), "case {i} panicked");
    }
}
