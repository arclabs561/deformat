use std::collections::HashMap;
use std::{fmt, mem};


use super::document::{DocumentInfo, RtfDocument};
use super::encoding::{code_page_for_charset, code_page_for_document, code_page_for_font, decode_into, needs_trail_byte, symbol_font_char, SYMBOL_CODE_PAGE};
use super::header::{CharacterSet, Color, ColorRef, ColorTable, Font, FontFamily, FontRef, FontTable, RtfHeader, Style, StyleRef, StyleSheet};
use super::paragraph::{Alignment, Paragraph, SpaceBetweenLine};
use super::tokens::{ControlWord, Property, Token};

macro_rules! header_control_word {
    ($cw:ident) => {
        &Token::ControlSymbol((ControlWord::$cw, _))
    };
    ($cw:ident, $prop:ident) => {
        &Token::ControlSymbol((ControlWord::$cw, Property::$prop))
    };
}

#[derive(Debug, Default, PartialEq, Clone)]
#[non_exhaustive]
pub struct StyleBlock {
    pub painter: Painter,
    pub paragraph: Paragraph,

    pub part: Part,
    pub text: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Part {
    #[default]
    Body,
    NoteReference(u32),
    Note(u32),
    Header,
    Footer,
}

#[derive(Debug, Clone, PartialEq, Hash)]
#[non_exhaustive]
pub struct Painter {
    pub color_ref: ColorRef,
    pub font_ref: FontRef,
    pub font_size: u16,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub smallcaps: bool,
    pub strike: bool,
    pub hidden: bool,
}

impl Default for Painter {
    fn default() -> Self {
        Self {
            color_ref: Default::default(),
            font_ref: Default::default(),
            font_size: 12,
            bold: Default::default(),
            italic: Default::default(),
            underline: Default::default(),
            superscript: Default::default(),
            subscript: Default::default(),
            smallcaps: Default::default(),
            strike: Default::default(),
            hidden: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ParserError {
    InvalidToken(String),
    IgnorableDestinationParsingError,
    MalformedPainterStack,
    InvalidFontIdentifier(Property),
    InvalidColorIdentifier(Property),
    NoMoreToken,
    ValueCastError(String),
    UnicodeParsingError(i32),
    ParseEmptyToken,
}

impl std::error::Error for ParserError {}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[RTF Parser] : ")?;
        match self {
            ParserError::InvalidToken(msg) => write!(f, "{}", msg),
            ParserError::IgnorableDestinationParsingError => write!(f, "No ignorable destination should be left"),
            ParserError::MalformedPainterStack => write!(f, "Malformed painter stack : Unbalanced number of brackets"),
            ParserError::InvalidFontIdentifier(property) => write!(f, "Invalid font identifier : {:?}", property),
            ParserError::InvalidColorIdentifier(property) => write!(f, "Invalid color identifier : {:?}", property),
            ParserError::NoMoreToken => write!(f, "No more token to parse"),
            ParserError::ValueCastError(_type) => write!(f, "Unable to cast i32 to {_type}"),
            ParserError::UnicodeParsingError(value) => write!(f, "Unable to parse {value} value to unicode"),
            ParserError::ParseEmptyToken => write!(f, "Try to parse an empty token, this should never happen. If so, please open an issue in the github repository"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum InfoField {
    Title,
    Subject,
    Author,
    Manager,
    Company,
    Operator,
    Category,
    Keywords,
    Comment,
}

impl InfoField {
    fn from_word(word: &ControlWord) -> Option<Self> {
        let ControlWord::Unknown(name) = word else {
            return None;
        };
        match name.trim_start_matches('\\') {
            "title" => Some(Self::Title),
            "subject" => Some(Self::Subject),
            "author" => Some(Self::Author),
            "manager" => Some(Self::Manager),
            "company" => Some(Self::Company),
            "operator" => Some(Self::Operator),
            "category" => Some(Self::Category),
            "keywords" => Some(Self::Keywords),
            "doccomm" | "comment" => Some(Self::Comment),
            _ => None,
        }
    }

    fn slot(self, info: &mut DocumentInfo) -> &mut Option<String> {
        match self {
            Self::Title => &mut info.title,
            Self::Subject => &mut info.subject,
            Self::Author => &mut info.author,
            Self::Manager => &mut info.manager,
            Self::Company => &mut info.company,
            Self::Operator => &mut info.operator,
            Self::Category => &mut info.category,
            Self::Keywords => &mut info.keywords,
            Self::Comment => &mut info.comment,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Deferred {
    Note,
    Header,
    Footer,
}

impl Deferred {
    fn from_word(word: &ControlWord) -> Option<Self> {
        let ControlWord::Unknown(name) = word else {
            return None;
        };
        match name.trim_start_matches('\\') {
            "footnote" => Some(Self::Note),
            "header" | "headerl" | "headerr" | "headerf" => Some(Self::Header),
            "footer" | "footerl" | "footerr" | "footerf" => Some(Self::Footer),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
enum Destination {
    #[default]
    Body,
    Skip,
    InfoRoot,
    Info(InfoField),
    Deferred(Deferred),
}

#[derive(Debug, Clone, PartialEq, Hash)]
struct ParserState {
    pub painter: Painter,
    pub paragraph: Paragraph,
    pub unicode_ignore_count: i32,
    default_font: FontRef,
    destination: Destination,
    upr_resume: Option<Destination>,
}

impl Default for ParserState {
    fn default() -> Self {
        Self {
            painter: Default::default(),
            paragraph: Default::default(),
            unicode_ignore_count: 1,
            default_font: 0,
            destination: Destination::Body,
            upr_resume: None,
        }
    }
}

impl ParserState {
    fn open_group(&mut self, token: &Token, ignorable: bool) {
        let parent = self.destination;
        let resume = self.upr_resume.take();
        let word = match token {
            Token::ControlSymbol((word, _)) => Some(word),
            _ => None,
        };
        self.destination = match (parent, word) {
            (_, Some(ControlWord::Ud)) if resume.is_some() => resume.unwrap_or(Destination::Skip),
            (Destination::Skip, _) => Destination::Skip,
            (Destination::InfoRoot, Some(ControlWord::Upr)) => {
                self.upr_resume = Some(Destination::InfoRoot);
                Destination::Skip
            }
            (Destination::InfoRoot, Some(word)) => InfoField::from_word(word).map_or(Destination::Skip, Destination::Info),
            (Destination::InfoRoot, None) => Destination::Skip,
            (_, Some(ControlWord::Info)) => Destination::InfoRoot,
            (Destination::Body | Destination::Deferred(_), Some(word)) if !ignorable && Deferred::from_word(word).is_some() => {
                Deferred::from_word(word).map_or(parent, Destination::Deferred)
            }
            (_, Some(ControlWord::Upr)) => {
                self.upr_resume = Some(parent);
                Destination::Skip
            }
            (_, Some(word)) if ignorable || word.skips_group() => Destination::Skip,
            (_, None) if ignorable => Destination::Skip,
            _ => parent,
        };
    }

    fn apply(&mut self, control_word: &ControlWord, property: &Property) -> Result<(), ParserError> {
        let painter = &mut self.painter;
        let paragraph = &mut self.paragraph;
        #[rustfmt::skip]
        match control_word {
            ControlWord::ColorNumber        => painter.color_ref = property.get_value_as::<ColorRef>()?,
            ControlWord::FontNumber         => painter.font_ref = property.get_value_as::<FontRef>()?,
            ControlWord::DefaultFont        => {
                self.default_font = property.get_value_as::<FontRef>()?;
                painter.font_ref = self.default_font;
            }
            ControlWord::FontSize           => painter.font_size = property.get_value_as::<u16>()?,
            ControlWord::Bold               => painter.bold = property.as_bool(),
            ControlWord::Italic             => painter.italic = property.as_bool(),
            ControlWord::Underline          => painter.underline = property.as_bool(),
            ControlWord::UnderlineNone      => painter.underline = false,
            ControlWord::Superscript        => painter.superscript = property.as_bool(),
            ControlWord::Subscript          => painter.subscript = property.as_bool(),
            ControlWord::Smallcaps          => painter.smallcaps = property.as_bool(),
            ControlWord::Strikethrough      => painter.strike = property.as_bool(),
            ControlWord::Hidden             => painter.hidden = property.as_bool(),
            ControlWord::Pard               => *paragraph = Paragraph::default(),
            ControlWord::Plain              => *painter = Painter { font_ref: self.default_font, ..Painter::default() },
            ControlWord::ParDefTab          => paragraph.tab_width = property.get_value(),
            ControlWord::LeftAligned
                | ControlWord::RightAligned
                | ControlWord::Center
                | ControlWord::Justify      => paragraph.alignment = Alignment::from(control_word),
            ControlWord::SpaceBefore        => paragraph.spacing.before = property.get_value(),
            ControlWord::SpaceAfter         => paragraph.spacing.after = property.get_value(),
            ControlWord::SpaceBetweenLine   => paragraph.spacing.between_line = SpaceBetweenLine::from(property.get_value()),
            ControlWord::SpaceLineMul       => paragraph.spacing.line_multiplier = property.get_value(),
            ControlWord::LeftIndent         => paragraph.indent.left = property.get_value(),
            ControlWord::RightIndent        => paragraph.indent.right = property.get_value(),
            ControlWord::FirstLineIdent     => paragraph.indent.first_line = property.get_value(),
            ControlWord::UnicodeIgnoreCount => self.unicode_ignore_count = property.get_value(),
            _ => {}
        };
        Ok(())
    }
}

enum Emit {
    Nothing,
    Text(&'static str),
    Unicode(i32),
    Cell,
    Row,
    NoteNumber,
}

struct TextSink {
    document_code_page: u16,
    hex: Vec<u8>,
    high_surrogate: Option<u16>,
    pending_cell: bool,
    fallback: usize,
    notes: Vec<StyleBlock>,
    header_footer: Vec<StyleBlock>,
    note_count: u32,
}

impl TextSink {
    fn new(document_code_page: u16) -> Self {
        Self {
            document_code_page,
            hex: Vec::new(),
            high_surrogate: None,
            pending_cell: false,
            fallback: 0,
            notes: Vec::new(),
            header_footer: Vec::new(),
            note_count: 0,
        }
    }

    fn emit(&mut self, text: &str, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        self.emit_as(text, None, states, document)
    }

    fn emit_as(&mut self, text: &str, reference: Option<u32>, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        if self.high_surrogate.take().is_some() {
            self.route("\u{FFFD}", None, states, document)?;
        }
        if mem::take(&mut self.pending_cell) {
            self.route("\t", None, states, document)?;
        }
        self.route(text, reference, states, document)
    }

    fn route(&mut self, text: &str, reference: Option<u32>, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        let Some(state) = states.last() else {
            return Err(ParserError::MalformedPainterStack);
        };
        match state.destination {
            Destination::Body => Parser::add_text_to_blocks(text, state, reference.map_or(Part::Body, Part::NoteReference), &mut document.body),
            Destination::Deferred(Deferred::Note) => Parser::add_text_to_blocks(text, state, Part::Note(self.note_count), &mut self.notes),
            Destination::Deferred(Deferred::Header) => Parser::add_text_to_blocks(text, state, Part::Header, &mut self.header_footer),
            Destination::Deferred(Deferred::Footer) => Parser::add_text_to_blocks(text, state, Part::Footer, &mut self.header_footer),
            Destination::Info(field) => field.slot(&mut document.info).get_or_insert_with(String::new).push_str(text),
            Destination::Skip | Destination::InfoRoot => {}
        }
        Ok(())
    }

    fn begin_deferred(&mut self, kind: Deferred) {
        let blocks = match kind {
            Deferred::Note => {
                self.note_count = self.note_count.saturating_add(1);
                &mut self.notes
            }
            Deferred::Header | Deferred::Footer => &mut self.header_footer,
        };
        if let Some(last) = blocks.last_mut() {
            if !last.text.ends_with('\n') {
                last.text.push('\n');
            }
        }
    }

    fn note_number(&self, state: &ParserState) -> (u32, Option<u32>) {
        match state.destination {
            Destination::Deferred(Deferred::Note) => (self.note_count, None),
            Destination::Body => {
                let number = self.note_count.saturating_add(1);
                (number, Some(number))
            }
            _ => (self.note_count.saturating_add(1), None),
        }
    }

    fn take_trail_byte(&mut self, text: &str, states: &[ParserState], header: &RtfHeader) -> bool {
        match text.as_bytes().first() {
            Some(&byte) if !self.hex.is_empty() && (0x40..=0x7E).contains(&byte) && needs_trail_byte(self.code_page(states, header), &self.hex) => {
                self.hex.push(byte);
                true
            }
            _ => false,
        }
    }

    fn code_page(&self, states: &[ParserState], header: &RtfHeader) -> u16 {
        states
            .last()
            .and_then(|state| header.font_table.get(&state.painter.font_ref))
            .and_then(code_page_for_font)
            .unwrap_or(self.document_code_page)
    }

    fn flush_hex(&mut self, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        if self.hex.is_empty() {
            return Ok(());
        }
        let mut decoded = String::with_capacity(self.hex.len());
        decode_into(self.code_page(states, &document.header), &self.hex, &mut decoded);
        self.hex.clear();
        self.emit(&decoded, states, document)
    }

    fn unicode(&mut self, value: i32, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        let code = if value < 0 { value.checked_add(65_536).filter(|code| *code >= 0) } else { Some(value) };
        let code = code.and_then(|code| u32::try_from(code).ok()).unwrap_or(0xFFFD);
        match code {
            0xD800..=0xDBFF => {
                if self.high_surrogate.take().is_some() {
                    self.emit("\u{FFFD}", states, document)?;
                }
                self.high_surrogate = Some(code as u16);
                Ok(())
            }
            0xDC00..=0xDFFF => match self.high_surrogate.take() {
                Some(high) => self.emit(&String::from_utf16_lossy(&[high, code as u16]), states, document),
                None => self.emit("\u{FFFD}", states, document),
            },
            _ => {
                let symbol = (0xF020..=0xF0FF).contains(&code) && self.code_page(states, &document.header) == SYMBOL_CODE_PAGE;
                let character = if symbol { symbol_font_char((code - 0xF000) as u8) } else { char::from_u32(code) };
                let mut buffer = [0; 4];
                self.emit(character.unwrap_or('\u{FFFD}').encode_utf8(&mut buffer), states, document)
            }
        }
    }

    fn finish(&mut self, states: &[ParserState], document: &mut RtfDocument) -> Result<(), ParserError> {
        if !states.is_empty() {
            self.flush_hex(states, document)?;
            if self.high_surrogate.take().is_some() {
                self.route("\u{FFFD}", None, states, document)?;
            }
        }
        for deferred in [mem::take(&mut self.notes), mem::take(&mut self.header_footer)] {
            if deferred.is_empty() {
                continue;
            }
            match document.body.last_mut() {
                Some(last) if last.text.ends_with('\n') => {}
                Some(last) if matches!(last.part, Part::NoteReference(_)) => {
                    let paragraph = last.paragraph;
                    document.body.push(StyleBlock {
                        paragraph,
                        text: String::from("\n"),
                        ..StyleBlock::default()
                    });
                }
                Some(last) => last.text.push('\n'),
                None => {}
            }
            document.body.extend(deferred);
        }
        Ok(())
    }
}

pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    parsed_item: Vec<bool>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            parsed_item: vec![false; tokens.len()],
            tokens,
            cursor: 0,
        }
    }

    pub fn get_tokens(&self) -> Vec<&Token<'_>> {
        self.tokens.iter().filter(|t| *t != &Token::Empty).collect()
    }

    fn check_document_validity(&self) -> Result<(), ParserError> {
        if let Some(token) = self.tokens.first() {
            if token != &Token::OpeningBracket {
                return Err(ParserError::InvalidToken(format!("Invalid first token : {:?} not a '{{'", token)));
            }
        } else {
            return Err(ParserError::NoMoreToken);
        }
        if let Some(token) = self.tokens.last() {
            if token != &Token::ClosingBracket {
                return Err(ParserError::InvalidToken(format!("Invalid last token : {:?} not a '}}'", token)));
            }
        } else {
            return Err(ParserError::NoMoreToken);
        }
        Ok(())
    }

    pub fn parse(&mut self) -> Result<RtfDocument, ParserError> {
        self.check_document_validity()?;
        let mut document = RtfDocument {
            header: self.parse_header()?,
            ..RtfDocument::default()
        };
        let mut states: Vec<ParserState> = vec![ParserState::default()];
        let mut sink = TextSink::new(code_page_for_document(&document.header.character_set));
        let mut group_start = false;
        let mut ignorable = false;
        let mut seen_rtf = false;
        for index in 0..self.tokens.len() {
            if self.parsed_item[index] {
                continue;
            }
            let token = &self.tokens[index];
            let is_hex = matches!(token, Token::ControlSymbol((ControlWord::HexChar, _)));
            let mut trail_taken = false;
            if !is_hex && !sink.hex.is_empty() {
                if let Token::PlainText(text) = token {
                    trail_taken = sink.take_trail_byte(text, &states, &document.header);
                }
                sink.flush_hex(&states, &mut document)?;
            }
            // A control word (including the payload of \bin) or a control symbol is
            // one fallback character. Braces end fallback skipping at the group boundary.
            // Some writers omit the fallback between the two halves of a surrogate pair.
            let continues_surrogate = sink.high_surrogate.is_some()
                && matches!(token, Token::ControlSymbol((ControlWord::Unicode, Property::Value(value))) if matches!(*value, -9216..=-8193 | 0xDC00..=0xDFFF));
            if sink.fallback > 0 && !continues_surrogate && matches!(token, Token::ControlSymbol(_) | Token::Crlf | Token::IgnorableDestination) {
                sink.fallback -= 1;
                continue;
            }
            match token {
                Token::OpeningBracket => {
                    let state = states.last().cloned().unwrap_or_default();
                    states.push(state);
                    group_start = true;
                    ignorable = false;
                    sink.fallback = 0;
                    continue;
                }
                Token::ClosingBracket => {
                    if states.pop().is_none() {
                        return Err(ParserError::MalformedPainterStack);
                    }
                    group_start = false;
                    ignorable = false;
                    sink.fallback = 0;
                    continue;
                }
                Token::IgnorableDestination => {
                    ignorable |= group_start;
                    continue;
                }
                Token::Empty => continue,
                _ => {}
            }
            let Some(state) = states.last_mut() else {
                return Err(ParserError::MalformedPainterStack);
            };
            if mem::take(&mut group_start) {
                let parent = state.destination;
                state.open_group(token, mem::take(&mut ignorable));
                if let Destination::Deferred(kind) = state.destination {
                    if parent != state.destination {
                        sink.begin_deferred(kind);
                    }
                }
                if let (Destination::Info(field), Destination::InfoRoot) = (state.destination, parent) {
                    *field.slot(&mut document.info) = None;
                }
            }
            match token {
                Token::PlainText(text) => {
                    let mut text = if trail_taken { &text[1..] } else { *text };
                    if sink.fallback > 0 {
                        let cut = text.char_indices().nth(sink.fallback).map_or(text.len(), |(at, _)| at);
                        sink.fallback -= text[..cut].chars().count();
                        text = &text[cut..];
                    }
                    if !text.is_empty() && (seen_rtf || !text.trim().is_empty()) {
                        sink.emit(text, &states, &mut document)?;
                    }
                }
                Token::Crlf => sink.emit("\n", &states, &mut document)?,
                Token::ControlSymbol((control_word, property)) => {
                    state.apply(control_word, property)?;
                    let unicode_ignore_count = state.unicode_ignore_count;
                    #[rustfmt::skip]
                    let emit = match control_word {
                        ControlWord::Rtf               => { seen_rtf = true; Emit::Nothing }
                        ControlWord::HexChar           => {
                            if let Property::Value(byte) = property {
                                sink.hex.push(*byte as u8);
                            }
                            Emit::Nothing
                        }
                        ControlWord::Unicode           => match property {
                            Property::Value(value) => {
                                sink.fallback = usize::try_from(unicode_ignore_count).unwrap_or(0);
                                Emit::Unicode(*value)
                            }
                            _ => Emit::Nothing,
                        },
                        ControlWord::Par
                            | ControlWord::Sect
                            | ControlWord::Page
                            | ControlWord::Line        => Emit::Text("\n"),
                        ControlWord::Tab               => Emit::Text("\t"),
                        ControlWord::Cell
                            | ControlWord::NestCell    => Emit::Cell,
                        ControlWord::Row
                            | ControlWord::NestRow     => Emit::Row,
                        ControlWord::Emdash            => Emit::Text("\u{2014}"),
                        ControlWord::Endash            => Emit::Text("\u{2013}"),
                        ControlWord::Bullet            => Emit::Text("\u{2022}"),
                        ControlWord::LeftSingleQuote   => Emit::Text("\u{2018}"),
                        ControlWord::RightSingleQuote  => Emit::Text("\u{2019}"),
                        ControlWord::LeftDoubleQuote   => Emit::Text("\u{201C}"),
                        ControlWord::RightDoubleQuote  => Emit::Text("\u{201D}"),
                        ControlWord::NonBreakingSpace  => Emit::Text("\u{A0}"),
                        ControlWord::OptionalHyphen    => Emit::Text("\u{AD}"),
                        ControlWord::NonBreakingHyphen => Emit::Text("\u{2011}"),
                        ControlWord::Unknown(r"\chftn") => Emit::NoteNumber,
                        _                              => Emit::Nothing,
                    };
                    match emit {
                        Emit::Nothing => {}
                        Emit::Text(text) => sink.emit(text, &states, &mut document)?,
                        Emit::Unicode(value) => sink.unicode(value, &states, &mut document)?,
                        Emit::Cell => {
                            if mem::replace(&mut sink.pending_cell, true) {
                                sink.pending_cell = false;
                                sink.emit("\t", &states, &mut document)?;
                                sink.pending_cell = true;
                            }
                        }
                        Emit::Row => {
                            sink.pending_cell = false;
                            sink.emit("\n", &states, &mut document)?;
                        }
                        Emit::NoteNumber => {
                            if let Some((number, reference)) = states.last().map(|state| sink.note_number(state)) {
                                sink.emit_as(&number.to_string(), reference, &states, &mut document)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        sink.finish(&states, &mut document)?;
        Ok(document)
    }

    fn add_text_to_blocks(text: &str, state: &ParserState, part: Part, blocks: &mut Vec<StyleBlock>) {
        if let Some(group) = blocks.last_mut() {
            if group.part == part && group.painter == state.painter && group.paragraph == state.paragraph {
                group.text.push_str(text);
                return;
            }
        }
        blocks.push(StyleBlock {
            painter: state.painter.clone(),
            paragraph: state.paragraph,
            part,
            text: String::from(text),
        });
    }

    fn get_token_at(&'a self, index: usize) -> Option<&'a Token<'a>> {
        self.tokens.get(index)
    }

    #[inline]
    fn consume_token_at(&mut self, index: usize) -> Option<Token<'a>> {
        if self.tokens.is_empty() || index >= self.tokens.len() {
            return None;
        }
        self.cursor += 1;
        self.parsed_item[index] = true;
        Some(mem::replace(&mut self.tokens[index], Token::Empty))
    }

    fn consume_next_token(&mut self) -> Option<Token<'a>> {
        self.consume_token_at(self.cursor)
    }

    fn consume_tokens_until_matching_bracket(&mut self) -> Vec<Token<'a>> {
        let mut ret = vec![];
        let mut count = 0;
        while let Some(token) = self.consume_next_token() {
            match token {
                Token::OpeningBracket => count += 1,
                Token::ClosingBracket => count -= 1,
                _ => {}
            }
            ret.push(token);
            if count < 0 {
                break;
            }
        }
        ret
    }

    fn consume_group(&mut self) -> Vec<Token<'a>> {
        self.consume_token_at(self.cursor);
        self.consume_tokens_until_matching_bracket()
    }

    fn parse_header(&mut self) -> Result<RtfHeader, ParserError> {
        self.cursor = 0;
        let mut header = RtfHeader::default();
        while let (Some(token), Some(mut next_token)) = (self.get_token_at(self.cursor), self.get_token_at(self.cursor + 1)) {
            let mut i = 0;
            while *next_token == Token::Crlf {
                if let Some(next_token_not_crlf) = self.get_token_at(self.cursor + 1 + i) {
                    next_token = next_token_not_crlf;
                    i += 1;
                } else {
                    break;
                }
            }
            match (token, next_token) {
                (Token::OpeningBracket, header_control_word!(FontTable, None)) => {
                    let font_table_tokens = self.consume_group();
                    header.font_table = Self::parse_font_table(&font_table_tokens, code_page_for_document(&header.character_set))?;
                }
                (Token::OpeningBracket, header_control_word!(ColorTable, None)) => {
                    let color_table_tokens = self.consume_group();
                    header.color_table = Self::parse_color_table(&color_table_tokens)?;
                }
                (Token::OpeningBracket, header_control_word!(StyleSheet, None)) => {
                    let stylesheet_tokens = self.consume_group();
                    header.stylesheet = Self::parse_stylesheet(&stylesheet_tokens)?;
                }
                (token, _) => {
                    if let Some(charset) = CharacterSet::from(token) {
                        header.character_set = charset;
                    }
                    self.cursor += 1;
                }
            }
        }
        Ok(header)
    }

    fn skip_ignorable_group(tokens: &[Token<'a>], index: usize) -> Option<usize> {
        if tokens.get(index) != Some(&Token::OpeningBracket) || tokens.get(index + 1) != Some(&Token::IgnorableDestination) {
            return None;
        }
        let mut depth = 0;
        for (offset, token) in tokens[index..].iter().enumerate() {
            match token {
                Token::OpeningBracket => depth += 1,
                Token::ClosingBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index + offset + 1);
                    }
                }
                _ => {}
            }
        }
        Some(tokens.len())
    }

    fn parse_font_table(font_tables_tokens: &[Token<'a>], document_code_page: u16) -> Result<FontTable, ParserError> {
        let Some(font_table_first_token) = font_tables_tokens.first() else {
            return Err(ParserError::NoMoreToken);
        };
        if font_table_first_token != header_control_word!(FontTable, None) {
            return Err(ParserError::InvalidToken(format!("{:?} is not a FontTable token", font_table_first_token)));
        }
        let mut table = HashMap::new();
        let mut current: Option<(FontRef, Font)> = None;
        let mut name_bytes: Vec<u8> = Vec::new();
        let mut name_done = false;
        let mut index = 0;
        while index < font_tables_tokens.len() {
            if let Some(next) = Self::skip_ignorable_group(font_tables_tokens, index) {
                index = next;
                continue;
            }
            match &font_tables_tokens[index] {
                Token::ControlSymbol((ControlWord::FontNumber, property)) => {
                    Self::insert_font(&mut table, current.take(), &mut name_bytes, document_code_page);
                    let Property::Value(key) = property else {
                        return Err(ParserError::InvalidFontIdentifier(*property));
                    };
                    let key = FontRef::try_from(*key).map_err(|_| ParserError::InvalidFontIdentifier(*property))?;
                    current = Some((key, Font::default()));
                    name_done = false;
                }
                Token::ControlSymbol((ControlWord::FontCharset, property)) => {
                    if let Some((_, font)) = current.as_mut() {
                        font.character_set = u8::try_from(property.get_value()).unwrap_or(0);
                    }
                }
                Token::ControlSymbol((ControlWord::FontCodePage, Property::Value(code_page))) => {
                    if let Some((_, font)) = current.as_mut() {
                        font.code_page = u16::try_from(*code_page).ok();
                    }
                }
                Token::ControlSymbol((ControlWord::Unknown(name), _)) => {
                    if let (Some((_, font)), Some(font_family)) = (current.as_mut(), FontFamily::from(name)) {
                        font.font_family = font_family;
                    }
                }
                Token::ControlSymbol((ControlWord::HexChar, Property::Value(byte))) if !name_done => name_bytes.push(*byte as u8),
                Token::PlainText(text) if !name_done => match text.split_once(';') {
                    Some((before, _)) => {
                        name_bytes.extend_from_slice(before.as_bytes());
                        name_done = true;
                    }
                    None => name_bytes.extend_from_slice(text.as_bytes()),
                },
                _ => {}
            }
            index += 1;
        }
        Self::insert_font(&mut table, current, &mut name_bytes, document_code_page);
        Ok(table)
    }

    fn insert_font(table: &mut FontTable, entry: Option<(FontRef, Font)>, name_bytes: &mut Vec<u8>, document_code_page: u16) {
        let Some((key, mut font)) = entry else {
            name_bytes.clear();
            return;
        };
        let code_page = font.code_page.or_else(|| code_page_for_charset(font.character_set)).unwrap_or(document_code_page);
        let mut name = String::new();
        match std::str::from_utf8(name_bytes) {
            Ok(text) if text.is_ascii() || !name_bytes.iter().any(|b| *b >= 0x80 && !text.is_char_boundary(0)) => name.push_str(text),
            _ => decode_into(code_page, name_bytes, &mut name),
        }
        name_bytes.clear();
        font.name = name.trim().to_string();
        table.insert(key, font);
    }

    fn parse_color_table(color_table_tokens: &[Token<'a>]) -> Result<ColorTable, ParserError> {
        let Some(color_table_first_token) = color_table_tokens.first() else {
            return Err(ParserError::NoMoreToken);
        };
        if color_table_first_token != header_control_word!(ColorTable, None) {
            return Err(ParserError::InvalidToken(format!("ParserError: {:?} is not a ColorTable token", color_table_first_token)));
        }
        let mut table = HashMap::new();
        let mut current_key: ColorRef = 0;
        let mut current_color = Color::default();
        let mut has_color = false;
        for token in color_table_tokens.iter() {
            match token {
                Token::ControlSymbol((control_word, property)) => match control_word {
                    ControlWord::ColorRed => {
                        current_color.red = property.get_value_as::<u8>()?;
                        has_color = true;
                    }
                    ControlWord::ColorGreen => {
                        current_color.green = property.get_value_as::<u8>()?;
                        has_color = true;
                    }
                    ControlWord::ColorBlue => {
                        current_color.blue = property.get_value_as::<u8>()?;
                        has_color = true;
                    }
                    _ => {}
                },
                Token::PlainText(text) => {
                    for _ in text.matches(';') {
                        if mem::take(&mut has_color) {
                            table.insert(current_key, mem::take(&mut current_color));
                        }
                        current_key = current_key.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
        if has_color {
            table.insert(current_key, current_color);
        }
        Ok(table)
    }

    fn parse_stylesheet(stylesheet_tokens: &[Token<'a>]) -> Result<StyleSheet, ParserError> {
        let mut stylesheet = StyleSheet::new();
        let mut depth = 0;
        let mut index = 0;
        let mut entry: Option<(StyleRef, ParserState, String, bool)> = None;
        while index < stylesheet_tokens.len() {
            let token = &stylesheet_tokens[index];
            if depth == 1 {
                if let Some(next) = Self::skip_ignorable_group(stylesheet_tokens, index) {
                    index = next;
                    continue;
                }
            }
            match token {
                Token::OpeningBracket => {
                    depth += 1;
                    if depth == 1 {
                        let paragraph_style = stylesheet_tokens.get(index + 1) != Some(&Token::IgnorableDestination);
                        entry = Some((0, ParserState::default(), String::new(), paragraph_style));
                    }
                }
                Token::ClosingBracket => {
                    if depth == 1 {
                        if let Some((key, state, name, true)) = entry.take() {
                            let name = name.split(';').next().unwrap_or("").trim().to_string();
                            stylesheet.insert(
                                key,
                                Style {
                                    name,
                                    painter: state.painter,
                                    paragraph: state.paragraph,
                                },
                            );
                        }
                    }
                    depth -= 1;
                }
                Token::ControlSymbol((ControlWord::ParStyle, property)) if depth == 1 => {
                    if let Some((key, ..)) = entry.as_mut() {
                        *key = property.get_value_as::<StyleRef>()?;
                    }
                }
                Token::ControlSymbol((control_word, property)) if depth == 1 => {
                    if let Some((_, state, ..)) = entry.as_mut() {
                        state.apply(control_word, property)?;
                    }
                }
                Token::PlainText(text) if depth == 1 => {
                    if let Some((_, _, name, _)) = entry.as_mut() {
                        name.push_str(text);
                    }
                }
                _ => {}
            }
            index += 1;
        }
        Ok(stylesheet)
    }
}
