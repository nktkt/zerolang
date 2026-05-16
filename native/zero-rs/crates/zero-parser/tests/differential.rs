//! Phase 3 differential test: parse the corpus with the Rust port and
//! compare the resulting `parse --json` summary against the C compiler's
//! output, structurally (parsed as `serde_json::Value`).
//!
//! Skips if `bin/zero` is not built; set `ZERO_DIFFERENTIAL_REQUIRED=1`
//! to fail instead.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use zero_diag::Diag;
use zero_lexer::tokenize;
use zero_parser::{parse, parse_to_json};

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has 4 ancestors")
        .to_path_buf()
}

fn c_compiler_path(root: &Path) -> PathBuf {
    if let Ok(custom) = std::env::var("ZERO_BIN_C") {
        return PathBuf::from(custom);
    }
    root.join("bin").join("zero")
}

fn required() -> bool {
    matches!(
        std::env::var("ZERO_DIFFERENTIAL_REQUIRED").as_deref(),
        Ok("1") | Ok("true")
    )
}

fn diff_path(path: &str, a: &Value, b: &Value) -> Option<String> {
    if a == b {
        return None;
    }
    match (a, b) {
        (Value::Object(am), Value::Object(bm)) => {
            let mut keys: Vec<&String> = am.keys().chain(bm.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let next_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (am.get(k), bm.get(k)) {
                    (Some(av), Some(bv)) => {
                        if let Some(d) = diff_path(&next_path, av, bv) {
                            return Some(d);
                        }
                    }
                    (Some(_), None) => return Some(format!("{next_path} only in rust")),
                    (None, Some(_)) => return Some(format!("{next_path} only in c")),
                    (None, None) => {}
                }
            }
            None
        }
        (Value::Array(aa), Value::Array(ba)) => {
            if aa.len() != ba.len() {
                return Some(format!(
                    "{path}.len() differs: rust={} c={}",
                    aa.len(),
                    ba.len()
                ));
            }
            for (i, (av, bv)) in aa.iter().zip(ba.iter()).enumerate() {
                if let Some(d) = diff_path(&format!("{path}[{i}]"), av, bv) {
                    return Some(d);
                }
            }
            None
        }
        _ => Some(format!("{path}: rust={a} c={b}")),
    }
}

/// Files we expect the summary parser to handle. Conservative list — the
/// full conformance/* sweep arrives when the parser grows real expression
/// parsing (Phase 3.5 / Phase 4 prerequisite).
fn corpus(root: &Path) -> Vec<PathBuf> {
    [
        "examples/hello.0",
        "examples/add.0",
        "examples/branch.0",
        "examples/countdown.0",
    ]
    .iter()
    .map(|p| root.join(p))
    .filter(|p| p.exists())
    .collect()
}

#[test]
fn parse_json_matches_c_compiler() {
    let root = repo_root();
    let zero_bin = c_compiler_path(&root);
    if !zero_bin.exists() {
        if required() {
            panic!(
                "ZERO_DIFFERENTIAL_REQUIRED set but {} does not exist",
                zero_bin.display()
            );
        }
        eprintln!("differential skipped: {} not built", zero_bin.display());
        return;
    }

    let inputs = corpus(&root);
    assert!(!inputs.is_empty());

    for input in &inputs {
        let source = std::fs::read_to_string(input).expect("read corpus file");
        let mut diag = Diag::default();
        let tokens = tokenize(&source, &mut diag);
        assert_eq!(diag.code, 0, "{}: lexer error: {:?}", input.display(), diag);
        let mut pdiag = Diag::default();
        let program = parse(&tokens, &mut pdiag);
        assert_eq!(pdiag.code, 0, "{}: parser error: {:?}", input.display(), pdiag);

        // Use the exact same path string for both compilers so sourceFile
        // matches verbatim.
        let path_str = input.display().to_string();
        let rust_json_str = parse_to_json(&path_str, &program);
        let rust_json: Value = serde_json::from_str(&rust_json_str).unwrap();

        let output = Command::new(&zero_bin)
            .args(["parse", "--json"])
            .arg(input)
            .output()
            .expect("spawning C compiler");
        assert!(
            output.status.success(),
            "{}: C compiler failed: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let c_json: Value = serde_json::from_slice(&output.stdout).expect("C JSON valid");

        if let Some(diff) = diff_path("", &rust_json, &c_json) {
            panic!(
                "{}: parse JSON differs from C:\n  {diff}\nrust={rust_json:#}\nc={c_json:#}",
                input.display()
            );
        }
    }
    eprintln!("parse differential ok: {} files matched", inputs.len());
}
