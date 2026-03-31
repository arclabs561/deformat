#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
"""Benchmark deformat against the Scrapinghub Article Extraction Benchmark.

Downloads the MIT-licensed benchmark (181 HTML files + ground truth),
runs deformat's strip example on each, and compares output to ground truth
using token overlap (F1).

Requirements:
    cargo build --example strip   (builds the deformat strip binary)

Usage:
    uv run scripts/bench_extraction.py              # full benchmark
    uv run scripts/bench_extraction.py --limit 20   # quick sample
    uv run scripts/bench_extraction.py --redownload  # force re-download
"""

import argparse
import gzip
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = "scrapinghub/article-extraction-benchmark"
CACHE_DIR = Path("target/bench-fixtures/scrapinghub")
GT_URL = f"https://raw.githubusercontent.com/{REPO}/refs/heads/master/ground-truth.json"
HTML_URL = f"https://raw.githubusercontent.com/{REPO}/refs/heads/master/html"


def download_file(url: str, dest: Path) -> None:
    import requests

    resp = requests.get(url, timeout=30)
    resp.raise_for_status()
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(resp.content)


def ensure_fixtures(redownload: bool = False) -> tuple[dict, Path]:
    gt_path = CACHE_DIR / "ground-truth.json"
    html_dir = CACHE_DIR / "html"

    if redownload or not gt_path.exists():
        print(f"Downloading ground truth from {REPO}...")
        download_file(GT_URL, gt_path)

    gt = json.loads(gt_path.read_text())

    html_dir.mkdir(parents=True, exist_ok=True)
    missing = [k for k in gt if not (html_dir / f"{k}.html.gz").exists()]
    if missing:
        print(f"Downloading {len(missing)} HTML files...")
        for i, key in enumerate(missing):
            url = f"{HTML_URL}/{key}.html.gz"
            dest = html_dir / f"{key}.html.gz"
            try:
                download_file(url, dest)
            except Exception as e:
                print(f"  [{i+1}/{len(missing)}] FAILED {key[:12]}...: {e}")
                continue
            if (i + 1) % 20 == 0:
                print(f"  [{i+1}/{len(missing)}] downloaded")
        print(f"  Done. {len(missing)} files.")

    return gt, html_dir


def tokenize(text: str) -> set[str]:
    """Simple whitespace tokenization for F1 computation."""
    return set(text.lower().split())


def token_f1(extracted: str, reference: str) -> tuple[float, float, float]:
    """Compute token-level precision, recall, F1."""
    ext_tokens = tokenize(extracted)
    ref_tokens = tokenize(reference)
    if not ref_tokens:
        return (1.0, 1.0, 1.0) if not ext_tokens else (0.0, 0.0, 0.0)
    if not ext_tokens:
        return (0.0, 0.0, 0.0)
    overlap = ext_tokens & ref_tokens
    precision = len(overlap) / len(ext_tokens)
    recall = len(overlap) / len(ref_tokens)
    if precision + recall == 0:
        return (0.0, 0.0, 0.0)
    f1 = 2 * precision * recall / (precision + recall)
    return (precision, recall, f1)


def run_strip(html: str) -> str:
    """Run deformat strip example on HTML, return extracted text."""
    result = subprocess.run(
        ["cargo", "run", "--example", "strip", "--release", "-q"],
        input=html,
        capture_output=True,
        text=True,
        timeout=10,
    )
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description="Benchmark deformat extraction")
    parser.add_argument("--limit", type=int, default=0, help="Limit to N samples")
    parser.add_argument("--redownload", action="store_true", help="Force re-download")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show per-file scores")
    args = parser.parse_args()

    # Build the strip example first
    print("Building deformat strip example (release)...")
    subprocess.run(
        ["cargo", "build", "--example", "strip", "--release", "-q"],
        check=True,
    )

    gt, html_dir = ensure_fixtures(args.redownload)

    keys = sorted(gt.keys())
    if args.limit:
        keys = keys[: args.limit]

    scores = []
    errors = 0
    for i, key in enumerate(keys):
        gz_path = html_dir / f"{key}.html.gz"
        if not gz_path.exists():
            errors += 1
            continue
        html = gzip.decompress(gz_path.read_bytes()).decode("utf-8", errors="replace")
        extracted = run_strip(html)
        reference = gt[key]["articleBody"]
        p, r, f1 = token_f1(extracted, reference)
        scores.append((key, p, r, f1))
        if args.verbose:
            url = gt[key].get("url", "")
            status = "OK" if f1 > 0.5 else "LOW"
            print(f"  [{status}] F1={f1:.3f} P={p:.3f} R={r:.3f} {url[:60]}")
        if (i + 1) % 20 == 0 and not args.verbose:
            print(f"  [{i+1}/{len(keys)}] processed")

    if not scores:
        print("No scores computed. Check fixture download.")
        sys.exit(1)

    f1s = [s[3] for s in scores]
    ps = [s[1] for s in scores]
    rs = [s[2] for s in scores]
    avg_f1 = sum(f1s) / len(f1s)
    avg_p = sum(ps) / len(ps)
    avg_r = sum(rs) / len(rs)
    median_f1 = sorted(f1s)[len(f1s) // 2]
    low_f1 = [s for s in scores if s[3] < 0.3]

    print(f"\n{'='*60}")
    print(f"Scrapinghub Article Extraction Benchmark")
    print(f"{'='*60}")
    print(f"Samples:    {len(scores)} / {len(keys)} (errors: {errors})")
    print(f"Avg  F1:    {avg_f1:.3f}  (P={avg_p:.3f}, R={avg_r:.3f})")
    print(f"Median F1:  {median_f1:.3f}")
    print(f"Low (<0.3): {len(low_f1)} / {len(scores)}")
    if low_f1:
        print(f"\nLowest scoring pages:")
        for key, p, r, f1 in sorted(low_f1, key=lambda x: x[3])[:5]:
            url = gt[key].get("url", key[:16])
            print(f"  F1={f1:.3f} P={p:.3f} R={r:.3f} {url[:70]}")


if __name__ == "__main__":
    main()
