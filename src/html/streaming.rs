//! Private `html5gum` streaming experiment.

use std::io::{self, Read};
use std::{cell::Cell, rc::Rc};

use html5gum::{naive_next_state, Emitter, Error, IoReader, State, Tokenizer};

use super::{cleanup_whitespace, is_block_tag, is_skip_tag};

const MAX_POLICY_TAG_LEN: usize = 16;
const SKIP_TAG_COUNT: usize = 14;
const RAW_SKIP_TAG_COUNT: usize = 2;

fn skip_tag_index(tag: &[u8]) -> Option<usize> {
    match tag {
        b"head" => Some(0),
        b"nav" => Some(1),
        b"footer" => Some(2),
        b"aside" => Some(3),
        b"menu" => Some(4),
        b"noscript" => Some(5),
        b"select" => Some(6),
        b"figcaption" => Some(7),
        b"template" => Some(8),
        b"svg" => Some(9),
        b"textarea" => Some(10),
        b"iframe" => Some(11),
        b"rt" => Some(12),
        b"rp" => Some(13),
        b"script" | b"style" => None,
        _ => None,
    }
}

fn raw_skip_tag_index(tag: &[u8]) -> Option<usize> {
    match tag {
        b"script" => Some(0),
        b"style" => Some(1),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagKind {
    Start,
    End,
}

#[derive(Debug)]
struct TextEmitter {
    current_kind: Option<TagKind>,
    current_tag: Vec<u8>,
    current_tag_overflowed: bool,
    last_start_tag: Vec<u8>,
    last_start_tag_overflowed: bool,
    output: Vec<u8>,
    skip_depths: [u32; SKIP_TAG_COUNT],
    raw_skip_depths: [u32; RAW_SKIP_TAG_COUNT],
    finished: Option<String>,
    saw_text: Option<Rc<Cell<bool>>>,
}

impl TextEmitter {
    fn new() -> Self {
        Self {
            current_kind: None,
            current_tag: Vec::with_capacity(16),
            current_tag_overflowed: false,
            last_start_tag: Vec::with_capacity(16),
            last_start_tag_overflowed: false,
            output: Vec::new(),
            skip_depths: [0; SKIP_TAG_COUNT],
            raw_skip_depths: [0; RAW_SKIP_TAG_COUNT],
            finished: None,
            saw_text: None,
        }
    }

    fn with_text_observer(saw_text: Rc<Cell<bool>>) -> Self {
        Self {
            saw_text: Some(saw_text),
            ..Self::new()
        }
    }

    fn is_skipping(&self) -> bool {
        self.skip_depths.iter().any(|&depth| depth > 0)
            || self.raw_skip_depths.iter().any(|&depth| depth > 0)
    }

    fn finish_output(&mut self) {
        let decoded = String::from_utf8_lossy(&self.output);
        self.finished = Some(cleanup_whitespace(&decoded).trim().to_owned());
    }

    fn handle_tag(&mut self, kind: TagKind) -> Option<State> {
        let raw_skip = raw_skip_tag_index(&self.current_tag);
        let semantic_skip = skip_tag_index(&self.current_tag);
        let tag = String::from_utf8_lossy(&self.current_tag);
        debug_assert_eq!(semantic_skip.is_some(), is_skip_tag(&tag));
        let is_block = is_block_tag(&tag);

        match kind {
            TagKind::Start => {
                self.last_start_tag.clear();
                self.last_start_tag.extend_from_slice(&self.current_tag);
                self.last_start_tag_overflowed = self.current_tag_overflowed;
                if let Some(index) = raw_skip {
                    self.raw_skip_depths[index] = self.raw_skip_depths[index].saturating_add(1);
                } else if let Some(index) = semantic_skip {
                    self.skip_depths[index] = self.skip_depths[index].saturating_add(1);
                }
            }
            TagKind::End => {
                if let Some(index) = raw_skip {
                    self.raw_skip_depths[index] = self.raw_skip_depths[index].saturating_sub(1);
                } else if let Some(index) = semantic_skip {
                    self.skip_depths[index] = self.skip_depths[index].saturating_sub(1);
                }
            }
        }

        if !self.is_skipping()
            && is_block
            && !self.output.is_empty()
            && !self.output.ends_with(b"\n")
        {
            self.output.push(b'\n');
        }

        (kind == TagKind::Start)
            .then(|| naive_next_state(&self.current_tag))
            .flatten()
    }
}

impl Emitter for TextEmitter {
    type Token = String;

    fn set_last_start_tag(&mut self, last_start_tag: Option<&[u8]>) {
        self.last_start_tag.clear();
        self.last_start_tag_overflowed = false;
        if let Some(tag) = last_start_tag {
            let retained = tag.len().min(MAX_POLICY_TAG_LEN);
            self.last_start_tag.extend_from_slice(&tag[..retained]);
            self.last_start_tag_overflowed = tag.len() > retained;
        }
    }

    fn emit_eof(&mut self) {
        self.finish_output();
    }

    fn emit_error(&mut self, _error: Error) {}

    fn should_emit_errors(&mut self) -> bool {
        false
    }

    fn pop_token(&mut self) -> Option<Self::Token> {
        self.finished.take()
    }

    fn emit_string(&mut self, value: &[u8]) {
        if !value.is_empty() {
            if let Some(saw_text) = &self.saw_text {
                saw_text.set(true);
            }
        }
        if !self.is_skipping() {
            self.output.extend_from_slice(value);
        }
    }

    fn init_start_tag(&mut self) {
        self.current_kind = Some(TagKind::Start);
        self.current_tag.clear();
        self.current_tag_overflowed = false;
    }

    fn init_end_tag(&mut self) {
        self.current_kind = Some(TagKind::End);
        self.current_tag.clear();
        self.current_tag_overflowed = false;
    }

    fn init_comment(&mut self) {}

    fn emit_current_tag(&mut self) -> Option<State> {
        self.handle_tag(self.current_kind.expect("tag initialized before emission"))
    }

    fn emit_current_comment(&mut self) {}

    fn emit_current_doctype(&mut self) {}

    fn set_self_closing(&mut self) {}

    fn set_force_quirks(&mut self) {}

    fn push_tag_name(&mut self, value: &[u8]) {
        let remaining = MAX_POLICY_TAG_LEN.saturating_sub(self.current_tag.len());
        self.current_tag
            .extend(value.iter().take(remaining).map(u8::to_ascii_lowercase));
        self.current_tag_overflowed |= value.len() > remaining;
    }

    fn push_comment(&mut self, _value: &[u8]) {}

    fn push_doctype_name(&mut self, _value: &[u8]) {}

    fn init_doctype(&mut self) {}

    fn init_attribute(&mut self) {}

    fn push_attribute_name(&mut self, _value: &[u8]) {}

    fn push_attribute_value(&mut self, _value: &[u8]) {}

    fn set_doctype_public_identifier(&mut self, _value: &[u8]) {}

    fn set_doctype_system_identifier(&mut self, _value: &[u8]) {}

    fn push_doctype_public_identifier(&mut self, _value: &[u8]) {}

    fn push_doctype_system_identifier(&mut self, _value: &[u8]) {}

    fn current_is_appropriate_end_tag_token(&mut self) -> bool {
        self.current_kind == Some(TagKind::End)
            && !self.current_tag_overflowed
            && !self.last_start_tag_overflowed
            && self.current_tag == self.last_start_tag
    }
}

/// Extract plain text from an incrementally read HTML byte stream.
///
/// This is crate-private while tokenizer recovery and memory bounds are evaluated.
#[allow(dead_code)]
pub(crate) fn extract_reader(reader: impl Read) -> io::Result<String> {
    let tokenizer = Tokenizer::new_with_emitter(IoReader::new(reader), TextEmitter::new());
    let mut output = None;
    for token in tokenizer {
        output = Some(token?);
    }
    Ok(output.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use proptest::prelude::*;

    use super::*;
    use crate::html::strip_to_text;

    struct Chunked<'a> {
        input: &'a [u8],
        chunks: &'a [usize],
        offset: usize,
        chunk: usize,
    }

    struct AssertProgressBeforeEof {
        first: Option<&'static [u8]>,
        saw_text: Rc<Cell<bool>>,
    }

    impl Read for AssertProgressBeforeEof {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if let Some(input) = self.first.take() {
                buf[..input.len()].copy_from_slice(input);
                return Ok(input.len());
            }
            assert!(
                self.saw_text.get(),
                "tokenizer must deliver text before requesting EOF"
            );
            Ok(0)
        }
    }

    impl Read for Chunked<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.offset == self.input.len() {
                return Ok(0);
            }
            let scheduled = self.chunks[self.chunk % self.chunks.len()];
            self.chunk += 1;
            let len = scheduled.min(buf.len()).min(self.input.len() - self.offset);
            buf[..len].copy_from_slice(&self.input[self.offset..self.offset + len]);
            self.offset += len;
            Ok(len)
        }
    }

    fn extract_chunked(html: &str, chunks: &[usize]) -> String {
        extract_reader(Chunked {
            input: html.as_bytes(),
            chunks,
            offset: 0,
            chunk: 0,
        })
        .unwrap()
    }

    #[test]
    fn supported_well_formed_cases_match_legacy_oracle() {
        let cases = [
            "<p>Hello <b>world</b>!</p>",
            "<h1>Title</h1><p>First.</p><p>Second.</p>",
            "<main><p>A &amp; B &#x1f642;</p><aside>hidden</aside></main>",
            "<p>before</p><!-- comment --><p>after</p>",
            "<p>visible</p><script>if (a < b) alert('&amp;')</script><style>x{}</style>",
        ];

        for html in cases {
            assert_eq!(
                extract_reader(html.as_bytes()).unwrap(),
                strip_to_text(html)
            );
        }
    }

    #[test]
    fn committed_regression_fixtures_match_legacy_oracle() {
        let fixtures = [
            (
                "clean_news_article",
                include_str!("../../tests/fixtures/regression/clean_news_article.html"),
            ),
            (
                "blog_with_sidebar",
                include_str!("../../tests/fixtures/regression/blog_with_sidebar.html"),
            ),
            (
                "documentation_page",
                include_str!("../../tests/fixtures/regression/documentation_page.html"),
            ),
        ];

        for (name, html) in fixtures {
            assert_eq!(
                extract_reader(html.as_bytes()).unwrap(),
                strip_to_text(html),
                "fixture={name}"
            );
        }
    }

    #[test]
    fn output_is_invariant_across_adversarial_chunk_schedules() {
        let html = "<main title=\"café &amp; tea\"><p>A &amp; 🙂</p><!-- split --><script>if (a < b) {}</script><style>x{}</style><p>Z</p></main>";
        let expected = extract_reader(html.as_bytes()).unwrap();

        for chunks in [&[1][..], &[2, 3, 5], &[7, 1, 11, 2], &[16, 31]] {
            assert_eq!(extract_chunked(html, chunks), expected, "chunks={chunks:?}");
        }
        assert_eq!(expected, strip_to_text(html));
    }

    proptest! {
        #[test]
        fn output_is_invariant_across_generated_chunk_schedules(
            chunks in prop::collection::vec(1usize..33, 1..32),
        ) {
            let html = "<main title=\"café &amp; tea\"><p>A &amp; 🙂</p><!-- split --><script>if (a < b) {}</script><style>x{}</style><p>Z</p></main>";
            let expected = extract_reader(html.as_bytes()).unwrap();
            prop_assert_eq!(extract_chunked(html, &chunks), expected);
        }
    }

    #[test]
    fn tokenizer_delivers_text_before_eof() {
        let saw_text = Rc::new(Cell::new(false));
        let reader = AssertProgressBeforeEof {
            first: Some(b"<p>visible</p>"),
            saw_text: Rc::clone(&saw_text),
        };
        let tokenizer = Tokenizer::new_with_emitter(
            IoReader::new(reader),
            TextEmitter::with_text_observer(saw_text),
        );

        let output: Vec<_> = tokenizer.collect::<Result<_, _>>().unwrap();
        assert_eq!(output, ["visible"]);
    }

    #[test]
    fn intentional_mismatches_are_classified() {
        // html5gum applies the WHATWG entity table; the legacy scanner intentionally
        // supports a smaller named-entity table.
        let html = "<p>&CounterClockwiseContourIntegral;</p>";
        assert_eq!(extract_reader(html.as_bytes()).unwrap(), "∳");
        assert_eq!(strip_to_text(html), "&CounterClockwiseContourIntegral;");
    }

    #[test]
    fn mismatched_skip_closes_do_not_end_another_skip_region() {
        let html = "<nav>x</aside>still hidden</nav><p>visible</p>";
        assert_eq!(extract_reader(html.as_bytes()).unwrap(), "visible");
    }
}
