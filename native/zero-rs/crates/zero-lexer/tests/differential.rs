//! Phase 2 differential test: tokenize a corpus with both the Rust port
//! and the C compiler (`bin/zero tokens --json`), then compare the parsed
//! token streams element-by-element.
//!
//! The C compiler emits JSON with its own formatting conventions (pretty
//! top-level, compact per-token). Rather than mimic that text format, we
//! parse both outputs as JSON and compare the semantically meaningful
//! fields. This keeps the gate sensitive to real semantic divergence
//! (different token kind, text, offset, line, column, length) without
//! coupling to C-side whitespace conventions.
//!
//! Skips when `bin/zero` is not present (e.g. fresh checkout that has
//! not built the C compiler yet). To force a fail-on-skip in CI, set
//! `ZERO_DIFFERENTIAL_REQUIRED=1`.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use zero_diag::Diag;
use zero_lexer::tokenize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct CToken {
    kind: String,
    text: String,
    line: u32,
    column: u32,
    offset: usize,
    length: usize,
}

#[derive(Debug, Deserialize)]
struct CTokensOutput {
    #[allow(dead_code)]
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[allow(dead_code)]
    #[serde(rename = "sourceFile")]
    source_file: String,
    tokens: Vec<CToken>,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../native/zero-rs/crates/zero-lexer
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent() // crates
        .and_then(|p| p.parent()) // zero-rs
        .and_then(|p| p.parent()) // native
        .and_then(|p| p.parent()) // repo root
        .expect("CARGO_MANIFEST_DIR has 4 ancestors")
        .to_path_buf()
}

fn c_compiler_path(root: &Path) -> PathBuf {
    if let Ok(custom) = std::env::var("ZERO_BIN_C") {
        return PathBuf::from(custom);
    }
    root.join("bin").join("zero")
}

fn c_tokens(zero_bin: &Path, input_path: &Path) -> Vec<CToken> {
    let output = Command::new(zero_bin)
        .args(["tokens", "--json"])
        .arg(input_path)
        .output()
        .expect("spawning C compiler");
    assert!(
        output.status.success(),
        "C compiler exited non-zero on {}: stderr={}",
        input_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: CTokensOutput =
        serde_json::from_slice(&output.stdout).expect("C compiler emitted valid JSON");
    parsed.tokens
}

fn rust_tokens(source: &str) -> Vec<CToken> {
    let mut diag = Diag::default();
    let tokens = tokenize(source, &mut diag);
    assert_eq!(
        diag.code, 0,
        "Rust lexer reported diagnostic {}: {}",
        diag.code, diag.message
    );
    // Include EOF — the C compiler emits it as the last entry in tokens --json.
    tokens
        .into_iter()
        .map(|t| CToken {
            kind: t.kind.as_str().to_string(),
            text: t.text,
            line: t.line,
            column: t.column,
            offset: t.offset,
            length: t.length,
        })
        .collect()
}

/// Files we know the lexer should handle. Kept small for fast iteration;
/// the full conformance/* sweep arrives once Phase 3 (parser) lands so
/// the harness can also gate parse output.
fn corpus(root: &Path) -> Vec<PathBuf> {
    [
        "examples/hello.0",
        "examples/add.0",
        "examples/branch.0",
        "examples/countdown.0",
        "examples/cli-file.0",
        "examples/codec-varint.0",
        "examples/const-arithmetic.0",
        "examples/direct-alloc-bump.0",
    ]
    .iter()
    .map(|p| root.join(p))
    .filter(|p| p.exists())
    .collect()
}

fn required() -> bool {
    matches!(
        std::env::var("ZERO_DIFFERENTIAL_REQUIRED").as_deref(),
        Ok("1") | Ok("true")
    )
}

#[test]
fn token_streams_match_c_compiler() {
    let root = repo_root();
    let zero_bin = c_compiler_path(&root);
    if !zero_bin.exists() {
        if required() {
            panic!(
                "ZERO_DIFFERENTIAL_REQUIRED set but {} does not exist; run `make -C native/zero-c`",
                zero_bin.display()
            );
        }
        eprintln!(
            "differential test skipped: {} not built (set ZERO_DIFFERENTIAL_REQUIRED=1 to fail instead)",
            zero_bin.display()
        );
        return;
    }

    let inputs = corpus(&root);
    assert!(!inputs.is_empty(), "no corpus files found under {}", root.display());

    let mut total_tokens = 0usize;
    for input in &inputs {
        let source = std::fs::read_to_string(input).expect("read corpus file");
        let rust = rust_tokens(&source);
        let c = c_tokens(&zero_bin, input);
        total_tokens += rust.len();

        assert_eq!(
            rust.len(),
            c.len(),
            "{}: token count differs (rust={}, c={})",
            input.display(),
            rust.len(),
            c.len()
        );

        for (i, (r, c_tok)) in rust.iter().zip(c.iter()).enumerate() {
            assert_eq!(
                r, c_tok,
                "{}: token #{i} differs\n  rust={:?}\n     c={:?}",
                input.display(),
                r,
                c_tok
            );
        }
    }
    eprintln!(
        "differential ok: {} files, {} tokens matched",
        inputs.len(),
        total_tokens
    );
}
