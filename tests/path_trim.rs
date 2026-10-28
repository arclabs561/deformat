use deformat::html::{strip_to_text_with_paths, SpanKind};

#[test]
fn trimmed_path_spans_do_not_claim_byte_exact_source_ranges() {
    for html in [
        "<p>  élan</p>",
        "<p>élan  </p>",
        "<p>\u{2003}élan\u{2003}</p>",
    ] {
        let (text, spans) = strip_to_text_with_paths(html);
        assert_eq!(text, "élan");
        let span = spans.iter().find(|span| span.path == "p").unwrap();
        assert_eq!(span.kind, SpanKind::WhitespaceCollapsed, "{html:?}");
        assert_eq!(&text[span.output_start..span.output_end], "élan");
        assert!(html[span.source_start..span.source_end].contains("élan"));
    }
}

#[test]
fn untouched_path_spans_remain_byte_exact_after_neighbor_trimming() {
    let html = "<p>  <b>élan</b>  </p>";
    let (text, spans) = strip_to_text_with_paths(html);
    assert_eq!(text, "élan");
    let span = spans.iter().find(|span| span.path == "p/b").unwrap();
    assert_eq!(span.kind, SpanKind::Direct);
    assert_eq!(
        &text[span.output_start..span.output_end],
        &html[span.source_start..span.source_end]
    );
}
