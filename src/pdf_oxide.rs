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
        // MediaBox gives the page in PDF user space; /Rotate turns it for display.
        let media_box = doc
            .get_page_media_box(i)
            .map(|(x0, y0, x1, y1)| {
                (
                    f64::from(x0.min(x1)),
                    f64::from(y0.min(y1)),
                    f64::from(x0.max(x1)),
                    f64::from(y0.max(y1)),
                )
            })
            .unwrap_or_default();
        let rotation = doc.get_page_rotation(i).unwrap_or(0);
        let frame = PageFrame::new(media_box, rotation);
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
            let (x0, y0, x1, y1) = frame.map_rect(
                f64::from(bbox.x),
                f64::from(bbox.y),
                f64::from(bbox.x + bbox.width),
                f64::from(bbox.y + bbox.height),
            );
            // PointSpace has y pointing up, so the top edge is y1.
            let coords = Coordinates {
                points: vec![(x0, y1), (x1, y1), (x1, y0), (x0, y0)],
                system: "PointSpace".to_string(),
                layout_width: frame.width,
                layout_height: frame.height,
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

/// The page as displayed: origin at the MediaBox's lower-left corner, turned
/// clockwise by the page's `/Rotate`, y pointing up.
struct PageFrame {
    llx: f64,
    lly: f64,
    /// Unrotated MediaBox width and height.
    media_w: f64,
    media_h: f64,
    /// Clockwise rotation, one of 0, 90, 180 or 270.
    rotation: i32,
    /// Displayed width and height.
    width: f64,
    height: f64,
}

impl PageFrame {
    fn new((llx, lly, urx, ury): (f64, f64, f64, f64), rotation: i32) -> Self {
        // /Rotate must be a multiple of 90 (ISO 32000-1, Table 30); treat any
        // other value as unrotated.
        let rotation = match rotation.rem_euclid(360) {
            r @ (90 | 180 | 270) => r,
            _ => 0,
        };
        let (media_w, media_h) = (urx - llx, ury - lly);
        let (width, height) = if rotation % 180 == 0 {
            (media_w, media_h)
        } else {
            (media_h, media_w)
        };
        Self {
            llx,
            lly,
            media_w,
            media_h,
            rotation,
            width,
            height,
        }
    }

    fn map_point(&self, x: f64, y: f64) -> (f64, f64) {
        let (rx, ry) = (x - self.llx, y - self.lly);
        match self.rotation {
            90 => (ry, self.media_w - rx),
            180 => (self.media_w - rx, self.media_h - ry),
            270 => (self.media_h - ry, rx),
            _ => (rx, ry),
        }
    }

    /// Map a user-space rectangle; returns `(x0, y0, x1, y1)` with `x0 <= x1`
    /// and `y0 <= y1`. A quadrant rotation keeps rectangles axis-aligned.
    fn map_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64, f64, f64) {
        let (ax, ay) = self.map_point(x0, y0);
        let (bx, by) = self.map_point(x1, y1);
        (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_frame_removes_media_box_offset() {
        let f = PageFrame::new((50.0, 600.0, 400.0, 800.0), 0);
        assert_eq!((f.width, f.height), (350.0, 200.0));
        assert_eq!(
            f.map_rect(72.0, 700.0, 172.0, 712.0),
            (22.0, 100.0, 122.0, 112.0)
        );
    }

    #[test]
    fn page_frame_rotates_clockwise() {
        // Letter page, text near its top-left corner in user space.
        let rect = (72.0, 700.0, 172.0, 712.0);
        let f = PageFrame::new((0.0, 0.0, 612.0, 792.0), 90);
        assert_eq!((f.width, f.height), (792.0, 612.0));
        // Turned clockwise, the page's top-left corner ends up top-right.
        assert_eq!(
            f.map_rect(rect.0, rect.1, rect.2, rect.3),
            (700.0, 440.0, 712.0, 540.0)
        );
        let f = PageFrame::new((0.0, 0.0, 612.0, 792.0), 180);
        assert_eq!(
            f.map_rect(rect.0, rect.1, rect.2, rect.3),
            (440.0, 80.0, 540.0, 92.0)
        );
        let f = PageFrame::new((0.0, 0.0, 612.0, 792.0), -90);
        assert_eq!((f.rotation, f.width), (270, 792.0));
        assert_eq!(
            f.map_rect(rect.0, rect.1, rect.2, rect.3),
            (80.0, 72.0, 92.0, 172.0)
        );
    }

    #[test]
    fn page_frame_ignores_non_quadrant_rotation() {
        let f = PageFrame::new((0.0, 0.0, 612.0, 792.0), 45);
        assert_eq!((f.rotation, f.width, f.height), (0, 612.0, 792.0));
    }

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
