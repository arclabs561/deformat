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

/// Largest total decompressed size read from one archive, summed over every
/// entry a reader opens.
///
/// The per-entry cap alone lets an EPUB or PPTX with many entries expand
/// without bound: each chapter or slide stays under [`MAX_ENTRY_BYTES`]
/// while the output grows linearly with the entry count.
#[cfg(not(test))]
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(test)]
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024;

/// Decompressed-bytes budget shared by every entry read from one archive.
pub(crate) struct ArchiveBudget {
    remaining: u64,
    exceeded: bool,
}

impl ArchiveBudget {
    pub(crate) fn new() -> Self {
        Self {
            remaining: MAX_ARCHIVE_BYTES,
            exceeded: false,
        }
    }

    /// Append the entry's text to `out`, failing with `InvalidData` instead
    /// of buffering more than [`MAX_ENTRY_BYTES`] for this entry or more
    /// than the budget left for the archive.
    ///
    /// Readers that skip unreadable entries must still call
    /// [`check`](Self::check) so an exhausted budget is reported.
    pub(crate) fn read_to_string<R: Read>(
        &mut self,
        entry: R,
        out: &mut String,
    ) -> std::io::Result<()> {
        let limit = MAX_ENTRY_BYTES.min(self.remaining);
        let mut bytes = Vec::new();
        entry.take(limit + 1).read_to_end(&mut bytes)?;
        let read = bytes.len() as u64;
        if read > MAX_ENTRY_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ZIP entry exceeds {MAX_ENTRY_BYTES} bytes decompressed"),
            ));
        }
        if read > self.remaining {
            self.remaining = 0;
            self.exceeded = true;
            return Err(budget_error());
        }
        self.remaining -= read;
        let text = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        out.push_str(&text);
        Ok(())
    }

    /// `Err(Error::Parse)` once any read has run past the archive budget.
    // DOCX reads one entry and reports the read error directly.
    #[cfg_attr(not(any(feature = "epub", feature = "pptx")), allow(dead_code))]
    pub(crate) fn check(&self) -> Result<(), crate::Error> {
        if self.exceeded {
            Err(crate::Error::Parse(budget_error().to_string()))
        } else {
            Ok(())
        }
    }
}

fn budget_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("ZIP archive exceeds {MAX_ARCHIVE_BYTES} bytes decompressed in total"),
    )
}
