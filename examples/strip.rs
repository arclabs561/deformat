//! Read HTML from stdin, write extracted plain text to stdout.
//!
//! Used by `scripts/qa_random_urls.py` for real-world QA testing.

fn main() {
    let mut html = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut html).expect("read stdin");
    let text = deformat::html::strip_to_text(&html);
    print!("{text}");
}
