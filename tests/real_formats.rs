//! Tests against real document files (DOCX, EPUB, XLSX, PPTX, RTF).
//!
//! Fixture files live at `tests/fixtures/synthetic/` and are committed
//! to the repo. They are produced by
//! `scripts/generate_synthetic_fixtures.py` (deterministic, stdlib-only
//! Python, rerun to regenerate byte-identical outputs). Each file is
//! under 3 KB and is authored by this crate -- dual-licensed
//! MIT-OR-Apache-2.0 alongside the rest of the source. See
//! `tests/fixtures/PROVENANCE.md` for the manifest and rationale.
//!
//! These tests run as part of `cargo test --all-features` -- no fetch
//! step required.

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic")
}

// =============================================================================
// DOCX
// =============================================================================

#[cfg(feature = "docx")]
mod docx_tests {
    use super::*;

    #[test]
    fn extract_minimal_docx() {
        let result = deformat::docx::extract_file(&fixtures_dir().join("minimal.docx")).unwrap();
        assert_eq!(result.format, deformat::Format::Docx);
        assert!(
            result.text.contains("Hello from DOCX"),
            "text: {}",
            result.text
        );
        assert!(
            !result.text.contains("<w:"),
            "XML namespace leaked: {}",
            result.text
        );
    }

    #[test]
    fn extract_unicode_docx() {
        let result = deformat::docx::extract_file(&fixtures_dir().join("unicode.docx")).unwrap();
        // CJK
        assert!(
            result.text.contains("中文"),
            "CJK preserved: {}",
            result.text
        );
        // Accented
        assert!(result.text.contains("Café"), "accented: {}", result.text);
        // Cyrillic / Eastern European
        assert!(
            result.text.contains("Łódź") || result.text.contains("München"),
            "extended-latin: {}",
            result.text
        );
    }

    #[test]
    fn extract_docx_table_preserves_cells() {
        let result = deformat::docx::extract_file(&fixtures_dir().join("table.docx")).unwrap();
        for expected in ["Name", "Value", "Note", "Alpha", "42", "First row body"] {
            assert!(
                result.text.contains(expected),
                "cell {expected:?} missing: {}",
                result.text
            );
        }
    }

    #[test]
    fn docx_bytes_api_matches_file_api() {
        let path = fixtures_dir().join("minimal.docx");
        let bytes = std::fs::read(&path).unwrap();
        let bytes_result = deformat::docx::extract_bytes(&bytes).unwrap();
        let file_result = deformat::docx::extract_file(&path).unwrap();
        assert_eq!(bytes_result.text, file_result.text);
    }

    #[test]
    fn docx_table_segment_carries_text_as_html() {
        // The Segment-level API must emit the <w:tbl> as Segment::Table
        // with metadata.text_as_html populated (landed 0.12.0).
        let bytes = std::fs::read(fixtures_dir().join("table.docx")).unwrap();
        let segs = deformat::docx::extract_bytes_to_segments(&bytes).unwrap();
        let table = segs
            .iter()
            .find(|s| s.type_name() == "Table")
            .expect("Table segment emitted");
        let html = table
            .data()
            .metadata
            .text_as_html
            .as_deref()
            .expect("text_as_html populated");
        assert!(html.starts_with("<table>"));
        assert!(html.contains("<tr>"));
        assert!(html.contains("Alpha"));
    }
}

// =============================================================================
// XLSX
// =============================================================================

#[cfg(feature = "xlsx")]
mod xlsx_tests {
    use super::*;

    #[test]
    fn extract_minimal_xlsx_cells() {
        let result = deformat::xlsx::extract_file(&fixtures_dir().join("minimal.xlsx")).unwrap();
        assert_eq!(result.format, deformat::Format::Xlsx);
        for expected in ["Name", "Score", "Alice", "Bob"] {
            assert!(
                result.text.contains(expected),
                "cell {expected:?} missing: {}",
                result.text
            );
        }
    }

    #[test]
    fn extract_unicode_multi_sheet_xlsx() {
        let result = deformat::xlsx::extract_file(&fixtures_dir().join("unicode.xlsx")).unwrap();
        // CJK shared string
        assert!(result.text.contains("中文"), "CJK cell: {}", result.text);
        // Cyrillic
        assert!(
            result.text.contains("Привет"),
            "Cyrillic cell: {}",
            result.text
        );
        // Second sheet content
        assert!(
            result.text.contains("Totals at bottom"),
            "second sheet visited: {}",
            result.text
        );
    }

    #[test]
    fn xlsx_bytes_api_matches_file_api() {
        let path = fixtures_dir().join("minimal.xlsx");
        let bytes = std::fs::read(&path).unwrap();
        let b = deformat::xlsx::extract_bytes(&bytes).unwrap();
        let f = deformat::xlsx::extract_file(&path).unwrap();
        assert_eq!(b.text, f.text);
    }
}

// =============================================================================
// EPUB
// =============================================================================

#[cfg(feature = "epub")]
mod epub_tests {
    use super::*;

    #[test]
    fn extract_minimal_epub() {
        let result = deformat::epub::extract_file(&fixtures_dir().join("minimal.epub")).unwrap();
        assert_eq!(result.format, deformat::Format::Epub);
        // Both chapters reach the output.
        assert!(result.text.contains("Chapter One"));
        assert!(result.text.contains("Chapter Two"));
        assert!(result.text.contains("First chapter body"));
        // Unicode in chapter two.
        assert!(result.text.contains("中文"));
        // No HTML leakage.
        assert!(!result.text.contains("<html"));
        assert!(!result.text.contains("<body"));
    }

    #[test]
    fn epub_bytes_api_matches_file_api() {
        let path = fixtures_dir().join("minimal.epub");
        let bytes = std::fs::read(&path).unwrap();
        let b = deformat::epub::extract_bytes(&bytes).unwrap();
        let f = deformat::epub::extract_file(&path).unwrap();
        assert_eq!(b.text, f.text);
    }
}

// =============================================================================
// PPTX
// =============================================================================

#[cfg(feature = "pptx")]
mod pptx_tests {
    use super::*;

    #[test]
    fn extract_minimal_pptx() {
        let result = deformat::pptx::extract_file(&fixtures_dir().join("minimal.pptx")).unwrap();
        assert_eq!(result.format, deformat::Format::Pptx);
        assert!(result.text.contains("Slide Title Text"));
        assert!(result.text.contains("body paragraph"));
    }
}

// =============================================================================
// RTF
// =============================================================================

#[cfg(feature = "rtf")]
mod rtf_tests {
    use super::*;

    #[test]
    fn extract_minimal_rtf() {
        let result = deformat::rtf::extract_file(&fixtures_dir().join("minimal.rtf")).unwrap();
        assert_eq!(result.format, deformat::Format::Rtf);
        assert!(result.text.contains("Hello from RTF"));
        assert!(result.text.contains("Second sentence here"));
    }

    #[test]
    fn extract_unicode_rtf_does_not_panic() {
        // Note: rtf-parser-tt does not fully decode `\uN?` escapes --
        // the unicode chars come through as the ANSI fallback. Not
        // ideal, but this test just pins the current behavior so a
        // future upstream fix surfaces as a test update rather than a
        // silent regression.
        let result = deformat::rtf::extract_file(&fixtures_dir().join("unicode.rtf")).unwrap();
        assert_eq!(result.format, deformat::Format::Rtf);
        assert!(!result.text.is_empty(), "extracted non-empty");
        // At least the ASCII parts survive.
        assert!(result.text.contains("hello world"));
    }
}

// =============================================================================
// Format detection on committed fixtures
// =============================================================================

#[test]
fn detect_committed_fixture_formats() {
    use deformat::Format;
    let cases: &[(&str, Format)] = &[
        #[cfg(feature = "docx")]
        ("minimal.docx", Format::Docx),
        #[cfg(feature = "docx")]
        ("unicode.docx", Format::Docx),
        #[cfg(feature = "docx")]
        ("table.docx", Format::Docx),
        #[cfg(feature = "xlsx")]
        ("minimal.xlsx", Format::Xlsx),
        #[cfg(feature = "xlsx")]
        ("unicode.xlsx", Format::Xlsx),
        #[cfg(feature = "pptx")]
        ("minimal.pptx", Format::Pptx),
        #[cfg(feature = "epub")]
        ("minimal.epub", Format::Epub),
        #[cfg(feature = "rtf")]
        ("minimal.rtf", Format::Rtf),
    ];
    for (name, expected) in cases {
        let path = fixtures_dir().join(name);
        let bytes = std::fs::read(&path).unwrap();
        let detected = deformat::detect::detect_bytes(&bytes);
        assert_eq!(
            &detected, expected,
            "detect_bytes({name}) = {detected:?}, expected {expected:?}"
        );
    }
}
