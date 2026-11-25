#![cfg(feature = "source-map")]

use deformat::mapped::{self, SourceKind};
use deformat::Extractor;

#[test]
fn byte_dispatch_preserves_passthrough_offsets_and_errors() {
    let bytes = "🦊  Ada\r\n".as_bytes();
    let doc = mapped::extract(bytes).unwrap();
    assert_eq!(doc.extracted.text.as_bytes(), bytes);
    doc.source_map.verify(bytes, &doc.extracted.text).unwrap();
    let runs = doc
        .source_map
        .project_chars(&doc.extracted.text, 3..6)
        .unwrap();
    assert_eq!(runs[0].kind, SourceKind::Direct);
    assert_eq!(&bytes[runs[0].source_bytes.clone().unwrap()], b"Ada");
    assert!(mapped::extract(b"\xff").is_err());
}

#[test]
fn identical_html_outputs_keep_the_actual_renderers_evidence() {
    let source = "<p>A&#64;B</p>";
    let text = "A@B";
    let strip = mapped::html_source_map(source, text, Extractor::Strip).unwrap();
    assert_eq!(
        strip.project_chars(text, 1..2).unwrap()[0].kind,
        SourceKind::Transformed
    );
    let other = mapped::html_source_map(source, text, Extractor::Readability).unwrap();
    assert_eq!(
        other.project_chars(text, 1..2).unwrap()[0].kind,
        SourceKind::Unmapped
    );
    other.verify(source.as_bytes(), text).unwrap();
}

#[cfg(feature = "prepare")]
#[test]
fn nfc_preserves_disjoint_html_origins_for_repeated_occurrences() {
    let raw = "<p>e<b>&#x301;</b>   <i>e</i>&#x301;</p>";
    let options = mapped::CleanOptions {
        normalize_unicode: true,
        clean_whitespace: true,
    };
    let doc = mapped::prepare(raw.as_bytes(), &options).unwrap();
    assert_eq!(doc.extracted.text, "é é");
    doc.source_map
        .verify(raw.as_bytes(), &doc.extracted.text)
        .unwrap();
    let expected = [
        [raw.find(">e<b>").unwrap() + 1, raw.find("&#x301;").unwrap()],
        [
            raw.find(">e</i>").unwrap() + 1,
            raw.rfind("&#x301;").unwrap(),
        ],
    ];
    for (character, [letter, reference]) in [0, 2].into_iter().zip(expected) {
        let runs = doc
            .source_map
            .project_chars(&doc.extracted.text, character..character + 1)
            .unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].source_bytes, None);
        assert_eq!(
            runs[0].source_ranges,
            vec![letter..letter + 1, reference..reference + 7]
        );
        assert_eq!(runs[0].kind, SourceKind::Transformed);
    }
}

#[cfg(feature = "serde")]
#[test]
fn serialized_maps_require_the_same_rendered_text() {
    let doc = mapped::extract(b"<p>one</p>").unwrap();
    let mut value = serde_json::to_value(&doc).unwrap();
    let saved: mapped::MappedExtraction = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(saved.source_map, doc.source_map);
    value["extracted"]["text"] = "two".into();
    assert!(serde_json::from_value::<mapped::MappedExtraction>(value).is_err());
}

#[cfg(all(feature = "rtf", feature = "prepare"))]
#[test]
fn opaque_rtf_cleanup_retains_the_original_document_digest() {
    let raw = br"{\rtf1\ansi A   B}";
    let doc = mapped::prepare(raw, &mapped::CleanOptions::default()).unwrap();
    assert_eq!(doc.extracted.text.trim(), "A B");
    doc.source_map.verify(raw, &doc.extracted.text).unwrap();
    assert!(doc
        .source_map
        .runs
        .iter()
        .all(|run| run.kind == SourceKind::Unmapped));
}
