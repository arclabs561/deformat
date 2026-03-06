#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""
QA random URL testing for deformat HTML extraction.

Fetches random web pages and runs deformat's strip_to_text on each,
checking invariants: no double spaces, no leaked HTML tags, no script
content, proper entity decoding, trimmed output.

Usage:
    uv run scripts/qa_random_urls.py [OPTIONS]

Examples:
    uv run scripts/qa_random_urls.py --count 3
    uv run scripts/qa_random_urls.py --count 5 --seed 42
    uv run scripts/qa_random_urls.py --category wikipedia
"""

from __future__ import annotations

import argparse
import random
import re
import subprocess
import sys
import time
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError, URLError

# ---------------------------------------------------------------------------
# URL source pool -- diverse languages, structures, HTML patterns
# ---------------------------------------------------------------------------

SOURCES: list[dict] = [
    # Wikipedia random (diverse HTML, languages, structures)
    {"url": "https://en.wikipedia.org/wiki/Special:Random", "lang": "en", "cat": "wikipedia"},
    {"url": "https://de.wikipedia.org/wiki/Special:Random", "lang": "de", "cat": "wikipedia"},
    {"url": "https://fr.wikipedia.org/wiki/Special:Random", "lang": "fr", "cat": "wikipedia"},
    {"url": "https://ja.wikipedia.org/wiki/Special:Random", "lang": "ja", "cat": "wikipedia"},
    {"url": "https://es.wikipedia.org/wiki/Special:Random", "lang": "es", "cat": "wikipedia"},
    {"url": "https://pt.wikipedia.org/wiki/Special:Random", "lang": "pt", "cat": "wikipedia"},
    {"url": "https://zh.wikipedia.org/wiki/Special:Random", "lang": "zh", "cat": "wikipedia"},
    {"url": "https://ru.wikipedia.org/wiki/Special:Random", "lang": "ru", "cat": "wikipedia"},
    {"url": "https://ar.wikipedia.org/wiki/Special:Random", "lang": "ar", "cat": "wikipedia"},
    {"url": "https://ko.wikipedia.org/wiki/Special:Random", "lang": "ko", "cat": "wikipedia"},
    {"url": "https://it.wikipedia.org/wiki/Special:Random", "lang": "it", "cat": "wikipedia"},
    {"url": "https://nl.wikipedia.org/wiki/Special:Random", "lang": "nl", "cat": "wikipedia"},
    {"url": "https://pl.wikipedia.org/wiki/Special:Random", "lang": "pl", "cat": "wikipedia"},
    {"url": "https://sv.wikipedia.org/wiki/Special:Random", "lang": "sv", "cat": "wikipedia"},
    {"url": "https://uk.wikipedia.org/wiki/Special:Random", "lang": "uk", "cat": "wikipedia"},
    {"url": "https://vi.wikipedia.org/wiki/Special:Random", "lang": "vi", "cat": "wikipedia"},
    {"url": "https://he.wikipedia.org/wiki/Special:Random", "lang": "he", "cat": "wikipedia"},
    {"url": "https://th.wikipedia.org/wiki/Special:Random", "lang": "th", "cat": "wikipedia"},
    {"url": "https://hi.wikipedia.org/wiki/Special:Random", "lang": "hi", "cat": "wikipedia"},
    {"url": "https://fa.wikipedia.org/wiki/Special:Random", "lang": "fa", "cat": "wikipedia"},
    # Wikimedia sister projects
    {"url": "https://en.wikisource.org/wiki/Special:Random", "lang": "en", "cat": "wikimedia"},
    {"url": "https://en.wikivoyage.org/wiki/Special:Random", "lang": "en", "cat": "wikimedia"},
    {"url": "https://en.wikinews.org/wiki/Special:Random", "lang": "en", "cat": "wikimedia"},
    {"url": "https://en.wikibooks.org/wiki/Special:Random", "lang": "en", "cat": "wikimedia"},
    # Non-Wikipedia: news, tech, reference (test beyond Wikimedia patterns)
    {"url": "https://httpbin.org/html", "lang": "en", "cat": "reference"},
    {"url": "https://news.ycombinator.com/", "lang": "en", "cat": "forum"},
    {"url": "https://lite.cnn.com/", "lang": "en", "cat": "news"},
    {"url": "https://text.npr.org/", "lang": "en", "cat": "news"},
    {"url": "https://lobste.rs/", "lang": "en", "cat": "forum"},
    {"url": "https://www.gutenberg.org/cache/epub/1342/pg1342-images.html", "lang": "en", "cat": "reference"},
]

# ---------------------------------------------------------------------------
# Invariant checks
# ---------------------------------------------------------------------------

TAG_RE = re.compile(r"<(script|style|nav|header|footer|noscript|template|svg)\b", re.I)
DOUBLE_SPACE_RE = re.compile(r"  ")
# Only flag standard HTML tags, not arbitrary <word> patterns.
# Decoded entities like &lt;ref&gt; legitimately produce <ref> in output,
# but standard HTML tags (p, div, span, etc.) should never appear.
HTML_TAG_RE = re.compile(
    r"<(script|style|div|span|p|a|b|i|em|strong|h[1-6]|table|tr|td|th|"
    r"ul|ol|li|nav|header|footer|aside|form|img|br|hr|section|article|"
    r"main|blockquote|code|pre|head|body|html|meta|link|title|input|"
    r"button|select|option|textarea|iframe|noscript|template|svg)\b[^>]*>",
    re.I,
)


@dataclass
class CheckResult:
    name: str
    passed: bool
    detail: str = ""


def check_invariants(text: str, html: str) -> list[CheckResult]:
    """Run extraction invariants against the output text."""
    results = []

    # 1. No double spaces
    if DOUBLE_SPACE_RE.search(text):
        pos = text.index("  ")
        ctx = text[max(0, pos - 20) : pos + 20]
        results.append(CheckResult("no_double_spaces", False, f"at pos {pos}: ...{ctx!r}..."))
    else:
        results.append(CheckResult("no_double_spaces", True))

    # 2. Output is trimmed
    if text != text.strip():
        results.append(CheckResult("trimmed", False, f"leading/trailing whitespace"))
    else:
        results.append(CheckResult("trimmed", True))

    # 3. No leaked skip-tag content (script, style, nav, etc.)
    # Only flag if multiple JS-specific patterns appear together (single
    # keywords like "function" or "var" appear in legitimate article text)
    js_patterns = ["function(", "window.", "document.", "addEventListener", "typeof ", "==="]
    js_hits = sum(1 for kw in js_patterns if kw in text)
    if "<script" in html.lower() and js_hits >= 3:
        results.append(CheckResult("no_script_leak", False, f"{js_hits} JS keywords in output"))
    else:
        results.append(CheckResult("no_script_leak", True))

    # 4. No raw HTML tags in output
    tag_match = HTML_TAG_RE.search(text)
    if tag_match:
        results.append(CheckResult("no_html_tags", False, f"tag: {tag_match.group()[:60]}"))
    else:
        results.append(CheckResult("no_html_tags", True))

    # 5. Non-empty output for non-empty HTML with body content
    if "<body" in html.lower() and len(html) > 200 and len(text) < 10:
        results.append(
            CheckResult("non_empty", False, f"html={len(html)} chars, text={len(text)} chars")
        )
    else:
        results.append(CheckResult("non_empty", True))

    # 6. No null bytes or C0 control characters (except newline, tab)
    bad_chars = [c for c in text if ord(c) < 0x20 and c not in "\n\r\t"]
    if bad_chars:
        codes = [f"U+{ord(c):04X}" for c in bad_chars[:5]]
        results.append(CheckResult("no_control_chars", False, f"found: {codes}"))
    else:
        results.append(CheckResult("no_control_chars", True))

    # 7. No invisible Unicode characters (bidi marks, ZWSP, soft hyphen, etc.)
    invisible = [
        c for c in text
        if ord(c) in (0x200B, 0x200C, 0x200D, 0x200E, 0x200F, 0x00AD, 0x2060, 0xFEFF)
    ]
    if invisible:
        codes = [f"U+{ord(c):04X}" for c in invisible[:5]]
        results.append(CheckResult("no_invisible_chars", False, f"found: {codes}"))
    else:
        results.append(CheckResult("no_invisible_chars", True))

    return results


# ---------------------------------------------------------------------------
# Fetch + extract
# ---------------------------------------------------------------------------


def fetch_url(url: str, timeout: int = 15) -> tuple[str, str]:
    """Fetch a URL, return (final_url, html_content)."""
    req = urllib.request.Request(url, headers={"User-Agent": "deformat-qa/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        final_url = resp.url
        html = resp.read().decode("utf-8", errors="replace")
    return final_url, html


def run_deformat(html: str) -> str:
    """Run deformat's strip_to_text via a small Rust test binary."""
    # Use cargo test --no-run to ensure it's built, then use a Python-native approach
    # by writing HTML to a temp file and using a small inline Rust program.
    # Simpler: just use the Python regex-based check against the compiled test.
    #
    # Actually, the simplest approach: write a small integration binary.
    # But for now, we can test the invariants using Python's own HTML stripping
    # as a sanity check, and rely on cargo test for the Rust-level testing.
    #
    # For real QA, we'll use subprocess with a small Rust example.
    proc = subprocess.run(
        ["cargo", "run", "--example", "strip", "--quiet"],
        input=html,
        capture_output=True,
        text=True,
        timeout=30,
        cwd=Path(__file__).parent.parent,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"strip example failed: {proc.stderr[:200]}")
    return proc.stdout


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


@dataclass
class TestResult:
    url: str
    final_url: str
    lang: str
    category: str
    html_len: int
    text_len: int
    fetch_ms: float
    extract_ms: float
    checks: list[CheckResult] = field(default_factory=list)
    error: str = ""


def main():
    parser = argparse.ArgumentParser(description="QA random URL testing for deformat")
    parser.add_argument("--count", type=int, default=5, help="Number of URLs to test")
    parser.add_argument("--seed", type=int, default=None, help="Random seed for reproducibility")
    parser.add_argument(
        "--category",
        choices=["wikipedia", "wikimedia", "news", "forum", "reference", "all"],
        default="all",
        help="URL category to test",
    )
    parser.add_argument(
        "--report",
        type=str,
        default=None,
        help="Write markdown report to this file",
    )
    args = parser.parse_args()

    if args.seed is not None:
        random.seed(args.seed)

    # Filter sources
    sources = SOURCES
    if args.category != "all":
        sources = [s for s in SOURCES if s["cat"] == args.category]

    # Pick random URLs
    picks = random.choices(sources, k=args.count)

    # Build the example binary first
    print("Building strip example...", flush=True)
    build = subprocess.run(
        ["cargo", "build", "--example", "strip", "--quiet"],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent,
    )
    if build.returncode != 0:
        print(f"Build failed: {build.stderr}", file=sys.stderr)
        sys.exit(1)

    results: list[TestResult] = []
    for i, src in enumerate(picks, 1):
        url = src["url"]
        print(f"[{i}/{args.count}] {url} ({src['lang']})...", end=" ", flush=True)

        result = TestResult(
            url=url,
            final_url="",
            lang=src["lang"],
            category=src["cat"],
            html_len=0,
            text_len=0,
            fetch_ms=0,
            extract_ms=0,
        )

        try:
            t0 = time.monotonic()
            final_url, html = fetch_url(url)
            result.fetch_ms = (time.monotonic() - t0) * 1000
            result.final_url = final_url
            result.html_len = len(html)

            t0 = time.monotonic()
            text = run_deformat(html)
            result.extract_ms = (time.monotonic() - t0) * 1000
            result.text_len = len(text)

            result.checks = check_invariants(text, html)
            failed = [c for c in result.checks if not c.passed]
            if failed:
                names = ", ".join(c.name for c in failed)
                print(f"FAIL ({names})")
            else:
                print(f"OK ({result.text_len} chars, {result.extract_ms:.0f}ms)")

        except Exception as e:
            result.error = str(e)[:200]
            print(f"ERROR: {result.error}")

        results.append(result)

    # Summary
    total = len(results)
    errored = sum(1 for r in results if r.error)
    all_checks = [c for r in results for c in r.checks]
    failed_checks = [c for c in all_checks if not c.passed]

    print(f"\n--- Summary ---")
    print(f"URLs tested: {total}")
    print(f"Fetch errors: {errored}")
    print(f"Invariant checks: {len(all_checks)} total, {len(failed_checks)} failed")

    if failed_checks:
        print(f"\nFailed checks:")
        for c in failed_checks:
            print(f"  - {c.name}: {c.detail}")

    # Write report
    if args.report:
        report_path = Path(args.report)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        with open(report_path, "w") as f:
            f.write(f"# deformat QA Random URL Report\n\n")
            f.write(f"Date: {datetime.now(timezone.utc).isoformat()}\n")
            f.write(f"Seed: {args.seed}\n")
            f.write(f"Count: {args.count}\n\n")
            f.write(f"## Results\n\n")
            for r in results:
                status = "ERROR" if r.error else ("FAIL" if any(not c.passed for c in r.checks) else "PASS")
                f.write(f"### [{status}] {r.lang} -- {r.final_url or r.url}\n\n")
                if r.error:
                    f.write(f"Error: {r.error}\n\n")
                else:
                    f.write(f"- HTML: {r.html_len:,} chars, Text: {r.text_len:,} chars\n")
                    f.write(f"- Fetch: {r.fetch_ms:.0f}ms, Extract: {r.extract_ms:.0f}ms\n")
                    for c in r.checks:
                        mark = "PASS" if c.passed else "FAIL"
                        detail = f" -- {c.detail}" if c.detail else ""
                        f.write(f"- [{mark}] {c.name}{detail}\n")
                    f.write("\n")
            f.write(f"\n## Summary\n\n")
            f.write(f"- URLs tested: {total}\n")
            f.write(f"- Fetch errors: {errored}\n")
            f.write(f"- Checks: {len(all_checks)} total, {len(failed_checks)} failed\n")
        print(f"\nReport written to {report_path}")

    sys.exit(1 if failed_checks or errored else 0)


if __name__ == "__main__":
    main()
