#![no_main]

use libfuzzer_sys::fuzz_target;

// Panic-guard fuzz target for the core strip_to_text function.
// Any byte sequence, valid UTF-8 or not (libfuzzer provides an arbitrary
// &[u8]; we convert with from_utf8_lossy so the fuzzer can explore
// malformed-UTF-8 inputs without wasting budget on is_utf8 rejections).
fuzz_target!(|data: &[u8]| {
    let html = std::str::from_utf8(data)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|_| String::from_utf8_lossy(data).into_owned());
    let _ = deformat::html::strip_to_text(&html);
});
