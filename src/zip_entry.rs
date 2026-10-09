//! Bounded reads of ZIP entries for the DOCX, EPUB and PPTX extractors.

use std::io::Read;

/// Largest decompressed size read from one ZIP entry.
///
/// Deflate inflates up to about 1000x, so a sub-megabyte archive can carry a
/// multi-gigabyte entry. Real `document.xml`, slide and chapter files are far
/// below this.
#[cfg(not(test))]
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(test)]
pub(crate) const MAX_ENTRY_BYTES: u64 = 1024 * 1024;

/// Append the entry's text to `out`, failing with `InvalidData` instead of
/// buffering more than [`MAX_ENTRY_BYTES`].
pub(crate) fn read_to_string<R: Read>(entry: R, out: &mut String) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("ZIP entry exceeds {MAX_ENTRY_BYTES} bytes decompressed"),
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.push_str(&text);
    Ok(())
}
