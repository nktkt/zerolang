# Rust Port Plan

This document defines the plan to rewrite the zerolang native compiler (`native/zero-c/`, ~31,000 lines of C11) and its CLI shell (`bin/zero` + `scripts/zero-cli.mjs`) in Rust, without changing the user-facing language or the published CLI contract.

The Zero language itself, the `.0` source files, the conformance fixtures, the docs site, and the self-hosted `compiler-zero/` sources are inputs to the compiler and do **not** need to be ported.

---

## 1. Goals & Non-goals

### Goals
- Replace the C compiler with a Rust implementation that is behaviorally identical to the existing conformance and command-contract suites.
- Replace the Node.js CLI wrapper with a single Rust binary so `bin/zero` becomes a thin shell that execs one native executable.
- Replace the Node.js build/test/release orchestration scripts (`scripts/*.mjs`, `tests/*.ts`, `conformance/run.mjs`) with a Rust `cargo xtask` workflow, so the only Node remaining in the project is in `docs-site/` (Next.js) and `extensions/vscode/` (which must be TypeScript).
- Decide and execute a path for `compiler-zero/` (the Zero-authored self-hosted compiler) so its role is either preserved by the Rust compiler or formally retired — see Phase 11.
- Keep all object-file emitters direct (no LLVM dependency), matching today's architecture.
- Land the port in slices that each leave `main` green.

### Non-goals
- No language changes (syntax, semantics, stdlib, diagnostic codes, JSON shapes).
- No new targets in the port window.
- No swap to an external codegen framework (LLVM, Cranelift) — only a Rust rewrite of the existing direct emitters.
- No port of `docs-site/` (Next.js) or `extensions/vscode/` (TypeScript) — these are intentionally kept in their native runtimes.

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

### Phase 0 — Scaffolding + Determinism Contract (1 PR)
- Add `native/zero-rs/` Cargo workspace with empty crates.
- Add `make -C native/zero-rs` target and a side-by-side `.zero/bin/zero-rs` install path.
- Add `npm run rust:check` / `rust:test` scripts (`cargo check --workspace`, `cargo test --workspace`).
- Add a CI job that builds the Rust workspace; failures do not block until phase 8.
- **Land the determinism contract from §5.1**: `clippy.toml` banning `HashMap`/`HashSet`/`Instant::now()` in output paths, and `xtask check-determinism` (runs compiler twice, fails on byte diff).
- **Land the normalization layer skeleton from §5.2**: `xtask/src/normalize.rs` with timing/path masking; used by every later phase's differential test.
- **Gate**: workspace builds clean on macOS and Linux; `xtask check-determinism` passes (trivially, since no code yet); no behavior change to the existing CLI.

### Phase 0.5 — C-side preparation (1 PR, blocks Phase 2)
Lands additive instrumentation in the existing C compiler so the differential harness can actually run. **Required before any phase that does differential testing (Phase 2+).**

- Add `--dump-tokens-json`, `--dump-ast-json`, `--dump-ir` flags to `native/zero-c/src/main.c`. All default off, additive, no behavior change to existing subcommands.
- Refresh `command-contracts` snapshots affected by the new flags in the same PR (help text, flag listings). This prevents the dumper additions from polluting later diffs.
- Audit C output paths for any internal structure whose iteration order could leak to output; replace with sorted alternatives if found. (Expected to be a no-op — C uses linear arrays — but must be audited.)
- Add `ZERO_BIN` env var support to `conformance/run.mjs`, `scripts/native-test-sandbox.mjs`, and the snapshot scripts so the harness can swap compilers.
- **Gate**: existing `npm test` passes after this PR; no behavior change to the released CLI; new dumpers verified to produce identical output across two consecutive runs of the same input.

### Phase 1 — Diagnostics + Targets (1 PR)
- Port `ZDiag`, diagnostic code table, and `ZBuf`-equivalent formatting helpers.
- Port `targets.manifest` parsing and `ZTargetInfo` lookups (parity with `target.c`).
- Add unit tests that load `targets.manifest` and assert each target's fields match a fixture snapshot derived from the current C `--json` output.
- **Gate**: `zero-rs target list --json` matches `zero target list --json` byte-for-byte.

### Phase 2 — Lexer (1 PR)
- Port `lexer.c` (231 lines) to `zero-lexer`. Mirror keyword table and two-char symbol table from C verbatim.
- Add a differential test: for every `.0` file in `examples/` and `conformance/`, tokenize with both compilers (via the Phase 0.5 `--dump-tokens-json` flag on C and the native Rust output) and compare the `(kind, text, line, column)` stream via JSON, passing through the §5.2 normalizer.
- **Gate**: zero diffs across the corpus after normalization; `xtask check-determinism` passes.

### Phase 3 — Parser + AST (2–3 PRs)
- Port AST types from `zero.h` into `zero-ast`. Use `Box` for owned children; keep field names aligned with the C side to make diffs reviewable.
- Port `parser.c` (1,189 lines) to `zero-parser`.
- Differential test: re-emit AST as JSON via `--dump-ast-json` (Phase 0.5 in C, native in Rust) and compare across the corpus through the §5.2 normalizer.
- Activate the §5.5 grammar fuzzer for the parser; any divergence found goes into `conformance/fuzz-corpus/parser/`.
- **Gate**: AST JSON parity on `examples/` and `conformance/parse/`; property tests from §5.6 (total parse-or-error, diagnostic locality) pass.

### Phase 4 — Checker (4–6 PRs, biggest single block)
- Port `checker.c` (4,713 lines) in slices targeted at the natural seams: type resolution → effects/raises → borrow/ownership → meta/check expressions → match exhaustiveness.
- **Caveat**: borrow inference and type inference share state, so the slice boundaries will leak. Expect at least one PR to be "fold borrow inference into the type-resolution slice" rather than a clean addition.
- After each slice, run the new checker against `conformance/check/` and `conformance/diagnostics/` with `--json` output diffed against the C compiler.
- **Gate per slice**: zero new failures in the relevant `conformance/` subset.
- **Gate overall**: `zero-rs check --json` matches `zero check --json` for every fixture in `conformance/check/` and `conformance/diagnostics/`.

### Phase 5 — IR Lowering (2–3 PRs)
- Port the IR types (`IrProgram`, `IrFunction`, `IrInstr`, `IrValue`, ~150 enum variants) and `ir.c` (3,199 lines) to `zero-ir`.
- Introduce a debug-only IR text dumper in both compilers (small additive change to the C side, behind `--dump-ir`) so we can diff IR.
- **Gate**: IR dumps match across the corpus for every input that the checker accepts.

### Phase 6 — Object emitters (6–8 PRs total)
Order: WASM → ELF64 x86_64 → Mach-O → ELF aarch64 → COFF. WASM goes first because it has the most coverage in `npm run wasm:runtime:smoke` and `npm run docs:wasm`, and because `compiler-zero/` (Phase 11) depends on this emitter producing a working `.wasm`.

**Gate stance**: byte-equality is the goal but not the gate. ELF/Mach-O/COFF give freedom over symbol table order, relocation order, and string table layout that the C and Rust implementations are unlikely to match exactly. Gate on **run-equality** — the binary produced by the Rust compiler must pass every conformance and runtime smoke test that the C compiler's binary passes. Treat byte-equality as a nice-to-have diagnostic, not a release blocker. Update the release checksum tooling once per emitter as it lands.

For each emitter:
- Port the C emitter to Rust.
- Differential test on behavior: run both binaries against `conformance/native/pass/` and compare stdout/stderr/exit-code.
- The COFF emitter likely needs an extra PR (1,497 lines is the second-largest) and the WASM emitter likely needs two PRs (4,040 lines, plus the `compiler-zero/` integration test surface).
- **Gate per emitter**: zero behavior diffs across `examples/` and `conformance/native/pass/`; refresh release-binary checksum fixtures.

### Phase 7 — Driver + CLI (4–5 PRs)
- Port `main.c` (9,016 lines) to `zero-driver` + `zero-cli`. Split across PRs by subcommand cluster: (a) `check` / `explain`, (b) `build` / `run` (the pipeline-heavy ones), (c) `graph` / `size` / `routes` / `doctor` (introspection), (d) `fix` / `skills` / release helpers (`ShipArtifacts`).
- Port the parts of `scripts/zero-cli.mjs` (1,230 lines) that are actual CLI logic. Sandbox plumbing and smoke tests are moved out in Phase 10, not here.
- Run `npm run command-contracts:local` against the Rust binary. All ~1,800 lines of expected output must match.
- **Gate**: every `npm run *` script in `package.json` passes against `.zero/bin/zero` produced by the Rust compiler.

### Phase 7.5 — Pre-switchover dress rehearsal (1 PR, blocks Phase 8)
Before flipping `make` to default to Rust, prove the full test surface is green end-to-end with the Rust binary as `ZERO_BIN`.

1. Run `xtask test` (full suite) with `ZERO_BIN=.zero/bin/zero-rs` on macOS arm64 and Linux x64 CI.
2. Run the release pipeline end-to-end producing binaries for all 10 targets.
3. Run `xtask differential` against the union of `examples/`, all `conformance/*/fixtures/`, and the accumulated `conformance/fuzz-corpus/` (§5.5). Require zero diffs.
4. Smoke-test the docs playground with the Rust-compiled WASM (Phase 11 dependency).
5. Open a "freeze" PR that pins the C tree under `native/zero-c-legacy/` but does **not** flip the default yet — soak for ≥1 week with `ZERO_USE_LEGACY=1` allowing C re-runs side-by-side.

**Gate**: Phase 8 only proceeds if all five items pass with zero `Snapshot-Change` overrides classified as (c) "intentional semantic change" during the soak week. Any (c) classification halts the switchover until reviewed.

### Phase 8 — Switchover (1 PR)
- Flip `make` to build the Rust binary by default, archive the C tree under `native/zero-c-legacy/`, and update the Makefile invoked by `npm run native:install`.
- Update `AGENTS.md`, `README.md`, and `docs-site/articles/*` references from "the C compiler" to "the Rust compiler" (text-only updates).
- One release behind a `--legacy-backend=c` escape hatch that re-runs the C compiler from `native/zero-c-legacy/` — kept for a single point release, then deleted.
- **Gate**: full `npm test` green on macOS arm64 and Linux x64 CI; release pipeline produces working binaries for all 10 targets.

### Phase 9 — Cleanup (1 PR)
- Delete `native/zero-c-legacy/` and the `--legacy-backend` flag.
- Delete Node-side compiler shims in `scripts/zero-cli.mjs` that are no longer reachable.
- Final pass on `AGENTS.md` and `README.md`.

### Phase 10 — Node tooling → Rust `cargo xtask` (5–7 PRs)

Scope: the ~6,000 lines of Node.js orchestration that drive testing, benchmarking, release packaging, and smoke checks. After this phase, the only Node remaining in the repo is `docs-site/` (Next.js, kept) and `extensions/vscode/` (TypeScript, kept — VS Code requires it).

Add a new workspace crate `xtask/` and an `xtask` binary that exposes subcommands. `package.json` becomes thin shims (`"conformance": "cargo xtask conformance"`) and eventually goes away entirely except for `docs-site/`'s own `package.json`.

| Sub-PR | What lands | Replaces |
|---|---|---|
| 10.1 | `xtask conformance` + `xtask command-contracts` | `conformance/run.mjs`, `scripts/snapshot-command-contracts.mjs` (1,811 lines) |
| 10.2 | `xtask native-test` + sandbox runner | `scripts/native-test-sandbox.mjs` (308 lines), `scripts/native-smoke.mjs` (31 lines), `scripts/test-native.sh` |
| 10.3 | `xtask bench` | `scripts/bench.mjs` (965 lines), `scripts/bench.sh` |
| 10.4 | `xtask wasm-smoke` + `xtask playground-smoke` + `xtask browser-hardening` | `scripts/wasm-runtime-smoke.mjs` (311), `scripts/playground-wasm-smoke.mjs` (407), `scripts/browser-compiler-hardening.mjs` (283) |
| 10.5 | `xtask cli-test` (replaces `tests/zero-cli.test.ts`) | `tests/zero-cli.test.ts` + `tests/tsconfig.json` |
| 10.6 | `xtask zls` and remaining utilities | `scripts/zls.mjs` (516), `scripts/agent-repair-demo.mjs` (85), `scripts/reliability-smoke.mjs` (115), `scripts/sanitizer-smoke.sh` |
| 10.7 | Strip `package.json` to docs-site only; remove `@vercel/sandbox` and `typescript` devDependencies; update `AGENTS.md` "Useful Checks" section to `cargo xtask *` | — |

**Sandbox dependency caveat**: `scripts/native-test-sandbox.mjs` uses `@vercel/sandbox`. There is no first-party Rust SDK. Phase 10.2 must either (a) call Vercel Sandbox's HTTP API directly from Rust (preferred), or (b) keep a ~50-line Node sandbox launcher and have `xtask` exec it. Decide at the start of 10.2 based on how stable Vercel's HTTP API is.

**Gate per sub-PR**: the replaced `npm run *` script and the new `cargo xtask *` subcommand both pass on the same fixtures; the `npm run` shim continues to work during the transition so CI doesn't break.

**Gate overall**: `npm test` is deleted; `cargo xtask test` runs the full conformance, native-test, command-contracts, wasm-smoke, docs-test, zls self-test, and agent-demo suites. CI is updated to invoke `cargo xtask test`.

### Phase 11 — `compiler-zero/` strategy (variable)

`compiler-zero/src/*.0` is the self-hosted compiler: ~11 Zero source files that re-implement lexer/parser/checker/mir/wasm in Zero itself, compiled to WASM for the playground. Three viable paths; pick one before starting Phase 11.

**Option A — Retire it (1 PR, smallest)**
- Compile the Rust compiler to `wasm32-unknown-unknown` (it's a straightforward `cargo build --target` plus a `wasm-bindgen`-style JS shim).
- Replace `docs-site/public/playground/zeroc-zero.wasm` with the Rust-compiled WASM.
- Delete `compiler-zero/`, `scripts/playground-wasm-smoke.mjs`'s self-host paths, and the `selfHostDriverInputs` set in `scripts/zero-cli.mjs`.
- **Trade-off**: ends the "Zero compiles itself" demonstration. The language still *can* be self-hosted in principle but no longer is in this repo.
- **When to pick**: if the self-hosting story is not a strategic asset of the project, or if maintaining `compiler-zero/` has been a drag.

**Option B — Keep it, ensure Rust compiler builds it (already covered by Phase 6, 1 PR for the gating)**
- Add a CI gate that `cargo xtask docs-wasm` (Phase 10.7) builds `compiler-zero/` to WASM and the result passes `xtask playground-smoke`.
- Update `compiler-zero/` only when language changes require it (current cadence).
- **Trade-off**: dual maintenance of two compilers (Rust canonical + Zero self-hosted). Every language change is paid for twice.
- **When to pick**: if self-hosting is a strategic narrative for the language ("the language is real enough to compile itself") and worth ongoing cost.

**Option C — Keep it AND treat the Rust compiler as the trusted root (2 PRs)**
- Like Option B, plus: add `cargo xtask bootstrap-check` that builds Zero → Rust-compiler-produced WASM, runs the WASM compiler on the same source, and verifies the two produce identical IR/object output for `examples/`.
- Establishes Rust as the trusted compiler and `compiler-zero/` as a verified bootstrap, not just a demo.
- **Trade-off**: most work, but gives the strongest correctness story.
- **When to pick**: if self-hosting is strategic AND compiler correctness assurance is a stated goal.

**Default recommendation**: Option B. It's the lowest-risk middle path — preserves self-hosting without committing to the bootstrap-verification machinery of Option C, and doesn't require a strategic call to delete `compiler-zero/`. If the project later decides self-hosting isn't paying for itself, dropping to Option A is a single PR.

**Gate**: depends on option chosen. For Option B (recommended): `cargo xtask docs-wasm && cargo xtask playground-smoke` green in CI on every PR that touches the Rust compiler.

---

## 5. Test Integrity Protocol

This section is the runbook that makes the phase gates above actually pass. The original gates (§4) describe *what* must hold; this section describes *how* to keep them holding under the realities of cross-implementation testing (non-determinism, snapshot drift, over-specified tests, coverage gaps).

Every Phase in §4 depends on §5.1–5.6 being in place. §5.7–5.8 cover failure handling and post-switchover validation.

### 5.1 Determinism contract (Rust side, enforced from Phase 0)

The Rust implementation must produce bit-identical output for the same input across runs and across machines. Without this, differential testing is meaningless.

**Banned in Rust code that produces user-visible or test-visible output:**
- `std::collections::HashMap`, `HashSet` — randomized iteration order
- `std::time::Instant::now()`, `SystemTime::now()` outside of explicitly-masked timing fields
- `std::env::temp_dir()` paths leaked into output text
- `std::process::id()`, `std::thread::current().id()` in output
- Default `Debug` / `Display` for floats in stable output (use explicit `{:.N}` or a custom formatter)
- `rayon` parallelism in any path where output order can leak

**Required substitutes:**
- `indexmap::IndexMap` or `std::collections::BTreeMap` for any map whose iteration order can reach output
- All paths normalized to repository-relative or `<workspace>/`-prefixed before emission
- All wall-clock fields routed through the masking function in §5.2

**Enforcement mechanisms (all land in Phase 0):**
- `clippy.toml` with `disallowed-types` listing the banned types. `cargo clippy -- -D clippy::disallowed_types` is a hard CI gate.
- `xtask check-determinism`: runs the compiler twice on a fixed input set, fails on any byte diff. Wired into every phase's CI step from Phase 2 onward.
- Per-crate `#![deny(clippy::disallowed_types)]` in `zero-driver` and emitter crates.

### 5.2 Output normalization layer

The differential harness never compares raw output. Both compilers' output passes through a normalizer before diffing.

**Normalized:**
- Timing fields (any JSON key matching `*_ms`, `*_seconds`, `*_ns`): replaced with literal `"<masked>"`
- Absolute paths under `$HOME`, `$TMPDIR`, system temp: replaced with `<HOME>`, `<TMP>`
- Repository-absolute paths: relativized to repo root
- Timestamps inside object files (Mach-O `LC_SYMTAB` mtime, COFF header timestamp): zeroed before comparison
- Random tokens (sandbox request IDs, UUIDs): masked

**Where it lives:**
- `xtask/src/normalize.rs`
- Used by `xtask conformance`, `xtask differential`, `xtask snapshot-diff`, and `xtask command-contracts`
- Single source of truth — no per-test normalization rules

**Contract for adding new JSON fields**: any new field added to a `--json` output must be classified in the same PR as either `stable` (subject to snapshot) or `masked` (passes through normalizer). Reviewers reject PRs missing the classification.

### 5.3 Snapshot freeze and migration protocol

**Phase 7 entry condition**: at the start of Phase 7, the current C-produced `command-contracts` snapshots are frozen into `tests/snapshots/c-legacy/`. These are the reference truth for the port.

**During Phase 7**: Rust-produced snapshots land in `tests/snapshots/`. The diff between Rust and frozen C output is classified per PR using the `Snapshot-Change:` trailer (see §5.7).

**Phase 9 (cleanup) condition**: `tests/snapshots/c-legacy/` is deleted after one full release with zero divergence reports during the post-switchover soak (§5.8).

**Hard rule**: no silent snapshot updates. `xtask snapshot-diff` requires a classification annotation for every changed snapshot file. Commits that update snapshots without classification fail CI.

### 5.4 Test classification and per-class gate strategy

Classify every test in the repo into one of five classes; each class has a defined gate strategy. The classification lives in `tests/test-classes.toml`, landed in Phase 0.

| Class | Examples | Gate strategy |
|---|---|---|
| **Behavioral** | `conformance/native/pass/*.0` (compile, run, check stdout) | Run-equality with normalized output |
| **Snapshot** | `command-contracts`, `--json` outputs, help text | Normalized snapshot diff; updates require classification (§5.7) |
| **Property** | "every well-typed program produces an IR", "checker is deterministic" | Implementation-agnostic; same test runs against both Rust and C, must hold for both |
| **Performance** | `npm run bench` derivatives | Tolerance band (default ±15% per stage); fails only on regression beyond band |
| **Smoke** | `npm run native:smoke`, `wasm:runtime:smoke` | Boolean pass/fail; no diff |

`xtask classify-tests` emits the table from the annotation file and verifies every test target in `package.json` is classified. New tests added without classification fail CI.

### 5.5 Coverage augmentation (fuzzing)

Existing tests cover known behaviors. To catch unspecified behaviors that would silently diverge:

- `xtask fuzz`: grammar-driven generator for synthetic `.0` programs. Targets each compiler stage per phase:
  - Phase 3 fuzzes the parser (syntax)
  - Phase 4 fuzzes the checker (`cargo-fuzz` style harness for type/borrow/effect)
  - Phase 5 fuzzes IR lowering (well-typed programs to IR dump)
  - Phase 6 fuzzes runnable programs (compile to binary, run, compare stdout/exit-code)
- Every divergence found by the fuzzer is added to `conformance/fuzz-corpus/<phase>/` as a regression test with an annotation stating which implementation's output was deemed correct.
- **Not a per-PR hard gate** (fuzzing is non-deterministic). Runs nightly on `main`; blocks only on regression in the seeded corpus.
- The fuzz corpus participates in the Phase 7.5 dress-rehearsal differential.

### 5.6 Property-test invariants (implementation-agnostic)

A small set of properties must hold for any correct Zero compiler. These are tested against both Rust and C from Phase 0; if they fail on C, the property is wrong (not the compiler).

- **Determinism**: compiling the same input twice produces identical output (all classes)
- **Total parse-or-error**: every input either produces an AST or a diagnostic; never a partial AST without a diagnostic
- **Diagnostic locality**: every diagnostic has a valid `line >= 1`, `column >= 1`, and the cited offset is within the source
- **IR well-formedness**: every locally-defined IR local is set before use
- **Round-trip**: for every IR program, the IR dump → re-parse → IR dump cycle is a fixed point (lands when IR dumper is available in Phase 5)

These live in `tests/property/` and run under `xtask test` for both backends.

### 5.7 Failure triage protocol

When any gate fails during the port, the following classification is **mandatory** before merging the resolution PR. The classification goes in the PR description with a `Snapshot-Change:` trailer for snapshot failures, or a `Triage:` trailer for behavioral failures.

```
gate failure
├── output difference
│   ├── only in masked fields (timings, paths) → bug in normalizer; fix normalizer (no snapshot change)
│   ├── only in formatting (whitespace, precision)
│   │   ├── (a) Rust impl wrong → fix Rust → trailer: "Triage: rust-fix"
│   │   ├── (b) test over-specified to C → update test → trailer: "Snapshot-Change: formatting (b)" + reviewer ack
│   │   └── (c) intentional semantic change → STOP; open RFC issue; needs project-lead sign-off → trailer: "Snapshot-Change: semantic (c)"
│   └── in semantic content (different acceptance/rejection, different IR shape)
│       → ALWAYS (a) or (c); never (b) without lead sign-off
├── crash / panic
│   ├── in Rust → fix Rust; add regression test to fuzz corpus → trailer: "Triage: rust-fix + corpus-add"
│   └── in C → file issue; do not block port (legacy bug)
└── timeout / hang
    └── investigate root cause; do not bypass with longer timeouts unless C also hangs
```

**Hard rule**: `Snapshot-Change: semantic (c)` requires sign-off from a code owner listed in `.github/CODEOWNERS` for the affected stage. Bot enforcement.

### 5.8 Post-switchover soak (Phase 8 + 1 week minimum)

After Phase 8 flips the default but before Phase 9 deletes the legacy tree:

- Nightly CI job runs both binaries (Rust default, C from `native/zero-c-legacy/`) against `examples/`, `conformance/`, and `conformance/fuzz-corpus/`. Any divergence files an auto-issue with the `port-divergence` label.
- `--legacy-backend=c` escape hatch remains live the entire week; any user-reported issue auto-prioritizes.
- Repository accepts no language or compiler changes during the soak (docs and infra only).
- Phase 9 (cleanup) only proceeds after seven consecutive days with zero `port-divergence` issues and zero `--legacy-backend=c` invocations in CI logs.

---

## 6. Cross-cutting workstreams

These run in parallel with the phased rollout.

### Differential test harness
See §5. Implementation: `xtask differential` (Phase 0); modes for tokens, AST JSON, check JSON, IR dump, and object behavior; non-blocking signal in CI until Phase 8, blocking thereafter.

### Conformance corpus discipline
- No new conformance fixture lands during the port without being green on **both** compilers from the moment it's added.
- `conformance/run.mjs` and the snapshot scripts honor `ZERO_BIN` (landed in Phase 0.5) so the harness can swap compilers per run.
- §5.5 fuzz corpus (`conformance/fuzz-corpus/`) is treated as first-class conformance — must stay green across both backends from the time it's seeded.

### Release pipeline
- Existing release flow (`AGENTS.md` Releasing section) keeps shipping the C compiler until Phase 8. The version bump procedure stays untouched.
- Phase 8 release notes state the language is unchanged; the change is implementation-only.
- During the §5.8 soak week, releases are paused (no language or compiler changes; docs and infra only).

### Performance tracking
- `xtask bench` (Phase 10.3) compiles the `examples/` corpus end-to-end and records wall time per stage.
- Track Rust-vs-C per phase per §5.4 performance tolerance band (default ±15% per stage; net release-build time must not regress beyond 10%).

---

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Checker semantics drift mid-port — silent acceptance changes | Phase 4 gates on `conformance/check/` + `conformance/diagnostics/` JSON parity per slice; §5.5 fuzzing surfaces unspecified-behavior divergences |
| Object emitter byte drift breaks release checksum tests | Phase 6 gates on run-equality (binary behavior), not byte-equality; §5.2 normalizer zeroes object-file timestamps before any byte comparison; release checksum fixtures refreshed per emitter |
| `main.c` (9k lines) carries undocumented driver behavior | Phase 7 is gated on `command-contracts:local` parity (~1,800 lines) split across 4–5 subcommand-clustered PRs; §5.3 snapshot-freeze protocol prevents silent drift |
| Rust workspace inflates build time on CI | Keep deps minimal (see §3); single workspace target dir; cache `target/` in CI |
| Self-hosted `compiler-zero/` build (WASM) breaks | Phase 6 WASM emitter is gated on `npm run docs:wasm` succeeding and the `.wasm` passing `playground-wasm-smoke`; Phase 11 formalizes the long-term strategy |
| Port stalls in phase 4 (checker is the largest single chunk) | Slice along natural seams (type → effects → borrow → meta → match); pre-acknowledge borrow + type likely fold into one slice |
| Diff harness requires modifying the C compiler (JSON dumpers don't exist today) | Phase 0.5 lands `--dump-tokens-json` / `--dump-ast-json` / `--dump-ir` flags additively, plus refreshes any affected `command-contracts` snapshots in the same PR |
| Phase 10 blocked by `@vercel/sandbox` having no Rust SDK | Phase 10.2 fallback: keep a ~50 line Node sandbox launcher invoked by `xtask` if Vercel's HTTP API is unstable |
| Phase 11 choice (A/B/C) deferred indefinitely → ambiguous compiler-zero status | Force the decision before Phase 11 starts; default to Option B |
| **Non-determinism in Rust impl (HashMap, timing, paths) corrupts differential tests** | §5.1 determinism contract (clippy-banned types) + §5.2 normalization layer + `xtask check-determinism` runs on every PR |
| **Snapshots silently updated to mask real bugs** | §5.3 snapshot-freeze + §5.7 `Snapshot-Change:` trailer required with classification (a/b/c); semantic changes need lead sign-off |
| **Tests pass differential but Rust impl actually wrong (over-specified C-favoring tests)** | §5.6 property tests run against both backends; if Rust passes property tests but fails snapshot diff, classify as (b) over-specified test |
| **Behaviors not covered by tests diverge silently** | §5.5 fuzzer runs nightly from Phase 3; every divergence found is added to `conformance/fuzz-corpus/` |
| **Switchover surprises users with regressions** | Phase 7.5 dress rehearsal + §5.8 one-week soak with `--legacy-backend=c` escape hatch; Phase 9 cleanup blocked until zero divergence reports |

---

## 8. Rough size estimate

| Phase | C / Node LOC ported | New Rust LOC (est.) | PRs |
|---|---|---|---|
| 0 | — | ~1,200 (scaffold + determinism + normalizer) | 1 |
| 0.5 | C-side dumpers + harness env var | ~600 C | 1 |
| 1 | ~1,250 C | ~800 | 1 |
| 2 | 231 C | ~400 | 1 |
| 3 | 1,189 C + AST types | ~2,500 | 2–3 |
| 4 | 4,713 C | ~6,000 | 4–6 |
| 5 | 3,199 C | ~3,800 | 2–3 |
| 6 | 10,135 C (5 emitters) | ~11,000 | 6–8 |
| 7 | 9,016 C + ~1,200 mjs | ~7,000 | 4–5 |
| 7.5 | dress-rehearsal harness only | ~200 | 1 |
| 8 | — | ~200 | 1 |
| 9 | — | — | 1 |
| 10 | ~4,800 mjs + ~300 ts | ~5,000 | 5–7 |
| 11 (Option A) | — | ~500 (wasm-bindgen shim) | 1 |
| 11 (Option B, recommended) | — | ~100 (CI gate only) | 1 |
| 11 (Option C) | — | ~1,500 (bootstrap-check) | 2 |

**Total ported C**: ~31,000 LOC.
**Total ported Node/TS**: ~6,000 mjs + ~300 ts.
**Total expected Rust**: ~38,000–39,000 LOC depending on Phase 11 option.

**PR count**: ~30–34 total. Calendar estimate is intentionally omitted — depends on review bandwidth and how much surprise lives in `checker.c` and `main.c`.

Phase 0 grew (~300 → ~1,200 LOC) because it now lands the determinism contract, normalizer skeleton, and `xtask check-determinism` from §5.1–5.2 — these are the foundation that makes every subsequent gate actually meaningful. Phase 0.5 is new and is the precondition for any differential testing (Phase 2+). Phase 7.5 is new and is the precondition for Phase 8.

---

## 9. Decision points to confirm before starting

1. **Single binary vs. multiple**: this plan assumes one `zero` binary for all subcommands. Alternative: split `zero-check`, `zero-build`, etc. — but the current C compiler is one binary and the CLI contract follows, so single binary is the default.
2. **Arena vs. `Box` for AST**: this plan defaults to `Box`. Switch to an arena (`bumpalo` or hand-rolled) only if profiling phase 3 shows it matters.
3. **Differential harness blocking-ness**: this plan keeps it non-blocking until phase 8. If reviewers want it blocking earlier, phase 0 needs to be tightened.
4. **Phase 11 option (A/B/C)**: §4 recommends Option B (keep `compiler-zero/`, ensure Rust compiler builds it). Confirm before Phase 11 starts.
5. **Sandbox dependency in Phase 10.2**: pick between calling Vercel Sandbox's HTTP API from Rust or keeping a thin Node launcher. Decision needed at start of Phase 10.2.
6. **`cargo xtask` vs. `Makefile` for top-level orchestration in Phase 10**: this plan assumes `xtask`. Alternative is a Rust binary called from a top-level `Makefile`. `xtask` is more idiomatic in the Rust ecosystem; `Makefile` is more familiar to existing contributors. Pick at start of Phase 10.
