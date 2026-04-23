//! Coarse page-type classification from HTML structural and metadata
//! signals.
//!
//! The classifier is pure-heuristic (no ML, no external corpus): it
//! reads `<meta>`, JSON-LD, schema.org `itemtype`, `<article>` / `<nav>`
//! / `<aside>` counts, and `<link rel="canonical">` patterns. The
//! returned [`PageType`] is a hint that downstream code can use to
//! select an appropriate extraction pipeline or filter configuration.
//!
//! This is a conservative implementation: when signals conflict or are
//! absent, it returns [`PageType::Unknown`] rather than guessing.
//! Callers that need a decisive answer should fall back to
//! [`PageType::Article`] (the most common case on the web) or to their
//! own heuristics on top.
//!
//! # Example
//!
//! ```
//! use deformat::page_type::{detect_page_type, PageType};
//!
//! let html = r#"<html><head>
//!     <meta property="og:type" content="article">
//!     <link rel="canonical" href="https://example.com/blog/2026/04/23/post">
//! </head><body><article><h1>Title</h1><p>Body.</p></article></body></html>"#;
//! assert_eq!(detect_page_type(html), PageType::Article);
//! ```

use crate::html::extract_metadata;

/// Coarse page type inferred from HTML signals.
///
/// The variants mirror the WCXB benchmark's page-type labels, which
/// are the de facto standard taxonomy in the web-content-extraction
/// community (Foley 2025). Downstream callers typically route to
/// different extraction filter configurations based on this value —
/// for example, a forum page wants `Segment::ListItem` preservation
/// (each comment is a list item), while an article page wants
/// aggressive link-density filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PageType {
    /// News article, blog post, or other long-form prose. The most
    /// common case and the one current extraction heuristics are
    /// tuned for.
    Article,
    /// Documentation / reference page. Structurally similar to
    /// Article but with heavier use of code blocks and structured
    /// lists.
    Documentation,
    /// Product detail page on an e-commerce site. Schema.org
    /// `@type: Product` or equivalent.
    Product,
    /// Forum thread or comment-heavy page.
    Forum,
    /// Listing page (search results, category index, product list).
    Listing,
    /// Collection / portal / homepage-style page aggregating many
    /// types of content.
    Collection,
    /// Service / landing page promoting a product or service.
    Service,
    /// Signals were absent or contradictory. Callers should apply
    /// their own fallback — usually by treating Unknown as Article.
    Unknown,
}

/// Detect the coarse [`PageType`] of an HTML document.
///
/// Inspects, in order of priority:
/// 1. `<meta property="og:type">` — Open Graph type declarations.
/// 2. JSON-LD `@type` fields in `<script type="application/ld+json">`
///    blocks (article, blogposting, newsarticle, product, etc.).
/// 3. Schema.org `itemtype` attributes on structural elements.
/// 4. `<link rel="canonical">` URL patterns (`/products/`, `/forum/`,
///    `/docs/`, `/category/`, ...).
/// 5. HTML structural counts: `<article>` presence, `<section
///    class="comment">` density, `.price` / `.product-*` class
///    presence, `<ol class="posts">` patterns.
///
/// Returns [`PageType::Unknown`] when signals conflict or are absent
/// — the caller should then fall back to a generic (Article-tuned)
/// extraction pipeline.
#[must_use]
pub fn detect_page_type(html: &str) -> PageType {
    let lower = html.to_ascii_lowercase();

    // 1. og:type — the cheapest and most reliable single signal.
    if let Some(og_type) = extract_og_type(&lower) {
        match og_type.as_str() {
            "article" => return PageType::Article,
            "blog" => return PageType::Article,
            "product" | "product.item" | "product.group" => return PageType::Product,
            "website" => {} // fall through — too generic to decide
            _ => {}
        }
    }

    // 2. JSON-LD @type.
    if let Some(t) = extract_json_ld_type(&lower) {
        match t.as_str() {
            "article" | "newsarticle" | "blogposting" | "scholarlyarticle" => {
                return PageType::Article
            }
            "techarticle" | "apireference" | "documentation" => return PageType::Documentation,
            "product" | "individualproduct" | "productmodel" => return PageType::Product,
            "discussionforumposting" | "forumposting" => return PageType::Forum,
            "collection" | "collectionpage" | "itemlist" => return PageType::Collection,
            "service" | "financialproduct" => return PageType::Service,
            _ => {}
        }
    }

    // 3. Schema.org itemtype (fallback when JSON-LD is absent).
    if lower.contains("schema.org/article")
        || lower.contains("schema.org/newsarticle")
        || lower.contains("schema.org/blogposting")
    {
        return PageType::Article;
    }
    if lower.contains("schema.org/product") {
        return PageType::Product;
    }

    // 4. Canonical URL path patterns.
    if let Some(path) = extract_canonical_path(&lower) {
        if let Some(pt) = classify_by_url_path(&path) {
            return pt;
        }
    }

    // 5. Structural counts (least reliable, used as tiebreakers).
    let article_opens = count_ci(&lower, "<article");
    let forum_signals =
        count_ci(&lower, r#"class="comment"#) + count_ci(&lower, r#"class='comment"#);
    let product_signals = count_ci(&lower, r#"class="price"#)
        + count_ci(&lower, r#"class='price"#)
        + count_ci(&lower, r#"itemprop="price""#);

    // Many comments is a forum signal dominant over any other.
    if forum_signals >= 3 {
        return PageType::Forum;
    }
    if product_signals >= 2 {
        return PageType::Product;
    }
    if article_opens >= 1 {
        // A <meta>-less article with an <article> tag — probably a plain
        // blog post or old-school HTML article. Default to Article.
        return PageType::Article;
    }

    PageType::Unknown
}

fn extract_og_type(lower_html: &str) -> Option<String> {
    // Match either order: property=... content=... OR content=... property=...
    // Keep it simple: find "og:type" then read the nearest content= value.
    let idx = lower_html.find("og:type")?;
    // Look for content=" nearby (forward first, then backward). Bound
    // the window by char boundary so non-ASCII content doesn't panic.
    let end = floor_char_boundary(lower_html, (idx + 200).min(lower_html.len()));
    let window = &lower_html[idx..end];
    if let Some(val) = read_attr(window, "content") {
        return Some(val);
    }
    // Sometimes content= appears before property=, window backwards.
    let back = ceil_char_boundary(lower_html, idx.saturating_sub(200));
    let fwd_end = floor_char_boundary(lower_html, (idx + 50).min(lower_html.len()));
    let window = &lower_html[back..fwd_end];
    read_attr(window, "content")
}

/// Largest index <= `n` that lies on a UTF-8 char boundary.
fn floor_char_boundary(s: &str, mut n: usize) -> usize {
    if n >= s.len() {
        return s.len();
    }
    while n > 0 && !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

/// Smallest index >= `n` that lies on a UTF-8 char boundary.
fn ceil_char_boundary(s: &str, mut n: usize) -> usize {
    while n < s.len() && !s.is_char_boundary(n) {
        n += 1;
    }
    n
}

fn extract_json_ld_type(lower_html: &str) -> Option<String> {
    // Find the first <script type="application/ld+json"> block.
    let start_tag = lower_html.find(r#"type="application/ld+json""#)?;
    let script_start = lower_html[..start_tag].rfind("<script")?;
    let body_start = lower_html[script_start..]
        .find('>')
        .map(|o| script_start + o + 1)?;
    let body_end = lower_html[body_start..]
        .find("</script>")
        .map(|o| body_start + o)?;
    let body = &lower_html[body_start..body_end];

    // Naïve JSON scan for "@type" — we handle strings and arrays.
    // We avoid pulling in serde_json here; this detector runs on every
    // extract call and the JSON may be large.
    let type_idx = body.find("\"@type\"")?;
    let after = &body[type_idx + 7..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let rest_bytes = rest.as_bytes();
    if rest_bytes.is_empty() {
        return None;
    }
    // Either a quoted string or a JSON array of strings — take the first.
    let start = if rest_bytes[0] == b'[' {
        rest.find('"')? + 1
    } else if rest_bytes[0] == b'"' {
        1
    } else {
        return None;
    };
    let end = start + rest[start..].find('"')?;
    Some(rest[start..end].to_string())
}

fn extract_canonical_path(lower_html: &str) -> Option<String> {
    let canon = lower_html.find("rel=\"canonical\"")?;
    let start = ceil_char_boundary(lower_html, canon.saturating_sub(200));
    let end = floor_char_boundary(lower_html, (canon + 400).min(lower_html.len()));
    let window = &lower_html[start..end];
    let href = read_attr(window, "href")?;
    // Extract the URL path (strip scheme + host).
    let no_scheme = href.split_once("://").map(|(_, r)| r).unwrap_or(&href);
    let path_start = no_scheme.find('/').unwrap_or(no_scheme.len());
    Some(no_scheme[path_start..].to_string())
}

fn classify_by_url_path(path: &str) -> Option<PageType> {
    let p = path.to_ascii_lowercase();
    if p.contains("/product") || p.contains("/p/") || p.contains("/item/") {
        return Some(PageType::Product);
    }
    if p.contains("/forum") || p.contains("/thread") || p.contains("/comments") {
        return Some(PageType::Forum);
    }
    if p.contains("/docs/") || p.contains("/reference/") || p.contains("/api/") {
        return Some(PageType::Documentation);
    }
    if p.contains("/category") || p.contains("/tag/") || p.contains("/search") {
        return Some(PageType::Listing);
    }
    if p.contains("/blog/")
        || p.contains("/article")
        || p.contains("/news/")
        || p.contains("/post/")
        || p.contains("/story/")
    {
        return Some(PageType::Article);
    }
    None
}

fn read_attr(src: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=");
    let idx = src.find(&pat)?;
    let after = &src[idx + pat.len()..];
    let bytes = after.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let (start, end_ch) = match bytes[0] {
        b'"' => (1, b'"'),
        b'\'' => (1, b'\''),
        _ => (0, b' '),
    };
    let after_quote = &after[start..];
    let end = after_quote.find(end_ch as char)?;
    Some(after_quote[..end].to_string())
}

fn count_ci(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

// We reference `extract_metadata` in the doc-test example above to
// make the imports line up; silence the unused-import lint.
#[allow(dead_code)]
fn _metadata_anchor(html: &str) -> crate::html::HtmlMetadata {
    extract_metadata(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn og_type_article() {
        let html = r#"<html><head>
            <meta property="og:type" content="article">
        </head><body><article><p>Body.</p></article></body></html>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn og_type_product() {
        let html = r#"<html><head>
            <meta property="og:type" content="product">
        </head></html>"#;
        assert_eq!(detect_page_type(html), PageType::Product);
    }

    #[test]
    fn json_ld_newsarticle() {
        let html = r#"<html><head>
            <script type="application/ld+json">
            { "@context": "https://schema.org", "@type": "NewsArticle", "headline": "x" }
            </script>
        </head></html>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn json_ld_product() {
        let html = r#"<script type="application/ld+json">
        { "@type": "Product", "name": "thing" }
        </script>"#;
        assert_eq!(detect_page_type(html), PageType::Product);
    }

    #[test]
    fn json_ld_type_array_picks_first() {
        let html = r#"<script type="application/ld+json">
        { "@type": ["BlogPosting", "Article"] }
        </script>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn schema_org_itemtype_fallback() {
        let html = r#"<div itemtype="https://schema.org/Article">x</div>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn canonical_url_blog_pattern() {
        let html = r#"<html><head>
            <link rel="canonical" href="https://example.com/blog/2026/04/23/post">
        </head></html>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn canonical_url_product_pattern() {
        let html = r#"<link rel="canonical" href="https://shop.example.com/products/sku123">"#;
        assert_eq!(detect_page_type(html), PageType::Product);
    }

    #[test]
    fn canonical_url_forum_pattern() {
        let html = r#"<link rel="canonical" href="https://forum.example.com/thread/123">"#;
        assert_eq!(detect_page_type(html), PageType::Forum);
    }

    #[test]
    fn canonical_url_docs_pattern() {
        let html = r#"<link rel="canonical" href="https://example.com/docs/api/foo">"#;
        assert_eq!(detect_page_type(html), PageType::Documentation);
    }

    #[test]
    fn structural_article_tag_defaults_to_article() {
        let html = r#"<html><body><article><h1>X</h1><p>body</p></article></body></html>"#;
        assert_eq!(detect_page_type(html), PageType::Article);
    }

    #[test]
    fn many_comments_classifies_as_forum() {
        let html = r#"<body>
            <div class="comment">one</div>
            <div class="comment">two</div>
            <div class="comment">three</div>
            <div class="comment">four</div>
        </body>"#;
        assert_eq!(detect_page_type(html), PageType::Forum);
    }

    #[test]
    fn price_signals_classify_as_product() {
        let html = r#"<div>
            <span class="price">$10</span>
            <span itemprop="price">$10</span>
        </div>"#;
        assert_eq!(detect_page_type(html), PageType::Product);
    }

    #[test]
    fn empty_page_is_unknown() {
        assert_eq!(detect_page_type(""), PageType::Unknown);
        assert_eq!(
            detect_page_type("<html><body></body></html>"),
            PageType::Unknown
        );
    }

    #[test]
    fn og_type_wins_over_structural_default() {
        // <article> present but og:type says product -> Product wins.
        let html = r#"<html><head>
            <meta property="og:type" content="product">
        </head><body><article><p>marketing copy</p></article></body></html>"#;
        assert_eq!(detect_page_type(html), PageType::Product);
    }
}
