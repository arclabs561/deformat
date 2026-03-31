//! Tests against real-world document files (DOCX, EPUB).
//!
//! These tests are `#[ignore]` by default because the fixture files
//! must be downloaded first. Run with:
//!
//! ```sh
//! # Download fixtures first (see scripts/fetch_fixtures.sh)
//! cargo test --all-features --test real_formats -- --ignored
//! ```

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures/real")
}

// =============================================================================
// DOCX
// =============================================================================

#[cfg(feature = "docx")]
mod docx_tests {
    use super::*;

    #[test]
    #[ignore]
    fn extract_real_docx_demo() {
        let path = fixtures_dir().join("sample1.docx");
        if !path.exists() {
            eprintln!("Skipping: {path:?} not found. Download with scripts/fetch_fixtures.sh");
            return;
        }
        let result = deformat::docx::extract_file(&path).unwrap();
        assert!(
            result.text.len() > 100,
            "expected substantial text from demo.docx, got {} chars",
            result.text.len()
        );
        assert_eq!(result.format, deformat::Format::Docx);
        // Should not contain XML tags
        assert!(
            !result.text.contains("<w:"),
            "XML namespace leaked: {}...",
            &result.text[..result.text.len().min(200)]
        );
        assert!(!result.text.contains("</w:"), "closing XML tags leaked");
        println!(
            "sample1.docx: {} chars extracted. First 200: {}",
            result.text.len(),
            &result.text[..result.text.len().min(200)]
        );
    }

    #[test]
    #[ignore]
    fn docx_bytes_api() {
        let path = fixtures_dir().join("sample1.docx");
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let result = deformat::docx::extract_bytes(&bytes).unwrap();
        let file_result = deformat::docx::extract_file(&path).unwrap();
        // bytes and file APIs should produce identical output
        assert_eq!(result.text, file_result.text, "bytes vs file mismatch");
    }
}

// =============================================================================
// EPUB
// =============================================================================

#[cfg(feature = "epub")]
mod epub_tests {
    use super::*;

    #[test]
    #[ignore]
    fn extract_alice_in_wonderland() {
        let path = fixtures_dir().join("alice.epub");
        if !path.exists() {
            eprintln!("Skipping: {path:?} not found");
            return;
        }
        let result = deformat::epub::extract_file(&path).unwrap();
        assert!(
            result.text.len() > 5000,
            "Alice should be substantial, got {} chars",
            result.text.len()
        );
        assert_eq!(result.format, deformat::Format::Epub);
        // Content checks
        assert!(
            result.text.contains("Alice"),
            "should contain 'Alice': first 500 chars: {}",
            &result.text[..result.text.len().min(500)]
        );
        // Should not contain HTML tags
        assert!(!result.text.contains("<html"), "HTML tags leaked");
        assert!(!result.text.contains("<body"), "body tag leaked");
        // Title should be extracted
        if let Some(ref title) = result.title {
            println!("Title: {title}");
        }
        println!(
            "alice.epub: {} chars, title={:?}. First 300: {}",
            result.text.len(),
            result.title,
            &result.text[..result.text.len().min(300)]
        );
    }

    #[test]
    #[ignore]
    fn extract_metamorphosis() {
        let path = fixtures_dir().join("metamorphosis.epub");
        if !path.exists() {
            return;
        }
        let result = deformat::epub::extract_file(&path).unwrap();
        assert!(
            result.text.len() > 5000,
            "Metamorphosis should be substantial, got {} chars",
            result.text.len()
        );
        assert!(
            result.text.contains("Gregor") || result.text.contains("Samsa"),
            "should contain Kafka character names: first 500: {}",
            &result.text[..result.text.len().min(500)]
        );
        assert!(!result.text.contains("<html"), "HTML leaked");
        println!(
            "metamorphosis.epub: {} chars, title={:?}",
            result.text.len(),
            result.title
        );
    }

    #[test]
    #[ignore]
    fn epub_bytes_api() {
        let path = fixtures_dir().join("alice.epub");
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let result = deformat::epub::extract_bytes(&bytes).unwrap();
        let file_result = deformat::epub::extract_file(&path).unwrap();
        assert_eq!(result.text, file_result.text, "bytes vs file mismatch");
    }
}

// =============================================================================
// Format detection on real files
// =============================================================================

#[test]
#[ignore]
fn detect_real_file_formats() {
    let fixtures = fixtures_dir();
    if !fixtures.exists() {
        return;
    }

    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext_format = deformat::detect::detect_path(&path);

        // Also test content-based detection
        if let Ok(bytes) = std::fs::read(&path) {
            let content_format = deformat::detect::detect_bytes(&bytes[..bytes.len().min(4096)]);
            let name = path.file_name().unwrap().to_string_lossy();
            println!(
                "{name}: ext={ext_format}, content={content_format}, size={}",
                bytes.len()
            );

            // Extension and content detection should agree for known formats
            // (content detection may return Unknown for small samples)
            if ext_format != deformat::Format::Unknown
                && content_format != deformat::Format::Unknown
            {
                assert_eq!(
                    ext_format, content_format,
                    "detection mismatch for {name}: ext={ext_format}, content={content_format}"
                );
            }
        }
    }
}
