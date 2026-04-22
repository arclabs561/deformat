//! Emit pure Segment JSON to stdout.
//!
//! Pairs with `scripts/langchain_interop.py` to demonstrate that
//! deformat's serde output drops straight into LangChain's
//! `Document(page_content, metadata)` shape.
//!
//! ```sh
//! cargo run --example segments_json --features serde > /tmp/segments.json
//! uv run scripts/langchain_interop.py /tmp/segments.json
//! ```
//!
//! Reads HTML from stdin if given `-`, otherwise uses a built-in demo.

use std::io::{Read, Write};

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let html = if arg == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .expect("read stdin");
        buf
    } else {
        DEMO_HTML.to_string()
    };

    let segs = deformat::html::strip_to_segments(&html);

    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string_pretty(&segs).unwrap();
        let mut out = std::io::stdout().lock();
        out.write_all(json.as_bytes()).unwrap();
        out.write_all(b"\n").unwrap();
    }
    #[cfg(not(feature = "serde"))]
    {
        eprintln!("Re-run with --features serde to emit JSON");
        eprintln!("Got {} segments", segs.len());
    }
}

const DEMO_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Demo</title></head>
<body>
  <article>
    <h1>Structured extraction</h1>
    <p>deformat produces Unstructured.io-compatible segments that feed
       LangChain pipelines without an adapter.</p>
    <h2>Why it matters</h2>
    <p>Elements carry type (Title, NarrativeText, ListItem, Table, Image)
       plus heading depth and parent links. Downstream chunkers can respect
       semantic boundaries.</p>
    <ul>
      <li>Title and heading levels preserved</li>
      <li>Each list item is its own segment</li>
    </ul>
    <figure><img src="x.png" alt="Architecture diagram"></figure>
  </article>
</body>
</html>"#;
