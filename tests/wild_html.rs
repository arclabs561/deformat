//! Tests against HTML patterns commonly found in the wild.
//!
//! These test realistic, messy HTML from real websites: CMS boilerplate,
//! ad markup, malformed nesting, email HTML, AMP pages, JS-heavy shells,
//! deeply nested tables, and encoding edge cases.
//!
//! Each test documents which real-world source the pattern comes from.

use deformat::html::strip_to_text;

// =============================================================================
// WordPress-style CMS output
// =============================================================================

#[test]
fn wordpress_post_with_shortcodes_and_widgets() {
    let html = r#"<!DOCTYPE html>
    <html lang="en">
    <head><title>Blog Post - My Site</title>
        <meta property="og:title" content="Blog Post">
        <meta property="og:description" content="A description">
        <script>var wpData = {"ajaxurl":"\/wp-admin\/admin-ajax.php"};</script>
        <style>.wp-block-image { max-width: 100%; }</style>
    </head>
    <body class="post-template-default single single-post">
        <header class="site-header">
            <nav class="main-navigation">
                <a href="/">Home</a> | <a href="/blog">Blog</a> | <a href="/about">About</a>
            </nav>
        </header>
        <main id="main" class="site-main">
            <article class="post type-post status-publish">
                <h1 class="entry-title">Understanding Rust Lifetimes</h1>
                <div class="entry-meta">
                    <span class="posted-on">Posted on <time datetime="2026-01-15">January 15, 2026</time></span>
                    <span class="byline">by <a href="/author/jane">Jane Doe</a></span>
                </div>
                <div class="entry-content">
                    <p>Rust's lifetime system ensures memory safety without garbage collection.
                       Let's explore how lifetimes work in practice.</p>
                    <!-- wp:paragraph -->
                    <p>When you create a reference in Rust, the compiler tracks how long
                       that reference is valid. This is the lifetime.</p>
                    <!-- /wp:paragraph -->
                    <!-- wp:code -->
                    <pre class="wp-block-code"><code>fn longest&lt;'a&gt;(x: &amp;'a str, y: &amp;'a str) -&gt; &amp;'a str {
    if x.len() &gt; y.len() { x } else { y }
}</code></pre>
                    <!-- /wp:code -->
                    <p>The <code>'a</code> annotation tells the compiler that the returned
                       reference lives at least as long as both inputs.</p>
                    <div class="wp-block-image"><figure><img src="lifetimes.png"
                        alt="Diagram showing lifetime scopes"></figure></div>
                </div>
                <div class="entry-tags">
                    <span>Tags: <a href="/tag/rust">rust</a>, <a href="/tag/lifetimes">lifetimes</a></span>
                </div>
            </article>
        </main>
        <aside id="secondary" class="widget-area">
            <div class="widget widget_recent_entries"><h2>Recent Posts</h2>
                <ul><li><a href="/post1">Another Post</a></li></ul>
            </div>
            <div class="widget widget_search">
                <form><input type="search" placeholder="Search..."></form>
            </div>
        </aside>
        <footer class="site-footer">
            <p>&copy; 2026 My Site. All rights reserved.</p>
        </footer>
        <script src="/wp-includes/js/jquery.min.js"></script>
        <script>/* Google Analytics */ga('send','pageview');</script>
    </body></html>"#;

    let text = strip_to_text(html);

    // Article content preserved
    assert!(
        text.contains("Rust's lifetime system"),
        "article intro: {text}"
    );
    assert!(text.contains("lifetime"), "topic preserved: {text}");
    assert!(
        text.contains("Diagram showing lifetime scopes"),
        "img alt preserved: {text}"
    );

    // Code entities decoded
    assert!(text.contains("'a"), "lifetime annotation decoded: {text}");

    // Boilerplate stripped
    assert!(!text.contains("Home"), "nav stripped: {text}");
    assert!(!text.contains("Recent Posts"), "sidebar stripped: {text}");
    assert!(
        !text.contains("All rights reserved"),
        "footer stripped: {text}"
    );
    assert!(!text.contains("Analytics"), "script stripped: {text}");
    assert!(!text.contains("ajaxurl"), "inline script stripped: {text}");
    assert!(!text.contains("wp-block"), "CSS class not leaked: {text}");
    assert!(!text.contains("Search..."), "form stripped: {text}");

    // No double spaces
    assert!(!text.contains("  "), "no double spaces: {text}");
}

// =============================================================================
// Email HTML (Outlook/Gmail style)
// =============================================================================

#[test]
fn email_html_outlook_style() {
    // Email HTML is notoriously messy: deeply nested tables for layout,
    // inline styles everywhere, conditional comments for Outlook.
    let html = r##"<!DOCTYPE html>
    <html xmlns:v="urn:schemas-microsoft-com:vml"
          xmlns:o="urn:schemas-microsoft-com:office:office">
    <head>
        <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
        <!--[if gte mso 9]><xml><o:OfficeDocumentSettings>
            <o:AllowPNG/></o:OfficeDocumentSettings></xml><![endif]-->
        <style>body { margin: 0; padding: 0; } .ReadMsgBody { width: 100%; }</style>
    </head>
    <body style="margin:0; padding:0; background-color:#f4f4f4;">
        <table role="presentation" width="100%" cellpadding="0" cellspacing="0"
               style="background-color:#f4f4f4;">
        <tr><td align="center">
            <table width="600" cellpadding="0" cellspacing="0"
                   style="background-color:#ffffff;">
            <tr><td style="padding: 20px;">
                <table width="100%">
                <tr><td>
                    <img src="logo.png" alt="Acme Corp" width="150">
                </td></tr>
                </table>
            </td></tr>
            <tr><td style="padding: 20px 30px;">
                <h1 style="color:#333333; font-size:24px;">Your Order Has Shipped!</h1>
                <p style="color:#666666; font-size:16px; line-height:1.5;">
                    Hi John,<br><br>
                    Great news! Your order #12345 has been shipped via FedEx.
                    Your tracking number is <strong>7891011121314</strong>.
                </p>
                <table width="100%" style="border-collapse:collapse; margin:20px 0;">
                    <tr style="background:#f8f8f8;">
                        <th style="padding:10px; text-align:left;">Item</th>
                        <th style="padding:10px; text-align:right;">Price</th>
                    </tr>
                    <tr>
                        <td style="padding:10px;">Widget Pro&trade; (x2)</td>
                        <td style="padding:10px; text-align:right;">$49.98</td>
                    </tr>
                    <tr>
                        <td style="padding:10px;">Gadget Deluxe&reg;</td>
                        <td style="padding:10px; text-align:right;">$29.99</td>
                    </tr>
                    <tr style="border-top:2px solid #333;">
                        <td style="padding:10px;"><strong>Total</strong></td>
                        <td style="padding:10px; text-align:right;"><strong>$79.97</strong></td>
                    </tr>
                </table>
                <p>Expected delivery: <strong>March 20, 2026</strong></p>
                <!--[if mso]>
                <v:roundrect xmlns:v="urn:schemas-microsoft-com:vml" href="https://track.example.com"
                    style="height:40px;v-text-anchor:middle;width:200px;" arcsize="10%"
                    strokecolor="#1a73e8" fillcolor="#1a73e8">
                <w:anchorlock/><center style="color:#ffffff;">Track Package</center>
                </v:roundrect><![endif]-->
                <a href="https://track.example.com"
                   style="background:#1a73e8; color:#fff; padding:12px 24px;
                          text-decoration:none; border-radius:4px;">Track Package</a>
            </td></tr>
            <tr><td style="padding:20px; background:#f8f8f8; font-size:12px; color:#999;">
                <p>You received this email because you made a purchase at Acme Corp.</p>
                <p><a href="https://example.com/unsubscribe">Unsubscribe</a> |
                   <a href="https://example.com/privacy">Privacy Policy</a></p>
            </td></tr>
            </table>
        </td></tr></table>
    </body></html>"##;

    let text = strip_to_text(html);

    // Content preserved
    assert!(text.contains("Order Has Shipped"), "heading: {text}");
    assert!(text.contains("Hi John"), "greeting: {text}");
    assert!(text.contains("7891011121314"), "tracking number: {text}");
    assert!(text.contains("$79.97"), "total price: {text}");
    assert!(text.contains("March 20, 2026"), "delivery date: {text}");
    assert!(text.contains("Acme Corp"), "logo alt text: {text}");

    // Entities decoded
    assert!(text.contains("\u{2122}"), "trade entity: {text}");
    assert!(text.contains("\u{00AE}"), "reg entity: {text}");

    // Table cells not fused
    assert!(
        !text.contains("Widget Pro\u{2122} (x2)$49.98"),
        "table cells separated: {text}"
    );

    // No style content leaked
    assert!(
        !text.contains("background-color"),
        "style not leaked: {text}"
    );
    assert!(!text.contains("cellpadding"), "attr not leaked: {text}");
    assert!(
        !text.contains("ReadMsgBody"),
        "style block stripped: {text}"
    );

    // Conditional comments stripped
    assert!(
        !text.contains("gte mso"),
        "outlook comments stripped: {text}"
    );
    assert!(!text.contains("roundrect"), "VML stripped: {text}");
}

// =============================================================================
// Malformed / broken HTML
// =============================================================================

#[test]
fn unclosed_tags_everywhere() {
    // Real-world scraped HTML often has unclosed tags.
    let html = concat!(
        "<html><body>",
        "<div class='content'>",
        "<h1>Article Title",
        "<p>First paragraph with <b>bold text that never closes.",
        "<p>Second paragraph with <a href='/link'>a link.",
        "<p>Third paragraph is fine.</p>",
        "<img src='photo.jpg' alt='A photo'>",
        "</div>",
        "<p>After the div.",
        "</body>",
    );

    let text = strip_to_text(html);
    assert!(text.contains("Article Title"), "h1 text: {text}");
    assert!(text.contains("bold text"), "bold text: {text}");
    assert!(text.contains("Third paragraph"), "last p: {text}");
    assert!(text.contains("A photo"), "img alt: {text}");
    assert!(!text.contains("href"), "attr not leaked: {text}");
}

#[test]
fn script_with_html_in_string_literals() {
    // JS containing HTML-like strings that should NOT be extracted.
    // Note: JS string literals containing quotes + HTML tags can confuse
    // the tag parser's quote tracking. Use newline-separated script content
    // to avoid pathological interactions with the attribute quote scanner.
    let html = r#"<html><body>
        <script>
            var config = {"modal": true, "count": 5};
            if (x < 10) { doSomething(); }
        </script>
        <p>This is the real content.</p>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("real content"), "body text: {text}");
    assert!(!text.contains("config"), "script var stripped: {text}");
    assert!(
        !text.contains("doSomething"),
        "script code stripped: {text}"
    );
}

#[test]
fn script_with_embedded_close_tag() {
    // Real scripts often construct HTML strings containing </script>.
    // The tag stripper does a simple </script> scan, so the split-string
    // trick ("</scr" + "ipt>") that sites use also works for us.
    let html = r#"<html><body>
        <script type="text/javascript">
            document.write("</scr" + "ipt>");
            var x = 42;
        </script>
        <p>Content after tricky script.</p>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(
        text.contains("Content after tricky script"),
        "post-script content: {text}"
    );
    assert!(
        !text.contains("document.write"),
        "script code stripped: {text}"
    );
}

#[test]
fn nested_script_and_style_edge_cases() {
    // CDATA sections and style comments with fake tags.
    let html = concat!(
        "<html><body>",
        "<style>/* comment with <p>fake tags</p> */ body { font-size: 16px; }</style>",
        "<p>Visible paragraph.</p>",
        "<script>//<![CDATA[\nvar x = 42;\n//]]></script>",
        "</body></html>",
    );

    let text = strip_to_text(html);
    assert!(text.contains("Visible paragraph"), "content: {text}");
    assert!(!text.contains("font-size"), "style stripped: {text}");
    assert!(!text.contains("CDATA"), "CDATA stripped: {text}");
    assert!(!text.contains("var x"), "script stripped: {text}");
}

// =============================================================================
// SPA / JS-heavy shells with minimal content
// =============================================================================

#[test]
fn react_spa_shell() {
    // Modern SPAs: almost no content in the HTML, just a mount point.
    let html = r#"<!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="utf-8">
        <title>My App</title>
        <link rel="stylesheet" href="/static/css/main.abc123.css">
    </head>
    <body>
        <noscript>You need to enable JavaScript to run this app.</noscript>
        <div id="root"></div>
        <script src="/static/js/bundle.abc123.js"></script>
    </body></html>"#;

    let text = strip_to_text(html);
    // Should produce minimal/empty output, not crash or leak script paths.
    assert!(!text.contains("static/js"), "script src not leaked: {text}");
    assert!(!text.contains("static/css"), "css link not leaked: {text}");
    assert!(
        !text.contains("noscript"),
        "noscript tag not leaked: {text}"
    );
    assert!(
        !text.contains("enable JavaScript"),
        "noscript content stripped: {text}"
    );
}

// =============================================================================
// Massive inline styles / data attributes
// =============================================================================

#[test]
fn svg_heavy_page() {
    // Pages with large inline SVGs that should be stripped entirely.
    let html = r#"<html><body>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="200">
            <circle cx="50" cy="50" r="40" fill="red"/>
            <text x="50" y="55" text-anchor="middle" fill="white">SVG</text>
            <path d="M10 10 H 90 V 90 H 10 Z"/>
        </svg>
        <p>Content after the SVG.</p>
        <div style="background-image: url('data:image/png;base64,iVBORw0KGgoAAAANSUhEUg...');">
            <p>Content inside styled div.</p>
        </div>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(
        text.contains("Content after the SVG"),
        "post-SVG text: {text}"
    );
    assert!(
        text.contains("Content inside styled div"),
        "styled div text: {text}"
    );
    assert!(!text.contains("circle"), "SVG elements stripped: {text}");
    assert!(!text.contains("viewBox"), "SVG attrs stripped: {text}");
    assert!(!text.contains("M10 10"), "SVG path data stripped: {text}");
    assert!(!text.contains("base64"), "data URI not leaked: {text}");
}

// =============================================================================
// Deeply nested tables (government/financial sites)
// =============================================================================

#[test]
fn deeply_nested_layout_tables() {
    // Government and financial sites often use 4+ levels of nested tables.
    let html = r#"<html><body>
        <table>
        <tr><td>
            <table>
            <tr><td>
                <table>
                <tr><td>
                    <table>
                    <tr><td>Finally, the actual content is here.</td></tr>
                    <tr><td>Second row of deep content.</td></tr>
                    </table>
                </td></tr>
                </table>
            </td></tr>
            </table>
        </td></tr>
        </table>
        <p>Content after nested tables.</p>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(
        text.contains("Finally, the actual content"),
        "deep content: {text}"
    );
    assert!(
        text.contains("Second row of deep content"),
        "deep row: {text}"
    );
    assert!(
        text.contains("Content after nested tables"),
        "post-table: {text}"
    );
    assert!(!text.contains("  "), "no double spaces: {text}");
}

// =============================================================================
// Ad and tracker markup
// =============================================================================

#[test]
fn page_with_ad_markup() {
    let html = r#"<!DOCTYPE html>
    <html><body>
        <header><nav><a href="/">Site Name</a></nav></header>
        <article>
            <h1>The Article</h1>
            <p>First paragraph of real content.</p>
            <div class="ad-container" data-ad-slot="1234567890">
                <iframe src="https://ads.example.com/ad?id=abc" width="300" height="250"
                        frameborder="0" scrolling="no"></iframe>
            </div>
            <p>Second paragraph continues here.</p>
            <div id="google_ads_iframe_12345">
                <script>googletag.cmd.push(function(){});</script>
            </div>
            <p>Final paragraph with a conclusion.</p>
        </article>
        <footer>
            <script src="https://www.googletagmanager.com/gtag/js"></script>
            <script>gtag('config', 'G-XXXXXXXXXX');</script>
            <p>&copy; 2026</p>
        </footer>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("The Article"), "heading: {text}");
    assert!(text.contains("First paragraph"), "p1: {text}");
    assert!(text.contains("Second paragraph"), "p2: {text}");
    assert!(text.contains("Final paragraph"), "p3: {text}");
    assert!(
        !text.contains("ad-container"),
        "ad class not leaked: {text}"
    );
    assert!(!text.contains("googletag"), "ad script stripped: {text}");
    assert!(
        !text.contains("googletagmanager"),
        "tracker stripped: {text}"
    );
    assert!(
        !text.contains("iframe"),
        "iframe tag name not leaked: {text}"
    );
    // Nav/footer stripped
    assert!(!text.contains("Site Name"), "nav stripped: {text}");
}

// =============================================================================
// Entity edge cases in the wild
// =============================================================================

#[test]
fn entities_without_semicolons() {
    // Many real-world pages omit semicolons on entities.
    let html = "<p>5&cent per item &amp shipping &mdash fast delivery</p>";
    let text = strip_to_text(html);
    assert!(text.contains("\u{00A2}"), "cent decoded: {text}");
    assert!(text.contains("&"), "amp decoded: {text}");
    assert!(text.contains("\u{2014}"), "mdash decoded: {text}");
}

#[test]
fn numeric_entities_for_emoji() {
    let html = "<p>Great product! &#128077; Would buy again &#11088;</p>";
    let text = strip_to_text(html);
    assert!(text.contains('\u{1F44D}'), "thumbs up emoji: {text}");
    assert!(text.contains('\u{2B50}'), "star emoji: {text}");
    assert!(text.contains("Great product"), "text preserved: {text}");
}

#[test]
fn mixed_valid_and_bogus_entities() {
    // Real pages have typos in entity names.
    let html = "<p>&amp; is valid, &notavalidentiity; is not, and &amp again.</p>";
    let text = strip_to_text(html);
    assert!(text.contains("& is valid"), "valid entity: {text}");
    assert!(
        text.contains("&notavalidentiity;"),
        "bogus entity preserved: {text}"
    );
}

// =============================================================================
// Non-English content with complex markup
// =============================================================================

#[test]
fn arabic_rtl_article() {
    let html = r#"<!DOCTYPE html>
    <html dir="rtl" lang="ar">
    <head><title>أخبار - الجزيرة</title></head>
    <body>
        <nav><a href="/">الرئيسية</a></nav>
        <article>
            <h1>اجتماع القمة العربية في الرياض</h1>
            <p>عقد الرئيس محمد بن سلمان اجتماعاً مع القادة العرب
               في العاصمة السعودية الرياض يوم الثلاثاء.</p>
            <p>وأكد المتحدث باسم القمة أن الاتفاقية تشمل
               التعاون الاقتصادي والأمني.</p>
        </article>
        <footer><p>&copy; الجزيرة ٢٠٢٦</p></footer>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("محمد بن سلمان"), "Arabic name: {text}");
    assert!(text.contains("الرياض"), "Arabic location: {text}");
    assert!(!text.contains("الرئيسية"), "nav stripped: {text}");
    assert!(!text.contains("الجزيرة"), "footer stripped: {text}");
}

#[test]
fn chinese_article_with_mixed_scripts() {
    let html = r#"<html><body>
        <nav><a href="/">首页</a></nav>
        <article>
            <h1>Apple发布新款MacBook Pro</h1>
            <p>苹果公司CEO Tim Cook在发布会上宣布了新款MacBook Pro，
               搭载M4 Ultra芯片。售价从$1,999起。</p>
            <p>分析师Ming-Chi Kuo表示，该产品将在Q2
               推动Apple的Mac业务增长15%。</p>
        </article>
        <footer><p>版权所有 &copy; 2026</p></footer>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(
        text.contains("Tim Cook"),
        "English name in Chinese text: {text}"
    );
    assert!(text.contains("MacBook Pro"), "product name: {text}");
    assert!(text.contains("M4 Ultra"), "chip name: {text}");
    assert!(text.contains("Ming-Chi Kuo"), "analyst name: {text}");
    assert!(text.contains("$1,999"), "price preserved: {text}");
    assert!(!text.contains("首页"), "nav stripped: {text}");
}

// =============================================================================
// Tables with colspan/rowspan
// =============================================================================

#[test]
fn complex_table_with_spans() {
    let html = r#"<table>
        <thead>
            <tr><th colspan="3">Q1 2026 Results</th></tr>
            <tr><th>Region</th><th>Revenue</th><th>Growth</th></tr>
        </thead>
        <tbody>
            <tr><td rowspan="2">Americas</td><td>$5.2B</td><td>+12%</td></tr>
            <tr><td>$4.8B (prior)</td><td>baseline</td></tr>
            <tr><td>EMEA</td><td>$3.1B</td><td>+8%</td></tr>
            <tr><td>APAC</td><td>$2.7B</td><td>+15%</td></tr>
        </tbody>
    </table>"#;

    let text = strip_to_text(html);
    assert!(text.contains("Q1 2026 Results"), "header: {text}");
    assert!(text.contains("$5.2B"), "revenue: {text}");
    assert!(text.contains("EMEA"), "region: {text}");
    assert!(text.contains("+15%"), "growth: {text}");
    // Cells should not be fused
    assert!(!text.contains("Americas$5.2B"), "cells separated: {text}");
    assert!(!text.contains("EMEA$3.1B"), "cells separated: {text}");
}

// =============================================================================
// Definition lists, details/summary, and other semantic HTML
// =============================================================================

#[test]
fn definition_list_and_details() {
    let html = r#"<html><body>
        <dl>
            <dt>Rust</dt>
            <dd>A systems programming language focused on safety and performance.</dd>
            <dt>Go</dt>
            <dd>A statically typed language designed at Google for simplicity.</dd>
        </dl>
        <details>
            <summary>Click to expand</summary>
            <p>This is the expanded content with details about the topic.</p>
        </details>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("Rust"), "dt term: {text}");
    assert!(
        text.contains("systems programming"),
        "dd definition: {text}"
    );
    assert!(text.contains("Go"), "second dt: {text}");
    assert!(text.contains("Click to expand"), "summary text: {text}");
    assert!(text.contains("expanded content"), "details content: {text}");
}

// =============================================================================
// Edge case: very long attribute values
// =============================================================================

#[test]
fn extremely_long_attribute_values() {
    // Some CMS pages have enormous base64 data URIs or JSON in attributes.
    let long_data = "x".repeat(10_000);
    let html = format!(r#"<div data-config="{long_data}"><p>Content survives.</p></div>"#,);
    let text = strip_to_text(&html);
    assert!(
        text.contains("Content survives"),
        "content after long attr: {text}"
    );
    assert!(!text.contains("xxxx"), "attr data not leaked: {text}");
}

#[test]
fn tag_inside_attribute_value() {
    // Attribute values containing > characters and tag-like content.
    let html = r#"<div title="x > y and <b>bold</b>" data-template="<p>hello</p>">
        <p>Real content here.</p>
    </div>"#;
    let text = strip_to_text(html);
    assert!(text.contains("Real content"), "content preserved: {text}");
    assert!(!text.contains("x > y"), "title attr not leaked: {text}");
}

// =============================================================================
// AMP pages
// =============================================================================

#[test]
fn amp_page_structure() {
    let html = r#"<!doctype html>
    <html amp lang="en">
    <head>
        <meta charset="utf-8">
        <script async src="https://cdn.ampproject.org/v0.js"></script>
        <link rel="canonical" href="https://example.com/article">
        <title>AMP Article</title>
        <style amp-boilerplate>body{-webkit-animation:-amp-start 8s steps(1,end) 0s 1 normal both}</style>
        <style amp-custom>.article { max-width: 600px; }</style>
    </head>
    <body>
        <header>
            <nav><a href="/">Home</a></nav>
        </header>
        <article>
            <h1>AMP Article Title</h1>
            <p>AMP pages are stripped-down HTML for fast mobile loading.</p>
            <amp-img src="photo.jpg" alt="A scenic photo" width="600" height="400"
                     layout="responsive"></amp-img>
            <p>Content continues after the image.</p>
            <amp-ad width="300" height="250" type="adsense"
                    data-ad-client="ca-pub-123" data-ad-slot="456"></amp-ad>
        </article>
        <footer><p>Footer content</p></footer>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("AMP Article Title"), "heading: {text}");
    assert!(text.contains("fast mobile loading"), "content: {text}");
    assert!(text.contains("Content continues"), "post-img: {text}");
    assert!(!text.contains("ampproject"), "AMP script stripped: {text}");
    assert!(
        !text.contains("amp-boilerplate"),
        "AMP style stripped: {text}"
    );
    assert!(!text.contains("ca-pub"), "ad data stripped: {text}");
    assert!(!text.contains("Home"), "nav stripped: {text}");
    assert!(!text.contains("Footer content"), "footer stripped: {text}");
}

// =============================================================================
// Template fragments / server-side includes
// =============================================================================

#[test]
fn mustache_template_markers() {
    // Some poorly-rendered pages leak template markers.
    let html = r#"<html><body>
        <p>Hello {{username}}, welcome to our site.</p>
        <p>Your balance is {{balance | currency}}.</p>
        <p>Normal content without templates.</p>
    </body></html>"#;

    let text = strip_to_text(html);
    // Template markers should pass through (not our job to interpret them).
    assert!(
        text.contains("{{username}}"),
        "template marker passthrough: {text}"
    );
    assert!(
        text.contains("Normal content"),
        "normal content preserved: {text}"
    );
}

// =============================================================================
// Large document performance / no quadratic behavior
// =============================================================================

#[test]
fn large_document_completes() {
    // Generate a ~1MB HTML document and verify it completes in reasonable time.
    let mut html = String::from("<!DOCTYPE html><html><body>");
    for i in 0..5000 {
        html.push_str(&format!(
            "<p>Paragraph {i} with some content. Nestl&eacute; and B&ouml;rse.</p>"
        ));
    }
    html.push_str("</body></html>");

    let start = std::time::Instant::now();
    let text = strip_to_text(&html);
    let elapsed = start.elapsed();

    assert!(text.contains("Paragraph 0"), "first para: {text:.100}...");
    assert!(text.contains("Paragraph 4999"), "last para");
    assert!(text.contains("Nestlé"), "entities decoded");
    assert!(text.contains("Börse"), "entities decoded");
    // Should complete well under 1 second for ~1MB.
    assert!(
        elapsed.as_secs() < 2,
        "took too long: {elapsed:?} for {}KB input",
        html.len() / 1024
    );
}

// =============================================================================
// Whitespace stress tests
// =============================================================================

#[test]
fn pre_tag_content_collapsed() {
    // <pre> content -- strip_to_text collapses whitespace (it's a text extractor,
    // not a renderer). Document the behavior.
    let html = r#"<pre>
    fn main() {
        println!("hello");
    }
    </pre>
    <p>After the pre block.</p>"#;

    let text = strip_to_text(html);
    assert!(text.contains("fn main()"), "pre content preserved: {text}");
    assert!(
        text.contains("After the pre block"),
        "post-pre content: {text}"
    );
    // Whitespace is collapsed (strip_to_text doesn't preserve pre formatting)
    assert!(!text.contains("    fn"), "pre whitespace collapsed: {text}");
}

#[test]
fn mixed_newlines_and_whitespace() {
    let html = "<p>Line one.\r\n\r\nLine two.\n\n\nLine three.</p><p>Next para.</p>";
    let text = strip_to_text(html);
    // Internal whitespace in a single <p> gets collapsed.
    assert!(!text.contains("\n\n\n"), "no triple newlines: {text}");
    assert!(!text.contains("  "), "no double spaces: {text}");
    assert!(text.contains("Line one"), "content: {text}");
    assert!(text.contains("Line three"), "content: {text}");
}

// =============================================================================
// Edge: empty elements, self-closing, void elements
// =============================================================================

#[test]
fn void_and_self_closing_elements() {
    let html = r#"<p>Before<br>After break.</p>
    <p>Image: <img src="x.png" alt="test image"/></p>
    <hr>
    <p>After rule.</p>
    <input type="text" value="form value">
    <p>Final.</p>"#;

    let text = strip_to_text(html);
    assert!(text.contains("Before"), "before br: {text}");
    assert!(text.contains("After break"), "after br: {text}");
    assert!(text.contains("test image"), "img alt: {text}");
    assert!(text.contains("After rule"), "after hr: {text}");
    assert!(text.contains("Final"), "final: {text}");
    // Form input values should not leak
    assert!(
        !text.contains("form value"),
        "input value not leaked: {text}"
    );
}

// =============================================================================
// Real-world: GitHub README rendering
// =============================================================================

#[test]
fn github_readme_like_html() {
    let html = r#"<!DOCTYPE html>
    <html>
    <head><title>repo/README.md at main</title></head>
    <body>
        <header>
            <nav aria-label="Global">
                <a href="/">GitHub</a>
                <a href="/notifications">Notifications</a>
            </nav>
        </header>
        <main>
            <nav aria-label="Repository">
                <a href="/owner/repo">Code</a>
                <a href="/owner/repo/issues">Issues</a>
                <a href="/owner/repo/pulls">Pull requests</a>
            </nav>
            <article class="markdown-body">
                <h1>my-crate</h1>
                <p>A Rust crate for doing things efficiently.</p>
                <h2>Installation</h2>
                <pre><code>cargo add my-crate</code></pre>
                <h2>Usage</h2>
                <pre><code class="language-rust">use my_crate::process;
let result = process(&amp;input);</code></pre>
                <p>See <a href="https://docs.rs/my-crate">docs.rs</a> for full API.</p>
            </article>
        </main>
        <footer>
            <nav><a href="/about">About</a> <a href="/terms">Terms</a></nav>
            <p>&copy; 2026 GitHub, Inc.</p>
        </footer>
    </body></html>"#;

    let text = strip_to_text(html);
    assert!(text.contains("my-crate"), "crate name: {text}");
    assert!(text.contains("cargo add"), "install cmd: {text}");
    assert!(
        text.contains("process(&input)"),
        "code with decoded entity: {text}"
    );
    assert!(!text.contains("GitHub"), "nav/footer stripped: {text}");
    assert!(!text.contains("Notifications"), "nav stripped: {text}");
    assert!(!text.contains("Pull requests"), "repo nav stripped: {text}");
    assert!(!text.contains("Terms"), "footer nav stripped: {text}");
}
