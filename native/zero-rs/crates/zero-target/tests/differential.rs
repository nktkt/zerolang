//! Phase 1.5 differential test: produce the targets report with the Rust
//! port and compare it structurally against `bin/zero targets --json`.
//!
//! The C compiler emits non-standard formatting (pretty top-level, compact
//! per-target), so we parse both outputs as JSON values and compare via
//! `serde_json::Value` equality. This is sensitive to semantic divergence
//! (different field, different value, missing key) without coupling to
//! C-side whitespace conventions.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use zero_target::{parse_manifest, targets_report};

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
            // Report first differing key in deterministic order.
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
                return Some(format!("{path}.len() differs: rust={} c={}", aa.len(), ba.len()));
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

#[test]
fn targets_json_matches_c_compiler() {
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

    let output = Command::new(&zero_bin)
        .args(["targets", "--json"])
        .output()
        .expect("spawning C compiler");
    assert!(
        output.status.success(),
        "C compiler exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let c_json: Value =
        serde_json::from_slice(&output.stdout).expect("C compiler emitted valid JSON");

    let manifest_path = root.join("native").join("zero-c").join("targets").join("targets.manifest");
    let manifest_text = std::fs::read_to_string(&manifest_path).expect("manifest exists");
    let manifest = parse_manifest(&manifest_text).expect("manifest parses");
    let rust_report = targets_report(&manifest);
    let rust_json: Value =
        serde_json::to_value(&rust_report).expect("rust report serializes to JSON value");

    if let Some(diff) = diff_path("", &rust_json, &c_json) {
        panic!("Rust targets JSON differs from C:\n  {diff}\nrust={rust_json:#}\nc={c_json:#}");
    }
}
