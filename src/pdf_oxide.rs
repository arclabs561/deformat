//! PDF text extraction via the `pdf_oxide` crate.
//!
//! Coordinate-aware PDF extraction. The `pdf` feature exposes plain text
//! extraction through [`crate::pdf`]; this module keeps the backend-specific
//! segment APIs available behind the `pdf_oxide` feature.
//!
//! The modules can be enabled simultaneously. For plain text, prefer
//! [`crate::pdf`]. Use this module when page segments or coordinates are
//! needed.

use crate::segment::{element_id, Coordinates, Segment, SegmentData, SegmentMetadata};
use crate::{Error, Extracted, Extractor, Format};
use std::path::Path;

/// Extract text from a PDF file using `pdf_oxide`.
///
/// # Errors
///
/// Returns [`Error::Parse`] on failure, or [`Error::EmptyResult`] if no
/// pages produce any text.
pub fn extract_file(path: &Path) -> Result<Extracted, Error> {
    let mut doc = pdf_oxide::PdfDocument::open(path)
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    extract_doc(&mut doc)
}

/// Extract text from PDF bytes using `pdf_oxide`.
///
/// # Errors
///
/// Returns [`Error::Parse`] on failure, or [`Error::EmptyResult`] if no
/// pages produce any text.
pub fn extract_bytes(bytes: &[u8]) -> Result<Extracted, Error> {
    let mut doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    extract_doc(&mut doc)
}

fn extract_doc(doc: &mut pdf_oxide::PdfDocument) -> Result<Extracted, Error> {
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Parse(format!("PDF (oxide) page_count: {e}")))?;
    let mut pages: Vec<String> = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let page_text = doc
            .extract_text(i)
            .map_err(|e| Error::Parse(format!("PDF (oxide) page {i}: {e}")))?;
        if !page_text.trim().is_empty() {
            pages.push(page_text);
        }
    }
    let text = pages.join("\n\n");
    if text.trim().is_empty() {
        return Err(Error::EmptyResult);
    }
    Ok(Extracted {
        text,
        format: Format::Pdf,
        extractor: Extractor::PdfOxide,
        title: None,
        excerpt: None,
        fallback: false,
    })
}

/// Extract typed [`Segment`]s from a PDF file, one per page.
///
/// Emits one [`Segment::NarrativeText`] per non-empty page, with
/// `metadata.page_number` set. Does not split paragraphs within a
/// page — that requires layout analysis beyond what `pdf_oxide`
/// exposes at this seam.
///
/// # Errors
///
/// Returns [`Error::Parse`] on any failure from the PDF backend.
pub fn extract_to_segments(path: &Path) -> Result<Vec<Segment>, Error> {
    let mut doc = pdf_oxide::PdfDocument::open(path)
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    segments_from_doc(&mut doc)
}

/// Like [`extract_to_segments`], from bytes in memory.
pub fn extract_bytes_to_segments(bytes: &[u8]) -> Result<Vec<Segment>, Error> {
    let mut doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    segments_from_doc(&mut doc)
}

fn segments_from_doc(doc: &mut pdf_oxide::PdfDocument) -> Result<Vec<Segment>, Error> {
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Parse(format!("PDF (oxide) page_count: {e}")))?;
    let mut segments: Vec<Segment> = Vec::new();
    for i in 0..page_count {
        let page_text = doc
            .extract_text(i)
            .map_err(|e| Error::Parse(format!("PDF (oxide) page {i}: {e}")))?;
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let page_num = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let id = element_id("NarrativeText", trimmed, i);
        let meta = SegmentMetadata {
            page_number: Some(page_num),
            ..SegmentMetadata::default()
        };
        segments.push(Segment::NarrativeText(SegmentData {
            element_id: id,
            text: trimmed.to_string(),
            metadata: meta,
        }));
    }
    if segments.is_empty() {
        return Err(Error::EmptyResult);
    }
    Ok(segments)
}

/// Extract typed segments with per-line bounding boxes.
///
/// Unlike [`extract_to_segments`] (one `NarrativeText` per page), this
/// emits one segment per detected text line with
/// `metadata.coordinates` populated from `pdf_oxide`'s layout
/// reconstruction. Suitable for RAG pipelines that cite "page N, line
/// at (x,y,w,h)" — Docling-style page anchors.
///
/// # Errors
///
/// Returns [`Error::Parse`] on any failure, or [`Error::EmptyResult`]
/// if no lines produce text.
pub fn extract_to_segments_with_coords(path: &Path) -> Result<Vec<Segment>, Error> {
    let mut doc = pdf_oxide::PdfDocument::open(path)
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    segments_with_coords(&mut doc)
}

/// Like [`extract_to_segments_with_coords`], from bytes.
pub fn extract_bytes_to_segments_with_coords(bytes: &[u8]) -> Result<Vec<Segment>, Error> {
    let mut doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Error::Parse(format!("PDF (oxide): {e}")))?;
    segments_with_coords(&mut doc)
}

fn segments_with_coords(doc: &mut pdf_oxide::PdfDocument) -> Result<Vec<Segment>, Error> {
    let page_count = doc
        .page_count()
        .map_err(|e| Error::Parse(format!("PDF (oxide) page_count: {e}")))?;
    let mut segments: Vec<Segment> = Vec::new();
    let mut ord = 0;
    for i in 0..page_count {
        // MediaBox gives the logical page dimensions in PDF user-space.
        let (page_w, page_h) = match doc.get_page_media_box(i) {
            Ok((x0, y0, x1, y1)) => (f64::from(x1 - x0), f64::from(y1 - y0)),
            Err(_) => (0.0, 0.0),
        };
        let lines = match doc.extract_text_lines(i) {
            Ok(l) => l,
            Err(e) => return Err(Error::Parse(format!("PDF (oxide) lines page {i}: {e}"))),
        };
        let page_num = u32::try_from(i + 1).unwrap_or(u32::MAX);
        for line in lines {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            let bbox = line.bbox;
            let x = f64::from(bbox.x);
            let y = f64::from(bbox.y);
            let w = f64::from(bbox.width);
            let h = f64::from(bbox.height);
            let coords = Coordinates {
                points: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
                system: "PointSpace".to_string(),
                layout_width: page_w,
                layout_height: page_h,
            };
            let id = element_id("NarrativeText", text, ord);
            ord += 1;
            let meta = SegmentMetadata {
                page_number: Some(page_num),
                coordinates: Some(coords),
                ..SegmentMetadata::default()
            };
            segments.push(Segment::NarrativeText(SegmentData {
                element_id: id,
                text: text.to_string(),
                metadata: meta,
            }));
        }
    }
    if segments.is_empty() {
        return Err(Error::EmptyResult);
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pdf_oxide_invalid_bytes() {
        let result = extract_bytes(b"not a pdf");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_oxide_empty_bytes() {
        let result = extract_bytes(b"");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn extract_pdf_oxide_nonexistent_file() {
        let result = extract_file(Path::new("/nonexistent/path/to/file.pdf"));
        assert!(result.is_err());
    }
}
