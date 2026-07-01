//! Convert extracted HTML segments into `slabs::Slab` records.
//!
//! ```sh
//! cargo run --example segments_to_slabs
//! ```
//!
//! `deformat` owns extraction and semantic segment boundaries. `slabs` owns
//! offset-safe retrieval spans over the extracted text that downstream
//! embedding and indexing code can consume.

use deformat::html::strip_to_segments;
use slabs::Slab;

fn main() {
    let html = r#"
    <article>
      <h1>Span Boundaries</h1>
      <p>deformat extracts typed document segments from source formats.</p>
      <p>slabs records those selected spans with byte and character offsets.</p>
      <footer>Share Subscribe Related</footer>
    </article>
    "#;

    let segments = strip_to_segments(html);
    let mut extracted = String::new();
    let mut slabs = Vec::new();

    for segment in &segments {
        let text = segment.data().text.trim();
        if text.is_empty() {
            continue;
        }

        if !extracted.is_empty() {
            extracted.push_str("\n\n");
        }

        let start = extracted.len();
        extracted.push_str(text);
        let end = extracted.len();

        let slab = Slab::from_byte_range(&extracted, start..end, slabs.len()).unwrap();
        slabs.push((segment.type_name(), slab));
    }

    println!("=== extracted text ===\n{extracted}\n");
    println!("=== slabs ===");
    for (kind, slab) in slabs {
        println!(
            "#{:02} {:<16} bytes={:?} chars={:?} text={:?}",
            slab.index,
            kind,
            slab.span(),
            slab.char_span(),
            slab.text
        );
    }
}
