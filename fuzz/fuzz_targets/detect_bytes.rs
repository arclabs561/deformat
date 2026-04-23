#![no_main]

use libfuzzer_sys::fuzz_target;

// The detect_bytes classifier reads magic bytes + ZIP central
// directory entries. Fuzzing guards against panics in the ZIP-parsing
// path used for DOCX/XLSX/PPTX/EPUB discrimination.
fuzz_target!(|data: &[u8]| {
    let _ = deformat::detect::detect_bytes(data);
    let _ = deformat::detect::detect_str(&String::from_utf8_lossy(data));
    #[cfg(feature = "encoding_rs")]
    let _ = deformat::detect::decode_bytes(data, "utf-8");
});
