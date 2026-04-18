//! XLSX/ODS spreadsheet text extraction.
//!
//! Requires the `xlsx` feature. Extracts cell text from spreadsheets
//! using the `calamine` crate. Supports XLSX, XLS, XLSB, and ODS formats.
//!
//! Cells are emitted row by row, tab-separated, one row per line.
//! Empty cells are skipped.

use crate::{Error, Extracted, Extractor, Format};
use calamine::Reader;
use std::path::Path;

/// Extract text from a spreadsheet file (XLSX, XLS, XLSB, ODS).
///
/// Reads all sheets, emitting cell values as tab-separated rows.
/// Sheet names appear as headers when multiple sheets exist.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read,
/// [`Error::Parse`] if spreadsheet parsing fails, or
/// [`Error::EmptyResult`] if no cells contain text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let mut workbook: calamine::Sheets<_> = calamine::open_workbook_auto(path)
        .map_err(|e| Error::Parse(format!("spreadsheet open failed: {e}")))?;

    extract_workbook(&mut workbook)
}

/// Extract text from spreadsheet bytes in memory.
///
/// # Errors
///
/// Returns [`Error::Parse`] if parsing fails, or
/// [`Error::EmptyResult`] if no cells contain text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let cursor = std::io::Cursor::new(bytes);
    let mut workbook: calamine::Sheets<_> = calamine::open_workbook_auto_from_rs(cursor)
        .map_err(|e| Error::Parse(format!("spreadsheet parse failed: {e}")))?;

    extract_workbook(&mut workbook)
}

fn extract_workbook<RS: std::io::Read + std::io::Seek>(
    workbook: &mut calamine::Sheets<RS>,
) -> Result<Extracted, Error> {
    let sheet_names = workbook.sheet_names().to_vec();
    let mut text = String::new();

    for name in &sheet_names {
        let range = workbook
            .worksheet_range(name)
            .map_err(|e| Error::Parse(format!("failed to read sheet '{name}': {e}")))?;

        let mut sheet_text = String::new();
        for row in range.rows() {
            let cells: Vec<String> = row
                .iter()
                .filter_map(|cell| {
                    let s = cell.to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
                .collect();
            if !cells.is_empty() {
                if !sheet_text.is_empty() {
                    sheet_text.push('\n');
                }
                sheet_text.push_str(&cells.join("\t"));
            }
        }

        if !sheet_text.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            if sheet_names.len() > 1 {
                text.push_str(name);
                text.push('\n');
            }
            text.push_str(&sheet_text);
        }
    }

    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }

    Ok(Extracted {
        text,
        format: Format::Xlsx,
        extractor: Extractor::Strip,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_xlsx_invalid_bytes() {
        let result = extract_bytes(b"not a spreadsheet");
        assert!(result.is_err(), "should fail on invalid input");
    }

    #[test]
    fn extract_xlsx_empty_bytes() {
        let result = extract_bytes(b"");
        assert!(result.is_err(), "should fail on empty input");
    }

    #[test]
    fn extract_xlsx_nonexistent_file() {
        let path = Path::new("/nonexistent/path/to/file.xlsx");
        let result = extract_file(path);
        assert!(result.is_err(), "should fail on missing file");
    }
}
