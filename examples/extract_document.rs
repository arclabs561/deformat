//! Print the text of a document using the enabled format features.

use std::{
    env, fs,
    io::{self, Write},
    path::Path,
};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("extract_document: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = match (args.next(), args.next()) {
        (Some(path), None) => path,
        _ => return Err("usage: extract_document PATH".into()),
    };
    extract_file(Path::new(&path), io::stdout().lock())
}

fn extract_file(path: &Path, mut out: impl Write) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let document = deformat::extract_from_bytes(&bytes)?;
    writeln!(out, "{}", document.text)?;
    Ok(())
}

#[cfg(all(test, feature = "docx"))]
mod tests {
    use super::*;

    #[test]
    fn prints_plain_text_from_a_docx() {
        let file =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic/minimal.docx");
        let mut out = Vec::new();
        extract_file(&file, &mut out).unwrap();
        assert!(out.starts_with(b"Hello from DOCX."));
        assert!(out.ends_with(b"\n"));
    }
}
