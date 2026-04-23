#![no_main]

use libfuzzer_sys::fuzz_target;

// DOCX is ZIP + XML; the zip crate can panic on malformed inputs
// without this guard. Accept arbitrary bytes; mismatched-magic inputs
// should return Error::Parse, not panic.
fuzz_target!(|data: &[u8]| {
    let _ = deformat::docx::extract_bytes(data);
    let _ = deformat::docx::extract_bytes_to_segments(data);
});
