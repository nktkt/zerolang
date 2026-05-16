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
        "check" | "build" | "run" | "ship" | "test" | "fmt" | "new" | "doctor"
        | "skills" | "doc" | "graph" | "size" | "mem" | "dev" | "time" | "abi"
        | "explain" | "fix" | "routes" | "clean" => {
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

fn print_help() {
    println!("zero {VERSION} (Rust port — partial)");
    println!();
    println!("Implemented:");
    println!("  zero --version [--json]");
    println!("  zero tokens --json <file.0>");
    println!("  zero parse --json <file.0>");
    println!("  zero targets --json");
    println!();
    println!("Not yet ported (delegate to bin/zero or set ZERO_BIN):");
    println!("  zero check | build | run | ship | test | fmt | doctor | ...");
}
