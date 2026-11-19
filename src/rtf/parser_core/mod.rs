//! Native parser from rtf-parser-tt v0.6.0 at commit 3178d62b0c05a9cb294127b2c42cb58afb62ed5a.
//! Apply the [parser fixes](https://github.com/arclabs561/deformat/blob/be6b1fb4f9a3791dbead9ef0855f432678621021/.github/rtf-parser-fixes.patch) to reproduce the parsing changes.
//! The source uses module-relative imports. The serialized, Wasm and test APIs are
//! omitted. Replace this module with the upstream crate when a published version
//! passes the RTF regression tests. See LICENSE-rtf-parser-tt for its MIT license.

#![allow(dead_code, irrefutable_let_patterns)]

mod codepages;
mod document;
mod encoding;
mod header;
mod lexer;
mod paragraph;
mod parser;
mod tokens;

pub(super) use document::RtfDocument;
