# Rust Port Plan

This document defines the plan to rewrite the zerolang native compiler (`native/zero-c/`, ~31,000 lines of C11) and its CLI shell (`bin/zero` + `scripts/zero-cli.mjs`) in Rust, without changing the user-facing language or the published CLI contract.

The Zero language itself, the `.0` source files, the conformance fixtures, the docs site, and the self-hosted `compiler-zero/` sources are inputs to the compiler and do **not** need to be ported.

---

## 1. Goals & Non-goals

### Goals
- Replace the C compiler with a Rust implementation that produces byte-for-byte (or behaviorally identical) output for the existing conformance and command-contract suites.
- Replace the Node.js CLI wrapper with a single Rust binary so `bin/zero` becomes a thin shell that execs one native executable.
- Keep all object-file emitters direct (no LLVM dependency), matching today's architecture.
- Land the port in slices that each leave `main` green.

### Non-goals
- No language changes (syntax, semantics, stdlib, diagnostic codes, JSON shapes).
- No new targets in the port window.
- No swap to an external codegen framework (LLVM, Cranelift) — only a Rust rewrite of the existing direct emitters.
- No change to the existing `compiler-zero/` self-hosted sources; they continue to be compiled by the new Rust compiler.

---

## 2. Constraints the port must satisfy

These are extracted from the current C compiler and CLI surface and must be preserved.

- **CLI contract**: `zero check | run | build | graph | size | routes | doctor | explain | fix | skills`, plus all flags exercised by `scripts/snapshot-command-contracts.mjs` (~1,800 lines of expected output).
- **Diagnostic codes**: `ERR001`, `APP001`, `BLD002`, `3024`, etc. — the integer code, code text, message format, expected/actual fields, and line/column reporting in `ZDiag` must round-trip.
- **JSON shapes**: every `--json` mode in the CLI (`check --json`, `build --json`, `graph --json`, `size --json`, `routes --json`, `doctor --json`, `fix --plan --json`) must produce identical schemas.
- **Targets manifest**: `native/zero-c/targets/targets.manifest` (10 targets) and the `ZTargetInfo` capability tags must be preserved verbatim.
- **Direct backends**: ELF64 x86_64, ELF aarch64, Mach-O x86_64, COFF x86_64, WASM. Each currently produces stable bytes that release tests checksum.
- **`compiler-zero/` build path**: the Rust compiler must be able to build the existing Zero-authored `compiler-zero/src/*.0` to WASM (`npm run docs:wasm` flow).
- **Performance**: should not regress meaningfully. Today's C `make` build and per-file compile times set the bar — Rust release builds should match or beat them.

---

## 3. Architecture target (Cargo workspace)

```
native/zero-rs/
  Cargo.toml                      # workspace
  crates/
    zero-diag/                    # ZDiag equivalent + diagnostic codes
    zero-lexer/                   # tokenizer (mirrors lexer.c)
    zero-parser/                  # AST + parser (mirrors parser.c + AST types in zero.h)
    zero-ast/                     # shared AST types (Expr/Stmt/Function/Shape/...)
    zero-checker/                 # type, borrow, effect checker (mirrors checker.c)
    zero-ir/                      # IR + lowering (mirrors ir.c, zero.h IR types)
    zero-target/                  # targets.manifest parsing, capabilities, toolchain plan
    zero-emit-elf/                # ELF64 x86_64 + ELF aarch64 emitter
    zero-emit-macho/              # Mach-O x86_64 emitter
    zero-emit-coff/               # COFF x86_64 emitter
    zero-emit-wasm/               # WebAssembly emitter
    zero-fs/                      # source resolution, manifest parsing, package layout
    zero-driver/                  # orchestrates the pipeline; equivalent to main.c logic split out
    zero-cli/                     # binary crate: argv parsing, subcommands, output formatting
```

`bin/zero` becomes:

```sh
#!/usr/bin/env bash
exec "$(dirname "$0")/../.zero/bin/zero" "$@"
```

The Rust binary lands at the same path the current Makefile installs to (`.zero/bin/zero`), so downstream scripts and CI keep working without changes.

### Crate dependencies (intentional minimum)
- `serde` + `serde_json` for JSON output and manifest parsing.
- `toml` for `targets.manifest` and `zero.json`.
- `clap` (derive) for argv parsing in `zero-cli`.
- `anyhow` + `thiserror` for error plumbing inside the driver.
- No regex, no codegen framework, no link to libc beyond what `std` brings.

### Memory model translation
- C `Vec` patterns (`ExprVec`, `StmtVec`, etc.) become Rust `Vec<T>`.
- C-string ownership (`char *name`) becomes `String` for owned, `&str` for borrowed; lifetimes ride on the arenas where helpful.
- AST nodes that the C code holds via raw pointers (`Expr *left`) become `Box<Expr>` or arena indices. Default to `Box<Expr>` for simplicity; switch a node to arena indices only if the checker turns out to need shared references.

---

## 4. Phased rollout

Each phase ends with a green `main` and a measurable acceptance gate. Until phase 8 ships, the C compiler stays in the tree and remains the released binary.

### Phase 0 — Scaffolding (1 PR)
- Add `native/zero-rs/` Cargo workspace with empty crates.
- Add `make -C native/zero-rs` target and a side-by-side `.zero/bin/zero-rs` install path.
- Add `npm run rust:check` / `rust:test` scripts (`cargo check --workspace`, `cargo test --workspace`).
- Add a CI job that builds the Rust workspace; failures do not block until phase 8.
- **Gate**: workspace builds clean on macOS and Linux; no behavior change to the existing CLI.

### Phase 1 — Diagnostics + Targets (1 PR)
- Port `ZDiag`, diagnostic code table, and `ZBuf`-equivalent formatting helpers.
- Port `targets.manifest` parsing and `ZTargetInfo` lookups (parity with `target.c`).
- Add unit tests that load `targets.manifest` and assert each target's fields match a fixture snapshot derived from the current C `--json` output.
- **Gate**: `zero-rs target list --json` matches `zero target list --json` byte-for-byte.

### Phase 2 — Lexer (1 PR)
- Port `lexer.c` (231 lines) to `zero-lexer`. Mirror keyword table and two-char symbol table from C verbatim.
- Add a differential test: for every `.0` file in `examples/` and `conformance/`, tokenize with both compilers and compare the `(kind, text, line, column)` stream via JSON.
- **Gate**: zero diffs across the corpus.

### Phase 3 — Parser + AST (2–3 PRs)
- Port AST types from `zero.h` into `zero-ast`. Use `Box` for owned children; keep field names aligned with the C side to make diffs reviewable.
- Port `parser.c` (1,189 lines) to `zero-parser`.
- Differential test: re-emit AST as JSON (a fresh small writer in both compilers) and compare across the corpus.
- **Gate**: AST JSON parity on `examples/` and `conformance/parse/`.

### Phase 4 — Checker (3–5 PRs, biggest single block)
- Port `checker.c` (4,713 lines) in slices: type resolution → effects/raises → borrow/ownership → meta/check expressions → match exhaustiveness.
- After each slice, run the new checker against `conformance/check/` and `conformance/diagnostics/` with `--json` output diffed against the C compiler.
- **Gate per slice**: zero new failures in the relevant `conformance/` subset.
- **Gate overall**: `zero-rs check --json` matches `zero check --json` for every fixture in `conformance/check/` and `conformance/diagnostics/`.

### Phase 5 — IR Lowering (2–3 PRs)
- Port the IR types (`IrProgram`, `IrFunction`, `IrInstr`, `IrValue`, ~150 enum variants) and `ir.c` (3,199 lines) to `zero-ir`.
- Introduce a debug-only IR text dumper in both compilers (small additive change to the C side, behind `--dump-ir`) so we can diff IR.
- **Gate**: IR dumps match across the corpus for every input that the checker accepts.

### Phase 6 — Object emitters (4 PRs, one per format)
Order: WASM → ELF64 x86_64 → Mach-O → ELF aarch64 → COFF. WASM goes first because it has the most coverage in `npm run wasm:runtime:smoke` and `npm run docs:wasm`.

For each emitter:
- Port the C emitter to Rust.
- Differential test: emit object bytes from both compilers, compare with a byte-level diff. Use the existing release checksum tooling as ground truth.
- If a byte difference is intentional (e.g. timestamps), gate on equivalent semantics (the produced binary runs and matches the C-produced binary's behavior under `npm run conformance` and `npm run native:test`).
- **Gate per emitter**: byte-identical or run-identical output across `examples/` and the runnable fixtures in `conformance/native/pass/`.

### Phase 7 — Driver + CLI (2 PRs)
- Port `main.c` (9,016 lines) to `zero-driver` + `zero-cli`. Split into: argv parsing, command dispatch, pipeline orchestration, JSON output, ship/release helpers.
- Port the parts of `scripts/zero-cli.mjs` (1,230 lines) that are actual CLI logic; everything else (sandbox plumbing, smoke tests) stays in Node for now.
- Run `npm run command-contracts:local` against the Rust binary. All ~1,800 lines of expected output must match.
- **Gate**: every `npm run *` script in `package.json` passes against `.zero/bin/zero` produced by the Rust compiler.

### Phase 8 — Switchover (1 PR)
- Flip `make` to build the Rust binary by default, archive the C tree under `native/zero-c-legacy/`, and update the Makefile invoked by `npm run native:install`.
- Update `AGENTS.md`, `README.md`, and `docs-site/articles/*` references from "the C compiler" to "the Rust compiler" (text-only updates).
- One release behind a `--legacy-backend=c` escape hatch that re-runs the C compiler from `native/zero-c-legacy/` — kept for a single point release, then deleted.
- **Gate**: full `npm test` green on macOS arm64 and Linux x64 CI; release pipeline produces working binaries for all 10 targets.

### Phase 9 — Cleanup (1 PR)
- Delete `native/zero-c-legacy/` and the `--legacy-backend` flag.
- Delete Node-side compiler shims in `scripts/zero-cli.mjs` that are no longer reachable.
- Final pass on `AGENTS.md` and `README.md`.

---

## 5. Cross-cutting workstreams

These run in parallel with the phased rollout.

### Differential test harness (lands in phase 0)
- Add `scripts/differential.mjs` that runs both `zero` (C) and `zero-rs` against the same input and diffs output.
- Modes: tokens, AST JSON, check JSON, IR dump, object bytes.
- Wired into CI as a non-blocking signal until phase 8.

### Conformance corpus discipline
- No new conformance fixtures land during the port without being green on **both** compilers from the moment they're added.
- The conformance harness `conformance/run.mjs` learns a `ZERO_BIN` env var (currently hardcodes `bin/zero`) so the differential harness can swap compilers.

### Release pipeline
- The existing release flow (`AGENTS.md` Releasing section) keeps shipping the C compiler until phase 8. The version bump procedure stays untouched.
- Phase 8's release notes call out the language is unchanged; the change is implementation-only.

### Performance tracking
- Add a `npm run bench:compiler` mode that compiles the `examples/` corpus end-to-end and records wall time per stage.
- Track Rust-vs-C per phase. Acceptable: ≤10% regression in any single stage; net release-build time must not regress.

---

## 6. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Checker semantics drift mid-port — silent acceptance changes | Phase 4 gates on `conformance/check/` + `conformance/diagnostics/` JSON parity per slice |
| Object emitter byte drift breaks release checksum tests | Phase 6 emits both byte-equality and run-equality fallbacks; switch to run-equality only with explicit per-target justification |
| `main.c` (9k lines) carries undocumented driver behavior | Phase 7 is gated on `command-contracts:local` parity (~1,800 lines of expected output already captures it) |
| Rust workspace inflates build time on CI | Keep deps minimal (see §3); use a single workspace target dir; cache `target/` in CI |
| Self-hosted `compiler-zero/` build (WASM) breaks | Phase 6 WASM emitter is gated specifically on `npm run docs:wasm` succeeding and the resulting `.wasm` passing `npm run playground-wasm-smoke` |
| Port stalls in phase 4 (checker is the largest single chunk) | Slicing the checker into 5 PRs along the natural seams (type → effects → borrow → meta → match) keeps each PR reviewable |

---

## 7. Rough size estimate

| Phase | C LOC ported | New Rust LOC (est.) | PRs |
|---|---|---|---|
| 0 | — | ~300 | 1 |
| 1 | ~1,250 (`target.c`, `zero.h` diag types, `ZBuf`) | ~800 | 1 |
| 2 | 231 | ~400 | 1 |
| 3 | 1,189 + AST types | ~2,500 | 2–3 |
| 4 | 4,713 | ~6,000 | 3–5 |
| 5 | 3,199 | ~3,800 | 2–3 |
| 6 | 10,135 (5 emitters) | ~11,000 | 4 |
| 7 | 9,016 + ~1,200 mjs | ~7,000 | 2 |
| 8 | — | ~200 | 1 |
| 9 | — | — | 1 |

Total ported C: ~31,000 LOC. Total expected Rust: ~32,000 LOC (Rust tends to be slightly more verbose than terse C, offset by less manual memory plumbing).

PR count: ~18. Calendar estimate is intentionally omitted — depends entirely on review bandwidth and how much surprise lives in `checker.c` and `main.c`.

---

## 8. Decision points to confirm before starting

1. **Single binary vs. multiple**: this plan assumes one `zero` binary for all subcommands. Alternative: split `zero-check`, `zero-build`, etc. — but the current C compiler is one binary and the CLI contract follows, so single binary is the default.
2. **Arena vs. `Box` for AST**: this plan defaults to `Box`. Switch to an arena (`bumpalo` or hand-rolled) only if profiling phase 3 shows it matters.
3. **Differential harness blocking-ness**: this plan keeps it non-blocking until phase 8. If reviewers want it blocking earlier, phase 0 needs to be tightened.
4. **`compiler-zero/` future**: out of scope for this port, but worth flagging that the self-hosted compiler will eventually want a parallel rewrite path. Not addressed here.
