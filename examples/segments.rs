//! Emit Unstructured.io-compatible typed segments from HTML.
//!
//! ```sh
//! cargo run --example segments --features serde
//! ```
//!
//! The JSON output is the wire format that `langchain-community`'s
//! `UnstructuredLoader` (element mode) and Haystack's
//! `UnstructuredDocumentConverter` consume directly — so a Rust
//! preprocessing step can feed a Python RAG pipeline with no adapter
//! code on either side.

fn main() {
    let html = r#"
    <article>
      <h1>Rust HTML Extraction</h1>
      <p>deformat produces plain text, markdown, and typed segments
         from HTML, PDF, and Office documents.</p>
      <h2>Why typed segments?</h2>
      <p>RAG pipelines chunk documents along semantic boundaries —
         title, paragraph, list item. A pre-segmented document
         is easier to chunk well.</p>
      <ul>
        <li>Title and heading levels preserved</li>
        <li>Each list item is its own segment</li>
        <li>Code blocks carry language hints when available</li>
      </ul>
      <pre><code class="language-rust">let segs = deformat::html::strip_to_segments(html);</code></pre>
    </article>
    "#;

    let segs = deformat::html::strip_to_segments(html);

    println!("=== {} segments ===\n", segs.len());
    for seg in &segs {
        let data = seg.data();
        let depth = data
            .metadata
            .category_depth
            .map(|d| format!(" (depth={d})"))
            .unwrap_or_default();
        let parent = data
            .metadata
            .parent_id
            .as_deref()
            .map(|id| format!(" under={id}"))
            .unwrap_or_default();
        println!(
            "[{}{}{}] {}",
            seg.type_name(),
            depth,
            parent,
            truncate(&data.text, 70),
        );
    }

    #[cfg(feature = "serde")]
    {
        println!("\n=== JSON form (first 2 segments) ===\n");
        let preview: Vec<_> = segs.iter().take(2).collect();
        let json = serde_json::to_string_pretty(&preview).unwrap();
        println!("{json}");
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}
