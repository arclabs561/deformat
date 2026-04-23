#![no_main]

use libfuzzer_sys::{arbitrary, fuzz_target, Corpus};

// Fuzz the four-filter pipeline over HTML with randomized threshold
// parameters. Each filter's knob has a legitimate range; fuzzing
// the parameter space surfaces numeric-edge bugs (NaN handling,
// division-by-zero, threshold = mean * 0.0).
#[derive(Debug, arbitrary::Arbitrary)]
struct Input<'a> {
    html: &'a str,
    link_cap: f32,
    sent_cap: f32,
    boiler_min: u8,
    cetd_frac: f32,
}

fuzz_target!(|input: Input<'_>| -> Corpus {
    if !input.link_cap.is_finite()
        || !input.sent_cap.is_finite()
        || !input.cetd_frac.is_finite()
    {
        return Corpus::Reject;
    }
    let link = input.link_cap.clamp(0.0, 1.0);
    let sent = input.sent_cap.clamp(0.0, 100.0);
    let cetd = input.cetd_frac.clamp(0.0, 2.0);
    let boiler = input.boiler_min as usize;

    let segs = deformat::html::strip_to_segments_filtered(input.html, link);
    let segs = deformat::html::filter_low_sentence_density(segs, sent);
    let segs = deformat::html::filter_boilerplate(segs, boiler);
    let _ = deformat::html::filter_low_cetd_density(segs, cetd);
    Corpus::Keep
});
