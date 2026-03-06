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
                in_tag = true;
                let mut tag_buffer = String::new();
                tag_buffer.push('<');
                let mut tag_name = String::new();
                let mut in_tag_name = true;

                while let Some(&next_ch) = chars.peek() {
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
                        tag_buffer.push(chars.next().expect("peek returned Some"));
                    }
                }

                // Insert space after block-level elements for readability
                if !in_script
                    && !in_style
                    && skip_depth == 0
                    && matches!(
                        tag_name.to_lowercase().as_str(),
                        "p" | "div" | "br" | "li" | "ul" | "ol"
                            | "td" | "th" | "tr" | "dt" | "dd"
                            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                            | "section" | "article" | "header" | "footer"
                            | "aside" | "main" | "blockquote" | "figcaption"
                            | "figure" | "details" | "summary"
                    )
                    && !text.ends_with(' ')
                    && !text.is_empty()
                {
                    text.push(' ');
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

    // Collapse whitespace (HTML rendering semantics)
    let mut cleaned = String::with_capacity(text.len());
    let mut last_was_space = true;
    for ch in text.chars() {
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

    // Strip Wikipedia reference markers
    let cleaned = WIKI_REF_BRACKET.replace_all(cleaned.trim(), "");
    cleaned.trim().to_string()
}

/// Decode an HTML entity starting after the `&`. Pushes the decoded
/// character(s) into `text`.
fn decode_entity(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, text: &mut String) {
    let mut entity = String::from("&");
    let mut found_semicolon = false;

    while let Some(&next_ch) = chars.peek() {
        entity.push(chars.next().expect("peek returned Some"));
        if next_ch == ';' {
            found_semicolon = true;
            break;
        }
        if next_ch.is_whitespace() || next_ch == '<' {
            break;
        }
    }

    if found_semicolon {
        match entity.as_str() {
            "&amp;" => text.push('&'),
            "&lt;" => text.push('<'),
            "&gt;" => text.push('>'),
            "&quot;" => text.push('"'),
            "&apos;" => text.push('\''),
            "&nbsp;" => text.push(' '),
            "&#39;" => text.push('\''),
            "&#8217;" => text.push('\u{2019}'),
            "&#8220;" => text.push('\u{201C}'),
            "&#8221;" => text.push('\u{201D}'),
            _ => {
                // Try numeric entity (decimal &#N; or hex &#xN;)
                if entity.starts_with("&#") && entity.len() > 2 {
                    let num_str = &entity[2..entity.len() - 1];
                    let parsed = if let Some(hex) =
                        num_str.strip_prefix('x').or_else(|| num_str.strip_prefix('X'))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num_str.parse::<u32>().ok()
                    };
                    if let Some(ch) = parsed.and_then(char::from_u32) {
                        text.push(ch);
                        return;
                    }
                }
                text.push_str(&entity);
            }
        }
    } else {
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
}
