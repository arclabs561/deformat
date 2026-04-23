#![no_main]

use libfuzzer_sys::fuzz_target;

// Exercise strip_to_text_with_paths + every span's bounds must be
// valid UTF-8 char boundaries in BOTH the output text and the source
// HTML. This is the invariant that the prior trim-end and void-element
// bugs violated.
fuzz_target!(|data: &[u8]| {
    let html = std::str::from_utf8(data)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|_| String::from_utf8_lossy(data).into_owned());

    let (text, spans) = deformat::html::strip_to_text_with_paths(&html);
    for span in &spans {
        assert!(
            text.is_char_boundary(span.output_start),
            "output_start not on char boundary"
        );
        assert!(
            text.is_char_boundary(span.output_end),
            "output_end not on char boundary"
        );
        assert!(
            html.is_char_boundary(span.source_start),
            "source_start not on char boundary"
        );
        assert!(
            html.is_char_boundary(span.source_end),
            "source_end not on char boundary"
        );
        assert!(span.output_start <= span.output_end);
        assert!(span.source_start <= span.source_end);
        assert!(span.output_end <= text.len());
        assert!(span.source_end <= html.len());
    }
});
