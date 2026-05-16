//! `zero` CLI entry point (Rust port).
//!
//! Currently implements: --version, --help, tokens --json, parse --json,
//! targets --json. All other subcommands return exit code 2 with a clear
//! "not yet implemented in Rust port" message so that scripts which call
//! them fail loudly instead of silently producing empty output.

use anyhow::Result;
use std::process::ExitCode;
use zero_diag::Diag;

const VERSION: &str = "0.1.1";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("zero: {e}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(args: &[String]) -> Result<u8> {
    if args.is_empty() {
        print_help();
        return Ok(1);
    }
    let first = args[0].as_str();
    let rest = &args[1..];

    if first == "--help" || first == "-h" || first == "help" {
        print_help();
        return Ok(0);
    }
    if first == "--version" || first == "version" {
        return cmd_version(rest);
    }

    match first {
        "tokens" => cmd_tokens(rest),
        "parse" => cmd_parse(rest),
        "targets" => cmd_targets(rest),
        "check" => cmd_check(rest),
        "explain" => cmd_explain(rest),
        "clean" => cmd_clean(rest),
        "doctor" => cmd_doctor(rest),
        "skills" => cmd_skills(rest),
        "routes" => cmd_routes(rest),
        "build" | "run" | "ship" | "test" | "fmt" | "new"
        | "doc" | "graph" | "size" | "mem" | "dev" | "time" | "abi"
        | "fix" => {
            eprintln!(
                "zero (Rust port): subcommand '{first}' is not yet implemented; use the C binary at bin/zero or set ZERO_BIN=bin/zero"
            );
            Ok(2)
        }
        _ => {
            eprintln!("zero: unknown subcommand '{first}'");
            print_help();
            Ok(1)
        }
    }
}

fn want_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

fn input_path(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str)
}

fn cmd_version(args: &[String]) -> Result<u8> {
    let commit = std::env::var("ZERO_COMMIT").unwrap_or_else(|_| "unknown".into());
    let commit = if commit.is_empty() { "unknown".to_string() } else { commit };
    let host = zero_target::host_target();
    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "version": VERSION,
            "commit": commit,
            "host": host,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("zero {VERSION}");
        println!("commit: {commit}");
        println!("host: {host}");
    }
    Ok(0)
}

fn cmd_tokens(args: &[String]) -> Result<u8> {
    if !want_json(args) {
        eprintln!("Usage: zero tokens --json <file.0>");
        return Ok(1);
    }
    let Some(path) = input_path(args) else {
        eprintln!("Usage: zero tokens --json <file.0>");
        return Ok(1);
    };
    let source = std::fs::read_to_string(path)?;
    let mut diag = Diag::default();
    let tokens = zero_lexer::tokenize(&source, &mut diag);
    if diag.code != 0 {
        eprintln!("{}:{}:{} {}", path, diag.line, diag.column, diag.message);
        return Ok(1);
    }
    println!("{}", zero_lexer::tokens_to_json(path, &tokens));
    Ok(0)
}

fn cmd_parse(args: &[String]) -> Result<u8> {
    if !want_json(args) {
        eprintln!("Usage: zero parse --json <file.0>");
        return Ok(1);
    }
    let Some(path) = input_path(args) else {
        eprintln!("Usage: zero parse --json <file.0>");
        return Ok(1);
    };
    let source = std::fs::read_to_string(path)?;
    let mut ldiag = Diag::default();
    let tokens = zero_lexer::tokenize(&source, &mut ldiag);
    if ldiag.code != 0 {
        eprintln!("{}:{}:{} {}", path, ldiag.line, ldiag.column, ldiag.message);
        return Ok(1);
    }
    let mut pdiag = Diag::default();
    let program = zero_parser::parse(&tokens, &mut pdiag);
    if pdiag.code != 0 {
        eprintln!("{}:{}:{} {}", path, pdiag.line, pdiag.column, pdiag.message);
        return Ok(1);
    }
    println!("{}", zero_parser::parse_to_json(path, &program));
    Ok(0)
}

fn cmd_targets(args: &[String]) -> Result<u8> {
    if !want_json(args) {
        eprintln!("Usage: zero targets --json");
        return Ok(1);
    }
    // Locate the manifest relative to the binary or use the well-known
    // path in the repo. We try both so the binary works both from the
    // workspace root and from the .zero/bin/ install location.
    let candidates = [
        "native/zero-c/targets/targets.manifest",
        "../native/zero-c/targets/targets.manifest",
        "../../native/zero-c/targets/targets.manifest",
    ];
    let manifest_text = candidates
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .ok_or_else(|| anyhow::anyhow!("could not locate targets.manifest"))?;
    let targets = zero_target::parse_manifest(&manifest_text)?;
    let report = zero_target::targets_report(&targets);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(0)
}

fn cmd_check(args: &[String]) -> Result<u8> {
    let Some(path) = input_path(args) else {
        eprintln!("Usage: zero check [--json] <file.0>");
        return Ok(1);
    };
    let source = std::fs::read_to_string(path)?;
    let mut ldiag = Diag::default();
    let tokens = zero_lexer::tokenize(&source, &mut ldiag);
    let mut pdiag = Diag::default();
    let _program = if ldiag.code == 0 {
        zero_parser::parse(&tokens, &mut pdiag)
    } else {
        zero_parser::parse(&[], &mut pdiag)
    };
    // PHASE 4 STUB: parser-only check. Programs that lex and parse are
    // reported as ok; type / borrow / effect errors are NOT detected.
    let active_diag = if ldiag.code != 0 { &ldiag } else { &pdiag };
    let ok = active_diag.code == 0;
    if want_json(args) {
        let diagnostics: Vec<serde_json::Value> = if ok {
            vec![]
        } else {
            vec![serde_json::json!({
                "code": zero_diag::diag_code(active_diag.code),
                "message": active_diag.message,
                "line": active_diag.line,
                "column": active_diag.column,
                "path": path,
            })]
        };
        let value = serde_json::json!({
            "schemaVersion": 1,
            "ok": ok,
            "sourceFile": path,
            "diagnostics": diagnostics,
            "note": "zero-rs: parser-only check; type/borrow/effect checking not yet ported",
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else if ok {
        println!("check ok");
    } else {
        eprintln!(
            "{}:{}:{} {}: {}",
            path,
            active_diag.line,
            active_diag.column,
            zero_diag::diag_code(active_diag.code),
            active_diag.message
        );
        return Ok(1);
    }
    if ok {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn cmd_explain(args: &[String]) -> Result<u8> {
    let Some(code) = input_path(args) else {
        eprintln!("Usage: zero explain [--json] <code>");
        return Ok(1);
    };
    // PHASE 7 STUB: only the diag_code -> string mapping is ported; the
    // rich explain text (category, summary, why, repair, examples) lives
    // in main.c::append_explain_json as ~1500 LOC of structured data.
    // We emit a minimal stub so the CLI surface exists.
    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "code": code,
            "summary": format!("Diagnostic {code} (full explain text not yet ported)"),
            "note": "zero-rs: explain text data port not yet complete",
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{code}: (full explain text not yet ported in Rust)");
        println!();
        println!("The C compiler at bin/zero has rich explain text for this code.");
        println!("Run: bin/zero explain {code}");
    }
    Ok(0)
}

fn cmd_clean(_args: &[String]) -> Result<u8> {
    // Mirrors the C `zero clean` subcommand: removes the per-checkout
    // `.zero/` cache directory (containing build outputs and caches).
    // Print each removed path on its own line, matching C output format.
    let target = std::path::Path::new(".zero");
    if !target.exists() {
        println!("nothing to remove");
        return Ok(0);
    }
    println!("removed:");
    // Walk and print top-level entries; rely on remove_dir_all for cleanup.
    if let Ok(entries) = std::fs::read_dir(target) {
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path().display().to_string())
            .collect();
        names.sort();
        for n in names {
            println!("{n}");
        }
    }
    println!(".zero");
    std::fs::remove_dir_all(target).ok();
    Ok(0)
}

fn cmd_doctor(args: &[String]) -> Result<u8> {
    // Minimal doctor: reports host info and toolchain availability for
    // the cc/zig commands. The C `doctor` emits a far richer report
    // (PATH audit, per-target toolchain plans, wasi runner detection,
    // .zero writability, etc.) — that's a future port.
    let host = zero_target::host_target();
    let has_cc = which_exists("cc");
    let has_zig = which_exists("zig");
    if want_json(args) {
        let status = if has_cc { "ok" } else { "warning" };
        let value = serde_json::json!({
            "schemaVersion": 1,
            "status": status,
            "host": host,
            "checks": [
                {"name": "host", "status": "ok", "message": host},
                {"name": "native-c-compiler", "status": if has_cc { "ok" } else { "missing" }, "message": if has_cc { "cc available on PATH" } else { "cc not found on PATH" }},
                {"name": "target-c-compiler", "status": if has_zig { "ok" } else { "missing" }, "message": if has_zig { "zig available on PATH" } else { "zig not found on PATH (needed for cross-compilation)" }},
            ],
            "note": "zero-rs: minimal doctor; PATH audit, per-target toolchain plans, wasi runner detection NOT yet ported",
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("host: {host}");
        println!("native-c-compiler: {}", if has_cc { "ok" } else { "missing" });
        println!("target-c-compiler: {}", if has_zig { "ok" } else { "missing" });
        println!();
        println!("(zero-rs: minimal report; use bin/zero doctor for the full audit)");
    }
    Ok(if has_cc { 0 } else { 1 })
}

fn which_exists(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else { return false };
    for dir in path.split(':') {
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

fn cmd_skills(args: &[String]) -> Result<u8> {
    // Minimal skills surface: list/get/path return JSON shells.
    // The C `skills` subcommand has rich data baked in from
    // skill-data/*.md; that's ~10KB of structured data that needs
    // its own port PR.
    let sub = args.iter().find(|a| !a.starts_with("--")).map(String::as_str);
    let want_json = args.iter().any(|a| a == "--json");
    match sub {
        Some("list") | None => {
            if want_json {
                let value = serde_json::json!({
                    "schemaVersion": 1,
                    "data": [
                        {"name": "zero", "description": "Zero language skill stub (rich data not yet ported)"}
                    ],
                    "note": "zero-rs: skill data not yet ported; use bin/zero skills for full content"
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("zero (skill stub; full data not yet ported in Rust)");
            }
            Ok(0)
        }
        Some("get") => {
            let name = args
                .iter()
                .filter(|a| !a.starts_with("--"))
                .nth(1)
                .map(String::as_str)
                .unwrap_or("zero");
            if want_json {
                let value = serde_json::json!({
                    "schemaVersion": 1,
                    "name": name,
                    "summary": "Skill data not yet ported",
                    "note": "zero-rs: use bin/zero skills get for full content"
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{name}: (skill data not yet ported in Rust)");
            }
            Ok(0)
        }
        Some("path") => {
            if want_json {
                let value = serde_json::json!({
                    "schemaVersion": 1,
                    "path": "skills/zero/SKILL.md",
                    "note": "zero-rs: path resolver minimal stub"
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("skills/zero/SKILL.md");
            }
            Ok(0)
        }
        Some(other) => {
            eprintln!("zero skills: unknown subcommand '{other}' (use list/get/path)");
            Ok(1)
        }
    }
}

fn cmd_routes(args: &[String]) -> Result<u8> {
    let Some(path) = input_path(args) else {
        eprintln!("Usage: zero routes [--json] <project>");
        return Ok(1);
    };
    // Minimal routes: enumerates .0 files under <project>/src/routes/
    // (or <project> if it points at routes/). Reports route count only;
    // does NOT compute method/path from source (which requires parsing
    // and analyzing per-file route declarations).
    let mut routes: Vec<String> = Vec::new();
    let project_root = std::path::Path::new(path);
    let candidates = [
        project_root.join("src").join("routes"),
        project_root.join("routes"),
        project_root.to_path_buf(),
    ];
    for dir in &candidates {
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    if e.path().extension().and_then(|s| s.to_str()) == Some("0") {
                        routes.push(e.path().display().to_string());
                    }
                }
            }
            if !routes.is_empty() {
                break;
            }
        }
    }
    routes.sort();
    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "routes": routes,
            "routeCount": routes.len(),
            "note": "zero-rs: file enumeration only; method/path extraction from source not yet ported",
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("routes ({}):", routes.len());
        for r in &routes {
            println!("  {r}");
        }
    }
    Ok(0)
}

fn print_help() {
    println!("zero {VERSION} (Rust port — partial)");
    println!();
    println!("Implemented (full or summary parity):");
    println!("  zero --version [--json]");
    println!("  zero tokens --json <file.0>");
    println!("  zero parse --json <file.0>      (summary schema)");
    println!("  zero targets --json");
    println!("  zero check [--json] <file.0>    (parser-only; type/borrow/effect NOT yet checked)");
    println!("  zero explain [--json] <code>    (stub; rich text not yet ported)");
    println!("  zero clean                      (removes .zero/ cache dir)");
    println!("  zero doctor [--json]            (minimal: host + cc/zig availability)");
    println!("  zero skills [list|get|path] [--json]  (stub; rich skill data not yet ported)");
    println!("  zero routes [--json] <project>  (file enumeration only; route analysis not yet ported)");
    println!();
    println!("Not yet ported (delegate to bin/zero or set ZERO_BIN):");
    println!("  zero build | run | ship | test | fmt | new | doc | graph | size | ...");
}
