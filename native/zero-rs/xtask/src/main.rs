//! Developer task runner for the Zero Rust workspace.
//!
//! Subcommands (Phase 0):
//!   - `check-determinism`: runs the Rust compiler twice on a fixed input set
//!     and fails on any byte diff. Currently a no-op success because the Rust
//!     compiler is a Phase 0 scaffold; will gain teeth in Phase 2.
//!   - `normalize`: stdin -> stdout normalizer using the §5.2 rules. Used by
//!     the differential harness in later phases.
//!
//! Planned subcommands (added in later phases):
//!   - `differential <corpus>` (Phase 2)
//!   - `conformance` (Phase 10.1)
//!   - `command-contracts` (Phase 10.1)
//!   - `bench` (Phase 10.3)
//!   - `fuzz` (Phase 3)
//!   - `snapshot-diff` (Phase 7)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

mod normalize;

#[derive(Parser)]
#[command(name = "xtask", about = "Zero Rust workspace task runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Runs the Rust compiler twice on a fixed input set and fails on any
    /// byte diff. Enforces the §5.1 determinism contract.
    CheckDeterminism {
        /// Path to a corpus of .0 files to compile twice. Defaults to a
        /// minimal smoke set; broader corpora arrive in later phases.
        #[arg(long)]
        corpus: Option<PathBuf>,
    },
    /// Apply §5.2 normalization rules to stdin; write to stdout.
    /// Used by the differential harness to compare outputs across
    /// implementations without machine-specific noise.
    Normalize,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::CheckDeterminism { corpus } => check_determinism(corpus),
        Cmd::Normalize => run_normalize(),
    }
}

fn check_determinism(corpus: Option<PathBuf>) -> Result<()> {
    let corpus = corpus.unwrap_or_else(|| PathBuf::from("../../examples"));
    if !corpus.exists() {
        eprintln!(
            "xtask check-determinism: corpus {} not found",
            corpus.display()
        );
        return Ok(());
    }

    let zero_rs = std::env::var("ZERO_RS").unwrap_or_else(|_| "../../.zero/bin/zero-rs".into());
    if !std::path::Path::new(&zero_rs).exists() {
        eprintln!(
            "xtask check-determinism: {} not built yet (run `make -C native/zero-rs`); skipping",
            zero_rs
        );
        return Ok(());
    }

    // Real determinism check: run the Rust binary twice against the
    // corpus for each subcommand that produces output, and fail on
    // any byte diff between the two runs.
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&corpus).context("reading corpus dir")? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("0") {
            files.push(p);
        }
    }
    files.sort();
    if files.is_empty() {
        eprintln!("xtask check-determinism: no .0 files in {}", corpus.display());
        return Ok(());
    }

    let mut checks = 0usize;
    for file in &files {
        for sub in ["tokens", "parse"] {
            let a = Command::new(&zero_rs)
                .args([sub, "--json"])
                .arg(file)
                .output()
                .context("running zero-rs first time")?;
            let b = Command::new(&zero_rs)
                .args([sub, "--json"])
                .arg(file)
                .output()
                .context("running zero-rs second time")?;
            if !a.status.success() {
                // skip files this subcommand can't handle (lex/parse errors)
                continue;
            }
            if a.stdout != b.stdout {
                anyhow::bail!(
                    "non-deterministic {sub} output for {}",
                    file.display()
                );
            }
            checks += 1;
        }
    }
    eprintln!("xtask check-determinism: ok ({checks} runs across {} files)", files.len());
    Ok(())
}

fn run_normalize() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin")?;
    let normalized = normalize::normalize_text(&input);
    print!("{}", normalized);
    Ok(())
}

