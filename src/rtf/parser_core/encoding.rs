use super::codepages::{single_byte_table, SYMBOL_FONT};
use super::header::{CharacterSet, Font};

pub(crate) const DEFAULT_CODE_PAGE: u16 = 1252;
pub(crate) const SYMBOL_CODE_PAGE: u16 = 42;

pub(crate) fn code_page_for_font(font: &Font) -> Option<u16> {
    if let Some(code_page) = font.code_page {
        return Some(code_page);
    }
    if font.character_set == 2 && font.name.eq_ignore_ascii_case("Symbol") {
        return Some(SYMBOL_CODE_PAGE);
    }
    code_page_for_charset(font.character_set)
}

pub(crate) fn symbol_font_char(byte: u8) -> Option<char> {
    let mapped = SYMBOL_FONT.get(usize::from(byte).checked_sub(0x20)?)?;
    char::from_u32(u32::from(*mapped))
}

pub(crate) fn code_page_for_charset(charset: u8) -> Option<u16> {
    match charset {
        77 => Some(10000),
        128 => Some(932),
        129 => Some(949),
        130 => Some(1361),
        134 => Some(936),
        136 => Some(950),
        161 => Some(1253),
        162 => Some(1254),
        163 => Some(1258),
        177 => Some(1255),
        178 => Some(1256),
        186 => Some(1257),
        204 => Some(1251),
        222 => Some(874),
        238 => Some(1250),
        254 => Some(437),
        255 => Some(850),
        _ => None,
    }
}

pub(crate) fn code_page_for_document(character_set: &CharacterSet) -> u16 {
    match character_set {
        CharacterSet::Ansi => DEFAULT_CODE_PAGE,
        CharacterSet::Mac => 10000,
        CharacterSet::Pc => 437,
        CharacterSet::Pca => 850,
        CharacterSet::Ansicpg(code_page) => *code_page,
    }
}

#[cfg(feature = "rtf")]
fn external_encoding(code_page: u16) -> Option<&'static encoding_rs::Encoding> {
    match code_page {
        932 => Some(encoding_rs::SHIFT_JIS),
        936 => Some(encoding_rs::GBK),
        949 => Some(encoding_rs::EUC_KR),
        950 => Some(encoding_rs::BIG5),
        20866 => Some(encoding_rs::KOI8_R),
        21866 => Some(encoding_rs::KOI8_U),
        28591..=28606 => encoding_rs::Encoding::for_label(format!("iso-8859-{}", code_page - 28590).as_bytes()),
        _ => None,
    }
}

#[cfg(feature = "rtf")]
pub(crate) fn needs_trail_byte(code_page: u16, bytes: &[u8]) -> bool {
    let lead = |byte: u8| match code_page {
        932 => matches!(byte, 0x81..=0x9F | 0xE0..=0xFC),
        936 | 949 | 950 => matches!(byte, 0x81..=0xFE),
        _ => false,
    };
    let mut index = 0;
    while index < bytes.len() {
        index += if lead(bytes[index]) { 2 } else { 1 };
    }
    index > bytes.len()
}

#[cfg(not(feature = "rtf"))]
pub(crate) fn needs_trail_byte(_code_page: u16, _bytes: &[u8]) -> bool {
    false
}

pub(crate) fn decode_into(code_page: u16, bytes: &[u8], out: &mut String) {
    if code_page == 65001 {
        out.push_str(&String::from_utf8_lossy(bytes));
        return;
    }
    #[cfg(feature = "rtf")]
    if let Some(encoding) = external_encoding(code_page) {
        out.push_str(&encoding.decode_without_bom_handling(bytes).0);
        return;
    }
    if code_page == SYMBOL_CODE_PAGE {
        out.extend(bytes.iter().map(|&byte| symbol_font_char(byte).unwrap_or(byte as char)));
        return;
    }
    #[cfg(not(feature = "rtf"))]
    if matches!(code_page, 932 | 936 | 949 | 950) {
        out.extend(bytes.iter().map(|&byte| char::from(byte)));
        return;
    }
    let table = match single_byte_table(code_page) {
        Some(table) => table,
        #[cfg(feature = "rtf")]
        None => {
            out.push_str(&encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0);
            return;
        }
        #[cfg(not(feature = "rtf"))]
        None => &super::codepages::CP1252,
    };
    for &byte in bytes {
        out.push(if byte.is_ascii() {
            char::from(byte)
        } else {
            char::from_u32(u32::from(table[usize::from(byte - 0x80)])).unwrap_or('\u{FFFD}')
        });
    }
}

pub(crate) fn decode_document_bytes(bytes: &[u8]) -> String {
    let code_page = document_code_page(bytes);
    let mut out = String::with_capacity(bytes.len());
    let mut run_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1..].starts_with(b"bin") {
            let digits_start = i + 4;
            let digits_end = digits_start + bytes[digits_start..].iter().take_while(|b| b.is_ascii_digit()).count();
            if digits_end > digits_start {
                // Decode through the candidate backslash. It may be a DBCS trail
                // byte, or pair with a preceding backslash as a literal RTF escape.
                decode_into(code_page, &bytes[run_start..=i], &mut out);
                run_start = i + 1;
                let slashes = out.bytes().rev().take_while(|&byte| byte == b'\\').count();
                if slashes % 2 == 0 {
                    i = digits_end;
                    continue;
                }
                out.push_str("bin0 ");
                let count: usize = std::str::from_utf8(&bytes[digits_start..digits_end]).ok().and_then(|n| n.parse().ok()).unwrap_or(0);
                let payload = if bytes.get(digits_end) == Some(&b' ') { digits_end + 1 } else { digits_end };
                i = payload.saturating_add(count).min(bytes.len());
                run_start = i;
                continue;
            }
        }
        i += 1;
    }
    decode_into(code_page, &bytes[run_start..], &mut out);
    out
}

fn document_code_page(bytes: &[u8]) -> u16 {
    let header = &bytes[..bytes.len().min(4096)];
    if let Some(at) = find(header, b"\\ansicpg") {
        let digits: String = header[at + 8..].iter().take_while(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
        if let Ok(code_page) = digits.parse() {
            return code_page;
        }
    }
    let word = |name: &[u8]| {
        find(header, name).is_some_and(|at| !matches!(header.get(at + name.len()), Some(b) if b.is_ascii_alphanumeric()))
    };
    if word(b"\\mac") {
        10000
    } else if word(b"\\pca") {
        850
    } else if word(b"\\pc") {
        437
    } else {
        DEFAULT_CODE_PAGE
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
