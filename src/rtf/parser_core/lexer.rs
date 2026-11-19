use std::fmt;

use super::tokens::{ControlWord, Property, Token};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LexerError {
    Error(String),
    InvalidUnicode(String),
    InvalidLastChar,
}

impl std::error::Error for LexerError {}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[RTF Lexer] : ")?;
        match self {
            LexerError::InvalidLastChar => write!(f, "Invalid last char, should be '}}'"),
            LexerError::InvalidUnicode(uc) => write!(f, "Invalid unicode : {uc}"),
            LexerError::Error(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<std::str::Utf8Error> for LexerError {
    fn from(value: std::str::Utf8Error) -> Self {
        LexerError::Error(value.to_string())
    }
}

impl From<std::num::ParseIntError> for LexerError {
    fn from(value: std::num::ParseIntError) -> Self {
        LexerError::Error(value.to_string())
    }
}

const fn special_bytes() -> [bool; 256] {
    let mut table = [false; 256];
    table[b'{' as usize] = true;
    table[b'}' as usize] = true;
    table[b'\\' as usize] = true;
    table[b'\r' as usize] = true;
    table[b'\n' as usize] = true;
    table
}

static SPECIAL: [bool; 256] = special_bytes();

pub struct Lexer;

impl Lexer {
    pub fn scan(src: &str) -> Result<Vec<Token<'_>>, LexerError> {
        let src = src.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let bytes = src.as_bytes();
        let len = bytes.len();
        let mut tokens = Vec::with_capacity(len / 32 + 1);
        let mut text_start = 0;
        let mut i = 0;
        while let Some(offset) = bytes[i..].iter().position(|&b| SPECIAL[usize::from(b)]) {
            i += offset;
            if text_start < i {
                tokens.push(Token::PlainText(&src[text_start..i]));
            }
            i = match bytes[i] {
                b'{' => {
                    tokens.push(Token::OpeningBracket);
                    i + 1
                }
                b'}' => {
                    tokens.push(Token::ClosingBracket);
                    i + 1
                }
                b'\\' => Self::escape(src, i, &mut tokens)?,
                _ => i + 1,
            };
            text_start = i;
        }
        if text_start < len {
            tokens.push(Token::PlainText(&src[text_start..len]));
        }
        Ok(tokens)
    }

    fn escape<'a>(src: &'a str, start: usize, tokens: &mut Vec<Token<'a>>) -> Result<usize, LexerError> {
        let bytes = src.as_bytes();
        let Some(&next) = bytes.get(start + 1) else {
            return Ok(start + 1);
        };
        let symbol = |word| Token::ControlSymbol((word, Property::None));
        match next {
            b'a'..=b'z' | b'A'..=b'Z' => Self::control_word(src, start, tokens),
            b'\'' => {
                let hex = src.get(start + 2..start + 4).filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()));
                let Some(hex) = hex else {
                    let found: String = src[start + 1..].chars().take(3).collect();
                    return Err(LexerError::InvalidUnicode(found));
                };
                let byte = u8::from_str_radix(hex, 16)?;
                tokens.push(Token::ControlSymbol((ControlWord::HexChar, Property::Value(byte as i32))));
                Ok(start + 4)
            }
            b'{' | b'}' | b'\\' => {
                tokens.push(Token::PlainText(&src[start + 1..start + 2]));
                Ok(start + 2)
            }
            b'\n' => {
                tokens.push(Token::Crlf);
                Ok(start + 2)
            }
            b'\r' => {
                tokens.push(Token::Crlf);
                Ok(if bytes.get(start + 2) == Some(&b'\n') { start + 3 } else { start + 2 })
            }
            b'*' => {
                tokens.push(Token::IgnorableDestination);
                Ok(start + 2)
            }
            b'~' | b'-' | b'_' | b'|' | b':' | b'\t' => {
                tokens.push(symbol(match next {
                    b'~' => ControlWord::NonBreakingSpace,
                    b'-' => ControlWord::OptionalHyphen,
                    b'_' => ControlWord::NonBreakingHyphen,
                    b'|' => ControlWord::FormulaCharacter,
                    b':' => ControlWord::IndexSubentry,
                    _ => ControlWord::Tab,
                }));
                Ok(start + 2)
            }
            _ => {
                let end = start + 1 + src[start + 1..].chars().next().map_or(1, char::len_utf8);
                if next.is_ascii() {
                    tokens.push(symbol(ControlWord::Unknown(&src[start..end])));
                }
                Ok(end)
            }
        }
    }

    fn control_word<'a>(src: &'a str, start: usize, tokens: &mut Vec<Token<'a>>) -> Result<usize, LexerError> {
        let bytes = src.as_bytes();
        let len = bytes.len();
        let mut end = start + 1;
        while end < len && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        let digits = if end + 1 < len && bytes[end] == b'-' && bytes[end + 1].is_ascii_digit() { end + 1 } else { end };
        let mut param_end = digits;
        while param_end < len && bytes[param_end].is_ascii_digit() {
            param_end += 1;
        }
        if param_end > digits {
            end = param_end;
        }
        let symbol = ControlWord::from(&src[start..end])?;
        tokens.push(Token::ControlSymbol(symbol));
        let mut next = if bytes.get(end) == Some(&b' ') { end + 1 } else { end };
        if let (ControlWord::Bin, Property::Value(count)) = symbol {
            if count > 0 {
                next = next.saturating_add(count as usize).min(len);
                while !src.is_char_boundary(next) {
                    next += 1;
                }
            }
        }
        Ok(next)
    }
}
