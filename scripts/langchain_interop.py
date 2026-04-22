#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Demonstrate deformat Segment JSON -> LangChain Document shape.

Usage:
    # First, emit Segment JSON from a Rust binary or the `segments` example:
    cargo run --example segments --features serde > /tmp/segments.json

    # Then convert to LangChain-compatible dicts (stdlib-only, no langchain
    # required to run the demo):
    uv run scripts/langchain_interop.py /tmp/segments.json

Intent: show that the JSON shape emitted by `serde::Serialize` on
`Vec<Segment>` deserializes directly into (page_content, metadata) --
the same shape LangChain's UnstructuredLoader (element mode) produces.
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from typing import Any


@dataclass
class Document:
    """Minimal stand-in for langchain_core.documents.Document."""

    page_content: str
    metadata: dict[str, Any]


def segments_to_documents(segments: list[dict[str, Any]]) -> list[Document]:
    """Convert deformat Segment JSON to LangChain-compatible Documents.

    In a real LangChain pipeline, replace the local Document dataclass
    with `from langchain_core.documents import Document` -- the shape
    is identical.
    """
    docs: list[Document] = []
    for seg in segments:
        meta = seg.get("metadata", {})
        docs.append(
            Document(
                page_content=seg["text"],
                metadata={
                    "category": seg["type"],
                    "element_id": seg["element_id"],
                    # Include only present metadata; drop None values.
                    **{
                        k: meta[k]
                        for k in (
                            "parent_id",
                            "category_depth",
                            "text_as_html",
                            "page_number",
                            "filename",
                            "filetype",
                            "languages",
                            "coordinates",
                        )
                        if k in meta and meta[k] is not None
                    },
                },
            )
        )
    return docs


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <segments.json>", file=sys.stderr)
        return 2
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        segments = json.load(f)
    if not isinstance(segments, list):
        print("expected a top-level JSON array of segments", file=sys.stderr)
        return 2

    docs = segments_to_documents(segments)
    print(f"Converted {len(segments)} segment(s) -> {len(docs)} Document(s)")
    print()

    for doc in docs:
        category = doc.metadata.get("category", "?")
        depth = doc.metadata.get("category_depth")
        depth_str = f" depth={depth}" if depth is not None else ""
        preview = doc.page_content[:80].replace("\n", " ")
        ellipsis = "..." if len(doc.page_content) > 80 else ""
        print(f"[{category}{depth_str}] {preview!r}{ellipsis}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
