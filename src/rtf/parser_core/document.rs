use std::fmt;
use std::fs;
use std::io::Read;


use super::encoding::decode_document_bytes;
use super::header::RtfHeader;
use super::lexer::{Lexer, LexerError};
use super::parser::{Parser, ParserError, Part, StyleBlock};

pub fn parse_rtf(rtf: String) -> Result<RtfDocument, RtfError> {
    RtfDocument::try_from(rtf)
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RtfError {
    Lexer(LexerError),
    Parser(ParserError),
    Io(String),
}

impl std::error::Error for RtfError {}

impl fmt::Display for RtfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RtfError::Lexer(error) => write!(f, "{error}"),
            RtfError::Parser(error) => write!(f, "{error}"),
            RtfError::Io(error) => write!(f, "[RTF] : {error}"),
        }
    }
}

impl From<LexerError> for RtfError {
    fn from(error: LexerError) -> Self {
        RtfError::Lexer(error)
    }
}

impl From<ParserError> for RtfError {
    fn from(error: ParserError) -> Self {
        RtfError::Parser(error)
    }
}

impl From<std::io::Error> for RtfError {
    fn from(error: std::io::Error) -> Self {
        RtfError::Io(error.to_string())
    }
}


#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub author: Option<String>,
    pub manager: Option<String>,
    pub company: Option<String>,
    pub operator: Option<String>,
    pub category: Option<String>,
    pub keywords: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextOptions {
    pub hidden_text: bool,
    pub notes: bool,
    pub headers_footers: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            hidden_text: false,
            notes: true,
            headers_footers: true,
        }
    }
}

impl TextOptions {
    #[must_use]
    pub fn hidden_text(mut self, include: bool) -> Self {
        self.hidden_text = include;
        self
    }

    #[must_use]
    pub fn notes(mut self, include: bool) -> Self {
        self.notes = include;
        self
    }

    #[must_use]
    pub fn headers_footers(mut self, include: bool) -> Self {
        self.headers_footers = include;
        self
    }

    pub fn includes(&self, block: &StyleBlock) -> bool {
        let part = match block.part {
            Part::Body => true,
            Part::NoteReference(_) | Part::Note(_) => self.notes,
            Part::Header | Part::Footer => self.headers_footers,
        };
        part && (self.hidden_text || !block.painter.hidden)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct RtfDocument {
    pub header: RtfHeader,
    pub info: DocumentInfo,
    pub body: Vec<StyleBlock>,
}

impl TryFrom<String> for RtfDocument {
    type Error = RtfError;
    fn try_from(file_content: String) -> Result<Self, Self::Error> {
        Self::try_from(file_content.as_str())
    }
}

impl TryFrom<&str> for RtfDocument {
    type Error = RtfError;
    fn try_from(file_content: &str) -> Result<Self, Self::Error> {
        let tokens = Lexer::scan(file_content)?;
        Ok(Parser::new(tokens).parse()?)
    }
}

impl TryFrom<&mut fs::File> for RtfDocument {
    type Error = RtfError;
    fn try_from(file: &mut fs::File) -> Result<Self, Self::Error> {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }
}

impl RtfDocument {
    /// Create an `RtfDocument` from a rtf file path
    pub fn from_filepath(filename: &str) -> Result<RtfDocument, RtfError> {
        Self::from_bytes(&fs::read(filename)?)
    }

    /// Create an `RtfDocument` from the bytes of an RTF file. UTF-8 input is read as is; any other
    /// input is read in the code page the document declares.
    pub fn from_bytes(bytes: &[u8]) -> Result<RtfDocument, RtfError> {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::try_from(text),
            Err(_) => Self::try_from(decode_document_bytes(bytes).as_str()),
        }
    }

    /// Get the raw text of an RTF document
    pub fn get_text(&self) -> String {
        self.get_text_with(TextOptions::default())
    }

    pub fn get_text_with(&self, options: TextOptions) -> String {
        self.body.iter().filter(|block| options.includes(block)).map(|block| block.text.as_str()).collect()
    }
}
