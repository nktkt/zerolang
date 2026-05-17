# Changelog

## 1.0.0

<!-- release:start -->

### Compiler & language

- No language-surface changes. `bin/zero` (the C-built native compiler) remains the released binary; existing programs and target list are unchanged.

### Rust port — first published milestone

The Rust port lives at `native/zero-rs/` as a Cargo workspace and is shipped alongside the C compiler. Status snapshot at v1.0:

- 11 working subcommands in `.zero/bin/zero-rs`: `--version`, `tokens --json`, `parse --json`, `targets --json`, `check`, `explain`, `clean`, `doctor`, `skills`, `routes`, `build --emit wasm`.
- End-to-end pipeline: lex → full parse → name resolution → WASM emission. `zero-rs build --emit wasm <file|dir>` compiles single files or recursively-collected project directories to valid `.wasm`.
- WebAssembly backend implemented from the public W3C WebAssembly Core Specification: `i32` arithmetic and comparisons, `let` bindings, multi-function modules with cross-function calls, `if/else/while` control flow, `break/continue` with correct WASM label-depth resolution.
- Output validated end-to-end by `wasmparser` (structural) and `wasmtime` (execution): 12 integration tests run emitted modules and assert computed values.
- 108 tests total across the workspace, clippy clean, deterministic output gated by `xtask check-determinism`.
- §5.1 determinism contract enforced via `clippy.toml`: `HashMap`/`HashSet`/`Instant::now`/`SystemTime::now`/`env::temp_dir`/`process::id` are CI-banned in output paths; `IndexMap`/`BTreeMap` are the required substitutes.
- Differential tests against `bin/zero` verify byte-equivalent token streams (`tokens --json`, 8-file corpus) and structurally-equivalent JSON outputs for `parse --json` (4-file corpus) and `targets --json` (10 targets).

### Test harness

- Every test harness script that execs the compiler (`conformance/run.mjs`, `scripts/test-native.sh`, all `scripts/*-smoke.mjs`, `snapshot-command-contracts.mjs`, `zls.mjs`, `agent-repair-demo.mjs`) now honors `ZERO_BIN`, so the Rust binary can be swapped in for any subcommand it supports.

### CI

- `.github/workflows/ci.yml` runs five new Rust steps on every PR: toolchain install, `~/.cargo` + `native/zero-rs/target` cache, `cargo check`, `cargo clippy` (with the determinism contract), `cargo test` with `ZERO_DIFFERENTIAL_REQUIRED=1`, and `xtask check-determinism`.

### Docs

- `docs/RUST_PORT_PLAN.md`: full 11-phase plan with §5 Test Integrity Protocol (determinism contract, normalization layer, snapshot freeze protocol, failure triage flowchart, post-switchover soak).
- `docs/RUST_PORT_STATUS.md`: current state snapshot — phase status, subcommand surface, test counts, LOC accounting, anti-patterns observed, resume-from-here instructions.

### Contributors

- @nktkt

<!-- release:end -->

## 0.1.1

- Adds the public installer at `https://zerolang.ai/install.sh`, with platform selection, GitHub release downloads, checksum verification, and `$HOME/.zero/bin/zero` installation.
- Adds `zero run` for the everyday edit loop: build a host executable, run it, pass program arguments after `--`, forward stdout/stderr, and return the program exit status.
- Updates README, homepage, getting started, install, and CLI docs around the curl install path, copyable commands, and `zero run`.
- Reworks public docs to be more scannable and current, including stronger language, diagnostics, testing, target, package, optimization, and standard library references.
- Removes placeholder module docs that described surfaces not ready for users and adds current module docs for `std.crypto`, `std.http`, and `std.net`.
- Adds version-matched agent guidance through `zero skills`, including focused workflows for Zero syntax, diagnostics, builds, packages, standard library use, testing, and agent edit loops.
- Keeps the installable Zero skill as a thin bootstrap so external skill managers discover one Zero skill while the compiler serves the richer guidance for the installed version.
- Updates the `zero skills` CLI contract to serve bundled flat skill data while preserving list, get, path, and JSON workflows.

### Contributors

- @ctate
- @mvanhorn

## 0.1.0

- Initial public release of Zero as the programming language for agents.
- Includes the native compiler, examples, documentation site, and validation fixtures.
- Supported workflows use direct Zero emitters for the documented examples and targets.
