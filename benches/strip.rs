use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

fn small_fragment() -> &'static str {
    "<p>Hello <b>world</b>! Caf&eacute; in &Lstrok;&oacute;d&zacute;.</p>"
}

fn medium_article() -> String {
    let para = r#"<p>Nestl&eacute; CEO Laurent Freixe met with representatives from
        S&atilde;o Paulo at the B&ouml;rse Frankfurt. The &euro;5 billion deal
        was signed on March 15, 2026. Erdo&gbreve;an and &Ccaron;esk&yacute;
        delegates also attended the summit in &Lstrok;&oacute;d&zacute;.</p>"#;
    let mut html = String::from(
        r#"<!DOCTYPE html><html><head><title>Test</title>
        <style>body{color:black}</style><script>var x=1;</script></head><body>
        <nav><a href="/">Home</a> | <a href="/about">About</a></nav>
        <article><h1>Summit 2026</h1>"#,
    );
    for _ in 0..10 {
        html.push_str(para);
    }
    html.push_str(
        r#"<div id="toc"><h2>Contents</h2><ul><li>1 Overview</li></ul></div>
        <ol class="references"><li>Source 1</li><li>Source 2</li></ol>
        </article><footer><p>&copy; 2026</p></footer></body></html>"#,
    );
    html
}

fn large_wikipedia() -> String {
    let infobox = r#"<table class="infobox"><tr><th>Born</th><td>June 1, 1955</td></tr>
        <tr><th>Country</th><td>England</td></tr></table>"#;
    let para = r#"<p>The <ruby>東京<rt>とうきょう</rt></ruby> conference brought together
        researchers from around the world. Dr. M&uuml;ller and Prof. &Scaron;imi&cacute;
        presented findings on CRISPR-Cas9 gene editing. The European Commission
        allocated &#8364;2.3 billion for the programme. Jennifer Doudna and
        Emmanuelle Charpentier won the Nobel Prize in Chemistry in 2020.[1]
        Feng Zhang at the Broad Institute also made key contributions.[2]</p>"#;
    let nav = r#"<nav id="mw-navigation"><ul>
        <li><a href="/">Main page</a></li><li><a href="/random">Random</a></li>
        </ul></nav>"#;
    let refs = r#"<ol class="references">
        <li id="cite_note-1">Mojica FJ (2005). "Intervening sequences".</li>
        <li id="cite_note-2">Doudna JA (2012). "RNA-guided endonuclease".</li>
        </ol>"#;

    let mut html = String::from("<!DOCTYPE html><html><head><title>Article</title>");
    html.push_str("<style>.mw-body{max-width:960px}</style>");
    html.push_str("<script>window.__INITIAL_STATE__={}</script></head><body>");
    html.push_str(nav);
    html.push_str("<div id=\"content\"><h1>CRISPR</h1>");
    html.push_str(
        "<div id=\"toc\"><h2>Contents</h2><ul><li>1 History</li><li>2 Mechanism</li></ul></div>",
    );
    html.push_str(infobox);
    for _ in 0..50 {
        html.push_str(para);
    }
    html.push_str(refs);
    html.push_str(
        "<div class=\"navbox\"><table><tr><td>Gene editing topics</td></tr></table></div>",
    );
    html.push_str("</div><footer><p>&copy; Wikipedia 2026</p></footer></body></html>");
    html
}

fn entity_heavy() -> String {
    let mut html = String::from("<p>");
    for _ in 0..200 {
        html.push_str(
            "Nestl&eacute; &amp; B&ouml;rse &mdash; &Lstrok;&oacute;d&zacute; &#8364;100 ",
        );
    }
    html.push_str("</p>");
    html
}

fn bench_strip(c: &mut Criterion) {
    let small = small_fragment();
    let medium = medium_article();
    let large = large_wikipedia();
    let entities = entity_heavy();

    let mut group = c.benchmark_group("strip_to_text");

    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function("small_fragment", |b| {
        b.iter(|| deformat::html::strip_to_text(black_box(small)))
    });

    group.throughput(Throughput::Bytes(medium.len() as u64));
    group.bench_function("medium_article", |b| {
        b.iter(|| deformat::html::strip_to_text(black_box(&medium)))
    });

    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("large_wikipedia", |b| {
        b.iter(|| deformat::html::strip_to_text(black_box(&large)))
    });

    group.throughput(Throughput::Bytes(entities.len() as u64));
    group.bench_function("entity_heavy", |b| {
        b.iter(|| deformat::html::strip_to_text(black_box(&entities)))
    });

    group.finish();
}

fn bench_decode_entities(c: &mut Criterion) {
    let simple = "Caf&eacute; in S&atilde;o Paulo with &euro;5 billion";
    let heavy = entity_heavy();

    let mut group = c.benchmark_group("decode_entities");

    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple", |b| {
        b.iter(|| deformat::html::decode_entities(black_box(simple)))
    });

    group.throughput(Throughput::Bytes(heavy.len() as u64));
    group.bench_function("heavy", |b| {
        b.iter(|| deformat::html::decode_entities(black_box(&heavy)))
    });

    // No-entity fast path: should be near-zero (just memchr scan + to_owned)
    let plain = "The quick brown fox jumps over the lazy dog. No entities here at all.";
    group.throughput(Throughput::Bytes(plain.len() as u64));
    group.bench_function("no_entities", |b| {
        b.iter(|| deformat::html::decode_entities(black_box(plain)))
    });

    group.finish();
}

criterion_group!(benches, bench_strip, bench_decode_entities);
criterion_main!(benches);
