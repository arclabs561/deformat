#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "huggingface-hub>=0.24",
# ]
# ///
"""Download the WCXB (Web Content Extraction Benchmark) fixtures.

WCXB is CC-BY-4.0, maintained by Murrough Foley at
https://huggingface.co/datasets/murrough-foley/web-content-extraction-benchmark

Files land in target/bench-fixtures/wcxb/{dev,test}/. Idempotent: the
huggingface-hub client skips already-downloaded files.

Usage:
    scripts/fetch_wcxb.py           # dev split only (faster)
    scripts/fetch_wcxb.py --all     # dev + held-out test split
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ID = "murrough-foley/web-content-extraction-benchmark"
REPO_TYPE = "dataset"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--all", action="store_true", help="also download test split")
    parser.add_argument(
        "--dest",
        default=None,
        help="destination dir (default: <repo>/target/bench-fixtures/wcxb)",
    )
    args = parser.parse_args()

    try:
        from huggingface_hub import snapshot_download  # type: ignore[import-not-found]
    except ImportError as e:
        print(f"huggingface_hub not available: {e}", file=sys.stderr)
        return 1

    root = Path(__file__).resolve().parent.parent
    dest = Path(args.dest) if args.dest else root / "target" / "bench-fixtures" / "wcxb"
    dest.mkdir(parents=True, exist_ok=True)

    allow_patterns = ["dev/**", "metadata.json", "README.md", "LICENSE"]
    if args.all:
        allow_patterns.append("test/**")

    print(f"Downloading WCXB to {dest}")
    print(f"Patterns: {allow_patterns}")
    path = snapshot_download(
        repo_id=REPO_ID,
        repo_type=REPO_TYPE,
        local_dir=str(dest),
        allow_patterns=allow_patterns,
    )
    print(f"Done: {path}")

    # Report basic counts so the user sees the scale.
    for split in ("dev", "test"):
        gt_dir = dest / split / "ground-truth"
        html_dir = dest / split / "html"
        if gt_dir.exists() and html_dir.exists():
            gt_count = sum(1 for _ in gt_dir.glob("*.json"))
            html_count = sum(1 for _ in html_dir.glob("*.html"))
            print(f"  {split}: {gt_count} ground-truth, {html_count} html files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
