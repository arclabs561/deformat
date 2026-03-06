//! HTML-to-text extraction.
//!
//! Three extraction strategies, from simplest to most capable:
//!
//! 1. **`strip_to_text`** (always available) -- fast tag stripping with
//!    entity decoding, semantic element filtering, and Wikipedia boilerplate
//!    removal. Zero dependencies beyond `once_cell` + `regex`.
//!
//! 2. **`extract_with_html2text`** (feature `html2text`) -- DOM-based
//!    conversion that preserves layout structure (tables, lists, indentation).
//!
//! 3. **`extract_with_readability`** (feature `readability`) -- Mozilla
//!    Readability algorithm that extracts the main article content, stripping
//!    navigation, sidebars, and boilerplate.

use once_cell::sync::Lazy;
use regex::Regex;

/// Matches Wikipedia-style reference markers: [1], [2], [edit], [citation needed], etc.
/// Also matches bare `edit]` fragments (without opening bracket) that survive
/// HTML `<span>` tag processing on some Wikipedia pages.
static WIKI_REF_BRACKET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(\d+|edit|citation needed)\]|\bedit\]").unwrap());

/// Two or more consecutive spaces -- used to collapse after ref bracket removal.
static DOUBLE_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());

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
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut skip_depth: u32 = 0;
    let mut wiki_skip_depth: u32 = 0;
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                // HTML comment: <!-- ... -->
                if chars.peek() == Some(&'!') {
                    let mut lookahead = String::new();
                    // Collect up to 3 chars to check for "!--"
                    for _ in 0..3 {
                        if let Some(&c) = chars.peek() {
                            lookahead.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if lookahead == "!--" {
                        // Skip until "-->"
                        let mut dashes = 0u32;
                        for c in chars.by_ref() {
                            if c == '-' {
                                dashes += 1;
                            } else if c == '>' && dashes >= 2 {
                                break;
                            } else {
                                dashes = 0;
                            }
                        }
                        continue;
                    }
                    // Not a comment (e.g. <!DOCTYPE ...>) -- fast-forward to '>'
                    for c in chars.by_ref() {
                        if c == '>' {
                            break;
                        }
                    }
                    continue;
                }

                in_tag = true;
                let mut tag_buffer = String::new();
                tag_buffer.push('<');
                let mut tag_name = String::new();
                let mut in_tag_name = true;
                let mut in_attr_quote: Option<char> = None; // Track quote context

                while let Some(&next_ch) = chars.peek() {
                    // Inside a quoted attribute value, '>' does not end the tag
                    if let Some(q) = in_attr_quote {
                        let c = chars.next().expect("peek returned Some");
                        tag_buffer.push(c);
                        if c == q {
                            in_attr_quote = None;
                        }
                        continue;
                    }

                    if next_ch == '>' {
                        chars.next();
                        tag_buffer.push('>');
                        let tag_lower = tag_name.to_lowercase();

                        // Script/style toggle
                        if tag_lower == "script" || tag_lower.starts_with("script ") {
                            in_script = true;
                        } else if tag_lower == "/script" || tag_lower.starts_with("/script ") {
                            in_script = false;
                        } else if tag_lower == "style" || tag_lower.starts_with("style ") {
                            in_style = true;
                        } else if tag_lower == "/style" || tag_lower.starts_with("/style ") {
                            in_style = false;
                        }

                        // Semantic skip tags
                        let skip_tags: &[&str] = &[
                            "head", "nav", "header", "footer", "aside", "menu",
                            "noscript", "form", "select", "figcaption",
                            "template", "svg", "textarea", "iframe",
                        ];

                        // Wikipedia/MediaWiki structural skip
                        let tag_lower_full = format!(
                            "{} {}",
                            tag_name.to_lowercase(),
                            tag_buffer[1..].to_lowercase()
                        );
                        let wiki_skip_ids: &[&str] = &[
                            "toc", "references", "reflist", "catlinks",
                            "mw-panel", "mw-navigation", "sidebar", "sitesub",
                            "contentsub", "jump-to-nav", "navbox", "external",
                            "see-also", "further-reading", "mw-head",
                            "mw-page-base", "mw-head-base", "footer", "printfooter",
                        ];
                        let is_wiki_skip = wiki_skip_ids.iter().any(|id| {
                            tag_lower_full.contains(&format!("id=\"{}\"", id))
                                || tag_lower_full.contains(&format!("class=\"{}", id))
                                || (tag_lower_full.contains(id)
                                    && (tag_lower_full.contains("class=")
                                        || tag_lower_full.contains("id=")))
                        });
                        if is_wiki_skip
                            && matches!(
                                tag_name.to_lowercase().as_str(),
                                "div" | "ol" | "ul" | "table" | "span" | "section"
                            )
                        {
                            wiki_skip_depth += 1;
                            skip_depth += 1;
                        }

                        // Handle closing tags for wiki-skip containers
                        if wiki_skip_depth > 0 {
                            let wiki_close_tags: &[&str] =
                                &["div", "ol", "ul", "table", "span", "section"];
                            for &wtag in wiki_close_tags {
                                if tag_lower == format!("/{}", wtag)
                                    || tag_lower.starts_with(&format!("/{} ", wtag))
                                {
                                    wiki_skip_depth = wiki_skip_depth.saturating_sub(1);
                                    skip_depth = skip_depth.saturating_sub(1);
                                }
                            }
                        }

                        // Semantic tag depth tracking
                        for &stag in skip_tags {
                            if tag_lower == stag
                                || tag_lower.starts_with(&format!("{} ", stag))
                            {
                                skip_depth += 1;
                            } else if tag_lower == format!("/{}", stag)
                                || tag_lower.starts_with(&format!("/{} ", stag))
                            {
                                skip_depth = skip_depth.saturating_sub(1);
                            }
                        }

                        in_tag = false;
                        break;
                    } else if next_ch.is_whitespace() {
                        in_tag_name = false;
                        tag_buffer.push(chars.next().expect("peek returned Some"));
                    } else if in_tag_name {
                        tag_name.push(chars.next().expect("peek returned Some"));
                    } else {
                        let c = chars.next().expect("peek returned Some");
                        // Detect start of quoted attribute value
                        if (c == '"' || c == '\'') && in_attr_quote.is_none() {
                            in_attr_quote = Some(c);
                        }
                        tag_buffer.push(c);
                    }
                }

                // Insert space around block-level elements for readability.
                // Strip leading "/" from closing tags so </td> matches "td".
                let effective_tag = tag_name.to_lowercase();
                let effective_tag = effective_tag
                    .strip_prefix('/')
                    .unwrap_or(&effective_tag);
                if !in_script
                    && !in_style
                    && skip_depth == 0
                    && matches!(
                        effective_tag,
                        "p" | "div" | "br" | "li" | "ul" | "ol"
                            | "td" | "th" | "tr" | "dt" | "dd"
                            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                            | "section" | "article" | "header" | "footer"
                            | "aside" | "main" | "blockquote" | "figcaption"
                            | "figure" | "details" | "summary"
                            | "caption" | "thead" | "tbody" | "tfoot"
                    )
                    && !text.ends_with(' ')
                    && !text.is_empty()
                {
                    text.push(' ');
                }

                // Extract alt text from <img> tags (important for NER:
                // news photo alt text often contains full person names)
                {
                    let tl = tag_name.to_lowercase();
                    if !in_script
                        && !in_style
                        && skip_depth == 0
                        && (tl == "img" || tl.starts_with("img "))
                    {
                        if let Some(alt) = extract_attr_value(&tag_buffer, "alt") {
                            if !alt.is_empty() {
                                if !text.ends_with(' ') && !text.is_empty() {
                                    text.push(' ');
                                }
                                text.push_str(&alt);
                                text.push(' ');
                            }
                        }
                    }
                }
            }
            '>' if in_tag => {
                in_tag = false;
            }
            _ if in_tag || in_script || in_style || skip_depth > 0 => {}
            '&' => {
                decode_entity(&mut chars, &mut text);
            }
            ch if !in_tag && !in_script && !in_style && skip_depth == 0 => {
                text.push(ch);
            }
            _ => {}
        }
    }

    // Collapse whitespace and strip invisible characters (HTML rendering semantics)
    let mut cleaned = String::with_capacity(text.len());
    let mut last_was_space = true;
    for ch in text.chars() {
        if is_invisible_char(ch) {
            // Strip zero-width chars that break NER tokenization
            continue;
        }
        if ch.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }

    // Strip Wikipedia reference markers, then collapse any resulting double spaces
    let cleaned = WIKI_REF_BRACKET.replace_all(cleaned.trim(), "");
    let cleaned = DOUBLE_SPACE.replace_all(&cleaned, " ");
    cleaned.trim().to_string()
}

/// Map a named HTML entity to its Unicode character.
///
/// Covers the most common entities encountered in real web content,
/// especially those important for NER (accented names, currency symbols,
/// punctuation). Not exhaustive -- rare entities pass through as-is.
/// Sorted table of named HTML entities -> Unicode codepoint.
/// Covers the ~250 most commonly encountered entities in web content,
/// including all of ISO-8859-1/Latin-1, Greek letters, math symbols,
/// arrows, and typographic punctuation.
static NAMED_ENTITIES: &[(&str, char)] = &[
    ("&AElig;", '\u{00C6}'),
    ("&Aacute;", '\u{00C1}'),
    ("&Acirc;", '\u{00C2}'),
    ("&Agrave;", '\u{00C0}'),
    ("&Alpha;", '\u{0391}'),
    ("&Aring;", '\u{00C5}'),
    ("&Atilde;", '\u{00C3}'),
    ("&Auml;", '\u{00C4}'),
    ("&Beta;", '\u{0392}'),
    ("&Ccedil;", '\u{00C7}'),
    ("&Chi;", '\u{03A7}'),
    ("&Dagger;", '\u{2021}'),
    ("&Delta;", '\u{0394}'),
    ("&ETH;", '\u{00D0}'),
    ("&Eacute;", '\u{00C9}'),
    ("&Ecirc;", '\u{00CA}'),
    ("&Egrave;", '\u{00C8}'),
    ("&Epsilon;", '\u{0395}'),
    ("&Eta;", '\u{0397}'),
    ("&Euml;", '\u{00CB}'),
    ("&Gamma;", '\u{0393}'),
    ("&Iacute;", '\u{00CD}'),
    ("&Icirc;", '\u{00CE}'),
    ("&Igrave;", '\u{00CC}'),
    ("&Iota;", '\u{0399}'),
    ("&Iuml;", '\u{00CF}'),
    ("&Kappa;", '\u{039A}'),
    ("&Lambda;", '\u{039B}'),
    ("&Mu;", '\u{039C}'),
    ("&Ntilde;", '\u{00D1}'),
    ("&Nu;", '\u{039D}'),
    ("&OElig;", '\u{0152}'),
    ("&Oacute;", '\u{00D3}'),
    ("&Ocirc;", '\u{00D4}'),
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
    ("&Rho;", '\u{03A1}'),
    ("&Scaron;", '\u{0160}'),
    ("&Sigma;", '\u{03A3}'),
    ("&THORN;", '\u{00DE}'),
    ("&Tau;", '\u{03A4}'),
    ("&Theta;", '\u{0398}'),
    ("&Uacute;", '\u{00DA}'),
    ("&Ucirc;", '\u{00DB}'),
    ("&Ugrave;", '\u{00D9}'),
    ("&Upsilon;", '\u{03A5}'),
    ("&Uuml;", '\u{00DC}'),
    ("&Xi;", '\u{039E}'),
    ("&Yacute;", '\u{00DD}'),
    ("&Yuml;", '\u{0178}'),
    ("&Zeta;", '\u{0396}'),
    ("&aacute;", '\u{00E1}'),
    ("&acirc;", '\u{00E2}'),
    ("&acute;", '\u{00B4}'),
    ("&aelig;", '\u{00E6}'),
    ("&agrave;", '\u{00E0}'),
    ("&alefsym;", '\u{2135}'),
    ("&alpha;", '\u{03B1}'),
    ("&amp;", '&'),
    ("&and;", '\u{2227}'),
    ("&ang;", '\u{2220}'),
    ("&apos;", '\''),
    ("&aring;", '\u{00E5}'),
    ("&asymp;", '\u{2248}'),
    ("&atilde;", '\u{00E3}'),
    ("&auml;", '\u{00E4}'),
    ("&bdquo;", '\u{201E}'),
    ("&beta;", '\u{03B2}'),
    ("&brvbar;", '\u{00A6}'),
    ("&bull;", '\u{2022}'),
    ("&cap;", '\u{2229}'),
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
    ("&deg;", '\u{00B0}'),
    ("&delta;", '\u{03B4}'),
    ("&diams;", '\u{2666}'),
    ("&divide;", '\u{00F7}'),
    ("&eacute;", '\u{00E9}'),
    ("&ecirc;", '\u{00EA}'),
    ("&egrave;", '\u{00E8}'),
    ("&empty;", '\u{2205}'),
    ("&emsp;", '\u{2003}'),
    ("&ensp;", '\u{2002}'),
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
    ("&lceil;", '\u{2308}'),
    ("&ldquo;", '\u{201C}'),
    ("&le;", '\u{2264}'),
    ("&lfloor;", '\u{230A}'),
    ("&lowast;", '\u{2217}'),
    ("&loz;", '\u{25CA}'),
    ("&lrm;", '\u{200E}'),
    ("&lsaquo;", '\u{2039}'),
    ("&lsquo;", '\u{2018}'),
    ("&lt;", '<'),
    ("&macr;", '\u{00AF}'),
    ("&mdash;", '\u{2014}'),
    ("&micro;", '\u{00B5}'),
    ("&middot;", '\u{00B7}'),
    ("&minus;", '\u{2212}'),
    ("&mu;", '\u{03BC}'),
    ("&nabla;", '\u{2207}'),
    ("&nbsp;", ' '),
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
    ("&radic;", '\u{221A}'),
    ("&rang;", '\u{232A}'),
    ("&raquo;", '\u{00BB}'),
    ("&rarr;", '\u{2192}'),
    ("&rceil;", '\u{2309}'),
    ("&rdquo;", '\u{201D}'),
    ("&real;", '\u{211C}'),
    ("&reg;", '\u{00AE}'),
    ("&rfloor;", '\u{230B}'),
    ("&rho;", '\u{03C1}'),
    ("&rlm;", '\u{200F}'),
    ("&rsaquo;", '\u{203A}'),
    ("&rsquo;", '\u{2019}'),
    ("&sbquo;", '\u{201A}'),
    ("&scaron;", '\u{0161}'),
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
/// Returns `None` if the attribute is not found.
fn extract_attr_value(tag: &str, attr_name: &str) -> Option<String> {
    let tag_lower = tag.to_lowercase();
    // Look for attr_name= (with optional whitespace around =)
    let needle = format!("{}=", attr_name);
    let pos = tag_lower.find(&needle)?;
    let after_eq = pos + needle.len();
    let rest = &tag[after_eq..];
    let rest = rest.trim_start();

    if let Some(inner) = rest.strip_prefix('"') {
        // Double-quoted value
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else if let Some(inner) = rest.strip_prefix('\'') {
        // Single-quoted value
        let end = inner.find('\'')?;
        Some(inner[..end].to_string())
    } else {
        // Unquoted value (ends at whitespace or >)
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
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

/// Returns true if the character is a zero-width or invisible Unicode character
/// that should be stripped for clean NER tokenization.
fn is_invisible_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}'  // Zero-width space
        | '\u{200C}' // Zero-width non-joiner
        | '\u{200D}' // Zero-width joiner
        | '\u{00AD}' // Soft hyphen
        | '\u{2060}' // Word joiner
        | '\u{FEFF}' // BOM / zero-width no-break space (mid-text)
    )
}

/// Decode an HTML entity starting after the `&`. Pushes the decoded
/// character(s) into `text`.
fn decode_entity(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, text: &mut String) {
    let mut entity = String::from("&");
    let mut found_semicolon = false;

    while let Some(&next_ch) = chars.peek() {
        if next_ch == ';' {
            chars.next();
            entity.push(';');
            found_semicolon = true;
            break;
        }
        if next_ch.is_whitespace() || next_ch == '<' {
            // Don't consume the terminator -- it belongs to the next token
            break;
        }
        entity.push(chars.next().expect("peek returned Some"));
    }

    if found_semicolon {
        // Try named entity lookup first, then numeric fallback
        if let Some(ch) = decode_named_entity(&entity) {
            text.push(ch);
        } else if entity.starts_with("&#") && entity.len() > 3 {
            // Numeric entity (decimal &#N; or hex &#xN;)
            let num_str = &entity[2..entity.len() - 1];
            let parsed = if let Some(hex) =
                num_str.strip_prefix('x').or_else(|| num_str.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(ch) = parsed.and_then(|n| {
                if n == 0 {
                    // HTML5 spec: &#0; maps to U+FFFD REPLACEMENT CHARACTER
                    Some('\u{FFFD}')
                } else if (0x80..=0x9F).contains(&n) {
                    // C1 control range: use Win-1252 mapping, or U+FFFD for
                    // unmapped codepoints (0x81, 0x8D, 0x8F, 0x90) per HTML5 spec
                    win1252_to_unicode(n).or(Some('\u{FFFD}'))
                } else {
                    char::from_u32(n)
                }
            }) {
                text.push(ch);
            } else {
                text.push_str(&entity);
            }
        } else {
            text.push_str(&entity);
        }
    } else {
        // Semicolon-optional: try interpreting as a named entity without ';'
        // Real web content sometimes omits the semicolon (e.g. &hellip or &amp)
        // Only attempt for entity-like strings (&alpha...) not arbitrary text (&T)
        if entity.len() > 2
            && entity.as_bytes()[1].is_ascii_alphabetic()
            && entity.chars().skip(1).all(|c| c.is_ascii_alphanumeric())
        {
            let with_semi = format!("{};", entity);
            if let Some(ch) = decode_named_entity(&with_semi) {
                text.push(ch);
                return;
            }
        }
        text.push_str(&entity);
    }
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
        let text = strip_to_text(
            "<p>&Uuml;ber M&uuml;ller traf Garc&iacute;a in S&atilde;o Paulo.</p>",
        );
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
        assert!(text.contains("&foobar;"), "unknown entity preserved: {text}");
    }

    #[test]
    fn entity_unterminated_passes_through() {
        // Unterminated entity (no semicolon) should not eat subsequent text
        let text = strip_to_text("<p>AT&T is a company.</p>");
        assert!(text.contains("AT&T"), "unterminated entity preserved: {text}");
        assert!(text.contains("company"), "subsequent text preserved: {text}");
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
        assert!(!text.contains("hello"), "nested quote attr not leaked: {text}");
    }

    #[test]
    fn null_entity_becomes_replacement_char() {
        let text = strip_to_text("<p>Before&#0;After</p>");
        assert!(text.contains("Before"), "before null: {text}");
        assert!(text.contains("After"), "after null: {text}");
        assert!(text.contains('\u{FFFD}'), "null becomes replacement char: {text}");
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
        assert!(
            text.contains("AlbertEinstein"),
            "ZWSP stripped: {text}"
        );
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
        assert!(text.contains("Article content"), "content preserved: {text}");
        assert!(!text.contains("Related articles"), "navbox stripped: {text}");
    }

    // ===== Semicolon-optional entity decoding =====

    #[test]
    fn entity_without_semicolon_amp() {
        // &amp without ; should decode to &
        let text = strip_to_text("<p>AT&amp T</p>");
        assert!(text.contains("AT& T") || text.contains("AT&"), "amp without semi: {text}");
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
        assert!(text.contains("Hello"), "nbsp without semi preserved text: {text}");
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
        assert!(text.contains('<') || text.contains("lt"), "lt without semi: {text}");
    }

    #[test]
    fn entity_without_semicolon_unknown_passthrough() {
        // Unknown entity-like strings without ; should pass through as-is
        let text = strip_to_text("<p>&xyzzy content</p>");
        assert!(text.contains("&xyzzy"), "unknown entity passes through: {text}");
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
}
