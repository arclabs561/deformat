//! Decode non-UTF-8 HTML bytes before extracting.
//!
//! ```sh
//! cargo run --example charset --features encoding_rs
//! ```
//!
//! deformat's text extractors assume UTF-8. Real-world HTML still
//! ships Windows-1252, Shift-JIS, GB2312, and friends — especially
//! legacy CMS output and email bodies. The `encoding_rs` feature adds
//! [`detect::decode_bytes`] which reads the BOM, scans for
//! `<meta charset>`, and falls back to the label the caller passes.

#[cfg(not(feature = "encoding_rs"))]
fn main() {
    eprintln!("run with --features encoding_rs");
    std::process::exit(2);
}

#[cfg(feature = "encoding_rs")]
fn main() {
    // Construct a Windows-1252 payload: the byte 0xE9 is 'é' in cp1252
    // but invalid UTF-8. A naive `str::from_utf8` would fail.
    let mut bytes: Vec<u8> =
        br#"<html><head><meta charset="windows-1252"></head><body><p>Caf"#.to_vec();
    bytes.push(0xE9);
    // Embed cp1252-encoded text: em-dash (0x97), 'è' (0xE8), 'é' (0xE9).
    bytes.extend_from_slice(b" au lait ");
    bytes.push(0x97);
    bytes.extend_from_slice(b" Parisien du XIXe si");
    bytes.push(0xE8);
    bytes.push(0xE9);
    bytes.extend_from_slice(b"cle.</p></body></html>");

    println!("Raw bytes: {} bytes, not valid UTF-8", bytes.len());
    println!(
        "str::from_utf8 succeeds? {}",
        std::str::from_utf8(&bytes).is_ok()
    );

    // decode_bytes sniffs `<meta charset>` and returns a valid Rust String.
    let decoded = deformat::detect::decode_bytes(&bytes, "utf-8");
    println!("\nDecoded (first 120 chars):");
    println!("  {}", &decoded[..decoded.len().min(120)]);

    // Feed the decoded string to the normal HTML extractor.
    let text = deformat::html::strip_to_text(&decoded);
    println!("\nExtracted plain text:");
    println!("  {text}");
}
