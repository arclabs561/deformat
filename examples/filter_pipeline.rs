//! Compose the segment filters on a boilerplate-heavy HTML page.
//!
//! ```sh
//! cargo run --example filter_pipeline
//! ```
//!
//! Pipeline (the "triple" extractor in `bench_wcxb`):
//! `strip_to_segments_filtered` (link-density) →
//! `filter_low_sentence_density` (sentence shape) → `filter_boilerplate`
//! (length + punctuation). Measured on WCXB dev split: lifts overall F1
//! 0.746 → 0.774 and article F1 0.855 → 0.881. A fourth filter,
//! `filter_low_cetd_density` (Sun et al. SIGIR 2011), is available but
//! non-idempotent so it is omitted from the default chain. See the
//! README's WCXB table for the per-page-type breakdown.

use deformat::html::{
    filter_boilerplate, filter_low_sentence_density, strip_to_segments, strip_to_segments_filtered,
};

fn main() {
    // Four kinds of noise that cover the three filters' territory:
    //   - a <div> of related-link cards       (link-density target)
    //   - a tag-cloud paragraph with no punctuation (sentence-density target)
    //   - a single-word "Share" label          (boilerplate target)
    //   - the real article
    //
    // deformat's scanner already skips <nav>, <footer>, <aside>, etc.,
    // so anchor-heavy boilerplate that lives inside those tags never
    // reaches the filters. Real-world pages often wrap nav-like
    // content in plain <div>s for styling reasons — that's exactly the
    // case the link-density filter targets.
    let html = r#"
    <!DOCTYPE html>
    <html><body>
      <div class="related-articles">
        <p><a href="/a">Read: How the team spotted it</a>
           <a href="/b">Also: New measurements coming</a>
           <a href="/c">See: Habitable zone explained</a>
           <a href="/d">More: Planet discovery timeline</a></p>
      </div>
      <article>
        <h1>Study Finds New Exoplanet</h1>
        <p>Astronomers at the European Southern Observatory announced
           the detection of a new exoplanet orbiting Proxima Centauri.
           The planet has a mass similar to Earth and sits within the
           star's habitable zone. Follow-up observations are planned
           for the coming months.</p>
        <p>ruby python rust javascript typescript golang kotlin scala
           haskell elixir clojure ocaml fsharp erlang zig nim dart swift
           perl bash lua vim emacs nvim helix</p>
        <p>The discovery was made using radial-velocity measurements
           collected over three years. The team's paper appears in
           Nature Astronomy this week.</p>
        <p>Share</p>
      </article>
    </body></html>
    "#;

    println!("=== Baseline (strip_to_segments) ===");
    let baseline = strip_to_segments(html);
    print_segments(&baseline);

    println!("\n=== + link-density (cap 0.45) ===");
    let after_link = strip_to_segments_filtered(html, 0.45);
    print_segments(&after_link);

    println!("\n=== + sentence-density (1.0 per 100 words) ===");
    let after_sentence = filter_low_sentence_density(after_link, 1.0);
    print_segments(&after_sentence);

    println!("\n=== + boilerplate (min 40 chars) ===");
    let final_segs = filter_boilerplate(after_sentence, 40);
    print_segments(&final_segs);
}

fn print_segments(segs: &[deformat::html::Segment]) {
    println!("  {} segment(s):", segs.len());
    for s in segs {
        let preview: String = s.data().text.chars().take(70).collect();
        let ellipsis = if s.data().text.chars().count() > 70 {
            "…"
        } else {
            ""
        };
        println!("    {:<14} {:?}{}", s.type_name(), preview, ellipsis);
    }
}
