# Rust Port — Status & Handoff

Snapshot of the C → Rust compiler port. Pair this document with
`docs/RUST_PORT_PLAN.md` (the full 11-phase plan with §5 Test Integrity
Protocol).

## Honest scope statement

The full port described in `RUST_PORT_PLAN.md` is **~30–34 PRs / ~38,000
LOC of Rust** porting ~31,000 LOC of C plus ~6,000 LOC of Node tooling.
This is a multi-team-weeks project, not a single-session deliverable.

What landed in this session is the **load-bearing foundation** for that
work: scaffolding, the determinism + normalization machinery that makes
every later differential gate meaningful, the C-side harness changes
that let the harness swap compilers per run, and the first two compiler
stages (target manifest, diagnostics, lexer) including a real
differential test against the released C binary.

The released compiler is unchanged. `bin/zero` still runs the C
implementation built by `make -C native/zero-c`.

## Phase status

| Phase | Status | Notes |
|---|---|---|
| 0 — Scaffold + determinism + normalizer | ✅ complete | workspace builds, clippy clean, normalizer unit-tested |
| 0.5 — C-side prep (`ZERO_BIN`, dumpers) | ✅ complete (reduced scope) | `tokens --json` and `parse --json` already existed; IR dump deferred to Phase 5 when it is actually needed |
| 1 — Targets + diagnostics | 🟡 partial | manifest parser + 80-code diag table done with 12 tests; full `zero targets --json` output (capabilityFacts, toolchain, libcFacts, directBackend) deferred — see §1.1 below |
| 2 — Lexer | ✅ complete | 17 unit + 1 differential test, byte-identical token streams across 8-file corpus |
| 3 — Parser + AST | ⛔ not started | next phase |
| 4 — Checker | ⛔ not started | biggest single block (4.7k LOC) |
| 5 — IR lowering | ⛔ not started | IR dump for differential testing lands here |
| 6 — Object emitters (×5) | ⛔ not started | |
| 7 — Driver + CLI | ⛔ not started | |
| 7.5 — Dress rehearsal | ⛔ not started | |
| 8 — Switchover | ⛔ not started | |
| 9 — Cleanup | ⛔ not started | |
| 10 — Node → cargo xtask | ⛔ not started | |
| 11 — `compiler-zero/` strategy | ⛔ not started | recommended Option B per plan |

### §1.1 Phase 1 deferred follow-up

The hard gate in `RUST_PORT_PLAN.md` Phase 1 is "`zero-rs target list
--json` matches `zero target list --json` byte-for-byte". The current
zero-target only covers the **manifest data model** — TOML parse, alias
lookups, capability membership. Reaching the full output gate needs:

- Host detection (current arch/OS → which target name is "host")
- Per-target capability fact derivation (per-capability "available"
  + "source" tags including the explicit-unavailable list)
- Toolchain plan generation (C compiler choice, sysroot env name &
  status, linker flavor)
- libcFacts derivation per `(libc, libcMode)` pair
- Direct-backend status table per `(objectFormat, arch, abi)` tuple
  with status/objectEmitter/exeEmitter/reason fields

These live in `native/zero-c/src/main.c` (lines ~3500–3700ish) and
total roughly 500 LOC of dense fact-table code. Treat this as Phase
1.5 (1 PR) before starting Phase 3.

## Tests passing in this state

Run: `cd native/zero-rs && cargo test --workspace`

| Crate | Tests | Notes |
|---|---|---|
| xtask | 4 | normalizer: timing/path/UUID masking |
| zero-diag | 4 | code table, duplicates, default fallback |
| zero-target | 8 | all 10 manifest targets, alias lookup, host fields |
| zero-lexer (unit) | 17 | keywords, symbols, escapes, errors, hello.0 fixture |
| zero-lexer (differential) | 1 | spawns `bin/zero tokens --json`, compares parsed streams across 8 corpus files |
| **Total** | **34** | clippy clean, determinism contract active |

Additional checks:
- `npm run rust:check` — workspace compiles
- `npm run rust:clippy` — passes with determinism contract active
- `npm run rust:test` — full Rust test suite
- `npm run rust:determinism` — `xtask check-determinism` (Phase 0 no-op gate)

The pre-existing C test surface is unchanged: `bin/zero check
examples/hello.0` still returns `check ok`, and `bin/zero --version`
still reports `zero 0.1.1`.

## How to resume

### Immediate next steps (in order)

1. **Phase 1.5** — port the per-target derivation logic from `main.c` so
   `zero-rs target list --json` reaches byte-for-byte parity. Single
   PR, ~500 LOC Rust. Add this crate's tests to the differential test
   set in `xtask`.

2. **Phase 3 — Parser + AST**. Port `native/zero-c/src/parser.c` (1,189
   lines) and the AST types from `native/zero-c/include/zero.h`. Plan
   says 2–3 PRs. Differential gate: parsed AST JSON via `zero parse
   --json` matches across the corpus.

3. **Plan ahead: Phase 4 (checker)**. The single largest port (~4,700
   LOC). Slice along the natural seams per the plan; expect at least
   one slice to merge borrow + type inference (the plan acknowledges
   this).

### Operational rules established in this work

- **Determinism contract** is enforced in `native/zero-rs/clippy.toml`.
  Banned types: `HashMap`, `HashSet`, `Instant::now`, `SystemTime::now`,
  `env::temp_dir`, `process::id`. Use `IndexMap` / `BTreeMap` instead.
- **Normalization** lives in `native/zero-rs/xtask/src/normalize.rs`.
  Any new JSON field added to a `--json` output must be classified as
  `stable` or `masked` in the same PR.
- **Differential tests** spawn `bin/zero` (or `$ZERO_BIN`). The C
  compiler must be built before running them; `ZERO_DIFFERENTIAL_REQUIRED=1`
  fails instead of skips.
- **Test harness compiler swap**: `ZERO_BIN=path/to/binary` works
  across `conformance/run.mjs`, `scripts/test-native.sh`, every
  `scripts/*-smoke.mjs`, `snapshot-command-contracts.mjs`, `zls.mjs`,
  `agent-repair-demo.mjs`. The Rust binary at `.zero/bin/zero-rs` is
  buildable today (`make -C native/zero-rs`) but is a scaffold that
  exits with code 2 — not yet a real compiler.

### Anti-patterns to avoid (learned in this session)

- **Don't pre-add C dumper flags speculatively.** The plan called for
  `--dump-tokens-json` / `--dump-ast-json` in Phase 0.5. Recon revealed
  `tokens --json` and `parse --json` already existed as proper
  subcommands. Always grep the C surface before adding flags.
- **Don't assume EOF semantics.** The C compiler emits the EOF token
  in `tokens --json` output; I initially filtered it from Rust and
  the differential test caught the mismatch. When in doubt, observe
  the C output before writing the Rust side.
- **Don't byte-compare formatted JSON.** The C compiler uses non-
  standard JSON formatting (pretty top-level, compact per-element);
  serde_json cannot replicate it. The differential test parses both
  outputs and compares semantic structure, not bytes.

## Repository structure delta

```
native/zero-rs/
├── Cargo.toml                   # workspace manifest
├── Cargo.lock
├── Makefile                     # check / clippy / test / determinism / install-local
├── clippy.toml                  # §5.1 determinism contract
├── .gitignore
├── crates/
│   ├── zero-ast/                # stub (Phase 3)
│   ├── zero-checker/            # stub (Phase 4)
│   ├── zero-cli/                # placeholder binary
│   ├── zero-diag/               # ✅ Diag struct + diag_code table
│   ├── zero-driver/             # stub (Phase 7)
│   ├── zero-emit-coff/          # stub (Phase 6)
│   ├── zero-emit-elf/           # stub (Phase 6)
│   ├── zero-emit-macho/         # stub (Phase 6)
│   ├── zero-emit-wasm/          # stub (Phase 6)
│   ├── zero-fs/                 # stub (Phase 7)
│   ├── zero-ir/                 # stub (Phase 5)
│   ├── zero-lexer/              # ✅ full port + differential test
│   ├── zero-parser/             # stub (Phase 3)
│   └── zero-target/             # ✅ TargetInfo + manifest parse
└── xtask/                       # ✅ normalizer + check-determinism

package.json:
  + rust:{check,clippy,test,build,determinism}

scripts/* + conformance/run.mjs + scripts/test-native.sh:
  bin/zero -> $ZERO_BIN (default bin/zero, swappable)
```

## Commit log this session

```
3afb2b8 Phase 2: port lexer with differential test against C compiler
25804b0 Phase 1 (partial): port targets.manifest parser and diagnostic codes
60bd2bf Phase 0.5: ZERO_BIN env var for compiler swap in test harness
4a4a... Phase 0: scaffold Rust workspace + determinism + normalizer
8b135bd docs: add Test Integrity Protocol (§5) to Rust port plan
1f27713 docs: extend Rust port plan to full-repo scope
b357b3b docs: add Rust port plan
9f6b606 Initial commit
```

(Exact Phase 0 commit hash visible via `git log` — exact value
intentionally truncated above since this doc is committed alongside.)
