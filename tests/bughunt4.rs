//! Bug-hunt regression tests (round 4): ZIP-based formats and detection.
//! Run with `cargo test --features docx,epub,pptx,xlsx --test bughunt4`.
#![allow(dead_code)]

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

/// detect_bytes discriminates OOXML by searching the *whole archive* for the
/// 3-byte string "xl/" before "word/" (detect.rs:118-131). Media parts (JPEG,
/// PNG) are high-entropy and stored verbatim (deflate falls back to stored
/// blocks), so "xl/" occurs by chance about once per 16.7 MB: a DOCX with a
/// few MB of photos is routinely reported as XLSX and extract_from_bytes fails.
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

// ---------------------------------------------------------------- EPUB

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
    make_zip(&entries, true)
}

/// OPF manifest hrefs are URLs (EPUB 3.3 §3.4 / OPF item@href is a "valid
/// URL string"): a file named "Chapter 1.xhtml" is referenced as
/// "Chapter%201.xhtml". epub.rs:140-152 joins the raw href to the OPF prefix
/// and looks it up with by_name, so the chapter is silently dropped.
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

/// Hrefs are relative to the OPF; "../Text/ch1.xhtml" from "OEBPS/pkg/package.opf"
/// names "OEBPS/Text/ch1.xhtml". The raw join "OEBPS/pkg/../Text/ch1.xhtml" is
/// not a ZIP entry name, so every chapter is dropped and the book is EmptyResult
/// (the name-scan fallback does not run because the spine was non-empty).
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

/// dc:title is XML text; epub.rs:203-212 returns it without entity decoding.
#[cfg(feature = "epub")]
#[test]
#[ignore = "open: EPUB dc:title is not entity-decoded"]
fn epub_title_entities_are_decoded() {
    let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Pride &amp; Prejudice</dc:title></metadata><manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#;
    let bytes = epub(
        "content.opf",
        opf,
        &[(
            "c1.xhtml",
            "<html><body><p>It is a truth.</p></body></html>",
        )],
    );
    let out = deformat::epub::extract_bytes(&bytes).unwrap();
    assert_eq!(out.title.as_deref(), Some("Pride & Prejudice"));
}

// ---------------------------------------------------------------- PPTX

/// Notes parts are linked to slides by relationships, not by file number.
/// python-pptx (and any writer that numbers notes in creation order) names the
/// notes of slide 3 `notesSlide1.xml` when it is the only slide with notes
/// (verified with python-pptx 1.x: slides/_rels/slide3.xml.rels ->
/// ../notesSlides/notesSlide1.xml). pptx.rs:69-95 pairs notesSlideN with
/// slideN, so slide 3's notes are printed under slide 1.
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

// ---------------------------------------------------------------- XLSX

#[cfg(feature = "xlsx")]
fn xlsx(sheet: &str, styles: Option<&str>) -> Vec<u8> {
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let wb = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let wbrels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let default_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs></styleSheet>"#;
    make_zip(
        &[
            ("[Content_Types].xml", ct.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("xl/workbook.xml", wb.as_bytes()),
            ("xl/_rels/workbook.xml.rels", wbrels.as_bytes()),
            ("xl/styles.xml", styles.unwrap_or(default_styles).as_bytes()),
            ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ],
        false,
    )
}

#[cfg(feature = "xlsx")]
fn sheet(rows: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    )
}

/// xlsx.rs:49-58 drops empty cells before joining with '\t', so a blank in
/// the middle of a row shifts every later value one column left: the module
/// promises "tab-separated rows", and a reader of those rows assigns "Lee"
/// to the "middle" column.
#[cfg(feature = "xlsx")]
#[test]
#[ignore = "open: blank XLSX cells shift later columns"]
fn xlsx_blank_cell_keeps_column_position() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>first</t></is></c><c r="B1" t="inlineStr"><is><t>middle</t></is></c><c r="C1" t="inlineStr"><is><t>last</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Ann</t></is></c><c r="C2" t="inlineStr"><is><t>Lee</t></is></c></row>"#;
    let text = deformat::xlsx::extract_bytes(&xlsx(&sheet(rows), None))
        .unwrap()
        .text;
    let line2 = text.lines().nth(1).unwrap_or_default();
    assert_eq!(
        line2.split('\t').collect::<Vec<_>>(),
        ["Ann", "", "Lee"],
        "{text:?}"
    );
}

/// A date-formatted cell (numFmtId 14) is emitted as the raw serial number.
#[cfg(feature = "xlsx")]
#[test]
#[ignore = "open: XLSX dates come out as serial numbers"]
fn xlsx_date_cell_is_not_a_bare_serial_number() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>due</t></is></c><c r="B1" s="1"><v>45292</v></c></row>"#;
    let text = deformat::xlsx::extract_bytes(&xlsx(&sheet(rows), None))
        .unwrap()
        .text;
    assert!(
        !text.contains("45292"),
        "date shown as Excel serial: {text:?}"
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

/// docx.rs module doc: "Also reads word/header*.xml and word/footer*.xml if
/// present." extract_reader (docx.rs:343-375) reads only word/document.xml.
#[cfg(feature = "docx")]
#[test]
#[ignore = "open: DOCX headers and footers are not read"]
fn docx_reads_headers_and_footers_as_documented() {
    let doc = br#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Body text</w:t></w:r></w:p></w:body></w:document>"#;
    let header =
        br#"<w:hdr xmlns:w="w"><w:p><w:r><w:t>CONFIDENTIAL header</w:t></w:r></w:p></w:hdr>"#;
    let footer = br#"<w:ftr xmlns:w="w"><w:p><w:r><w:t>Page footer</w:t></w:r></w:p></w:ftr>"#;
    let bytes = make_zip(
        &[
            ("word/document.xml", doc),
            ("word/header1.xml", header),
            ("word/footer1.xml", footer),
        ],
        false,
    );
    let text = deformat::docx::extract_bytes(&bytes).unwrap().text;
    assert!(
        text.contains("CONFIDENTIAL header") && text.contains("Page footer"),
        "{text:?}"
    );
}

// ---------------------------------------------------------------- RTF

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

/// RTF is 7-bit by spec but writers emit raw 8-bit code-page bytes; rtf.rs:22
/// rejects any non-UTF-8 input outright instead of decoding per \ansicpg.
#[cfg(feature = "rtf")]
#[test]
#[ignore = "open: raw 8-bit RTF bytes are rejected"]
fn rtf_raw_cp1252_byte_is_not_a_parse_error() {
    let rtf = b"{\\rtf1\\ansi\\ansicpg1252 Caf\xe9 au lait.\\par}";
    let out = deformat::rtf::extract_bytes(rtf);
    assert!(out.is_ok(), "{out:?}");
}
