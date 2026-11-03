//! XLSX/ODS spreadsheet text extraction.
//!
//! Requires the `xlsx` feature. Extracts cell text from spreadsheets
//! using the `calamine` crate. Supports XLSX, XLS, XLSB, and ODS formats.
//!
//! Cells are emitted row by row, tab-separated, one row per line. Blank
//! cells inside a row stay as empty fields, so values keep their columns;
//! trailing blanks and empty rows are dropped. Date cells are written as
//! ISO 8601.

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

/// Text of one cell. Date cells become ISO 8601 (`2024-01-01`, with a time
/// when one is set) instead of the raw serial number the spreadsheet stores.
fn cell_text(cell: &calamine::Data) -> String {
    match cell {
        calamine::Data::DateTime(dt) if dt.is_datetime() => {
            let (y, mo, d, h, mi, s, ms) = dt.to_ymd_hms_milli();
            match (h, mi, s, ms) {
                (0, 0, 0, 0) => format!("{y:04}-{mo:02}-{d:02}"),
                (_, _, _, 0) => format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}"),
                _ => format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}"),
            }
        }
        other => other.to_string(),
    }
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
            // Keep blank cells so each value stays under its column; drop
            // only the trailing ones so rows don't end in tabs.
            let mut cells: Vec<String> = row.iter().map(cell_text).collect();
            while cells.last().is_some_and(String::is_empty) {
                cells.pop();
            }
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
    use calamine::{Data, ExcelDateTime, ExcelDateTimeType};

    fn date(serial: f64, is_1904: bool) -> Data {
        Data::DateTime(ExcelDateTime::new(
            serial,
            ExcelDateTimeType::DateTime,
            is_1904,
        ))
    }

    #[test]
    fn date_cells_are_iso_8601() {
        // Reference serials from Python's datetime: 2024-01-01 is 45292 days
        // after 1899-12-30 (1900 system) and 43830 after 1904-01-01.
        assert_eq!(cell_text(&date(45292.0, false)), "2024-01-01");
        assert_eq!(cell_text(&date(43830.0, true)), "2024-01-01");
        assert_eq!(cell_text(&date(45292.5, false)), "2024-01-01T12:00:00");
        // Non-date cells keep their usual text.
        assert_eq!(cell_text(&Data::Float(45292.0)), "45292");
        assert_eq!(cell_text(&Data::Empty), "");
    }

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
