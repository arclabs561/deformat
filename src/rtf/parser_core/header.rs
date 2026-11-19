use std::collections::HashMap;


use super::paragraph::Paragraph;
use super::parser::Painter;
use super::tokens::{ControlWord, Property, Token};

/// The ColorRef represent the index of the color in the ColorTable
/// It's use in the document's body to reference a specific color with the \cfN or \cbN control words
pub type ColorRef = u16;
pub type ColorTable = HashMap<ColorRef, Color>;

/// The FontRef represent the index of the color in the FontTable
/// It's use in the document's body to reference a specific font with the \fN control word
pub type FontRef = u16;
pub type FontTable = HashMap<FontRef, Font>;

/// The StyleRef represent the index of the style in the StyleSheet
/// It's use in the document's body to reference a specific style with the \sN control word
pub type StyleRef = u16;
pub type StyleSheet = HashMap<StyleRef, Style>;

/// Style for the StyleSheet
#[derive(Hash, Default, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Style {
    pub name: String,
    pub painter: Painter,
    pub paragraph: Paragraph,
}

/// Information about the document, including references to fonts & styles
#[derive(Default, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RtfHeader {
    pub character_set: CharacterSet,
    pub font_table: FontTable,
    pub color_table: ColorTable,
    pub stylesheet: StyleSheet,
}

#[derive(Hash, Default, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Font {
    pub name: String,
    pub character_set: u8,
    /// An explicit font code page overrides its character set.

    pub code_page: Option<u16>,
    pub font_family: FontFamily,
}

#[derive(Hash, Default, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Default, Clone, Hash)]
#[non_exhaustive]
pub enum CharacterSet {
    #[default]
    Ansi,
    Mac,
    Pc,
    Pca,
    Ansicpg(u16),
}

impl CharacterSet {
    pub fn from(token: &Token) -> Option<Self> {
        match token {
            Token::ControlSymbol((ControlWord::Ansi, _)) => Some(Self::Ansi),
            Token::ControlSymbol((ControlWord::Mac, _)) => Some(Self::Mac),
            Token::ControlSymbol((ControlWord::Pc, _)) => Some(Self::Pc),
            Token::ControlSymbol((ControlWord::Pca, _)) => Some(Self::Pca),
            Token::ControlSymbol((ControlWord::AnsiCodePage, Property::Value(code_page))) => u16::try_from(*code_page).ok().map(Self::Ansicpg),
            _ => None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Hash, Clone, Default)]
#[non_exhaustive]
pub enum FontFamily {
    #[default]
    Nil,
    Roman,
    Swiss,
    Modern,
    Script,
    Decor,
    Tech,
    Bidi,
}

impl FontFamily {
    pub fn from(string: &str) -> Option<Self> {
        match string {
            r"\fnil" => Some(Self::Nil),
            r"\froman" => Some(Self::Roman),
            r"\fswiss" => Some(Self::Swiss),
            r"\fmodern" => Some(Self::Modern),
            r"\fscript" => Some(Self::Script),
            r"\fdecor" => Some(Self::Decor),
            r"\ftech" => Some(Self::Tech),
            r"\fbidi" => Some(Self::Bidi),
            _ => None,
        }
    }
}
