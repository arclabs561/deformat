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
        if self.exceeded {
            return Err(budget_error());
        }
        let limit = MAX_ENTRY_BYTES.min(self.remaining);
        let mut bytes = Vec::new();
        let result = entry.take(limit + 1).read_to_end(&mut bytes);
        let read = bytes.len() as u64;
        self.exceeded = read > self.remaining;
        self.remaining = self.remaining.saturating_sub(read);
        if read > MAX_ENTRY_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ZIP entry exceeds {MAX_ENTRY_BYTES} bytes decompressed"),
            ));
        }
        if self.exceeded {
            return Err(budget_error());
        }
        result?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn charges_partial_reads_even_when_reader_fails() {
        struct Fails;
        impl Read for Fails {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        let mut budget = ArchiveBudget {
            remaining: 4,
            exceeded: false,
        };
        let mut out = String::new();
        assert!(budget
            .read_to_string(Cursor::new(b"abc").chain(Fails), &mut out)
            .is_err());
        assert_eq!(budget.remaining, 1);
        assert!(out.is_empty());
        assert!(budget.read_to_string(&b"ab"[..], &mut out).is_err());
        assert!(budget.check().is_err());
    }

    #[test]
    fn equal_entry_and_archive_limits_leave_budget_exhausted() {
        let mut budget = ArchiveBudget {
            remaining: MAX_ENTRY_BYTES,
            exceeded: false,
        };
        let mut out = String::new();
        assert!(budget.read_to_string(io::repeat(0), &mut out).is_err());
        assert!(budget.check().is_err());
        assert!(budget.read_to_string(io::empty(), &mut out).is_err());
    }

    #[test]
    fn charges_skipped_entries_and_stops_after_archive_limit() {
        let mut budget = ArchiveBudget {
            remaining: MAX_ENTRY_BYTES + 2,
            exceeded: false,
        };
        let mut out = String::new();
        assert!(budget.read_to_string(io::repeat(0), &mut out).is_err());
        assert_eq!(budget.remaining, 1);
        assert!(budget.read_to_string(&b"ab"[..], &mut out).is_err());
        assert!(budget.read_to_string(io::empty(), &mut out).is_err());
        assert!(out.is_empty());
    }
}
