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
        "build" => cmd_build(rest),
        "run" | "ship" | "test" | "fmt" | "new"
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
    let mut all_diags: Vec<Diag> = Vec::new();
    if ldiag.code != 0 {
        all_diags.push(ldiag);
    } else {
        let mut pdiag = Diag::default();
        let program = zero_parser::parse_full_program(&tokens, &mut pdiag);
        if pdiag.code != 0 {
            all_diags.push(pdiag);
        } else {
            // Phase 4 (partial): name resolution. Catches undefined
            // identifier references. Type / borrow / effect / generic /
            // interface dispatch / match exhaustiveness checks are not
            // yet ported.
            all_diags.extend(zero_checker::resolve_names(&program));
        }
    }
    let ok = all_diags.is_empty();
    if want_json(args) {
        let diagnostics: Vec<serde_json::Value> = all_diags
            .iter()
            .map(|d| {
                serde_json::json!({
                    "code": zero_diag::diag_code(d.code),
                    "message": d.message,
                    "line": d.line,
                    "column": d.column,
                    "length": d.length,
                    "path": path,
                })
            })
            .collect();
        let value = serde_json::json!({
            "schemaVersion": 1,
            "ok": ok,
            "sourceFile": path,
            "diagnostics": diagnostics,
            "note": "zero-rs: lexer + parser + name-resolution checks; type/borrow/effect/match-exhaustiveness not yet ported",
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else if ok {
        println!("check ok");
    } else {
        for d in &all_diags {
            eprintln!(
                "{}:{}:{} {}: {}",
                path,
                d.line,
                d.column,
                zero_diag::diag_code(d.code),
                d.message
            );
        }
        return Ok(1);
    }
    if ok { Ok(0) } else { Ok(1) }
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

fn positional_after_flags<'a>(
    args: &'a [String],
    flags_with_values: &[&str],
) -> Option<&'a str> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a.starts_with("--") {
            if flags_with_values.contains(&a.as_str()) {
                iter.next();
            }
            continue;
        }
        return Some(a);
    }
    None
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == flag {
            return iter.next().map(String::as_str);
        }
        if let Some(rest) = a.strip_prefix(&format!("{flag}=")) {
            return Some(rest);
        }
    }
    None
}

fn cmd_build(args: &[String]) -> Result<u8> {
    // Currently supports: --emit wasm only. The C compiler also
    // supports --emit exe and --emit obj for various target tuples;
    // those depend on Phase 6 emitters for ELF/Mach-O/COFF that are
    // not yet ported.
    let emit = flag_value(args, "--emit").unwrap_or("wasm");
    if emit != "wasm" {
        eprintln!(
            "zero (Rust port) build: --emit '{}' not yet supported (only wasm). Use bin/zero or set ZERO_BIN=bin/zero",
            emit
        );
        return Ok(2);
    }

    let Some(input_path_str) = positional_after_flags(args, &["--emit", "--out", "--target", "--profile", "--release", "--cc", "--backend"]) else {
        eprintln!("Usage: zero build [--emit wasm] [--out <file>] <file.0|project-dir>");
        return Ok(1);
    };
    let out_base = flag_value(args, "--out").unwrap_or("a.out");

    let (source, source_files) = collect_sources(input_path_str)?;
    if source_files.is_empty() {
        eprintln!("zero build: no .0 source files found under {}", input_path_str);
        return Ok(1);
    }

    // Pipeline: lex -> full parse -> name resolution -> wasm emit.
    let mut ldiag = Diag::default();
    let tokens = zero_lexer::tokenize(&source, &mut ldiag);
    if ldiag.code != 0 {
        report_diag_and_exit(input_path_str, &ldiag, args)?;
        return Ok(1);
    }
    let mut pdiag = Diag::default();
    let program = zero_parser::parse_full_program(&tokens, &mut pdiag);
    if pdiag.code != 0 {
        report_diag_and_exit(input_path_str, &pdiag, args)?;
        return Ok(1);
    }
    let nm_diags = zero_checker::resolve_names(&program);
    if !nm_diags.is_empty() {
        for d in &nm_diags {
            report_diag_and_exit(input_path_str, d, args)?;
        }
        return Ok(1);
    }
    let bytes = match zero_emit_wasm::emit_program(&program) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{input_path_str}: codegen error: {e}");
            return Ok(1);
        }
    };

    // Append `.wasm` if --out doesn't already specify it.
    let out_path = if out_base.ends_with(".wasm") {
        out_base.to_string()
    } else {
        format!("{out_base}.wasm")
    };
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out_path, &bytes)?;

    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "ok": true,
            "input": input_path_str,
            "sourceFiles": source_files,
            "emit": "wasm",
            "outPath": out_path,
            "bytes": bytes.len(),
            "functionsEmitted": program.functions.len(),
            "note": "zero-rs: parser + name resolution + WASM emit. Single file or recursive .0 collection from a directory."
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "built {} ({} bytes) from {} source file{}",
            out_path,
            bytes.len(),
            source_files.len(),
            if source_files.len() == 1 { "" } else { "s" }
        );
    }
    Ok(0)
}

/// Read source(s) at `input_path`. For a file path, returns the file's
/// contents and a single-entry path list. For a directory, walks the
/// tree recursively, picks all `.0` files in sorted-by-path order, and
/// returns their concatenated contents.
///
/// The sort makes the output of `zero build <project-dir>` deterministic
/// (matching the §5.1 determinism contract — no `HashMap` iteration
/// order leaking into emission).
fn collect_sources(input_path: &str) -> Result<(String, Vec<String>)> {
    let p = std::path::Path::new(input_path);
    if p.is_file() {
        let text = std::fs::read_to_string(p)?;
        return Ok((text, vec![input_path.to_string()]));
    }
    if !p.is_dir() {
        return Err(anyhow::anyhow!(
            "input '{}' is neither a file nor a directory",
            input_path
        ));
    }
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    walk_zero_files(p, &mut files)?;
    files.sort();
    let mut combined = String::new();
    let mut paths: Vec<String> = Vec::with_capacity(files.len());
    for f in &files {
        let text = std::fs::read_to_string(f)?;
        // Separator marker so the parser doesn't accidentally fuse a
        // function/decl across file boundaries.
        combined.push('\n');
        combined.push_str(&text);
        combined.push('\n');
        paths.push(f.display().to_string());
    }
    Ok((combined, paths))
}

fn walk_zero_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            // Skip cache dirs to keep `build .` reasonable.
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | ".zero" | "target" | ".git") {
                continue;
            }
            walk_zero_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("0") {
            out.push(path);
        }
    }
    Ok(())
}

fn report_diag_and_exit(path: &str, d: &Diag, args: &[String]) -> Result<()> {
    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "ok": false,
            "sourceFile": path,
            "diagnostic": {
                "code": zero_diag::diag_code(d.code),
                "message": d.message,
                "line": d.line,
                "column": d.column,
                "length": d.length,
            },
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        eprintln!(
            "{}:{}:{} {}: {}",
            path,
            d.line,
            d.column,
            zero_diag::diag_code(d.code),
            d.message
        );
    }
    Ok(())
}

fn cmd_routes(args: &[String]) -> Result<u8> {
    let Some(path) = positional_after_flags(args, &["--target", "--out"]) else {
        eprintln!("Usage: zero routes [--json] <project>");
        return Ok(1);
    };

    // Pick the routes source directory: prefer <project>/src/routes,
    // then <project>/routes, then the project root itself.
    let project_root = std::path::Path::new(path);
    let routes_dir = ["src/routes", "routes", ""]
        .iter()
        .map(|sub| {
            if sub.is_empty() {
                project_root.to_path_buf()
            } else {
                project_root.join(sub)
            }
        })
        .find(|p| p.is_dir())
        .unwrap_or_else(|| project_root.to_path_buf());

    // Walk the directory, collecting .0 files in a sorted-by-path order
    // (determinism per §5.1). For each file, parse with the full
    // parser and treat every `pub fun` as a route.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    walk_zero_files(&routes_dir, &mut files)?;
    files.sort();

    struct Route {
        file: String,
        function: String,
        line: u32,
        column: u32,
        param_count: usize,
        return_type: String,
        raises: bool,
    }

    impl Route {
        fn to_json(&self) -> serde_json::Value {
            serde_json::json!({
                "file": self.file,
                "function": self.function,
                "line": self.line,
                "column": self.column,
                "paramCount": self.param_count,
                "returnType": self.return_type,
                "raises": self.raises,
            })
        }
    }

    let mut routes: Vec<Route> = Vec::new();
    let mut parse_errors: Vec<(String, Diag)> = Vec::new();
    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        let mut ldiag = Diag::default();
        let tokens = zero_lexer::tokenize(&source, &mut ldiag);
        if ldiag.code != 0 {
            parse_errors.push((file.display().to_string(), ldiag));
            continue;
        }
        let mut pdiag = Diag::default();
        let program = zero_parser::parse_full_program(&tokens, &mut pdiag);
        if pdiag.code != 0 {
            parse_errors.push((file.display().to_string(), pdiag));
            continue;
        }
        for f in &program.functions {
            if !f.is_public {
                continue;
            }
            routes.push(Route {
                file: file.display().to_string(),
                function: f.name.clone(),
                line: f.line,
                column: f.column,
                param_count: f.params.len(),
                return_type: f.return_type.clone(),
                raises: f.raises,
            });
        }
    }

    if want_json(args) {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "project": path,
            "routesDir": routes_dir.display().to_string(),
            "routeCount": routes.len(),
            "routes": routes.iter().map(Route::to_json).collect::<Vec<_>>(),
            "parseErrors": parse_errors.iter().map(|(p, d)| serde_json::json!({
                "file": p,
                "line": d.line,
                "column": d.column,
                "message": d.message,
            })).collect::<Vec<_>>(),
            "note": "zero-rs: enumerates `pub fun` decls as routes; method/path attribute extraction not yet ported (requires #[route(...)] attribute parsing)."
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "{} route{} in {}:",
            routes.len(),
            if routes.len() == 1 { "" } else { "s" },
            routes_dir.display()
        );
        for r in &routes {
            println!(
                "  pub fun {}({} param{}) -> {}{}  [{}:{}:{}]",
                r.function,
                r.param_count,
                if r.param_count == 1 { "" } else { "s" },
                r.return_type,
                if r.raises { " raises" } else { "" },
                r.file,
                r.line,
                r.column
            );
        }
        if !parse_errors.is_empty() {
            eprintln!("(parse errors in {} file(s); use --json for details)", parse_errors.len());
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
    println!("  zero build [--emit wasm] [--out <file>] [--json] <file.0>");
    println!("                                  (lex -> parse -> name-resolve -> WASM emit; i32+let+call only)");
    println!();
    println!("Not yet ported (delegate to bin/zero or set ZERO_BIN):");
    println!("  zero build --emit exe|obj | run | ship | test | fmt | new | doc | graph | size | ...");
}
