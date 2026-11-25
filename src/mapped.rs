//! Rendered text and its correspondence to the original document bytes.

use crate::html::{self, SpanKind, SpanMap};
use crate::{Error, Extracted, Extractor, Format};
pub use textprep_source_map::{SourceKind, SourceMap, SourceRun};

/// The source map describes the current value of `extracted.text`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MappedExtraction {
    /// Extracted text and the renderer metadata.
    pub extracted: Extracted,
    /// Mapping from analyzed UTF-8 bytes to the original input bytes.
    pub source_map: SourceMap,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MappedExtraction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Wire {
            extracted: Extracted,
            source_map: SourceMap,
        }
        let Wire {
            extracted,
            source_map,
        } = Wire::deserialize(deserializer)?;
        source_map
            .validate_analyzed(&extracted.text)
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            extracted,
            source_map,
        })
    }
}

impl SpanMap {
    /// Convert deformat's renderer runs into a portable map, preserving gaps.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid ranges or direct runs whose source bytes
    /// disagree with the provided rendered text.
    pub fn to_source_map(&self, original: &str, rendered: &str) -> Result<SourceMap, String> {
        let mut runs = Vec::new();
        let mut cursor = 0;
        for span in self.iter() {
            if cursor < span.output_start {
                runs.push(SourceRun::new(
                    cursor..span.output_start,
                    None,
                    SourceKind::Unmapped,
                ));
            }
            let kind = match span.kind {
                SpanKind::Direct => SourceKind::Direct,
                SpanKind::EntityDecoded => SourceKind::Transformed,
                SpanKind::WhitespaceCollapsed => SourceKind::Approximate,
                SpanKind::Synthetic => SourceKind::Synthetic,
            };
            runs.push(SourceRun::new(
                span.output_start..span.output_end,
                Some(span.source_start..span.source_end),
                kind,
            ));
            cursor = span.output_end;
        }
        if cursor < rendered.len() {
            runs.push(SourceRun::new(
                cursor..rendered.len(),
                None,
                SourceKind::Unmapped,
            ));
        }
        SourceMap::new(original, rendered, runs)
    }
}

/// Recover the mapped-strip runs for an existing HTML rendering. Readability
/// and html2text results retain the source digest and unknown coordinates.
///
/// # Errors
///
/// Returns an error if the strip renderer produced invalid runs.
pub fn html_source_map(
    original: &str,
    rendered: &str,
    extractor: Extractor,
) -> Result<SourceMap, String> {
    if extractor == Extractor::Strip {
        let (text, spans) = html::strip_to_text_with_spans(original);
        if text == rendered {
            return spans.to_source_map(original, rendered);
        }
    }
    Ok(SourceMap::unmapped(rendered, Some(original.as_bytes())))
}

/// Extract text using the standard format dispatcher and retain source provenance.
/// HTML uses the mapped strip renderer. Plain UTF-8 text maps directly.
/// Other successful extractions carry the digest and length of the original
/// bytes, with unknown source coordinates.
///
/// # Errors
///
/// Propagates extraction errors, invalid UTF-8 and invalid renderer offsets.
pub fn extract(original: &[u8]) -> Result<MappedExtraction, Error> {
    let format = crate::detect::detect_bytes(original);
    if format == Format::Html {
        let html = std::str::from_utf8(original).map_err(|e| Error::Parse(format!("HTML: {e}")))?;
        let (text, spans) = html::strip_to_text_with_spans(html);
        let source_map = spans.to_source_map(html, &text).map_err(Error::Parse)?;
        let extracted = Extracted {
            text,
            format: Format::Html,
            extractor: Extractor::Strip,
            title: None,
            excerpt: None,
            fallback: false,
        };
        return Ok(MappedExtraction {
            extracted,
            source_map,
        });
    }
    let extracted = crate::extract_detected_bytes(original, format)?;
    extracted.with_source_map(original)
}

impl Extracted {
    /// Bind an existing extraction to its original bytes. This also supports
    /// alternative HTML renderers and format-specific extraction entry points.
    ///
    /// # Errors
    ///
    /// Returns an error if a strip renderer's offsets fail validation.
    pub fn with_source_map(self, original: &[u8]) -> Result<MappedExtraction, Error> {
        let source_map = match (self.format, self.extractor) {
            (Format::Html, Extractor::Strip) => match std::str::from_utf8(original) {
                Ok(source) => {
                    html_source_map(source, &self.text, self.extractor).map_err(Error::Parse)?
                }
                Err(_) => SourceMap::unmapped(&self.text, Some(original)),
            },
            (Format::PlainText | Format::Markdown | Format::Unknown, Extractor::Passthrough)
                if self.text.as_bytes() == original =>
            {
                SourceMap::identity(&self.text)
            }
            _ => SourceMap::unmapped(&self.text, Some(original)),
        };
        Ok(MappedExtraction {
            extracted: self,
            source_map,
        })
    }
}

#[cfg(feature = "prepare")]
pub use textprep::clean::CleanOptions;

/// Render a document and apply textprep cleanup, with one composed source map.
///
/// # Errors
///
/// Propagates extraction and mapping errors.
#[cfg(feature = "prepare")]
pub fn prepare(original: &[u8], options: &CleanOptions) -> Result<MappedExtraction, Error> {
    extract(original)?.prepare(options)
}

#[cfg(feature = "prepare")]
impl MappedExtraction {
    /// Apply cleanup to the selected rendering and compose its existing map.
    ///
    /// # Errors
    ///
    /// Returns an error when the transformation cannot be mapped.
    pub fn prepare(mut self, options: &CleanOptions) -> Result<Self, Error> {
        if options.normalize_unicode || options.clean_whitespace {
            let (text, step) = options
                .prepare_mapped(&self.extracted.text)
                .map_err(Error::Parse)?;
            self.source_map = step.compose(&self.source_map).map_err(Error::Parse)?;
            self.extracted.text = text;
        }
        Ok(self)
    }
}
