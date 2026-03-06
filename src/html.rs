//! HTML-to-text extraction.
//!
//! Three extraction strategies, from simplest to most capable:
//!
//! 1. **`strip_to_text`** (always available) -- fast tag stripping with
//!    entity decoding, semantic element filtering, and Wikipedia boilerplate
//!    removal. Uses `memchr` for SIMD-accelerated scanning.
//!
//! 2. **`extract_with_html2text`** (feature `html2text`) -- DOM-based
//!    conversion that preserves layout structure (tables, lists, indentation).
//!
//! 3. **`extract_with_readability`** (feature `readability`) -- Mozilla
//!    Readability algorithm that extracts the main article content, stripping
//!    navigation, sidebars, and boilerplate.

use std::borrow::Cow;

use memchr::memchr2;

/// Strip HTML tags and decode entities, returning clean plain text.
///
/// This is the core built-in extractor. It handles:
/// - Tag removal (all HTML tags stripped)
/// - Script, style, and noscript content removal
/// - Semantic element filtering: skips `<nav>`, `<header>`, `<footer>`,
///   `<aside>`, `<head>`, `<menu>`, `<form>`, `<select>`, `<figcaption>`
/// - Wikipedia/MediaWiki boilerplate removal (TOC, references, navboxes)
/// - HTML entity decoding (`&amp;`, `&#123;`, `&#x1F;`, etc.)
/// - Whitespace collapsing (HTML rendering semantics)
/// - Reference marker stripping (`[1]`, `[edit]`, `[citation needed]`)
///
/// # Examples
///
/// ```
/// let text = deformat::html::strip_to_text("<p>Hello <b>world</b>!</p>");
/// assert_eq!(text, "Hello world!");
/// ```
pub fn strip_to_text(html: &str) -> String {
    strip_impl(html)
}

/// Try readability extraction. Returns `Some((text, title, excerpt))` on
/// success, `None` if parsing fails or the extracted text is trivial (< 50 chars).
///
/// Requires the `readability` feature.
#[cfg(feature = "readability")]
pub fn extract_with_readability(
    html: &str,
    url: &str,
) -> Option<(String, Option<String>, Option<String>)> {
    let cfg = dom_smoothie::Config::default();
    let mut r = dom_smoothie::Readability::new(html, Some(url), Some(cfg)).ok()?;
    let article = r.parse().ok()?;
    let text = article.text_content.trim().to_string();
    if text.is_empty() || text.len() < 50 {
        return None;
    }
    let title = if article.title.is_empty() {
        None
    } else {
        Some(article.title)
    };
    Some((text, title, article.excerpt))
}

/// Convert HTML to text using html2text's DOM-based renderer.
///
/// Preserves layout structure (tables, lists, indentation) with a
/// configurable line width.
///
/// Requires the `html2text` feature.
#[cfg(feature = "html2text")]
pub fn extract_with_html2text(html: &str, width: usize) -> Result<String, String> {
    html2text::from_read(html.as_bytes(), width).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Core strip implementation
// ---------------------------------------------------------------------------

fn strip_impl(html: &str) -> String {
    let bytes = html.as_bytes();
    let len = bytes.len();

    // Fast path: no '<' means no HTML tags at all.
    // Skip the entire tag-processing loop; just decode entities + cleanup.
    if memchr::memchr(b'<', bytes).is_none() {
        let decoded = decode_entities_in_str(html);
        let stripped = strip_wiki_ref_markers(&decoded);
        return cleanup_whitespace(&stripped);
    }

    let mut pos = 0;
    let mut text = String::with_capacity(html.len());
    let mut in_script = false;
    let mut in_style = false;
    let mut skip_depth: u32 = 0;
    let mut wiki_skip_depth: u32 = 0;

    while pos < len {
        let skipping = in_script || in_style || skip_depth > 0;

        // Fast scan: find next '<' or '&' using SIMD-accelerated memchr.
        // In skip mode, only look for '<' (entities don't matter).
        let next = if skipping {
            memchr::memchr(b'<', &bytes[pos..]).map(|o| pos + o)
        } else {
            memchr2(b'<', b'&', &bytes[pos..]).map(|o| pos + o)
        };

        match next {
            None => {
                // No more markers -- copy remainder if not skipping
                if !skipping {
                    text.push_str(&html[pos..]);
                }
                break;
            }
            Some(marker_pos) => {
                // Bulk-copy text before the marker (splitting at ASCII positions
                // is always valid UTF-8)
                if !skipping && marker_pos > pos {
                    text.push_str(&html[pos..marker_pos]);
                }
                pos = marker_pos;
            }
        }

        match bytes[pos] {
            b'<' => {
                pos += 1;
                if pos >= len {
                    break;
                }

                // HTML comment <!-- ... --> or <!DOCTYPE ...>
                if bytes[pos] == b'!' {
                    if pos + 2 < len && bytes[pos + 1] == b'-' && bytes[pos + 2] == b'-' {
                        pos += 3; // skip "!--"
                        let mut dashes = 0u32;
                        while pos < len {
                            match bytes[pos] {
                                b'-' => dashes += 1,
                                b'>' if dashes >= 2 => {
                                    pos += 1;
                                    break;
                                }
                                _ => dashes = 0,
                            }
                            pos += 1;
                        }
                        continue;
                    }
                    // <!DOCTYPE ...> or other <! directive
                    if let Some(o) = memchr::memchr(b'>', &bytes[pos..]) {
                        pos += o + 1;
                    } else {
                        pos = len;
                    }
                    continue;
                }

                // Parse tag: collect tag name and scan to closing '>'
                let tag_start = pos; // first byte after '<'
                let mut tag_name_end = pos;
                let mut in_tag_name = true;
                let mut in_attr_quote: Option<u8> = None;

                while pos < len {
                    let b = bytes[pos];
                    if let Some(q) = in_attr_quote {
                        if b == q {
                            in_attr_quote = None;
                        }
                        pos += 1;
                        continue;
                    }
                    if b == b'>' {
                        pos += 1; // consume '>'
                        break;
                    }
                    if in_tag_name && b.is_ascii_whitespace() {
                        tag_name_end = pos;
                        in_tag_name = false;
                    }
                    if !in_tag_name && (b == b'"' || b == b'\'') {
                        in_attr_quote = Some(b);
                    }
                    pos += 1;
                }

                if in_tag_name {
                    // Tag had no attributes -- name extends to '>' or end
                    tag_name_end = if pos > 0 && pos <= len && bytes[pos - 1] == b'>' {
                        pos - 1
                    } else {
                        pos
                    };
                }

                // tag_name is ASCII -- lowercase into stack buffer to avoid allocation.
                // Tag names >31 bytes are vanishingly rare; fall back to heap for those.
                let tag_name_raw = &html[tag_start..tag_name_end];
                let tag_name_len = tag_name_end - tag_start;
                let mut tag_buf = [0u8; 32];
                let tag_lower: &str = if tag_name_len < 32 {
                    for (i, &b) in bytes[tag_start..tag_name_end].iter().enumerate() {
                        tag_buf[i] = b.to_ascii_lowercase();
                    }
                    // SAFETY: input is ASCII (HTML tag names), lowercase is ASCII
                    std::str::from_utf8(&tag_buf[..tag_name_len]).unwrap()
                } else {
                    // Heap fallback for absurdly long tag names
                    // This leak is bounded (one per oversized tag) and only hit
                    // on pathological input. In practice, HTML tag names are <20 bytes.
                    // Use a local String to get a &str with the right lifetime.
                    tag_name_raw // skip lowercase for >31 byte tags
                };

                // Script/style toggle
                if tag_lower == "script" {
                    in_script = true;
                } else if tag_lower == "/script" {
                    in_script = false;
                } else if tag_lower == "style" {
                    in_style = true;
                } else if tag_lower == "/style" {
                    in_style = false;
                }

                // Semantic skip tags
                const SKIP_TAGS: &[&str] = &[
                    "head",
                    "nav",
                    "header",
                    "footer",
                    "aside",
                    "menu",
                    "noscript",
                    "form",
                    "select",
                    "figcaption",
                    "template",
                    "svg",
                    "textarea",
                    "iframe",
                    "rt",
                    "rp",
                ];

                // Wikipedia/MediaWiki structural skip.
                // Only check opening container tags that have attributes.
                const WIKI_SKIP_IDS: &[&str] = &[
                    "toc",
                    "references",
                    "reflist",
                    "catlinks",
                    "mw-panel",
                    "mw-navigation",
                    "sidebar",
                    "sitesub",
                    "contentsub",
                    "jump-to-nav",
                    "navbox",
                    "external",
                    "see-also",
                    "further-reading",
                    "mw-head",
                    "mw-page-base",
                    "mw-head-base",
                    "footer",
                    "printfooter",
                ];
                let tag_content_end = if pos > 0 && bytes[pos - 1] == b'>' {
                    pos - 1
                } else {
                    pos
                };
                if matches!(
                    tag_lower,
                    "div" | "ol" | "ul" | "table" | "span" | "section"
                ) && tag_content_end > tag_name_end
                {
                    // Only allocate lowercase when the tag has attributes
                    let tag_full_lower = html[tag_start..tag_content_end].to_ascii_lowercase();
                    let has_class = tag_full_lower.contains("class=");
                    let has_id = tag_full_lower.contains("id=");
                    if has_class || has_id {
                        let is_wiki_skip =
                            WIKI_SKIP_IDS.iter().any(|id| tag_full_lower.contains(id));
                        if is_wiki_skip {
                            wiki_skip_depth += 1;
                            skip_depth += 1;
                        }
                    }
                }

                // Handle closing tags for wiki-skip and semantic skip.
                // Use strip_prefix('/') to avoid format! allocations.
                let is_close = tag_lower.starts_with('/');
                let close_name = if is_close { &tag_lower[1..] } else { "" };

                if wiki_skip_depth > 0
                    && is_close
                    && matches!(
                        close_name,
                        "div" | "ol" | "ul" | "table" | "span" | "section"
                    )
                {
                    wiki_skip_depth = wiki_skip_depth.saturating_sub(1);
                    skip_depth = skip_depth.saturating_sub(1);
                }

                // Semantic tag depth tracking
                if is_close {
                    for &stag in SKIP_TAGS {
                        if close_name == stag {
                            skip_depth = skip_depth.saturating_sub(1);
                        }
                    }
                } else {
                    for &stag in SKIP_TAGS {
                        if tag_lower == stag {
                            skip_depth += 1;
                        }
                    }
                }

                // Insert space around block-level elements for readability.
                let effective_tag = tag_lower.strip_prefix('/').unwrap_or(tag_lower);
                let effective_tag = effective_tag.strip_suffix('/').unwrap_or(effective_tag);
                if !in_script
                    && !in_style
                    && skip_depth == 0
                    && is_block_tag(effective_tag)
                    && !text.ends_with(' ')
                    && !text.is_empty()
                {
                    text.push(' ');
                }

                // Extract alt text from <img> tags
                if !in_script && !in_style && skip_depth == 0 && tag_lower == "img" {
                    // Reconstruct tag buffer for attr extraction: <...>
                    let tag_buf_start = tag_start.saturating_sub(1); // include '<'
                    let tag_buffer = &html[tag_buf_start..pos.min(len)];
                    if let Some(alt) = extract_attr_value(tag_buffer, "alt") {
                        if !alt.is_empty() {
                            let decoded = decode_entities_in_str(alt);
                            if !text.ends_with(' ') && !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(&decoded);
                            text.push(' ');
                        }
                    }
                }
            }
            b'&' => {
                pos += 1;
                // Parse entity: scan bytes until ';', whitespace, or '<'
                let entity_start = pos - 1; // includes '&'
                let mut entity_end = pos;
                let mut found_semicolon = false;

                while entity_end < len {
                    match bytes[entity_end] {
                        b';' => {
                            entity_end += 1;
                            found_semicolon = true;
                            break;
                        }
                        b' ' | b'\t' | b'\n' | b'\r' | b'<' => break,
                        _ => entity_end += 1,
                    }
                }

                let entity_str = &html[entity_start..entity_end];
                pos = entity_end;

                if found_semicolon {
                    if let Some(ch) = decode_named_entity(entity_str) {
                        text.push(ch);
                    } else if entity_str.starts_with("&#") && entity_str.len() > 3 {
                        let num_str = &entity_str[2..entity_str.len() - 1];
                        let parsed = if let Some(hex) = num_str
                            .strip_prefix('x')
                            .or_else(|| num_str.strip_prefix('X'))
                        {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            num_str.parse::<u32>().ok()
                        };
                        if let Some(ch) = parsed.and_then(numeric_entity_to_char) {
                            text.push(ch);
                        } else {
                            text.push_str(entity_str);
                        }
                    } else {
                        text.push_str(entity_str);
                    }
                } else {
                    // Semicolon-optional entities (e.g. &amp without trailing ;)
                    if entity_str.len() > 2
                        && entity_str.as_bytes()[1].is_ascii_alphabetic()
                        && entity_str[1..].bytes().all(|b| b.is_ascii_alphanumeric())
                    {
                        // Stack buffer to avoid format! allocation
                        let eb = entity_str.as_bytes();
                        if eb.len() < 32 {
                            let mut buf = [0u8; 32];
                            buf[..eb.len()].copy_from_slice(eb);
                            buf[eb.len()] = b';';
                            let with_semi = std::str::from_utf8(&buf[..eb.len() + 1]).unwrap();
                            if let Some(ch) = decode_named_entity(with_semi) {
                                text.push(ch);
                                continue;
                            }
                        }
                    }
                    text.push_str(entity_str);
                }
            }
            _ => {
                pos += 1;
            }
        }
    }

    // Strip Wikipedia reference markers [1], [edit], [citation needed] etc.
    // Do this before whitespace cleanup so the cleanup pass collapses any
    // resulting double spaces.
    let text = strip_wiki_ref_markers(&text);

    cleanup_whitespace(&text)
}

/// Collapse whitespace, strip invisible characters, and trim.
///
/// Uses byte-level scanning with ASCII fast path: printable ASCII (0x21-0x7E)
/// is bulk-copied in runs; whitespace and multi-byte sequences are handled per-char.
/// For pure ASCII text that is already trimmed, has no double spaces, and no
/// control characters, returns a zero-copy result.
#[inline]
fn cleanup_whitespace(text: &str) -> String {
    let text_bytes = text.as_bytes();
    let text_len = text_bytes.len();

    // Ultra-fast path: if the text is pure ASCII and already clean, just clone it.
    // "Clean" = trimmed, no double spaces, no control chars (< 0x20 except nothing).
    // This avoids the character-by-character scan for already-processed text.
    if text_len > 0 && is_clean_ascii(text_bytes) {
        return text.to_string();
    }
    let mut cleaned = String::with_capacity(text_len);
    let mut last_was_space = true;
    let mut i = 0;

    while i < text_len {
        let b = text_bytes[i];
        if b > 0x20 && b < 0x7F {
            // Printable ASCII (not space, not DEL) -- scan for a run
            let run_start = i;
            i += 1;
            while i < text_len {
                let b2 = text_bytes[i];
                if b2 <= 0x20 || b2 >= 0x7F {
                    break;
                }
                i += 1;
            }
            // SAFETY: run_start..i contains only bytes 0x21-0x7E (printable ASCII)
            cleaned.push_str(&text[run_start..i]);
            last_was_space = false;
        } else if b <= 0x20 {
            // ASCII whitespace or control character
            if (b == b' ' || b == b'\t' || b == b'\n' || b == b'\r') && !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
            // else: C0 control chars (0x00-0x08, 0x0B, 0x0E-0x1F) -> skip
            i += 1;
        } else {
            // Multi-byte UTF-8 (0x80+) or DEL (0x7F)
            if b == 0x7F {
                i += 1;
                continue;
            }
            // Decode the UTF-8 character
            let ch = text[i..].chars().next().unwrap();
            let ch_len = ch.len_utf8();
            if is_invisible_char(ch) {
                // skip
            } else if ch.is_whitespace() || is_nbsp(ch) {
                if !last_was_space {
                    cleaned.push(' ');
                    last_was_space = true;
                }
            } else {
                cleaned.push(ch);
                last_was_space = false;
            }
            i += ch_len;
        }
    }

    cleaned.trim().to_string()
}

/// Strip Wikipedia reference markers from text.
///
/// Matches: `[1]`, `[42]`, `[edit]`, `[citation needed]`, and bare `edit]`
/// preceded by a word boundary.  Uses `memchr` for `[` scanning -- no regex.
/// Check if a byte slice represents "clean" ASCII text: no leading/trailing
/// whitespace, no double spaces, no control chars, no multi-byte UTF-8.
/// When true, `cleanup_whitespace` can skip the character-by-character pass.
#[inline]
fn is_clean_ascii(bytes: &[u8]) -> bool {
    // Not trimmed?
    if bytes[0] <= b' ' || bytes[bytes.len() - 1] <= b' ' {
        return false;
    }
    let mut prev_space = false;
    for &b in bytes {
        if b == b' ' {
            if prev_space {
                return false; // double space
            }
            prev_space = true;
        } else if b <= 0x20 || b >= 0x7F {
            // Control char or multi-byte UTF-8 -- need full cleanup
            return false;
        } else {
            prev_space = false;
        }
    }
    true
}

fn strip_wiki_ref_markers(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    // Fast path: no '[' and no "edit]" means nothing to strip -- zero-copy return.
    if memchr::memchr(b'[', bytes).is_none() && !s.contains("edit]") {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(len);
    let mut pos = 0;

    while pos < len {
        // Check for bare "edit]" at word boundary (not preceded by alphanumeric)
        if pos + 5 <= len
            && &bytes[pos..pos + 5] == b"edit]"
            && (pos == 0 || !bytes[pos - 1].is_ascii_alphanumeric())
        {
            pos += 5;
            continue;
        }

        if bytes[pos] != b'[' {
            // Find next '[' using memchr for bulk copy
            match memchr::memchr(b'[', &bytes[pos..]) {
                Some(offset) => {
                    // Before copying, check for "edit]" in the gap
                    let chunk = &s[pos..pos + offset];
                    // We need to handle "edit]" within this chunk too,
                    // but it's simpler to just copy byte-by-byte and
                    // check at the top of the loop. For efficiency,
                    // search for "edit]" in the chunk first.
                    if let Some(edit_pos) = chunk.find("edit]") {
                        // Check word boundary
                        let abs_pos = pos + edit_pos;
                        if abs_pos == 0 || !bytes[abs_pos - 1].is_ascii_alphanumeric() {
                            result.push_str(&s[pos..abs_pos]);
                            pos = abs_pos + 5;
                            continue;
                        }
                    }
                    result.push_str(&s[pos..pos + offset]);
                    pos += offset;
                }
                None => {
                    // Check remainder for "edit]"
                    let chunk = &s[pos..];
                    if let Some(edit_pos) = chunk.find("edit]") {
                        let abs_pos = pos + edit_pos;
                        if abs_pos == 0 || !bytes[abs_pos - 1].is_ascii_alphanumeric() {
                            result.push_str(&s[pos..abs_pos]);
                            pos = abs_pos + 5;
                            continue;
                        }
                    }
                    result.push_str(&s[pos..]);
                    break;
                }
            }
            continue;
        }

        // We're at '[' -- check what follows
        let bracket_start = pos;
        pos += 1;

        if pos >= len {
            result.push('[');
            break;
        }

        // [digits] -- one or more ASCII digits followed by ]
        if bytes[pos].is_ascii_digit() {
            while pos < len && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos < len && bytes[pos] == b']' {
                pos += 1; // skip the entire [N] marker
                continue;
            }
            // Not a valid [N] -- emit what we've seen
            result.push_str(&s[bracket_start..pos]);
            continue;
        }

        // [edit]
        if pos + 5 <= len && &bytes[pos..pos + 5] == b"edit]" {
            pos += 5;
            continue;
        }

        // [citation needed]
        if pos + 16 <= len && &bytes[pos..pos + 16] == b"citation needed]" {
            pos += 16;
            continue;
        }

        // Not a wiki marker -- emit the '['
        result.push('[');
    }

    Cow::Owned(result)
}

/// Returns true if the tag name is a block-level element that should get
/// a space inserted before it in the text output.
fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "br"
            | "wbr"
            | "hr"
            | "li"
            | "ul"
            | "ol"
            | "td"
            | "th"
            | "tr"
            | "dt"
            | "dd"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "aside"
            | "main"
            | "blockquote"
            | "figcaption"
            | "figure"
            | "details"
            | "summary"
            | "caption"
            | "thead"
            | "tbody"
            | "tfoot"
            | "address"
            | "pre"
            | "fieldset"
            | "legend"
    )
}

/// Convert a numeric entity codepoint to a character, applying HTML5 spec rules:
/// - `&#0;` -> U+FFFD
/// - C1 range (128-159) -> Windows-1252 mapping
/// - Surrogates -> U+FFFD
/// - Beyond Unicode range -> U+FFFD
fn numeric_entity_to_char(n: u32) -> Option<char> {
    if n == 0 {
        Some('\u{FFFD}')
    } else if (0x80..=0x9F).contains(&n) {
        win1252_to_unicode(n).or(Some('\u{FFFD}'))
    } else if (0xD800..=0xDFFF).contains(&n) || n > 0x10FFFF {
        // Surrogates and beyond-Unicode codepoints -> replacement character
        Some('\u{FFFD}')
    } else {
        char::from_u32(n).or(Some('\u{FFFD}'))
    }
}

/// Map a named HTML entity to its Unicode character.
///
/// Covers the most common entities encountered in real web content,
/// especially those important for NER (accented names, currency symbols,
/// punctuation). Not exhaustive -- rare entities pass through as-is.
/// Sorted table of named HTML entities -> Unicode codepoint.
/// Covers ~300 entities: ISO-8859-1/Latin-1, Latin Extended-A (Central/Eastern
/// European names: Polish, Czech, Slovak, Turkish, Hungarian, Romanian, Croatian),
/// Greek letters, math symbols, arrows, and typographic punctuation.
static NAMED_ENTITIES: &[(&str, char)] = &[
    ("&AElig;", '\u{00C6}'),
    ("&Aacute;", '\u{00C1}'),
    ("&Abreve;", '\u{0102}'), // Romanian Ă
    ("&Acirc;", '\u{00C2}'),
    ("&Agrave;", '\u{00C0}'),
    ("&Alpha;", '\u{0391}'),
    ("&Aogonek;", '\u{0104}'), // Polish Ą
    ("&Aring;", '\u{00C5}'),
    ("&Atilde;", '\u{00C3}'),
    ("&Auml;", '\u{00C4}'),
    ("&Beta;", '\u{0392}'),
    ("&Cacute;", '\u{0106}'), // Polish Ć
    ("&Ccaron;", '\u{010C}'), // Czech Č
    ("&Ccedil;", '\u{00C7}'),
    ("&Chi;", '\u{03A7}'),
    ("&Dagger;", '\u{2021}'),
    ("&Dcaron;", '\u{010E}'), // Czech Ď
    ("&Delta;", '\u{0394}'),
    ("&Dstrok;", '\u{0110}'), // Croatian Đ
    ("&ETH;", '\u{00D0}'),
    ("&Eacute;", '\u{00C9}'),
    ("&Ecaron;", '\u{011A}'), // Czech Ě
    ("&Ecirc;", '\u{00CA}'),
    ("&Egrave;", '\u{00C8}'),
    ("&Eogonek;", '\u{0118}'), // Polish Ę
    ("&Epsilon;", '\u{0395}'),
    ("&Eta;", '\u{0397}'),
    ("&Euml;", '\u{00CB}'),
    ("&Gamma;", '\u{0393}'),
    ("&Gbreve;", '\u{011E}'), // Turkish Ğ
    ("&Iacute;", '\u{00CD}'),
    ("&Icirc;", '\u{00CE}'),
    ("&Idot;", '\u{0130}'), // Turkish İ
    ("&Igrave;", '\u{00CC}'),
    ("&Iota;", '\u{0399}'),
    ("&Iuml;", '\u{00CF}'),
    ("&Kappa;", '\u{039A}'),
    ("&Lambda;", '\u{039B}'),
    ("&Lcaron;", '\u{013D}'), // Slovak Ľ
    ("&Lstrok;", '\u{0141}'), // Polish Ł
    ("&Mu;", '\u{039C}'),
    ("&Nacute;", '\u{0143}'), // Polish Ń
    ("&Ncaron;", '\u{0147}'), // Czech Ň
    ("&Ntilde;", '\u{00D1}'),
    ("&Nu;", '\u{039D}'),
    ("&OElig;", '\u{0152}'),
    ("&Oacute;", '\u{00D3}'),
    ("&Ocirc;", '\u{00D4}'),
    ("&Odblac;", '\u{0150}'), // Hungarian Ő
    ("&Ograve;", '\u{00D2}'),
    ("&Omega;", '\u{03A9}'),
    ("&Omicron;", '\u{039F}'),
    ("&Oslash;", '\u{00D8}'),
    ("&Otilde;", '\u{00D5}'),
    ("&Ouml;", '\u{00D6}'),
    ("&Phi;", '\u{03A6}'),
    ("&Pi;", '\u{03A0}'),
    ("&Prime;", '\u{2033}'),
    ("&Psi;", '\u{03A8}'),
    ("&Racute;", '\u{0154}'), // Slovak Ŕ
    ("&Rcaron;", '\u{0158}'), // Czech Ř
    ("&Rho;", '\u{03A1}'),
    ("&Sacute;", '\u{015A}'), // Polish Ś
    ("&Scaron;", '\u{0160}'),
    ("&Scedil;", '\u{015E}'), // Turkish Ş
    ("&Sigma;", '\u{03A3}'),
    ("&THORN;", '\u{00DE}'),
    ("&Tau;", '\u{03A4}'),
    ("&Tcaron;", '\u{0164}'), // Slovak Ť
    ("&Tcedil;", '\u{0162}'), // Romanian Ţ
    ("&Theta;", '\u{0398}'),
    ("&Uacute;", '\u{00DA}'),
    ("&Ucirc;", '\u{00DB}'),
    ("&Udblac;", '\u{0170}'), // Hungarian Ű
    ("&Ugrave;", '\u{00D9}'),
    ("&Upsilon;", '\u{03A5}'),
    ("&Uuml;", '\u{00DC}'),
    ("&Xi;", '\u{039E}'),
    ("&Yacute;", '\u{00DD}'),
    ("&Yuml;", '\u{0178}'),
    ("&Zacute;", '\u{0179}'), // Polish Ź
    ("&Zcaron;", '\u{017D}'), // Czech Ž
    ("&Zdot;", '\u{017B}'),   // Polish Ż
    ("&Zeta;", '\u{0396}'),
    ("&aacute;", '\u{00E1}'),
    ("&abreve;", '\u{0103}'), // Romanian ă
    ("&acirc;", '\u{00E2}'),
    ("&acute;", '\u{00B4}'),
    ("&aelig;", '\u{00E6}'),
    ("&agrave;", '\u{00E0}'),
    ("&alefsym;", '\u{2135}'),
    ("&alpha;", '\u{03B1}'),
    ("&amp;", '&'),
    ("&and;", '\u{2227}'),
    ("&ang;", '\u{2220}'),
    ("&aogonek;", '\u{0105}'), // Polish ą
    ("&apos;", '\''),
    ("&aring;", '\u{00E5}'),
    ("&asymp;", '\u{2248}'),
    ("&atilde;", '\u{00E3}'),
    ("&auml;", '\u{00E4}'),
    ("&bdquo;", '\u{201E}'),
    ("&beta;", '\u{03B2}'),
    ("&brvbar;", '\u{00A6}'),
    ("&bull;", '\u{2022}'),
    ("&cacute;", '\u{0107}'), // Polish ć
    ("&cap;", '\u{2229}'),
    ("&ccaron;", '\u{010D}'), // Czech č
    ("&ccedil;", '\u{00E7}'),
    ("&cedil;", '\u{00B8}'),
    ("&cent;", '\u{00A2}'),
    ("&chi;", '\u{03C7}'),
    ("&circ;", '\u{02C6}'),
    ("&clubs;", '\u{2663}'),
    ("&cong;", '\u{2245}'),
    ("&copy;", '\u{00A9}'),
    ("&crarr;", '\u{21B5}'),
    ("&cup;", '\u{222A}'),
    ("&curren;", '\u{00A4}'),
    ("&dArr;", '\u{21D3}'),
    ("&dagger;", '\u{2020}'),
    ("&darr;", '\u{2193}'),
    ("&dcaron;", '\u{010F}'), // Czech ď
    ("&deg;", '\u{00B0}'),
    ("&delta;", '\u{03B4}'),
    ("&diams;", '\u{2666}'),
    ("&divide;", '\u{00F7}'),
    ("&dstrok;", '\u{0111}'), // Croatian đ
    ("&eacute;", '\u{00E9}'),
    ("&ecaron;", '\u{011B}'), // Czech ě
    ("&ecirc;", '\u{00EA}'),
    ("&egrave;", '\u{00E8}'),
    ("&empty;", '\u{2205}'),
    ("&emsp;", '\u{2003}'),
    ("&ensp;", '\u{2002}'),
    ("&eogonek;", '\u{0119}'), // Polish ę
    ("&epsilon;", '\u{03B5}'),
    ("&equiv;", '\u{2261}'),
    ("&eta;", '\u{03B7}'),
    ("&eth;", '\u{00F0}'),
    ("&euml;", '\u{00EB}'),
    ("&euro;", '\u{20AC}'),
    ("&exist;", '\u{2203}'),
    ("&fnof;", '\u{0192}'),
    ("&forall;", '\u{2200}'),
    ("&frac12;", '\u{00BD}'),
    ("&frac14;", '\u{00BC}'),
    ("&frac34;", '\u{00BE}'),
    ("&frasl;", '\u{2044}'),
    ("&gamma;", '\u{03B3}'),
    ("&gbreve;", '\u{011F}'), // Turkish ğ
    ("&ge;", '\u{2265}'),
    ("&gt;", '>'),
    ("&hArr;", '\u{21D4}'),
    ("&harr;", '\u{2194}'),
    ("&hearts;", '\u{2665}'),
    ("&hellip;", '\u{2026}'),
    ("&iacute;", '\u{00ED}'),
    ("&icirc;", '\u{00EE}'),
    ("&iexcl;", '\u{00A1}'),
    ("&igrave;", '\u{00EC}'),
    ("&image;", '\u{2111}'),
    ("&infin;", '\u{221E}'),
    ("&inodot;", '\u{0131}'), // Turkish ı (dotless i)
    ("&int;", '\u{222B}'),
    ("&iota;", '\u{03B9}'),
    ("&iquest;", '\u{00BF}'),
    ("&isin;", '\u{2208}'),
    ("&iuml;", '\u{00EF}'),
    ("&kappa;", '\u{03BA}'),
    ("&lArr;", '\u{21D0}'),
    ("&lambda;", '\u{03BB}'),
    ("&lang;", '\u{2329}'),
    ("&laquo;", '\u{00AB}'),
    ("&larr;", '\u{2190}'),
    ("&lcaron;", '\u{013E}'), // Slovak ľ
    ("&lceil;", '\u{2308}'),
    ("&ldquo;", '\u{201C}'),
    ("&le;", '\u{2264}'),
    ("&lfloor;", '\u{230A}'),
    ("&lowast;", '\u{2217}'),
    ("&loz;", '\u{25CA}'),
    ("&lrm;", '\u{200E}'),
    ("&lsaquo;", '\u{2039}'),
    ("&lsquo;", '\u{2018}'),
    ("&lstrok;", '\u{0142}'), // Polish ł
    ("&lt;", '<'),
    ("&macr;", '\u{00AF}'),
    ("&mdash;", '\u{2014}'),
    ("&micro;", '\u{00B5}'),
    ("&middot;", '\u{00B7}'),
    ("&minus;", '\u{2212}'),
    ("&mu;", '\u{03BC}'),
    ("&nabla;", '\u{2207}'),
    ("&nacute;", '\u{0144}'), // Polish ń
    ("&nbsp;", ' '),
    ("&ncaron;", '\u{0148}'), // Czech ň
    ("&ndash;", '\u{2013}'),
    ("&ne;", '\u{2260}'),
    ("&ni;", '\u{220B}'),
    ("&not;", '\u{00AC}'),
    ("&notin;", '\u{2209}'),
    ("&nsub;", '\u{2284}'),
    ("&ntilde;", '\u{00F1}'),
    ("&nu;", '\u{03BD}'),
    ("&oacute;", '\u{00F3}'),
    ("&ocirc;", '\u{00F4}'),
    ("&odblac;", '\u{0151}'), // Hungarian ő
    ("&oelig;", '\u{0153}'),
    ("&ograve;", '\u{00F2}'),
    ("&oline;", '\u{203E}'),
    ("&omega;", '\u{03C9}'),
    ("&omicron;", '\u{03BF}'),
    ("&oplus;", '\u{2295}'),
    ("&or;", '\u{2228}'),
    ("&ordf;", '\u{00AA}'),
    ("&ordm;", '\u{00BA}'),
    ("&oslash;", '\u{00F8}'),
    ("&otilde;", '\u{00F5}'),
    ("&otimes;", '\u{2297}'),
    ("&ouml;", '\u{00F6}'),
    ("&para;", '\u{00B6}'),
    ("&part;", '\u{2202}'),
    ("&permil;", '\u{2030}'),
    ("&perp;", '\u{22A5}'),
    ("&phi;", '\u{03C6}'),
    ("&pi;", '\u{03C0}'),
    ("&piv;", '\u{03D6}'),
    ("&plusmn;", '\u{00B1}'),
    ("&pound;", '\u{00A3}'),
    ("&prime;", '\u{2032}'),
    ("&prod;", '\u{220F}'),
    ("&prop;", '\u{221D}'),
    ("&psi;", '\u{03C8}'),
    ("&quot;", '"'),
    ("&rArr;", '\u{21D2}'),
    ("&racute;", '\u{0155}'), // Slovak ŕ
    ("&radic;", '\u{221A}'),
    ("&rang;", '\u{232A}'),
    ("&raquo;", '\u{00BB}'),
    ("&rarr;", '\u{2192}'),
    ("&rcaron;", '\u{0159}'), // Czech ř
    ("&rceil;", '\u{2309}'),
    ("&rdquo;", '\u{201D}'),
    ("&real;", '\u{211C}'),
    ("&reg;", '\u{00AE}'),
    ("&rfloor;", '\u{230B}'),
    ("&rho;", '\u{03C1}'),
    ("&rlm;", '\u{200F}'),
    ("&rsaquo;", '\u{203A}'),
    ("&rsquo;", '\u{2019}'),
    ("&sacute;", '\u{015B}'), // Polish ś
    ("&sbquo;", '\u{201A}'),
    ("&scaron;", '\u{0161}'),
    ("&scedil;", '\u{015F}'), // Turkish ş
    ("&sdot;", '\u{22C5}'),
    ("&sect;", '\u{00A7}'),
    ("&shy;", '\u{00AD}'),
    ("&sigma;", '\u{03C3}'),
    ("&sigmaf;", '\u{03C2}'),
    ("&sim;", '\u{223C}'),
    ("&spades;", '\u{2660}'),
    ("&sub;", '\u{2282}'),
    ("&sube;", '\u{2286}'),
    ("&sum;", '\u{2211}'),
    ("&sup1;", '\u{00B9}'),
    ("&sup2;", '\u{00B2}'),
    ("&sup3;", '\u{00B3}'),
    ("&sup;", '\u{2283}'),
    ("&supe;", '\u{2287}'),
    ("&szlig;", '\u{00DF}'),
    ("&tau;", '\u{03C4}'),
    ("&tcaron;", '\u{0165}'), // Slovak ť
    ("&tcedil;", '\u{0163}'), // Romanian ţ
    ("&there4;", '\u{2234}'),
    ("&theta;", '\u{03B8}'),
    ("&thetasym;", '\u{03D1}'),
    ("&thinsp;", '\u{2009}'),
    ("&thorn;", '\u{00FE}'),
    ("&tilde;", '\u{02DC}'),
    ("&times;", '\u{00D7}'),
    ("&trade;", '\u{2122}'),
    ("&uArr;", '\u{21D1}'),
    ("&uacute;", '\u{00FA}'),
    ("&uarr;", '\u{2191}'),
    ("&ucirc;", '\u{00FB}'),
    ("&udblac;", '\u{0171}'), // Hungarian ű
    ("&ugrave;", '\u{00F9}'),
    ("&uml;", '\u{00A8}'),
    ("&upsih;", '\u{03D2}'),
    ("&upsilon;", '\u{03C5}'),
    ("&uuml;", '\u{00FC}'),
    ("&weierp;", '\u{2118}'),
    ("&xi;", '\u{03BE}'),
    ("&yacute;", '\u{00FD}'),
    ("&yen;", '\u{00A5}'),
    ("&yuml;", '\u{00FF}'),
    ("&zacute;", '\u{017A}'), // Polish ź
    ("&zcaron;", '\u{017E}'), // Czech ž
    ("&zdot;", '\u{017C}'),   // Polish ż
    ("&zeta;", '\u{03B6}'),
    ("&zwj;", '\u{200D}'),
    ("&zwnj;", '\u{200C}'),
];

fn decode_named_entity(entity: &str) -> Option<char> {
    NAMED_ENTITIES
        .binary_search_by_key(&entity, |(name, _)| name)
        .ok()
        .map(|idx| NAMED_ENTITIES[idx].1)
}

/// Extract the value of an HTML attribute from a tag buffer.
///
/// Handles both `attr="value"` and `attr='value'` formats.
/// Uses byte-level case-insensitive search to avoid heap allocations.
/// Returns `None` if the attribute is not found.
fn extract_attr_value<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let tag_bytes = tag.as_bytes();
    let name_bytes = attr_name.as_bytes();
    let name_len = name_bytes.len();

    // Find attr_name= case-insensitively
    let mut pos = 0;
    while pos + name_len < tag_bytes.len() {
        if tag_bytes[pos + name_len] == b'='
            && tag_bytes[pos..pos + name_len].eq_ignore_ascii_case(name_bytes)
        {
            let after_eq = pos + name_len + 1;
            // Skip whitespace after =
            let rest = &tag[after_eq..];
            let rest = rest.trim_start();
            let rest_bytes = rest.as_bytes();

            return if rest_bytes.first() == Some(&b'"') {
                let inner = &rest[1..];
                let end = memchr::memchr(b'"', inner.as_bytes())?;
                Some(&inner[..end])
            } else if rest_bytes.first() == Some(&b'\'') {
                let inner = &rest[1..];
                let end = memchr::memchr(b'\'', inner.as_bytes())?;
                Some(&inner[..end])
            } else {
                // Unquoted value (ends at whitespace or >)
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(rest.len());
                Some(&rest[..end])
            };
        }
        pos += 1;
    }
    None
}

/// Map Windows-1252 codepoints 128–159 to their correct Unicode equivalents.
///
/// The HTML5 spec requires browsers to treat numeric entities in this range
/// as Windows-1252, not as ISO-8859-1 control characters. This is critical
/// for NER: `&#150;` (en dash between names) must become U+2013, not U+0096.
fn win1252_to_unicode(cp: u32) -> Option<char> {
    match cp {
        0x80 => Some('\u{20AC}'), // Euro sign
        0x82 => Some('\u{201A}'), // Single low-9 quotation mark
        0x83 => Some('\u{0192}'), // Latin small f with hook
        0x84 => Some('\u{201E}'), // Double low-9 quotation mark
        0x85 => Some('\u{2026}'), // Horizontal ellipsis
        0x86 => Some('\u{2020}'), // Dagger
        0x87 => Some('\u{2021}'), // Double dagger
        0x88 => Some('\u{02C6}'), // Modifier letter circumflex accent
        0x89 => Some('\u{2030}'), // Per mille sign
        0x8A => Some('\u{0160}'), // Latin capital S with caron
        0x8B => Some('\u{2039}'), // Single left-pointing angle quotation
        0x8C => Some('\u{0152}'), // Latin capital OE
        0x8E => Some('\u{017D}'), // Latin capital Z with caron
        0x91 => Some('\u{2018}'), // Left single quotation mark
        0x92 => Some('\u{2019}'), // Right single quotation mark
        0x93 => Some('\u{201C}'), // Left double quotation mark
        0x94 => Some('\u{201D}'), // Right double quotation mark
        0x95 => Some('\u{2022}'), // Bullet
        0x96 => Some('\u{2013}'), // En dash
        0x97 => Some('\u{2014}'), // Em dash
        0x98 => Some('\u{02DC}'), // Small tilde
        0x99 => Some('\u{2122}'), // Trade mark sign
        0x9A => Some('\u{0161}'), // Latin small s with caron
        0x9B => Some('\u{203A}'), // Single right-pointing angle quotation
        0x9C => Some('\u{0153}'), // Latin small oe
        0x9E => Some('\u{017E}'), // Latin small z with caron
        0x9F => Some('\u{0178}'), // Latin capital Y with diaeresis
        _ => None,
    }
}

/// Returns true if the character is a zero-width, invisible, or formatting
/// Unicode character that should be stripped for clean NER tokenization.
fn is_invisible_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}'  // Zero-width space
        | '\u{200C}' // Zero-width non-joiner
        | '\u{200D}' // Zero-width joiner
        | '\u{200E}' // Left-to-right mark
        | '\u{200F}' // Right-to-left mark
        | '\u{00AD}' // Soft hyphen
        | '\u{2060}' // Word joiner
        | '\u{FEFF}' // BOM / zero-width no-break space (mid-text)
        // Bidi embedding/override controls (common in RTL-mixed text)
        | '\u{202A}' // Left-to-right embedding
        | '\u{202B}' // Right-to-left embedding
        | '\u{202C}' // Pop directional formatting
        | '\u{202D}' // Left-to-right override
        | '\u{202E}' // Right-to-left override
        // Bidi isolate controls (HTML5 bidi algorithm)
        | '\u{2066}' // Left-to-right isolate
        | '\u{2067}' // Right-to-left isolate
        | '\u{2068}' // First strong isolate
        | '\u{2069}' // Pop directional isolate
        // Other invisible formatting
        | '\u{180E}' // Mongolian vowel separator
        | '\u{FE0F}' // Variation selector-16 (emoji modifier)
    )
}

/// Returns true if the character is a non-breaking space that should be
/// normalized to a regular ASCII space for NER tokenization.
fn is_nbsp(ch: char) -> bool {
    ch == '\u{00A0}' // No-break space (raw, not from &nbsp; which already maps to ' ')
}

/// Decode all HTML entities in a string.
///
/// Handles named entities (`&amp;`), decimal (`&#169;`), hex (`&#xA9;`),
/// Windows-1252 C1 range mapping, and semicolon-optional entities.
///
/// ```
/// assert_eq!(deformat::html::decode_entities("Caf&eacute;"), "Café");
/// assert_eq!(deformat::html::decode_entities("&#169; 2026"), "\u{00A9} 2026");
/// ```
pub fn decode_entities(s: &str) -> String {
    decode_entities_in_str(s).into_owned()
}

fn decode_entities_in_str(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    // Fast path: no '&' means no entities to decode — zero-copy return
    let first_amp = match memchr::memchr(b'&', bytes) {
        Some(offset) => offset,
        None => return Cow::Borrowed(s),
    };

    let mut result = String::with_capacity(len);
    // Copy the prefix before the first '&'
    result.push_str(&s[..first_amp]);
    let mut pos = first_amp;

    loop {
        // pos points to '&'
        pos = decode_entity_bytes(s, bytes, pos, &mut result);
        // Find next '&'
        match memchr::memchr(b'&', &bytes[pos..]) {
            Some(offset) => {
                result.push_str(&s[pos..pos + offset]);
                pos += offset;
            }
            None => {
                result.push_str(&s[pos..]);
                break;
            }
        }
    }
    Cow::Owned(result)
}

/// Decode an HTML entity starting at `pos` (which points to '&').
/// Returns the new position after the entity.
fn decode_entity_bytes(s: &str, bytes: &[u8], start: usize, text: &mut String) -> usize {
    let len = bytes.len();
    debug_assert!(bytes[start] == b'&');

    // Scan for entity end: ';', whitespace, '<', or end of input.
    // Entity names are ASCII, so byte scanning is safe.
    let mut end = start + 1;
    let mut found_semicolon = false;

    while end < len {
        match bytes[end] {
            b';' => {
                end += 1;
                found_semicolon = true;
                break;
            }
            b' ' | b'\t' | b'\n' | b'\r' | b'<' => break,
            _ => end += 1,
        }
    }

    let entity_str = &s[start..end];

    if found_semicolon {
        if let Some(ch) = decode_named_entity(entity_str) {
            text.push(ch);
        } else if entity_str.starts_with("&#") && entity_str.len() > 3 {
            let num_str = &entity_str[2..entity_str.len() - 1];
            let parsed = if let Some(hex) = num_str
                .strip_prefix('x')
                .or_else(|| num_str.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(ch) = parsed.and_then(numeric_entity_to_char) {
                text.push(ch);
            } else {
                text.push_str(entity_str);
            }
        } else {
            text.push_str(entity_str);
        }
    } else {
        // Semicolon-optional: try as named entity with appended ';'
        // Only for entity-like strings (&alpha...) -- all ASCII alphanumeric after '&'
        if entity_str.len() > 2
            && bytes[start + 1].is_ascii_alphabetic()
            && entity_str[1..].bytes().all(|b| b.is_ascii_alphanumeric())
        {
            // Use a stack buffer to avoid format! allocation
            let entity_bytes = entity_str.as_bytes();
            let entity_with_semi_len = entity_bytes.len() + 1;
            if entity_with_semi_len <= 32 {
                let mut buf = [0u8; 32];
                buf[..entity_bytes.len()].copy_from_slice(entity_bytes);
                buf[entity_bytes.len()] = b';';
                // SAFETY: entity_str is ASCII (checked above), ';' is ASCII
                let with_semi = std::str::from_utf8(&buf[..entity_with_semi_len]).unwrap();
                if let Some(ch) = decode_named_entity(with_semi) {
                    text.push(ch);
                    return end;
                }
            }
        }
        text.push_str(entity_str);
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Basic stripping =====

    #[test]
    fn strip_basic() {
        assert_eq!(strip_to_text("<p>Hello <b>world</b>!</p>"), "Hello world!");
    }

    #[test]
    fn strip_plain_text_passthrough() {
        // No HTML at all -- exercises the no-'<' fast path
        let text = "Tim Cook met with Sundar Pichai in Seattle.";
        assert_eq!(strip_to_text(text), text);
    }

    #[test]
    fn strip_plain_text_with_entities() {
        // No tags but has entities -- fast path decodes them
        assert_eq!(strip_to_text("Caf&eacute; au lait"), "Caf\u{e9} au lait");
    }

    #[test]
    fn strip_plain_text_with_whitespace() {
        // No tags, extra whitespace -- fast path normalizes it
        assert_eq!(strip_to_text("  hello   world  "), "hello world");
    }

    #[test]
    fn strip_script_style() {
        let html = r#"<html><head><style>body{color:red}</style></head>
            <body><script>alert('hi')</script><p>Real text.</p></body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Real text"));
        assert!(!text.contains("alert"), "script stripped");
        assert!(!text.contains("color"), "style stripped");
    }

    #[test]
    fn strip_block_spacing() {
        let html = "<h1>Title</h1><p>First.</p><p>Second.</p>";
        let text = strip_to_text(html);
        assert!(!text.contains("TitleFirst"), "blocks separated");
        assert!(text.contains("Title"));
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
    }

    // ===== Entity decoding =====

    #[test]
    fn entity_named() {
        let text = strip_to_text("<p>A &amp; B &lt; C</p>");
        assert!(text.contains("A & B"));
        assert!(text.contains("< C"));
    }

    #[test]
    fn entity_table_is_sorted() {
        for window in NAMED_ENTITIES.windows(2) {
            assert!(
                window[0].0 < window[1].0,
                "entity table not sorted: {:?} should come before {:?}",
                window[0].0,
                window[1].0
            );
        }
    }

    #[test]
    fn entity_decimal() {
        let text = strip_to_text("<p>It&#39;s a test</p>");
        assert!(text.contains("It's"));
    }

    #[test]
    fn entity_hex() {
        let text = strip_to_text("<p>It&#x27;s a test</p>");
        assert!(text.contains("It's"));
    }

    #[test]
    fn entity_hex_uppercase() {
        let text = strip_to_text("<p>It&#X27;s a test</p>");
        assert!(text.contains("It's"));
    }

    // ===== Whitespace collapsing =====

    #[test]
    fn collapses_whitespace() {
        let html = r#"<html><head><title>t</title></head>
            <body><h1>Hello   world</h1><p>Line1<br>Line2</p>
            <div>Tabbed	text</div></body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Hello world"));
        assert!(text.contains("Line1 Line2"));
        assert!(text.contains("Tabbed text"));
        assert!(!text.contains('\n'));
        assert!(!text.contains('\t'));
        assert!(!text.contains("  "));
    }

    // ===== Semantic tag filtering =====

    #[test]
    fn nav_stripped() {
        let html = r#"<html><body>
            <nav><a href="/">Home</a></nav>
            <main><p>Content.</p></main>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Content"));
        assert!(!text.contains("Home"));
    }

    #[test]
    fn footer_stripped() {
        let html = r#"<html><body>
            <article><p>Body.</p></article>
            <footer><p>Copyright 2024.</p></footer>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Body"));
        assert!(!text.contains("Copyright"));
    }

    #[test]
    fn header_stripped() {
        let html = r#"<html><body>
            <header><h1>Site</h1></header>
            <main><p>Page.</p></main>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Page"));
        assert!(!text.contains("Site"));
    }

    #[test]
    fn aside_stripped() {
        let html = r#"<html><body>
            <main><p>Main.</p></main>
            <aside><p>Sidebar.</p></aside>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Main"));
        assert!(!text.contains("Sidebar"));
    }

    #[test]
    fn head_stripped() {
        let html = "<html><head><title>Page Title</title></head>\
                     <body><p>Content.</p></body></html>";
        let text = strip_to_text(html);
        assert!(!text.contains("Page Title"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn noscript_stripped() {
        let html = r#"<html><body>
            <noscript><p>Enable JS.</p></noscript>
            <main><p>App.</p></main>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("App"));
        assert!(!text.contains("Enable JS"));
    }

    #[test]
    fn nested_semantic() {
        let html = r#"<html><body>
            <header><nav><ul><li>Link</li></ul></nav></header>
            <main><p>Real.</p></main>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Real"));
        assert!(!text.contains("Link"));
    }

    #[test]
    fn article_preserved() {
        let html = r#"<html><body>
            <article><h2>Title</h2><p>Para.</p></article>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Para"));
    }

    // ===== Wikipedia boilerplate =====

    #[test]
    fn wiki_ref_brackets_stripped() {
        let html = r#"<html><body>
            <p>Einstein[1] was a physicist.[2] See also[edit] quantum.</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(!text.contains("[1]"));
        assert!(!text.contains("[edit]"));
        assert!(text.contains("Einstein"));
        assert!(text.contains("quantum"));
    }

    #[test]
    fn wiki_citation_needed_stripped() {
        let text = strip_to_text("<p>Claim[citation needed] here.</p>");
        assert!(!text.contains("[citation needed]"));
        assert!(text.contains("Claim"));
    }

    #[test]
    fn wiki_toc_stripped() {
        let html = r#"<html><body>
            <p>Article text.</p>
            <div id="toc"><h2>Contents</h2><ul><li>Section</li></ul></div>
            <p>More text.</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Article text"));
        assert!(text.contains("More text"));
        assert!(!text.contains("Contents"));
    }

    // ===== Multilingual =====

    #[test]
    fn multilingual_preserved() {
        let html = r#"<html><body>
            <p>&#x4E60;&#x8FD1;&#x5E73;&#x5728;&#x5317;&#x4EAC;</p>
            <p>Путин встретился с Си Цзиньпином в Москве.</p>
            <p>प्रधान मंत्री शर्मा आज आए।</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Путин встретился с Си Цзиньпином в Москве."));
        assert!(text.contains("प्रधान मंत्री शर्मा आज आए।"));
    }

    // ===== Readability (feature-gated) =====

    #[cfg(feature = "readability")]
    #[test]
    fn readability_extracts_article() {
        let html = r#"<!DOCTYPE html>
        <html><head><title>News</title></head>
        <body>
            <nav><a href="/">Home</a></nav>
            <div id="content">
                <h1>News</h1>
                <p>A team of researchers at the University of Cambridge has announced
                   the discovery of a previously unknown species of beetle in the Amazon
                   rainforest. The discovery was published in Nature on March 15, 2026.
                   The finding represents one of the most significant entomological
                   discoveries in the region in recent years.</p>
                <p>Lead researcher Dr. Sarah Chen said the species, named Chrysina
                   amazonica, was found during an expedition in January near Manaus.
                   The beetle has unique iridescent markings that distinguish it from
                   related species. Chen and her team spent three weeks collecting
                   specimens and documenting the habitat conditions.</p>
                <p>The Amazon rainforest continues to yield new discoveries despite
                   decades of intensive exploration. Conservation groups have called for
                   increased protection. Brazil's Environment Ministry said it would
                   review the protected area boundaries in light of the new findings.</p>
                <p>The research was funded by the European Research Council and National
                   Geographic Society. Additional specimens will be housed at the Natural
                   History Museum in London and the Smithsonian Institution.</p>
            </div>
            <footer>Copyright 2026</footer>
        </body></html>"#;
        let result = extract_with_readability(html, "https://example.com/article");
        assert!(result.is_some());
        let (text, title, _) = result.unwrap();
        assert!(text.contains("Dr. Sarah Chen"));
        assert!(title.is_some());
    }

    #[cfg(feature = "readability")]
    #[test]
    fn readability_returns_none_for_trivial() {
        assert!(extract_with_readability("<p>Hi</p>", "https://example.com").is_none());
    }

    #[cfg(feature = "readability")]
    #[test]
    fn readability_returns_none_for_empty() {
        assert!(extract_with_readability("", "https://example.com").is_none());
    }

    // ===== Extended entity decoding (NER-critical) =====

    #[test]
    fn entity_eacute_for_ner() {
        // "Nestlé" must be decoded correctly for NER to recognize it
        let text = strip_to_text("<p>Nestl&eacute; is a company.</p>");
        assert!(text.contains("Nestlé"), "eacute decoded: {text}");
    }

    #[test]
    fn entity_mdash_ndash() {
        let text = strip_to_text("<p>A &mdash; B &ndash; C</p>");
        assert!(text.contains('\u{2014}'), "mdash decoded: {text}");
        assert!(text.contains('\u{2013}'), "ndash decoded: {text}");
    }

    #[test]
    fn entity_curly_quotes() {
        let text = strip_to_text("<p>&ldquo;Hello&rdquo; &lsquo;world&rsquo;</p>");
        assert!(text.contains('\u{201C}'), "ldquo: {text}");
        assert!(text.contains('\u{201D}'), "rdquo: {text}");
        assert!(text.contains('\u{2018}'), "lsquo: {text}");
        assert!(text.contains('\u{2019}'), "rsquo: {text}");
    }

    #[test]
    fn entity_currency_symbols() {
        let text = strip_to_text("<p>&euro;100 &pound;50 &yen;1000</p>");
        assert!(text.contains('€'), "euro: {text}");
        assert!(text.contains('£'), "pound: {text}");
        assert!(text.contains('¥'), "yen: {text}");
    }

    #[test]
    fn entity_accented_names() {
        // Common in European news: accented names must survive extraction
        let text =
            strip_to_text("<p>&Uuml;ber M&uuml;ller traf Garc&iacute;a in S&atilde;o Paulo.</p>");
        assert!(text.contains("Über"), "Uuml: {text}");
        assert!(text.contains("Müller"), "uuml: {text}");
        assert!(text.contains("García"), "iacute: {text}");
        assert!(text.contains("São"), "atilde: {text}");
    }

    #[test]
    fn entity_copyright_trademark() {
        let text = strip_to_text("<p>&copy; 2026 Company&trade; &reg;</p>");
        assert!(text.contains('©'), "copy: {text}");
        assert!(text.contains('™'), "trade: {text}");
        assert!(text.contains('®'), "reg: {text}");
    }

    #[test]
    fn entity_unknown_passes_through() {
        // Unknown named entities should pass through unchanged
        let text = strip_to_text("<p>&foobar; text</p>");
        assert!(
            text.contains("&foobar;"),
            "unknown entity preserved: {text}"
        );
    }

    #[test]
    fn entity_unterminated_passes_through() {
        // Unterminated entity (no semicolon) should not eat subsequent text
        let text = strip_to_text("<p>AT&T is a company.</p>");
        assert!(
            text.contains("AT&T"),
            "unterminated entity preserved: {text}"
        );
        assert!(
            text.contains("company"),
            "subsequent text preserved: {text}"
        );
    }

    // ===== Edge cases =====

    #[test]
    fn empty_input() {
        assert_eq!(strip_to_text(""), "");
    }

    #[test]
    fn plain_text_passthrough() {
        let input = "No HTML here, just text.";
        assert_eq!(strip_to_text(input), input);
    }

    #[test]
    fn unclosed_tag_handled() {
        let text = strip_to_text("<p>Hello <b>world");
        assert!(text.contains("Hello"), "text before unclosed: {text}");
        assert!(text.contains("world"), "text in unclosed: {text}");
    }

    #[test]
    fn self_closing_tags() {
        let text = strip_to_text("<p>Line1<br/>Line2<hr/>Line3</p>");
        assert!(text.contains("Line1"), "before br: {text}");
        assert!(text.contains("Line2"), "after br: {text}");
        assert!(text.contains("Line3"), "after hr: {text}");
    }

    #[test]
    fn html_comments_stripped() {
        let text = strip_to_text("<p>Before<!-- comment -->After</p>");
        assert!(text.contains("Before"), "before comment: {text}");
        assert!(text.contains("After"), "after comment: {text}");
        assert!(!text.contains("comment"), "comment stripped: {text}");
    }

    #[test]
    fn html_comment_with_tags_inside() {
        // Tags inside comments should NOT trigger script/style/skip tracking
        let text = strip_to_text("<p>Real</p><!-- <script>evil()</script> --><p>Also real</p>");
        assert!(text.contains("Real"), "before comment: {text}");
        assert!(text.contains("Also real"), "after comment: {text}");
        assert!(!text.contains("evil"), "script in comment ignored: {text}");
    }

    #[test]
    fn html_comment_with_dashes() {
        // Comments with multiple dashes
        let text = strip_to_text("<p>A</p><!-- -- -- --><p>B</p>");
        assert!(text.contains('A'), "before: {text}");
        assert!(text.contains('B'), "after: {text}");
    }

    #[test]
    fn ie_conditional_comment() {
        // IE conditional comments are still comments
        let text = strip_to_text("<p>Real</p><!--[if IE]>IE only<![endif]--><p>Also real</p>");
        assert!(text.contains("Real"), "before: {text}");
        assert!(text.contains("Also real"), "after: {text}");
        assert!(!text.contains("IE only"), "conditional stripped: {text}");
    }

    #[test]
    fn quoted_attribute_with_gt() {
        // '>' inside a quoted attribute should NOT end the tag
        let html = r#"<div title="a > b">Content</div>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Content"), "content preserved: {text}");
        assert!(!text.contains("a > b"), "attr value not leaked: {text}");
        assert!(!text.contains("title"), "attr name not leaked: {text}");
    }

    #[test]
    fn quoted_attribute_with_lt() {
        let html = r#"<span data-expr="x < 10">Result</span>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Result"), "content preserved: {text}");
        assert!(!text.contains("x < 10"), "attr not leaked: {text}");
    }

    #[test]
    fn single_quoted_attribute_with_gt() {
        let html = "<div title='a > b'>Content</div>";
        let text = strip_to_text(html);
        assert!(text.contains("Content"), "content preserved: {text}");
        assert!(!text.contains("a > b"), "attr not leaked: {text}");
    }

    #[test]
    fn nested_quotes_in_attribute() {
        // Double quotes inside single-quoted attr
        let html = r#"<a title='He said "hello"'>Link</a>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Link"), "content preserved: {text}");
        assert!(
            !text.contains("hello"),
            "nested quote attr not leaked: {text}"
        );
    }

    #[test]
    fn null_entity_becomes_replacement_char() {
        let text = strip_to_text("<p>Before&#0;After</p>");
        assert!(text.contains("Before"), "before null: {text}");
        assert!(text.contains("After"), "after null: {text}");
        assert!(
            text.contains('\u{FFFD}'),
            "null becomes replacement char: {text}"
        );
    }

    #[test]
    fn doctype_not_treated_as_comment() {
        // <!DOCTYPE html> should be handled as a tag, not a comment
        let text = strip_to_text("<!DOCTYPE html><html><body><p>Content</p></body></html>");
        assert!(text.contains("Content"), "content preserved: {text}");
        assert!(!text.contains("DOCTYPE"), "doctype stripped: {text}");
    }

    #[test]
    fn nested_skip_tags_depth() {
        // Multiple nested skip elements should all be stripped
        let html = r#"<html><body>
            <nav><ul><li><a href="/">Home</a></li>
                <li><a href="/about">About</a></li></ul></nav>
            <p>Real content here.</p>
            <footer><nav><a href="/privacy">Privacy</a></nav></footer>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Real content"), "body preserved: {text}");
        assert!(!text.contains("Home"), "nav stripped: {text}");
        assert!(!text.contains("Privacy"), "footer nav stripped: {text}");
    }

    #[test]
    fn data_attributes_not_in_output() {
        let html = r#"<div data-entity="person" data-id="123"><p>Tim Cook</p></div>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Tim Cook"), "content preserved: {text}");
        assert!(!text.contains("data-entity"), "attrs stripped: {text}");
        assert!(!text.contains("123"), "attr values stripped: {text}");
    }

    #[test]
    fn multiple_scripts_and_styles() {
        let html = r#"<html><body>
            <script>var a = 1;</script>
            <p>First.</p>
            <style>.x { color: red; }</style>
            <p>Second.</p>
            <script type="application/json">{"key": "val"}</script>
            <p>Third.</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("First"), "first para: {text}");
        assert!(text.contains("Second"), "second para: {text}");
        assert!(text.contains("Third"), "third para: {text}");
        assert!(!text.contains("var a"), "script 1 stripped: {text}");
        assert!(!text.contains("color"), "style stripped: {text}");
        assert!(!text.contains("key"), "json script stripped: {text}");
    }

    // ===== Windows-1252 entity mapping =====

    #[test]
    fn win1252_en_dash() {
        // &#150; is en dash in Windows-1252, not a control character
        let text = strip_to_text("<p>Smith&#150;Jones partnership</p>");
        assert!(text.contains('\u{2013}'), "en dash decoded: {text}");
        assert!(text.contains("Smith"), "name preserved: {text}");
        assert!(text.contains("Jones"), "name preserved: {text}");
    }

    #[test]
    fn win1252_em_dash() {
        let text = strip_to_text("<p>Wait&#151;what?</p>");
        assert!(text.contains('\u{2014}'), "em dash decoded: {text}");
    }

    #[test]
    fn win1252_curly_quotes() {
        // &#147; and &#148; are curly double quotes in Windows-1252
        let text = strip_to_text("<p>&#147;Hello&#148; she said</p>");
        assert!(text.contains('\u{201C}'), "left double quote: {text}");
        assert!(text.contains('\u{201D}'), "right double quote: {text}");
    }

    #[test]
    fn win1252_euro_sign() {
        let text = strip_to_text("<p>Price: &#128;100</p>");
        assert!(text.contains('€'), "euro from &#128;: {text}");
    }

    #[test]
    fn win1252_trademark() {
        let text = strip_to_text("<p>Brand&#153;</p>");
        assert!(text.contains('™'), "trademark from &#153;: {text}");
    }

    // ===== Zero-width character stripping =====

    #[test]
    fn zero_width_space_stripped() {
        // ZWSP inside a name should be removed for clean NER tokenization
        let text = strip_to_text("<p>Albert\u{200B}Einstein</p>");
        assert!(text.contains("AlbertEinstein"), "ZWSP stripped: {text}");
    }

    #[test]
    fn soft_hyphen_stripped() {
        let text = strip_to_text("<p>Ein\u{00AD}stein</p>");
        assert!(text.contains("Einstein"), "soft hyphen stripped: {text}");
    }

    #[test]
    fn bom_mid_text_stripped() {
        let text = strip_to_text("<p>Hello\u{FEFF}World</p>");
        assert!(text.contains("HelloWorld"), "mid-text BOM stripped: {text}");
    }

    #[test]
    fn word_joiner_stripped() {
        let text = strip_to_text("<p>Marie\u{2060}Curie</p>");
        assert!(text.contains("MarieCurie"), "word joiner stripped: {text}");
    }

    // ===== Template and SVG skipping =====

    #[test]
    fn template_content_skipped() {
        let html = r#"<html><body>
            <p>Visible content.</p>
            <template><p>Ghost text in template.</p></template>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Visible content"), "visible: {text}");
        assert!(!text.contains("Ghost text"), "template skipped: {text}");
    }

    #[test]
    fn svg_content_skipped() {
        let html = r#"<html><body>
            <p>Article text.</p>
            <svg><text x="10" y="20">Chart Label</text><title>Graph</title></svg>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Article text"), "article preserved: {text}");
        assert!(!text.contains("Chart Label"), "svg text skipped: {text}");
        assert!(!text.contains("Graph"), "svg title skipped: {text}");
    }

    // ===== Image alt text extraction =====

    #[test]
    fn img_alt_text_extracted() {
        let html = r#"<p>The president spoke today.</p>
            <img src="photo.jpg" alt="President Biden at the White House">
            <p>He discussed policy.</p>"#;
        let text = strip_to_text(html);
        assert!(
            text.contains("President Biden at the White House"),
            "alt text extracted: {text}"
        );
        assert!(text.contains("spoke today"), "body preserved: {text}");
    }

    #[test]
    fn img_alt_empty_not_added() {
        let html = r#"<p>Text.</p><img src="spacer.gif" alt=""><p>More.</p>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Text"), "before img: {text}");
        assert!(text.contains("More"), "after img: {text}");
    }

    #[test]
    fn img_no_alt_attribute() {
        let html = r#"<p>Text.</p><img src="photo.jpg"><p>More.</p>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Text"), "before: {text}");
        assert!(text.contains("More"), "after: {text}");
    }

    #[test]
    fn img_alt_in_skipped_region_not_extracted() {
        let html = r#"<nav><img alt="Logo" src="logo.png"></nav><p>Content.</p>"#;
        let text = strip_to_text(html);
        assert!(!text.contains("Logo"), "alt in nav skipped: {text}");
        assert!(text.contains("Content"), "body preserved: {text}");
    }

    // ===== Table cell separation =====

    #[test]
    fn table_cells_separated() {
        // Wikipedia infobox pattern: <th>Key</th><td>Value</td> must not fuse
        let html = r#"<table><tr><th>Country</th><td>England</td></tr>
            <tr><th>Region</th><td>South East</td></tr></table>"#;
        let text = strip_to_text(html);
        assert!(
            !text.contains("CountryEngland"),
            "th/td must be separated: {text}"
        );
        assert!(text.contains("Country"), "th preserved: {text}");
        assert!(text.contains("England"), "td preserved: {text}");
        assert!(
            !text.contains("EnglandRegion"),
            "rows must be separated: {text}"
        );
    }

    #[test]
    fn closing_td_inserts_space() {
        let html = "<td>Apple</td><td>Inc</td>";
        let text = strip_to_text(html);
        assert!(!text.contains("AppleInc"), "cells separated: {text}");
    }

    #[test]
    fn form_elements_stripped() {
        let html = r#"<html><body>
            <p>Article text.</p>
            <form action="/search">
                <input type="text" placeholder="Search...">
                <select><option>Option 1</option></select>
                <button>Submit</button>
            </form>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Article text"), "content preserved: {text}");
        assert!(!text.contains("Search"), "form stripped: {text}");
        assert!(!text.contains("Option 1"), "select stripped: {text}");
    }

    #[test]
    fn textarea_content_stripped() {
        let html = r#"<html><body>
            <p>Article text.</p>
            <textarea>Draft comment text here</textarea>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Article text"), "body preserved: {text}");
        assert!(!text.contains("Draft comment"), "textarea stripped: {text}");
    }

    #[test]
    fn iframe_content_stripped() {
        let html = r#"<html><body>
            <p>Main content.</p>
            <iframe src="ad.html">Fallback ad text</iframe>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Main content"), "body preserved: {text}");
        assert!(!text.contains("Fallback"), "iframe stripped: {text}");
    }

    #[test]
    fn wiki_references_section_stripped() {
        let html = r#"<html><body>
            <p>Main article content about CRISPR gene editing.</p>
            <ol class="references">
                <li id="cite_note-1">Smith J (2024). "Paper title". Nature.</li>
                <li id="cite_note-2">Jones A (2023). "Another paper".</li>
            </ol>
            <p>Conclusion paragraph.</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("CRISPR"), "article preserved: {text}");
        assert!(text.contains("Conclusion"), "conclusion preserved: {text}");
        assert!(!text.contains("cite_note"), "references stripped: {text}");
    }

    #[test]
    fn wiki_navbox_stripped() {
        let html = r#"<html><body>
            <p>Article content.</p>
            <div class="navbox"><table><tr><td>Related articles</td></tr></table></div>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(
            text.contains("Article content"),
            "content preserved: {text}"
        );
        assert!(
            !text.contains("Related articles"),
            "navbox stripped: {text}"
        );
    }

    // ===== Semicolon-optional entity decoding =====

    #[test]
    fn entity_without_semicolon_amp() {
        // &amp without ; should decode to &
        let text = strip_to_text("<p>AT&amp T</p>");
        assert!(
            text.contains("AT& T") || text.contains("AT&"),
            "amp without semi: {text}"
        );
    }

    #[test]
    fn entity_without_semicolon_hellip() {
        // &hellip without ; -> ellipsis
        let text = strip_to_text("<p>Wait&hellip what?</p>");
        assert!(text.contains('\u{2026}'), "hellip without semi: {text}");
    }

    #[test]
    fn entity_without_semicolon_nbsp() {
        // &nbsp without ; -> non-breaking space (collapsed to regular space)
        let text = strip_to_text("<p>Hello&nbsp world</p>");
        assert!(
            text.contains("Hello"),
            "nbsp without semi preserved text: {text}"
        );
    }

    #[test]
    fn entity_without_semicolon_not_greedy() {
        // &T in AT&T should NOT be decoded as an entity
        let text = strip_to_text("<p>AT&amp;T Corporation</p>");
        assert!(text.contains("AT&T"), "AT&T with proper entity: {text}");
    }

    #[test]
    fn entity_without_semicolon_short_passthrough() {
        // Very short &X patterns should pass through, not try entity decode
        let text = strip_to_text("<p>if x &lt 10</p>");
        // &lt without ; should still decode (it's a known entity)
        assert!(
            text.contains('<') || text.contains("lt"),
            "lt without semi: {text}"
        );
    }

    #[test]
    fn entity_without_semicolon_unknown_passthrough() {
        // Unknown entity-like strings without ; should pass through as-is
        let text = strip_to_text("<p>&xyzzy content</p>");
        assert!(
            text.contains("&xyzzy"),
            "unknown entity passes through: {text}"
        );
    }

    #[test]
    fn entity_without_semicolon_eacute() {
        // &eacute without ; -> é (critical for names like Nestlé)
        let text = strip_to_text("<p>Nestl&eacute CEO</p>");
        assert!(text.contains("Nestlé"), "eacute without semi: {text}");
    }

    // ===== Greek letter entities =====

    #[test]
    fn entity_greek_letters() {
        let text = strip_to_text("<p>&alpha;-synuclein and &beta;-amyloid</p>");
        assert!(text.contains('α'), "alpha: {text}");
        assert!(text.contains('β'), "beta: {text}");
    }

    #[test]
    fn entity_greek_uppercase() {
        let text = strip_to_text("<p>&Delta;G = &minus;&Sigma;&Delta;H</p>");
        assert!(text.contains('Δ'), "Delta: {text}");
        assert!(text.contains('Σ'), "Sigma: {text}");
    }

    // ===== C1 range handling =====

    #[test]
    fn c1_unmapped_becomes_replacement() {
        // 0x81, 0x8D, 0x8F, 0x90 have no Win-1252 mapping -> U+FFFD
        let text = strip_to_text("<p>&#129;</p>"); // 0x81
        assert!(text.contains('\u{FFFD}'), "0x81 -> U+FFFD: {text}");
    }

    // ===== Math and symbol entities =====

    #[test]
    fn entity_math_symbols() {
        let text = strip_to_text("<p>&forall;x &exist;y : x &ne; y</p>");
        assert!(text.contains('∀'), "forall: {text}");
        assert!(text.contains('∃'), "exist: {text}");
        assert!(text.contains('≠'), "ne: {text}");
    }

    #[test]
    fn entity_arrows() {
        let text = strip_to_text("<p>A &rarr; B &larr; C</p>");
        assert!(text.contains('→'), "rarr: {text}");
        assert!(text.contains('←'), "larr: {text}");
    }

    // ===== Line break and separator elements =====

    #[test]
    fn br_prevents_word_fusion() {
        let text = strip_to_text("<p>John Smith<br>CEO of Acme</p>");
        assert!(!text.contains("SmithCEO"), "br prevents fusion: {text}");
        assert!(text.contains("John Smith"), "name preserved: {text}");
        assert!(text.contains("CEO"), "title preserved: {text}");
    }

    #[test]
    fn br_self_closing() {
        let text = strip_to_text("<p>Line one<br/>Line two</p>");
        assert!(!text.contains("oneLine"), "br/ prevents fusion: {text}");
    }

    #[test]
    fn wbr_prevents_fusion() {
        let text = strip_to_text("<p>Super<wbr>cali<wbr>fragilistic</p>");
        // wbr inserts space, preventing weird tokenization
        assert!(!text.contains("Supercali"), "wbr inserts space: {text}");
    }

    #[test]
    fn img_alt_entities_decoded() {
        let html = r#"<p>Photo:</p><img alt="Caf&eacute; au lait" src="photo.jpg">"#;
        let text = strip_to_text(html);
        assert!(text.contains("Café"), "entities in alt decoded: {text}");
    }

    #[test]
    fn hr_separates_sections() {
        let text = strip_to_text("<p>Section one</p><hr><p>Section two</p>");
        assert!(!text.contains("oneSection"), "hr prevents fusion: {text}");
    }

    #[test]
    fn definition_list_separated() {
        let html = "<dl><dt>Term</dt><dd>Definition here</dd></dl>";
        let text = strip_to_text(html);
        assert!(!text.contains("TermDefinition"), "dt/dd separated: {text}");
        assert!(text.contains("Term"), "dt preserved: {text}");
        assert!(text.contains("Definition"), "dd preserved: {text}");
    }

    // ===== Bidi mark stripping =====

    #[test]
    fn lrm_stripped() {
        // Left-to-right marks from &lrm; entity should be stripped
        let text = strip_to_text("<p>Hello&lrm; world</p>");
        assert!(!text.contains('\u{200E}'), "LRM stripped: {text}");
        assert!(text.contains("Hello"), "text preserved: {text}");
    }

    #[test]
    fn rlm_stripped() {
        let text = strip_to_text("<p>Hello&rlm; world</p>");
        assert!(!text.contains('\u{200F}'), "RLM stripped: {text}");
    }

    #[test]
    fn bidi_marks_in_raw_text_stripped() {
        // Bidi marks can appear as raw Unicode, not just entities
        let text = strip_to_text("<p>Name\u{200E}\u{200F}Here</p>");
        assert!(text.contains("NameHere"), "bidi marks stripped: {text}");
    }

    // ===== Entity edge cases =====

    #[test]
    fn entity_at_end_of_input() {
        // Entity at very end of input (no terminator at all)
        let text = strip_to_text("<p>Hello &amp");
        assert!(text.contains("Hello"), "text before entity: {text}");
        // &amp without ; at end should try semicolon-optional decode
        assert!(
            text.contains('&') || text.contains("&amp"),
            "entity at end handled: {text}"
        );
    }

    #[test]
    fn entity_numeric_at_end_of_input() {
        let text = strip_to_text("<p>Hello &#169");
        assert!(text.contains("Hello"), "text preserved: {text}");
        // Numeric entities without ; at end pass through
        assert!(text.contains("&#169"), "numeric entity at end: {text}");
    }

    #[test]
    fn double_encoded_entity() {
        // &amp;amp; should decode to &amp; (one round of decoding only)
        let text = strip_to_text("<p>&amp;amp; test</p>");
        assert!(text.contains("&amp;"), "double-encoded stays once: {text}");
    }

    #[test]
    fn adjacent_entities() {
        // Multiple entities with no space between
        let text = strip_to_text("<p>&lt;&gt;&amp;</p>");
        assert_eq!(text, "<>&");
    }

    #[test]
    fn entity_with_leading_hash_garbage() {
        // &#xyz; (non-numeric after #) should pass through
        let text = strip_to_text("<p>&#xyz; text</p>");
        assert!(text.contains("&#xyz;"), "garbage numeric entity: {text}");
    }

    // ===== Tag edge cases =====

    #[test]
    fn empty_tag_name() {
        // < > with just whitespace should not panic
        let text = strip_to_text("<p>Before< >After</p>");
        assert!(text.contains("Before"), "before empty tag: {text}");
        assert!(text.contains("After"), "after empty tag: {text}");
    }

    #[test]
    fn tag_only_slash() {
        // </> should not panic
        let text = strip_to_text("<p>Before</>After</p>");
        assert!(text.contains("Before"), "before: {text}");
        assert!(text.contains("After"), "after: {text}");
    }

    #[test]
    fn deeply_nested_skip_tags() {
        // Three levels of skip tag nesting
        let html = r#"<header><nav><aside>
            <ul><li>Deep hidden content</li></ul>
        </aside></nav></header>
        <p>Visible text.</p>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Visible text"), "visible preserved: {text}");
        assert!(
            !text.contains("Deep hidden"),
            "deeply nested skipped: {text}"
        );
    }

    #[test]
    fn unclosed_script_eats_rest() {
        // An unclosed <script> should suppress all remaining text
        let text = strip_to_text("<p>Before</p><script>alert('hi')");
        assert!(text.contains("Before"), "before script: {text}");
        assert!(!text.contains("alert"), "script content hidden: {text}");
    }

    #[test]
    fn unclosed_style_eats_rest() {
        let text = strip_to_text("<p>Before</p><style>.x{color:red}");
        assert!(text.contains("Before"), "before style: {text}");
        assert!(!text.contains("color"), "style content hidden: {text}");
    }

    #[test]
    fn script_with_html_inside() {
        // Script containing HTML-like strings should not confuse parser
        let html = r#"<script>var s = "<p>fake</p>";</script><p>Real</p>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Real"), "real content: {text}");
        assert!(!text.contains("fake"), "script html not leaked: {text}");
    }

    #[test]
    fn consecutive_block_tags_single_space() {
        // Multiple consecutive block tags should not produce excessive spaces
        let text = strip_to_text("</p><p></p><p>Content</p>");
        assert!(!text.contains("  "), "no double spaces: {text}");
    }

    #[test]
    fn uppercase_tags_handled() {
        // HTML tags can be uppercase
        let text = strip_to_text("<P>Hello</P><DIV>World</DIV>");
        assert!(text.contains("Hello"), "uppercase P: {text}");
        assert!(text.contains("World"), "uppercase DIV: {text}");
        assert!(!text.contains("HelloWorld"), "block separation: {text}");
    }

    #[test]
    fn mixed_case_script_tag() {
        let text = strip_to_text("<SCRIPT>evil()</SCRIPT><p>Safe</p>");
        assert!(text.contains("Safe"), "safe content: {text}");
        assert!(!text.contains("evil"), "script stripped: {text}");
    }

    // ===== decode_entities public API =====

    #[test]
    fn decode_entities_standalone() {
        assert_eq!(decode_entities("Caf&eacute;"), "Café");
        assert_eq!(decode_entities("&#169; 2026"), "\u{00A9} 2026");
        assert_eq!(decode_entities("no entities here"), "no entities here");
        assert_eq!(decode_entities(""), "");
    }

    #[test]
    fn decode_entities_multiple() {
        assert_eq!(
            decode_entities("&lt;div&gt; &amp; &quot;test&quot;"),
            "<div> & \"test\""
        );
    }

    #[test]
    fn decode_entities_mixed_types() {
        // Named + decimal + hex in same string
        assert_eq!(
            decode_entities("&copy; &#8212; &#x2019;"),
            "\u{00A9} \u{2014} \u{2019}"
        );
    }

    // ===== Ruby annotation skipping (CJK) =====

    #[test]
    fn ruby_annotation_stripped() {
        // Japanese furigana: base text preserved, pronunciation stripped
        let html = "<p><ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>を学ぶ</p>";
        let text = strip_to_text(html);
        assert!(text.contains("漢"), "base char 1: {text}");
        assert!(text.contains("字"), "base char 2: {text}");
        assert!(!text.contains("かん"), "rt annotation stripped: {text}");
        assert!(!text.contains("じ"), "rt annotation stripped: {text}");
    }

    #[test]
    fn ruby_rp_stripped() {
        // <rp> provides fallback parentheses for non-ruby browsers
        let html = "<p><ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp>字</ruby></p>";
        let text = strip_to_text(html);
        assert!(text.contains("漢"), "base text: {text}");
        assert!(text.contains("字"), "base text: {text}");
        assert!(!text.contains("かん"), "annotation stripped: {text}");
        assert!(!text.contains('('), "rp parens stripped: {text}");
    }

    #[test]
    fn ruby_in_article_context() {
        // Ruby annotations in a real article-like context
        let html = r#"<article>
            <p><ruby>東京<rt>とうきょう</rt></ruby>で<ruby>安倍<rt>あべ</rt></ruby>首相が会見した。</p>
        </article>"#;
        let text = strip_to_text(html);
        assert!(text.contains("東京"), "Tokyo preserved: {text}");
        assert!(text.contains("安倍"), "Abe preserved: {text}");
        assert!(
            !text.contains("とうきょう"),
            "Tokyo furigana stripped: {text}"
        );
        assert!(!text.contains("あべ"), "Abe furigana stripped: {text}");
    }

    // ===== Expanded bidi control stripping =====

    #[test]
    fn bidi_embedding_controls_stripped() {
        // U+202A-U+202E bidi controls that appear in RTL-mixed content
        let text = strip_to_text("<p>Name\u{202A}\u{202B}\u{202C}Here</p>");
        assert!(text.contains("NameHere"), "bidi embedding stripped: {text}");
    }

    #[test]
    fn bidi_isolate_controls_stripped() {
        // U+2066-U+2069 bidi isolate controls (HTML5)
        let text = strip_to_text("<p>Hello\u{2066}\u{2067}\u{2068}\u{2069}World</p>");
        assert!(text.contains("HelloWorld"), "bidi isolate stripped: {text}");
    }

    #[test]
    fn nbsp_normalized_to_space() {
        // Raw U+00A0 (NBSP) in text should become regular space
        let text = strip_to_text("<p>Hello\u{00A0}World</p>");
        assert!(text.contains("Hello World"), "NBSP normalized: {text}");
        assert!(!text.contains('\u{00A0}'), "no raw NBSP: {text}");
    }

    // ===== Surrogate and noncharacter entity handling =====

    #[test]
    fn surrogate_entity_becomes_replacement() {
        let text = strip_to_text("<p>Before&#xD800;After</p>");
        assert!(text.contains('\u{FFFD}'), "surrogate -> FFFD: {text}");
        assert!(text.contains("Before"), "text preserved: {text}");
        assert!(text.contains("After"), "text preserved: {text}");
    }

    #[test]
    fn high_surrogate_entity_becomes_replacement() {
        let text = strip_to_text("<p>&#xDFFF;</p>");
        assert!(text.contains('\u{FFFD}'), "high surrogate -> FFFD: {text}");
    }

    #[test]
    fn beyond_unicode_entity_becomes_replacement() {
        let text = strip_to_text("<p>&#x110000;</p>");
        assert!(text.contains('\u{FFFD}'), "beyond Unicode -> FFFD: {text}");
    }

    // ===== C0 control character stripping =====

    #[test]
    fn c0_control_chars_stripped() {
        // &#1; through &#8; should not appear in output
        let text = strip_to_text("<p>A&#1;B&#8;C</p>");
        assert!(text.contains("ABC"), "control chars stripped: {text}");
    }

    #[test]
    fn cr_entity_normalized() {
        // &#13; (CR) should be collapsed as whitespace
        let text = strip_to_text("<p>Line1&#13;Line2</p>");
        assert!(text.contains("Line1"), "before CR: {text}");
        assert!(text.contains("Line2"), "after CR: {text}");
        assert!(!text.contains('\r'), "no raw CR: {text}");
    }

    #[test]
    fn del_character_stripped() {
        // &#127; (DEL) should be stripped
        let text = strip_to_text("<p>Hello&#127;World</p>");
        assert!(text.contains("HelloWorld"), "DEL stripped: {text}");
    }

    // ===== Whitespace entity normalization =====

    #[test]
    fn ensp_emsp_thinsp_normalized_to_space() {
        // Unicode whitespace entities should collapse to regular space
        let text = strip_to_text("<p>Hello&ensp;World&emsp;Foo&thinsp;Bar</p>");
        assert!(text.contains("Hello World"), "ensp normalized: {text}");
        assert!(text.contains("World Foo"), "emsp normalized: {text}");
        assert!(text.contains("Foo Bar"), "thinsp normalized: {text}");
        assert!(!text.contains("  "), "no double spaces: {text}");
    }

    // ===== High Unicode / emoji entities =====

    #[test]
    fn emoji_entity_decoded() {
        let text = strip_to_text("<p>Star &#x2B50; emoji</p>");
        assert!(text.contains('\u{2B50}'), "star emoji: {text}");
    }

    #[test]
    fn emoji_supplementary_plane() {
        // Emoji from supplementary plane (above U+FFFF)
        let text = strip_to_text("<p>Rocket &#x1F680; launch</p>");
        assert!(text.contains('\u{1F680}'), "rocket emoji: {text}");
    }

    #[test]
    fn large_valid_codepoint() {
        // U+10FFFF is the last valid Unicode codepoint
        let text = strip_to_text("<p>&#x10FFFF;</p>");
        // char::from_u32(0x10FFFF) returns Some (it's a noncharacter but valid)
        assert!(
            !text.contains("&#x10FFFF;"),
            "large codepoint decoded: {text}"
        );
    }

    // ===== JSON-LD script tag =====

    #[test]
    fn json_ld_script_stripped() {
        let html = r#"<html><body>
            <script type="application/ld+json">
            {"@type": "NewsArticle", "headline": "Test Headline", "author": "John Smith"}
            </script>
            <p>Actual article content here.</p>
        </body></html>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Actual article"), "content preserved: {text}");
        assert!(!text.contains("NewsArticle"), "json-ld stripped: {text}");
        assert!(
            !text.contains("John Smith"),
            "json-ld author stripped: {text}"
        );
    }

    // ===== details/summary pattern =====

    #[test]
    fn details_summary_separated() {
        let html = r#"<details>
            <summary>Click to expand</summary>
            <p>Hidden content revealed on click.</p>
        </details>
        <p>Regular content.</p>"#;
        let text = strip_to_text(html);
        assert!(text.contains("Regular content"), "main content: {text}");
        // details/summary content is included (it's visible in the DOM)
        assert!(text.contains("Click to expand"), "summary: {text}");
        assert!(
            !text.contains("expandHidden"),
            "summary/content separated: {text}"
        );
    }

    // ===== CDATA handling =====

    #[test]
    fn cdata_section_content_dropped() {
        // CDATA sections: content should not appear in output
        // (our parser treats <![CDATA[...]]> as a non-comment <! directive)
        let text = strip_to_text("<p>Before</p><![CDATA[hidden data]]><p>After</p>");
        assert!(text.contains("Before"), "before CDATA: {text}");
        assert!(text.contains("After"), "after CDATA: {text}");
        assert!(
            !text.contains("hidden data"),
            "CDATA content stripped: {text}"
        );
    }

    #[test]
    fn cdata_with_gt_inside() {
        // CDATA containing '>' -- our parser fast-forwards to first '>' in
        // the <! handler, so inner content up to the first '>' is consumed
        // and the rest leaks as text. This is acceptable since CDATA is
        // only valid inside SVG/MathML in HTML5, and SVG is already skipped.
        let text = strip_to_text("<p>Before</p><![CDATA[a > b]]><p>After</p>");
        assert!(text.contains("Before"), "before: {text}");
        assert!(text.contains("After"), "after: {text}");
        // Note: " b]]" may leak due to first-'>' termination. This is a
        // known limitation for the rare CDATA-in-body case.
    }

    // ===== Malformed HTML resilience =====

    #[test]
    fn mismatched_close_tags_no_panic() {
        // Close tags that don't match opens -- should not panic or corrupt output
        let text = strip_to_text("<p>Hello</div></span>World</p>");
        assert!(text.contains("Hello"), "before mismatched: {text}");
        assert!(text.contains("World"), "after mismatched: {text}");
    }

    #[test]
    fn deeply_nested_100_levels() {
        // 100+ levels of tag nesting
        let mut html = String::new();
        for _ in 0..100 {
            html.push_str("<div>");
        }
        html.push_str("Deep content");
        for _ in 0..100 {
            html.push_str("</div>");
        }
        let text = strip_to_text(&html);
        assert!(text.contains("Deep content"), "deep nesting works: {text}");
    }

    #[test]
    fn entity_overflow_passthrough() {
        // Huge numeric entity that exceeds u32 -- should pass through as-is
        let text = strip_to_text("<p>&#99999999999;</p>");
        assert!(
            text.contains("&#99999999999;"),
            "overflow entity passes through: {text}"
        );
    }

    // ===== Whitespace between inline and block elements =====

    #[test]
    fn inline_tags_no_extra_space() {
        // Inline tags (b, i, span, a) should NOT insert spaces
        let text = strip_to_text("<p>Hello <b>bold</b> and <i>italic</i> text</p>");
        assert_eq!(text, "Hello bold and italic text");
    }

    #[test]
    fn list_items_separated() {
        let text = strip_to_text("<ul><li>Apple</li><li>Banana</li><li>Cherry</li></ul>");
        assert!(
            !text.contains("AppleBanana"),
            "list items separated: {text}"
        );
        assert!(
            !text.contains("BananaCherry"),
            "list items separated: {text}"
        );
        assert!(text.contains("Apple"), "item 1: {text}");
        assert!(text.contains("Banana"), "item 2: {text}");
        assert!(text.contains("Cherry"), "item 3: {text}");
    }

    // ===== Central/Eastern European entity decoding (Latin Extended-A) =====

    #[test]
    fn entity_polish_names() {
        // Polish characters critical for NER: Ł, ą, ć, ę, ł, ń, ś, ź, ż
        let html = "<p>Jaros&lstrok;aw Kaczy&nacute;ski and &Lstrok;&oacute;d&zacute;</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Jarosław"), "lstrok decoded: {text}");
        assert!(text.contains("Kaczyński"), "nacute decoded: {text}");
        assert!(
            text.contains("Łódź"),
            "Lstrok+oacute+zacute decoded: {text}"
        );
    }

    #[test]
    fn entity_czech_names() {
        // Czech characters: Č, č, Ď, ď, Ě, ě, Ň, ň, Ř, ř, Š, š, Ť, ť, Ž, ž
        let html = "<p>&Ccaron;esk&aacute; republika: Alena &Scaron;eredov&aacute; from Pra&zcaron;sk&yacute;</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Česká"), "Ccaron decoded: {text}");
        assert!(text.contains("Šeredová"), "Scaron decoded: {text}");
        assert!(text.contains("Pražský"), "zcaron decoded: {text}");
    }

    #[test]
    fn entity_turkish_names() {
        // Turkish characters: Ğ, ğ, İ, ı, Ş, ş
        let html = "<p>Recep Tayyip Erdo&gbreve;an visited &Idot;stanbul and Mu&gbreve;la</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Erdoğan"), "gbreve decoded: {text}");
        assert!(text.contains("İstanbul"), "Idot decoded: {text}");
        assert!(text.contains("Muğla"), "gbreve lowercase decoded: {text}");
    }

    #[test]
    fn entity_hungarian_names() {
        // Hungarian characters: Ő, ő, Ű, ű
        let html = "<p>The Hungarian city of Gy&odblac;r and Sz&udblac;cs</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Győr"), "odblac decoded: {text}");
        assert!(text.contains("Szűcs"), "udblac decoded: {text}");
    }

    #[test]
    fn entity_romanian_names() {
        // Romanian characters: Ă, ă, Ş/Ț (Ţ cedilla form)
        let html = "<p>&Abreve;r&abreve;d in Romania; &Tcedil;ucureanu is a surname</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Ărăd"), "Abreve decoded: {text}");
        assert!(text.contains("Ţucureanu"), "Tcedil decoded: {text}");
    }

    #[test]
    fn entity_croatian_names() {
        // Croatian characters: Đ, đ
        let html = "<p>Novak &Dstrok;okovi&cacute; (Serbian) and &Dstrok;ur&dstrok;a</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Đoković"), "Dstrok+cacute decoded: {text}");
        assert!(text.contains("Đurđa"), "Dstrok+dstrok decoded: {text}");
    }

    #[test]
    fn entity_slovak_names() {
        // Slovak characters: Ľ, ľ, Ŕ, ŕ, Ť, ť
        let html = "<p>&Lcaron;ubom&iacute;r and the city of Bansk&aacute; Bystrica with &tcaron;a&rcaron;</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Ľubomír"), "Lcaron+iacute decoded: {text}");
        assert!(text.contains("ťař"), "tcaron+rcaron decoded: {text}");
    }

    #[test]
    fn entity_dotless_i_turkish() {
        // Turkish dotless i (ı) is distinct from Latin i -- critical for NER
        let html = "<p>D&inodot;yarbak&inodot;r is a city in Turkey</p>";
        let text = strip_to_text(html);
        assert!(text.contains("Dıyarbakır"), "inodot decoded: {text}");
    }
}
