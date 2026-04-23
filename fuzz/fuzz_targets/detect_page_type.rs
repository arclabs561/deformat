#![no_main]

use libfuzzer_sys::fuzz_target;

// Panic-guard fuzz target for the page-type detector. Exercises every
// internal slicing site (og:type window, JSON-LD @type scan, canonical
// URL path extraction, schema.org itemtype substring match) on
// arbitrary byte sequences.
fuzz_target!(|data: &[u8]| {
    let html = std::str::from_utf8(data)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|_| String::from_utf8_lossy(data).into_owned());
    let _ = deformat::page_type::detect_page_type(&html);
});
