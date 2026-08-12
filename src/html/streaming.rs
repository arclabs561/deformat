//! Private `html5gum` streaming experiment.

use std::io::{self, Read};
#[cfg(test)]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};
use std::{cell::Cell, rc::Rc};

use html5gum::{naive_next_state, Emitter, Error, IoReader, State, Tokenizer};

use super::{cleanup_whitespace, is_block_tag, is_skip_tag};

const MAX_POLICY_TAG_LEN: usize = 16;
const SKIP_TAG_COUNT: usize = 14;
const RAW_SKIP_TAG_COUNT: usize = 2;

#[cfg(test)]
struct TrackingAllocator;

#[cfg(test)]
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
fn record_live(live: usize) {
    let mut peak = PEAK_BYTES.load(Ordering::Relaxed);
    while live > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, live, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => peak = observed,
        }
    }
}

#[cfg(test)]
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            record_live(live);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            let live = if new_size >= layout.size() {
                LIVE_BYTES.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size
                    - layout.size()
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed)
                    - (layout.size() - new_size)
            };
            record_live(live);
        }
        new_pointer
    }
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[cfg(test)]
fn reset_peak() -> usize {
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

#[cfg(test)]
fn peak_above(baseline: usize) -> usize {
    PEAK_BYTES.load(Ordering::Relaxed).saturating_sub(baseline)
}

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
    count_only: Option<Rc<Cell<usize>>>,
    logical_output_len: usize,
    output_ends_with_newline: bool,
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
            count_only: None,
            logical_output_len: 0,
            output_ends_with_newline: false,
        }
    }

    #[cfg(test)]
    fn with_count_observer(output_len: Rc<Cell<usize>>) -> Self {
        Self {
            count_only: Some(output_len),
            ..Self::new()
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
        if self.count_only.is_some() {
            self.finished = Some(String::new());
            return;
        }
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
            && self.logical_output_len > 0
            && !self.output_ends_with_newline
        {
            self.logical_output_len += 1;
            self.output_ends_with_newline = true;
            if let Some(output_len) = &self.count_only {
                output_len.set(output_len.get() + 1);
            } else {
                self.output.push(b'\n');
            }
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
            self.logical_output_len += value.len();
            self.output_ends_with_newline = value.ends_with(b"\n");
            if let Some(output_len) = &self.count_only {
                output_len.set(output_len.get() + value.len());
            } else {
                self.output.extend_from_slice(value);
            }
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
    use std::process::Command;

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

    struct PatternReader {
        prefix: &'static [u8],
        repeated: u8,
        remaining: usize,
        suffix: &'static [u8],
        prefix_offset: usize,
        suffix_offset: usize,
    }

    impl PatternReader {
        fn new(
            prefix: &'static [u8],
            repeated: u8,
            remaining: usize,
            suffix: &'static [u8],
        ) -> Self {
            Self {
                prefix,
                repeated,
                remaining,
                suffix,
                prefix_offset: 0,
                suffix_offset: 0,
            }
        }
    }

    impl Read for PatternReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.prefix_offset < self.prefix.len() {
                let len = buffer
                    .len()
                    .min(self.prefix.len().saturating_sub(self.prefix_offset));
                buffer[..len]
                    .copy_from_slice(&self.prefix[self.prefix_offset..self.prefix_offset + len]);
                self.prefix_offset += len;
                return Ok(len);
            }
            if self.remaining > 0 {
                let len = buffer.len().min(self.remaining);
                buffer[..len].fill(self.repeated);
                self.remaining -= len;
                return Ok(len);
            }
            if self.suffix_offset < self.suffix.len() {
                let len = buffer
                    .len()
                    .min(self.suffix.len().saturating_sub(self.suffix_offset));
                buffer[..len]
                    .copy_from_slice(&self.suffix[self.suffix_offset..self.suffix_offset + len]);
                self.suffix_offset += len;
                return Ok(len);
            }
            Ok(0)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ProbeResult {
        peak_bytes: usize,
        logical_output_bytes: usize,
        output_capacity: usize,
    }

    fn pattern(case: &str, size: usize) -> PatternReader {
        match case {
            "comment" => PatternReader::new(b"<!--", b'x', size, b"-->"),
            "attribute" => PatternReader::new(b"<p title=\"", b'x', size, b"\"></p>"),
            "tag" => PatternReader::new(b"<", b'x', size, b"></xxxxxxxxxxxxxxxx>"),
            "script" => PatternReader::new(b"<script>", b'x', size, b"</script>"),
            "visible" | "visible_collect" => PatternReader::new(b"<span>", b'x', size, b"</span>"),
            "entity" => PatternReader::new(b"<span>&", b'a', size, b";</span>"),
            _ => panic!("unknown memory probe case: {case}"),
        }
    }

    fn run_count_probe(case: &str, size: usize) -> ProbeResult {
        let output_len = Rc::new(Cell::new(0));
        let emitter = TextEmitter::with_count_observer(Rc::clone(&output_len));
        let baseline = reset_peak();
        let tokenizer = Tokenizer::new_with_emitter(IoReader::new(pattern(case, size)), emitter);
        for token in tokenizer {
            assert!(token.unwrap().is_empty());
        }
        ProbeResult {
            peak_bytes: peak_above(baseline),
            logical_output_bytes: output_len.get(),
            output_capacity: 0,
        }
    }

    fn run_collect_probe(size: usize) -> ProbeResult {
        let baseline = reset_peak();
        let output = extract_reader(pattern("visible_collect", size)).unwrap();
        ProbeResult {
            peak_bytes: peak_above(baseline),
            logical_output_bytes: output.len(),
            output_capacity: output.capacity(),
        }
    }

    fn parse_probe_marker(stdout: &[u8]) -> ProbeResult {
        let stdout = String::from_utf8_lossy(stdout);
        let marker = stdout
            .lines()
            .find_map(|line| {
                line.split_once("DEFORMAT_MEMORY_PROBE ")
                    .map(|(_, marker)| marker)
            })
            .unwrap_or_else(|| panic!("probe marker missing from child stdout:\n{stdout}"));
        let mut fields = marker.split_whitespace().map(|field| {
            let (name, value) = field.split_once('=').expect("probe field uses name=value");
            (
                name,
                value.parse::<usize>().expect("probe value is an integer"),
            )
        });
        let peak_bytes = fields.next().expect("peak field").1;
        let logical_output_bytes = fields.next().expect("logical output field").1;
        let output_capacity = fields.next().expect("output capacity field").1;
        assert!(fields.next().is_none(), "unexpected extra probe fields");
        ProbeResult {
            peak_bytes,
            logical_output_bytes,
            output_capacity,
        }
    }

    fn spawn_probe(case: &str, size: usize) -> ProbeResult {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "html::streaming::tests::memory_probe_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("DEFORMAT_MEMORY_PROBE_CASE", case)
            .env("DEFORMAT_MEMORY_PROBE_SIZE", size.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "memory probe failed for {case}/{size}:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        parse_probe_marker(&output.stdout)
    }

    fn median(mut values: [usize; 3]) -> usize {
        values.sort_unstable();
        values[1]
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

    #[test]
    #[ignore = "64 MiB fresh-process allocation scaling experiment"]
    fn memory_probe_child() {
        let Ok(case) = std::env::var("DEFORMAT_MEMORY_PROBE_CASE") else {
            return;
        };
        let size = std::env::var("DEFORMAT_MEMORY_PROBE_SIZE")
            .expect("probe size is set")
            .parse::<usize>()
            .expect("probe size is an integer");
        let result = if case == "visible_collect" {
            run_collect_probe(size)
        } else {
            run_count_probe(&case, size)
        };
        println!(
            "DEFORMAT_MEMORY_PROBE peak={} logical={} capacity={}",
            result.peak_bytes, result.logical_output_bytes, result.output_capacity
        );
    }

    #[test]
    #[ignore = "64 MiB fresh-process allocation scaling experiment"]
    fn parser_memory_scales_independently_of_input_and_output() {
        const SMALL: usize = 1024 * 1024;
        const LARGE: usize = 64 * 1024 * 1024;
        const FIXED_ALLOWANCE: usize = 256 * 1024;
        const GROWTH_ALLOWANCE: usize = 64 * 1024;

        let mut failures = Vec::new();
        for case in ["comment", "attribute", "tag", "script", "visible", "entity"] {
            let small_runs = std::array::from_fn(|_| spawn_probe(case, SMALL));
            let large_runs = std::array::from_fn(|_| spawn_probe(case, LARGE));
            let small_peak = median(small_runs.map(|result| result.peak_bytes));
            let large_peak = median(large_runs.map(|result| result.peak_bytes));
            let small_output = median(small_runs.map(|result| result.logical_output_bytes));
            let large_output = median(large_runs.map(|result| result.logical_output_bytes));

            println!(
                "case={case} small_runs={small_runs:?} large_runs={large_runs:?} small_peak={small_peak} large_peak={large_peak} small_output={small_output} large_output={large_output}"
            );
            if large_peak > FIXED_ALLOWANCE || large_peak > small_peak + GROWTH_ALLOWANCE {
                failures.push(format!(
                    "{case}: peak grew from {small_peak} to {large_peak} bytes"
                ));
            }
            match case {
                "comment" | "attribute" | "tag" | "script" => {
                    assert_eq!(small_output, 0, "case={case}");
                    assert_eq!(large_output, 0, "case={case}");
                }
                "visible" => {
                    assert_eq!(small_output, SMALL);
                    assert_eq!(large_output, LARGE);
                }
                "entity" => {
                    assert_eq!(small_output, SMALL + 2);
                    assert_eq!(large_output, LARGE + 2);
                }
                _ => unreachable!(),
            }
        }

        let collected_small = std::array::from_fn(|_| spawn_probe("visible_collect", SMALL));
        let collected_large = std::array::from_fn(|_| spawn_probe("visible_collect", LARGE));
        let collected_small_peak = median(collected_small.map(|result| result.peak_bytes));
        let collected_large_peak = median(collected_large.map(|result| result.peak_bytes));
        let collected_small_capacity = median(collected_small.map(|result| result.output_capacity));
        let collected_large_capacity = median(collected_large.map(|result| result.output_capacity));
        println!(
            "case=visible_collect small_runs={collected_small:?} large_runs={collected_large:?} small_peak={collected_small_peak} large_peak={collected_large_peak} small_capacity={collected_small_capacity} large_capacity={collected_large_capacity}"
        );
        assert_eq!(
            median(collected_small.map(|result| result.logical_output_bytes)),
            SMALL
        );
        assert_eq!(
            median(collected_large.map(|result| result.logical_output_bytes)),
            LARGE
        );
        assert!(collected_small_capacity < 2 * SMALL + 4096);
        assert!(collected_large_capacity < 2 * LARGE + 4096);

        assert!(
            failures.is_empty(),
            "parser buffering gate failed:\n{}",
            failures.join("\n")
        );
    }
}
