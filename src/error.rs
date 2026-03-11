//! Error types for extraction failures.

use std::fmt;

/// Errors that can occur during text extraction.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The input format is not supported for extraction.
    UnsupportedFormat(String),
    /// An I/O error occurred (e.g., reading a PDF file).
    Io(std::io::Error),
    /// The extraction produced no usable text.
    EmptyResult,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnsupportedFormat(fmt_name) => {
                write!(f, "unsupported format: {fmt_name}")
            }
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::EmptyResult => write!(f, "extraction produced no text"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
