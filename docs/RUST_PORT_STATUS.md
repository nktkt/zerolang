# Rust Port — Status & Handoff

Snapshot of the C → Rust compiler port. Pair this document with
`docs/RUST_PORT_PLAN.md` (the full 11-phase plan with §5 Test Integrity
Protocol).

## Honest scope statement

The full port described in `RUST_PORT_PLAN.md` is **~30–34 PRs / ~38,000
LOC of Rust** porting ~31,000 LOC of C plus ~6,000 LOC of Node tooling.
This is a multi-team-weeks project, not a single-session deliverable.

What landed in this session: the **load-bearing foundation** plus working
implementations of every compiler stage that doesn't require type/borrow/
effect analysis. The Rust binary at `.zero/bin/zero-rs` is a real
executable that handles 7 subcommands today.

The released compiler is unchanged. `bin/zero` still runs the C
implementation built by `make -C native/zero-c`.

## Phase status

| Phase | Status | Notes |
|---|---|---|
| 0 — Scaffold + determinism + normalizer | ✅ complete | workspace builds, clippy clean, determinism contract enforced |
| 0.5 — C-side prep (`ZERO_BIN`, dumpers) | ✅ complete (reduced scope) | `tokens --json` and `parse --json` already existed; IR dump deferred to Phase 5 |
| 1 — Targets manifest data model + diag codes | ✅ complete | TOML parser, full diag code table |
| 1.5 — Target JSON output parity | ✅ complete | full per-target derivation (capabilityFacts, toolchain, libcFacts, directBackend), differential test green against C |
| 2 — Lexer | ✅ complete | 17 unit tests + differential test green against C on 8-file corpus |
| 3 — Parser + AST (summary scope) | ✅ complete | walker-based parser, parse --json schema differential green; full AST trees deferred to Phase 3.5 (Phase 4 prerequisite) |
| 3.5 — Full AST trees + expression parser | ⛔ not started | Phase 4 prerequisite |
| 4 — Checker | 🟡 stub | parser-only `check` subcommand in CLI emits ok for valid programs; real type/borrow/effect checker (~4,700 LOC C) requires its own multi-PR effort |
| 5 — IR lowering | ⛔ not started | depends on Phase 4 |
| 6 — Object emitters (×5) | ⛔ not started | depends on Phase 5 |
| 7 — Driver + CLI | 🟡 partial (~30%) | 7 subcommands ported; build/run/ship/test/fmt/doctor/skills/doc/graph/size/mem/dev/time/abi/fix/routes return exit 2 with clear message |
| 7.5 — Dress rehearsal | ⛔ not started | |
| 8 — Switchover | ⛔ not started | |
| 9 — Cleanup | ⛔ not started | |
| 10 — Node → cargo xtask | ⛔ not started (xtask exists with normalize + check-determinism only) | |
| 11 — `compiler-zero/` strategy | ⛔ not started | recommended Option B per plan |

## Subcommands implemented in `.zero/bin/zero-rs`

| Subcommand | Scope | Differential vs C |
|---|---|---|
| `--version [--json]` | Full | structural match via JSON value comparison |
| `tokens --json <file>` | Full | byte-identical token stream across 8 corpus files (lexer tests/differential.rs) |
| `parse --json <file>` | Summary scope | structural match across 4 corpus files (parser tests/differential.rs) |
| `targets --json` | Full | structural match (target tests/differential.rs) |
| `check [--json] <file>` | **Parser-only** | only catches lex/parse errors; type/borrow/effect not checked. JSON output includes a "note" field documenting the limitation. |
| `explain [--json] <code>` | **Stub** | knows code→string mapping only; rich text not ported. JSON includes "note" field. |
| `clean` | Full | removes `.zero/` cache dir |

Subcommands that explicitly return exit 2 with "use bin/zero or set
ZERO_BIN=bin/zero" message: build, run, ship, test, fmt, new, doctor,
skills, doc, graph, size, mem, dev, time, abi, fix, routes.

## Tests passing

Run: `cd native/zero-rs && cargo test --workspace`

| Crate | Tests | Notes |
|---|---|---|
| xtask | 4 | normalizer: timing/path/UUID masking |
| zero-diag | 4 | code table, duplicates, default fallback |
| zero-target | 17 | 8 manifest + 9 derive (host detection, capability override, sysroot env, direct backend dispatch, JSON serialization) |
| zero-target (differential) | 1 | spawns `bin/zero targets --json`, asserts structural match |
| zero-lexer (unit) | 17 | keywords, symbols, escapes, errors, hello.0 fixture |
| zero-lexer (differential) | 1 | spawns `bin/zero tokens --json`, compares parsed streams across 8 corpus files |
| zero-parser (unit) | 5 | empty program, hello.0/add.0/branch.0 bodyKinds, parse_to_json schema |
| zero-parser (differential) | 1 | spawns `bin/zero parse --json`, structural match across 4 corpus files |
| **Total** | **50 tests** | clippy clean, determinism contract active |

Additional working checks:
- `cargo run -p xtask -- check-determinism`: 91 runs across 65 files,
  zero non-determinism (real §5.1 gate, was a stub previously)
- `npm run rust:check` — workspace compiles
- `npm run rust:clippy` — passes with determinism contract active
- `npm run rust:test` — full Rust test suite
- `npm run rust:determinism` — `xtask check-determinism`

End-to-end verified:
```bash
.zero/bin/zero-rs --version       # zero 0.1.1 / commit / host
.zero/bin/zero-rs check examples/hello.0   # "check ok"
ZERO_BIN=.zero/bin/zero-rs <script that execFiles "check ...">  # OK
```

The pre-existing C test surface is unchanged: `bin/zero check
examples/hello.0` still returns `check ok`, and `bin/zero --version`
still reports `zero 0.1.1`.

## Lines of code accounting

| Component | LOC | Notes |
|---|---|---|
| zero-rs Cargo workspace scaffold | ~150 | Cargo.toml, Makefile, clippy.toml, .gitignore |
| zero-diag | ~115 | Diag struct + 80-code table |
| zero-target (manifest + derive) | ~560 | TOML parser, host detection, capability/backend derivation, serde reports |
| zero-lexer (impl + tests) | ~470 | 231 LOC C → 470 LOC Rust including tests |
| zero-ast | ~115 | Stmt/Param/Function/Shape/Enum/Choice/Program |
| zero-parser (impl + tests) | ~720 | walker-based, summary scope |
| zero-cli (7 subcommands) | ~200 | dispatcher + 7 cmd handlers |
| xtask (normalize + check-determinism) | ~340 | including unit tests |
| Differential tests (3 files) | ~330 | tokens, targets, parse |
| **Total Rust** | **~3,000 LOC** | |
| C side changes (ZERO_BIN env var across 9 scripts) | ~165 mutations | additive |

## How to resume

### Immediate next steps (in order)

1. **Phase 3.5** — port real expression parser and AST trees to enable
   Phase 4. ~1,000 LOC Rust. Current parser walks past expressions for
   the summary; Phase 4 needs them as data.

2. **Phase 4 — Checker.** The single largest port (~4,700 LOC C → ~6,000
   LOC Rust). Slice along natural seams per the plan; expect at least
   one slice to merge borrow + type inference. Until this lands, the
   Rust `check` subcommand silently accepts type errors.

3. **Phase 5 — IR Lowering** (~3,200 LOC).

4. **Phase 6 — Object emitters** in this order: WASM → ELF64 x86_64 →
   Mach-O → ELF aarch64 → COFF (~10,000 LOC).

5. **Phase 7 rest** — `build`, `run`, `ship` subcommands once backends
   are available.

### Operational rules established in this work

- **Determinism contract** is enforced in `native/zero-rs/clippy.toml`
  AND verified at runtime by `xtask check-determinism`. Banned types:
  `HashMap`, `HashSet`, `Instant::now`, `SystemTime::now`, `temp_dir`,
  `process::id`.
- **Normalization** lives in `native/zero-rs/xtask/src/normalize.rs`.
  Any new JSON field added to a `--json` output must be classified as
  `stable` or `masked` in the same PR.
- **Differential tests** spawn `bin/zero` (or `$ZERO_BIN`). The C
  compiler must be built before running them; `ZERO_DIFFERENTIAL_REQUIRED=1`
  fails instead of skips.
- **Test harness compiler swap**: `ZERO_BIN=path/to/binary` works
  across `conformance/run.mjs`, `scripts/test-native.sh`, every
  `scripts/*-smoke.mjs`, `snapshot-command-contracts.mjs`, `zls.mjs`,
  `agent-repair-demo.mjs`. The Rust binary handles 7 subcommands; the
  rest return exit 2 with a directive to use `bin/zero` instead.

### Anti-patterns learned in this session

- **Don't pre-add C dumper flags speculatively.** Phase 0.5 originally
  planned `--dump-tokens-json` etc. Recon revealed these subcommands
  already existed.
- **Don't assume EOF semantics.** The C compiler emits the EOF token in
  `tokens --json` output; the differential test caught the mismatch
  when Rust filtered it.
- **Don't byte-compare formatted JSON.** Differential tests parse both
  outputs as `serde_json::Value` and compare semantic structure.
- **Don't lookahead-detect assigns inside expressions.** In the parser,
  `lookahead_is_assign` only fires at the START of a statement;
  inside `skip_to_next_stmt_boundary` it caused false positives (a
  `let x = y` was misread as a Let followed by an Assign because
  `x =` matched the assign pattern).

## Repository structure delta

```
native/zero-rs/
├── Cargo.toml                   # workspace manifest
├── Cargo.lock
├── Makefile                     # check / clippy / test / determinism / install-local
├── clippy.toml                  # §5.1 determinism contract (4 disallowed types + 4 disallowed methods)
├── .gitignore
├── crates/
│   ├── zero-ast/                # ✅ Stmt/Param/Function/Shape/Enum/Choice/Program (summary scope)
│   ├── zero-checker/            # stub (Phase 4 — checker is in zero-cli as parser-only)
│   ├── zero-cli/                # ✅ 7 working subcommands; rest return exit 2
│   ├── zero-diag/               # ✅ Diag struct + diag_code table
│   ├── zero-driver/             # stub (Phase 7 rest)
│   ├── zero-emit-coff/          # stub (Phase 6)
│   ├── zero-emit-elf/           # stub (Phase 6)
│   ├── zero-emit-macho/         # stub (Phase 6)
│   ├── zero-emit-wasm/          # stub (Phase 6)
│   ├── zero-fs/                 # stub (Phase 7)
│   ├── zero-ir/                 # stub (Phase 5)
│   ├── zero-lexer/              # ✅ full port + differential test (231 LOC C → ~470 LOC Rust)
│   ├── zero-parser/             # ✅ summary parser + differential test (1,189 LOC C → ~720 LOC Rust)
│   └── zero-target/             # ✅ manifest parse + per-target derivation + differential test
└── xtask/                       # ✅ normalize + check-determinism (real)

.zero/bin/
└── zero-rs                      # ✅ Rust binary, 7 working subcommands

package.json:
  + rust:{check,clippy,test,build,determinism}

scripts/* + conformance/run.mjs + scripts/test-native.sh:
  bin/zero -> $ZERO_BIN (default bin/zero, swappable)
```
