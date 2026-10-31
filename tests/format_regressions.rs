//! Regression tests for charset prescan, XML CDATA, block boundaries,
//! ZIP-based format detection and extraction, and RTF decoding.
#![allow(dead_code)]

use deformat::html::strip_to_text;

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

#[cfg(any(feature = "docx", feature = "epub", feature = "pptx", feature = "xlsx"))]
fn make_zip(entries: &[(&str, &[u8])], stored: bool) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let method = if stored {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };
    let opts = zip::write::SimpleFileOptions::default().compression_method(method);
    for (name, body) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

// ---------------------------------------------------------------- detect

/// OOXML detection must look at entry names. Media parts (JPEG, PNG) are
/// high-entropy and stored verbatim, so a byte search for "xl/" matches by
/// chance about once per 16.7 MB and reports a DOCX with photos as XLSX.
#[cfg(feature = "docx")]
#[test]
fn docx_with_binary_media_is_detected_as_docx() {
    let doc = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>"#;
    let mut misdetected = Vec::new();
    for seed in 0u64..32 {
        // 1 MiB of xorshift bytes standing in for a photo.
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let media: Vec<u8> = (0..(1 << 20))
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        let bytes = make_zip(
            &[
                ("[Content_Types].xml", b"<Types/>"),
                ("word/document.xml", doc),
                ("word/media/image1.jpeg", &media),
            ],
            true,
        );
        let format = deformat::detect::detect_bytes(&bytes);
        if format != deformat::Format::Docx {
            misdetected.push((seed, format));
        }
    }
    assert!(
        misdetected.is_empty(),
        "{} of 32 DOCX files with 1 MiB of binary media misdetected: {misdetected:?}",
        misdetected.len()
    );
}

/// python-pptx (and any writer that numbers notes in creation order) names the
/// notes of slide 3 `notesSlide1.xml` when it is the only slide with notes
/// (python-pptx 1.x writes slides/_rels/slide3.xml.rels ->
/// ../notesSlides/notesSlide1.xml). Pairing notesSlideN with slideN prints
/// slide 3's notes under slide 1.
#[cfg(feature = "pptx")]
#[test]
fn pptx_notes_follow_the_slide_they_belong_to() {
    let slide = |n: u32| {
        format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Slide {n} title</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
        )
    };
    let (s1, s2, s3) = (slide(1), slide(2), slide(3));
    let rels3 = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#;
    let notes = r#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Notes for slide three</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#;
    let bytes = make_zip(
        &[
            ("ppt/slides/slide1.xml", s1.as_bytes()),
            ("ppt/slides/slide2.xml", s2.as_bytes()),
            ("ppt/slides/slide3.xml", s3.as_bytes()),
            ("ppt/slides/_rels/slide3.xml.rels", rels3.as_bytes()),
            ("ppt/notesSlides/notesSlide1.xml", notes.as_bytes()),
        ],
        false,
    );
    let text = deformat::pptx::extract_bytes(&bytes).unwrap().text;
    let notes_at = text.find("Notes for slide three").unwrap();
    let s3_at = text.find("Slide 3 title").unwrap();
    assert!(
        notes_at > s3_at,
        "slide 3 notes placed before slide 3:\n{text}"
    );
}

// ---------------------------------------------------------------- ZIP bombs

/// zip_entry.rs caps each entry at 64 MiB ("a sub-megabyte archive can carry a
/// multi-gigabyte entry"), but nothing bounds the total: EPUB reads every
/// spine item (and PPTX every slide) and concatenates the text. 70 distinct
/// 1 MiB chapters deflate to a few hundred KB and produce 70 MiB of output;
/// N such chapters scale linearly past any memory limit.
#[cfg(feature = "epub")]
#[test]
#[ignore = "needs a decision on how an archive-wide decompression budget is reported"]
fn epub_total_decompressed_text_is_bounded() {
    let n = 70;
    let mut manifest = String::new();
    let mut spine = String::new();
    let chapter = format!(
        "<html><body><p>{}</p></body></html>",
        "a ".repeat(512 * 1024)
    );
    let names: Vec<String> = (0..n).map(|i| format!("c{i}.xhtml")).collect();
    for (i, name) in names.iter().enumerate() {
        manifest.push_str(&format!(
            r#"<item id="c{i}" href="{name}" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{i}"/>"#));
    }
    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
    );
    let container = br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf.as_bytes()),
    ];
    for name in &names {
        entries.push((name.as_str(), chapter.as_bytes()));
    }
    let bytes = make_zip(&entries, false);
    let out = deformat::epub::extract_bytes(&bytes);
    let produced = out.as_ref().map_or(0, |o| o.text.len());
    eprintln!(
        "archive {} bytes -> {} bytes of text",
        bytes.len(),
        produced
    );
    assert!(
        produced <= 64 * 1024 * 1024,
        "{} KiB archive expanded to {} MiB of text with no total cap",
        bytes.len() / 1024,
        produced / (1024 * 1024)
    );
}

/// RTF paragraphs (\par) and code-page escapes (\'e9 under \ansicpg1252).
#[cfg(feature = "rtf")]
#[test]
fn rtf_par_and_hex_escapes() {
    let rtf = r"{\rtf1\ansi\ansicpg1252{\fonttbl\f0\fswiss Helvetica;}\f0 Caf\'e9 one.\par Second\'85 two.\par}";
    let text = deformat::rtf::extract_bytes(rtf.as_bytes()).unwrap().text;
    eprintln!("{text:?}");
    assert!(text.contains("Café one."), "{text:?}");
    assert!(text.contains("Second… two."), "{text:?}");
    assert!(
        text.contains("one.\n"),
        "paragraphs not separated: {text:?}"
    );
}

/// A `\'hh` run right after `\uN` is that character's fallback and must
/// not be emitted a second time.
#[cfg(feature = "rtf")]
#[test]
fn rtf_unicode_fallback_is_not_duplicated() {
    let rtf = r"{\rtf1\ansi\ansicpg1252 Caf\u233\'e9 au lait.\par}";
    let text = deformat::rtf::extract_bytes(rtf.as_bytes()).unwrap().text;
    assert_eq!(text, "Café au lait.");
}
