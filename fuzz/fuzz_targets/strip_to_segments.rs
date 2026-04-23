#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercise the segment-level extraction + all filter combinations.
// Panics here surface malformed-HTML bugs that the filters miss.
fuzz_target!(|data: &[u8]| {
    let html = std::str::from_utf8(data)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|_| String::from_utf8_lossy(data).into_owned());

    // Every public segment API.
    let _ = deformat::html::strip_to_segments(&html);
    let segs = deformat::html::strip_to_segments_filtered(&html, 0.45);
    let segs = deformat::html::filter_low_sentence_density(segs, 1.0);
    let segs = deformat::html::filter_boilerplate(segs, 40);
    let _ = deformat::html::filter_low_cetd_density(segs, 0.4);
});
