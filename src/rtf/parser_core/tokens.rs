use std::any::type_name;
use std::convert::TryFrom;
use std::fmt;

use super::lexer::LexerError;
use super::parser::ParserError;

/// Parser representation of an RTF token
#[allow(dead_code)]
#[derive(PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum Token<'a> {
    PlainText(&'a str),
    OpeningBracket,
    ClosingBracket,
    Crlf,                 // Line-return \n
    IgnorableDestination, // \*\ <destination-name>
    ControlSymbol(ControlSymbol<'a>),
    Empty, // Used by the parser for optimization
}

#[allow(dead_code)]
impl<'a> fmt::Debug for Token<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[rustfmt::skip]
        return match self {
            Token::PlainText(text)        => write!(f, r"PlainText : {:?}", *text),
            Token::OpeningBracket         => write!(f, "OpeningBracket"),
            Token::ClosingBracket         => write!(f, "ClosingBracket"),
            Token::Crlf                   => write!(f, "Crlf"),
            Token::IgnorableDestination   => write!(f, "IgnorableDestination"),
            Token::ControlSymbol(symbol)  => write!(f, "ControlSymbol : {:?}", symbol),
            Token::Empty                  => write!(f, "Empty"),
        };
    }
}

/// A control symbol is a pair (control_word, property)
/// In the RTF specification, it refers to 'control word entity'
pub type ControlSymbol<'a> = (ControlWord<'a>, Property);

/// Parameters for a control word
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Property {
    On,         // 1
    Off,        // 0
    Value(i32), // Specified as i16 in the spec 1.5 but some softwre use i32 (ex: TextEdit for unicode)
    None,       // No parameter
}

impl Property {
    pub fn as_bool(&self) -> bool {
        match self {
            Property::On => true,
            Property::Off => false,
            Property::None => true,
            Property::Value(val) => *val != 0,
        }
    }

    // Retrieve and cast the i32 value to a specific numeric type
    pub fn get_value_as<T: TryFrom<i32>>(&self) -> Result<T, ParserError> {
        let error: Result<T, ParserError> = Err(ParserError::ValueCastError(type_name::<T>().to_string()));
        if let Property::Value(value) = &self {
            return T::try_from(*value).or(error);
        }
        // If no value, returns 0
        T::try_from(0).or(error)
    }

    // Default variant
    pub fn get_value(&self) -> i32 {
        self.get_value_as::<i32>().expect("i32 to i32 conversion should never fail")
    }

    /// Return the u16 corresponding value of the unicode
    pub fn get_unicode_value(&self) -> Result<u16, ParserError> {
        // RTF control words generally accept signed 16-bit numbers as arguments.
        // For this reason, Unicode values greater than 32767 must be expressed as negative numbers.
        let mut offset = 0;
        if let Property::Value(value) = &self {
            if *value < 0 {
                offset = 65_536;
            }
            return u16::try_from(value + offset).or(Err(ParserError::UnicodeParsingError(*value)));
        }
        Err(ParserError::UnicodeParsingError(0))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlWord<'a> {
    Rtf,
    Ansi,

    Unicode,
    UnicodeIgnoreCount,

    FontTable,
    FontCharset,
    FontCodePage,
    FontNumber,
    FontSize, // Expressed in half point
    ColorNumber,

    ColorTable,
    FileTable,
    StyleSheet,

    Italic,
    Bold,
    Underline,
    UnderlineNone,
    Superscript, // 5th
    Subscript,   // H20
    Smallcaps,
    Strikethrough,

    Par,  // New paragraph
    Pard, // Resets to default paragraph properties
    Sectd,
    Plain,
    ParStyle,  // Designates paragraph style. If a paragraph style is specified, style properties must be specified with the paragraph. N references an entry in the stylesheet.
    ParDefTab, // Tab width
    // Paragraph indent
    FirstLineIdent,
    LeftIndent,
    RightIndent,
    // Paragraph alignment
    LeftAligned,
    RightAligned,
    Center,
    Justify,
    // Paragraph spacing
    SpaceBefore,
    SpaceAfter,
    SpaceBetweenLine,
    SpaceLineMul, // Line spacing multiple. Indicates that the current line spacing is a multiple of "Single" line spacing. This control word can follow only the \sl control word and works in conjunction with it.

    ColorRed,
    ColorGreen,
    ColorBlue,

    // Special characters
    Emdash,
    Endash,
    Bullet,
    LeftSingleQuote,
    RightSingleQuote,
    LeftDoubleQuote,
    RightDoubleQuote,
    Tab,
    Line,

    Page,
    Sect,
    Cell,
    Row,
    NestCell,
    NestRow,

    Hidden,
    HexChar,
    NonBreakingSpace,
    OptionalHyphen,
    NonBreakingHyphen,
    FormulaCharacter,
    IndexSubentry,

    AnsiCodePage,
    Mac,
    Pc,
    Pca,
    DefaultFont,

    Bin,
    Info,
    Upr,
    Ud,

    Unknown(&'a str),
}

const SKIPPED_DESTINATIONS: &[&str] = &[
    "bkmkend", "bkmkstart", "colorschememapping", "datafield", "datastore", "do", "docvar", "fldinst", "generator", "latentstyles",
    "listoverridetable", "listtable", "mmathPr", "nonshppict", "objclass", "objdata", "pgdsctbl", "pict", "pntxta", "pntxtb", "revtbl",
    "rsidtbl", "sp", "tc", "template", "themedata", "userprops", "xe", "xmlnstbl",
];

impl<'a> ControlWord<'a> {
    // https://www.biblioscape.com/rtf15_spec.htm
    // version 1.5 should be compatible with 1.9
    pub fn from(input: &str) -> Result<ControlSymbol<'_>, LexerError> {
        let digits = input.bytes().rev().take_while(|b| b.is_ascii_digit() || *b == b'-').count();
        let (prefix, suffix) = input.split_at(input.len() - digits);

        let property = if suffix.is_empty() {
            Property::None
        } else {
            let Ok(value) = suffix.parse::<i32>() else {
                return Err(LexerError::Error(format!("[Lexer] Unable to parse {} as integer", suffix)));
            };
            Property::Value(value)
        };

        let control_word = ControlWord::lookup(prefix);
        Ok((control_word, property))
    }

    pub(crate) fn lookup<'w>(name: &'w str) -> ControlWord<'w> {
        #[rustfmt::skip]
        let word = match name.as_bytes().get(1) {
            Some(b'a') => match name {
                r"\ansi" => ControlWord::Ansi,
                r"\ansicpg" => ControlWord::AnsiCodePage,
                _ => ControlWord::Unknown(name),
            },
            Some(b'b') => match name {
                r"\b" => ControlWord::Bold,
                r"\bin" => ControlWord::Bin,
                r"\blue" => ControlWord::ColorBlue,
                r"\bullet" => ControlWord::Bullet,
                _ => ControlWord::Unknown(name),
            },
            Some(b'c') => match name {
                r"\cpg" => ControlWord::FontCodePage,
                r"\cf" => ControlWord::ColorNumber,
                r"\cell" => ControlWord::Cell,
                r"\colortbl" => ControlWord::ColorTable,
                _ => ControlWord::Unknown(name),
            },
            Some(b'd') => match name {
                r"\deff" => ControlWord::DefaultFont,
                _ => ControlWord::Unknown(name),
            },
            Some(b'e') => match name {
                r"\emdash" => ControlWord::Emdash,
                r"\endash" => ControlWord::Endash,
                _ => ControlWord::Unknown(name),
            },
            Some(b'f') => match name {
                r"\f" => ControlWord::FontNumber,
                r"\fi" => ControlWord::FirstLineIdent,
                r"\fs" => ControlWord::FontSize,
                r"\filetbl" => ControlWord::FileTable,
                r"\fonttbl" => ControlWord::FontTable,
                r"\fcharset" => ControlWord::FontCharset,
                _ => ControlWord::Unknown(name),
            },
            Some(b'g') => match name {
                r"\green" => ControlWord::ColorGreen,
                _ => ControlWord::Unknown(name),
            },
            Some(b'i') => match name {
                r"\i" => ControlWord::Italic,
                r"\info" => ControlWord::Info,
                _ => ControlWord::Unknown(name),
            },
            Some(b'l') => match name {
                r"\li" => ControlWord::LeftIndent,
                r"\line" => ControlWord::Line,
                r"\lquote" => ControlWord::LeftSingleQuote,
                r"\ldblquote" => ControlWord::LeftDoubleQuote,
                _ => ControlWord::Unknown(name),
            },
            Some(b'm') => match name {
                r"\mac" => ControlWord::Mac,
                _ => ControlWord::Unknown(name),
            },
            Some(b'n') => match name {
                r"\nestrow" => ControlWord::NestRow,
                r"\nestcell" => ControlWord::NestCell,
                _ => ControlWord::Unknown(name),
            },
            Some(b'p') => match name {
                r"\pc" => ControlWord::Pc,
                r"\par" => ControlWord::Par,
                r"\pca" => ControlWord::Pca,
                r"\page" => ControlWord::Page,
                r"\pard" => ControlWord::Pard,
                r"\plain" => ControlWord::Plain,
                r"\pardeftab" => ControlWord::ParDefTab,
                _ => ControlWord::Unknown(name),
            },
            Some(b'q') => match name {
                r"\qc" => ControlWord::Center,
                r"\qj" => ControlWord::Justify,
                r"\ql" => ControlWord::LeftAligned,
                r"\qr" => ControlWord::RightAligned,
                _ => ControlWord::Unknown(name),
            },
            Some(b'r') => match name {
                r"\ri" => ControlWord::RightIndent,
                r"\red" => ControlWord::ColorRed,
                r"\row" => ControlWord::Row,
                r"\rtf" => ControlWord::Rtf,
                r"\rquote" => ControlWord::RightSingleQuote,
                r"\rdblquote" => ControlWord::RightDoubleQuote,
                _ => ControlWord::Unknown(name),
            },
            Some(b's') => match name {
                r"\s" => ControlWord::ParStyle,
                r"\sa" => ControlWord::SpaceAfter,
                r"\sb" => ControlWord::SpaceBefore,
                r"\sl" => ControlWord::SpaceBetweenLine,
                r"\sub" => ControlWord::Subscript,
                r"\sect" => ControlWord::Sect,
                r"\scaps" => ControlWord::Smallcaps,
                r"\sectd" => ControlWord::Sectd,
                r"\slmul" => ControlWord::SpaceLineMul,
                r"\super" => ControlWord::Superscript,
                r"\slmult" => ControlWord::SpaceLineMul,
                r"\strike" => ControlWord::Strikethrough,
                r"\stylesheet" => ControlWord::StyleSheet,
                _ => ControlWord::Unknown(name),
            },
            Some(b't') => match name {
                r"\tab" => ControlWord::Tab,
                _ => ControlWord::Unknown(name),
            },
            Some(b'u') => match name {
                r"\u" => ControlWord::Unicode,
                r"\uc" => ControlWord::UnicodeIgnoreCount,
                r"\ud" => ControlWord::Ud,
                r"\ul" => ControlWord::Underline,
                r"\upr" => ControlWord::Upr,
                r"\ulnone" => ControlWord::UnderlineNone,
                _ => ControlWord::Unknown(name),
            },
            Some(b'v') => match name {
                r"\v" => ControlWord::Hidden,
                _ => ControlWord::Unknown(name),
            },
            _ => ControlWord::Unknown(name),
        };
        word
    }

    pub(crate) fn skips_group(&self) -> bool {
        match self {
            ControlWord::FontTable | ControlWord::ColorTable | ControlWord::StyleSheet | ControlWord::FileTable => true,
            ControlWord::Unknown(name) => SKIPPED_DESTINATIONS.contains(&name.trim_start_matches('\\')),
            _ => false,
        }
    }
}
