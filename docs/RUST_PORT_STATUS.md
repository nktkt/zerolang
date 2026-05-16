# Rust Port — Status & Handoff

Snapshot of the C → Rust compiler port. Pair this document with
`docs/RUST_PORT_PLAN.md` (the full 11-phase plan with §5 Test Integrity
Protocol).

## Honest scope statement

The full port described in `RUST_PORT_PLAN.md` is **~30–34 PRs / ~38,000
LOC of Rust** porting ~31,000 LOC of C plus ~6,000 LOC of Node tooling.
This is multi-team-weeks of work, not a single-session deliverable.

What landed in this session: the **load-bearing foundation** plus working
implementations of every compiler stage that doesn't require type/borrow/
effect analysis. The Rust binary at `.zero/bin/zero-rs` is a real
executable that handles 10 subcommands today.

The released compiler is unchanged. `bin/zero` still runs the C
implementation built by `make -C native/zero-c`.

## Phase status

| Phase | Status | Notes |
|---|---|---|
| 0 — Scaffold + determinism + normalizer | ✅ complete | workspace builds, clippy clean, determinism contract enforced |
| 0.5 — C-side prep (`ZERO_BIN`, dumpers) | ✅ complete (reduced scope) | `tokens --json` and `parse --json` already existed; IR dump deferred to Phase 5 |
| 1 — Targets manifest data model + diag codes | ✅ complete | TOML parser, full diag code table |
| 1.5 — Target JSON output parity | ✅ complete | full per-target derivation, differential test green |
| 2 — Lexer | ✅ complete | 17 unit + 1 differential test green |
| 3 — Parser + AST (summary scope) | ✅ complete | walker-based parser, parse --json schema differential green |
| 3.5 — Full expression parser | ✅ complete | precedence climbing, 15 unit tests covering all node types |
| 4 — Checker | 🟡 stub | parser-only `check`; Stmt extended with expression fields as scaffold; real type/borrow/effect checker still pending |
| 5 — IR lowering | ⛔ not started | depends on Phase 4 |
| 6 — Object emitters (×5) | ⛔ not started | depends on Phase 5 |
| 7 — Driver + CLI | 🟡 partial (~50%) | 10 of ~20 subcommands ported; build/run/ship/test/fmt/doc/graph/size/mem/dev/time/abi/fix return exit 2 with directive |
| 7.5 — Dress rehearsal | ⛔ not started | |
| 8 — Switchover | ⛔ not started | |
| 9 — Cleanup | ⛔ not started | |
| 10 — Node → cargo xtask | ⛔ not started (xtask exists with normalize + real check-determinism only) | |
| 11 — `compiler-zero/` strategy | ⛔ not started | recommended Option B per plan |

## Subcommands implemented in `.zero/bin/zero-rs`

| Subcommand | Scope | Differential vs C |
|---|---|---|
| `--version [--json]` | Full | structural match |
| `tokens --json <file>` | Full | byte-identical token stream across 8 corpus files |
| `parse --json <file>` | Summary | structural match across 4 corpus files |
| `targets --json` | Full | structural match (10 targets) |
| `check [--json] <file>` | Parser-only | catches lex/parse errors; type/borrow/effect NOT checked |
| `explain [--json] <code>` | Stub | knows code→string mapping only |
| `clean` | Full | removes `.zero/` cache dir |
| `doctor [--json]` | Minimal | host + cc/zig PATH; full audit pending |
| `skills [list/get/path] [--json]` | Stub shell | JSON shape only; rich data not yet ported |
| `routes [--json] <project>` | File enum | file count; method/path extraction pending |

Subcommands that return exit 2 with directive: build, run, ship, test,
fmt, new, doc, graph, size, mem, dev, time, abi, fix.

## Tests passing

Run: `cd native/zero-rs && cargo test --workspace`

| Crate | Tests | Notes |
|---|---|---|
| xtask | 4 | normalizer: timing/path/UUID masking |
| zero-diag | 4 | code table, duplicates, default fallback |
| zero-target | 17 | 8 manifest + 9 derive |
| zero-target (differential) | 1 | structural match against `bin/zero targets --json` |
| zero-lexer (unit) | 17 | keywords, symbols, escapes, errors, hello.0 fixture |
| zero-lexer (differential) | 1 | byte-identical token stream across 8 corpus files |
| zero-parser (summary unit) | 5 | empty program, hello.0/add.0/branch.0 bodyKinds |
| zero-parser (differential) | 1 | structural match against `bin/zero parse --json` (4 files) |
| zero-parser (expression unit) | 15 | precedence, member chains, calls, casts, borrows, shape literals |
| **Total** | **65 tests** | clippy clean, determinism contract active |

Additional runtime checks:
- `cargo run -p xtask -- check-determinism`: 91 runs across 65 files,
  zero non-determinism (real §5.1 gate active)
- `npm run rust:{check,clippy,test,build,determinism}`

End-to-end verified:
```bash
.zero/bin/zero-rs --version                # zero 0.1.1 / commit / host
.zero/bin/zero-rs check examples/hello.0   # check ok
.zero/bin/zero-rs parse --json examples/add.0     # full schema-correct JSON
.zero/bin/zero-rs targets --json           # 10 targets, structural match C
.zero/bin/zero-rs doctor --json            # status, host, cc/zig checks
ZERO_BIN=.zero/bin/zero-rs <script>        # drop-in for 10 subcommands
```

## Lines of code

| Component | Rust LOC |
|---|---|
| zero-rs Cargo workspace scaffold | ~150 |
| zero-diag (Diag struct + code table) | ~115 |
| zero-target (manifest + derive) | ~560 |
| zero-lexer (impl + tests) | ~470 |
| zero-ast (with extended Stmt + Expr) | ~230 |
| zero-parser summary + expression (impl + tests) | ~1,060 |
| zero-cli (10 subcommands) | ~410 |
| xtask (normalize + check-determinism) | ~340 |
| Differential tests (3 files) | ~330 |
| **Total Rust** | **~3,700 LOC** |
| C-side changes (ZERO_BIN across 9 scripts) | ~165 mutations |

## How to resume

### Immediate next steps (in order)

1. **Phase 4 (proper) — name resolution checker.** Use the extended
   `Stmt` (`name`, `expr`, `then_body`, `else_body`) by adding a
   full-AST parser entry point on top of `zero_parser::ExprParser`.
   Then add a name-resolution walker in `zero-checker` that catches
   undefined variable references. Bounded scope (~500 LOC), real
   semantic value.

2. **Phase 4 (full) — type/borrow/effect checker.** ~4,700 LOC C →
   ~6,000 LOC Rust. Slice along natural seams (type → effects →
   borrow → meta → match). Pre-acknowledge: borrow + type likely
   fold into one slice.

3. **Phase 5 — IR Lowering** (~3,200 LOC).

4. **Phase 6 — Object emitters** in this order: WASM → ELF64 x86_64 →
   Mach-O → ELF aarch64 → COFF (~10,000 LOC). Gate on run-equality
   (not byte) per §5.

5. **Phase 7 rest** — `build`, `run`, `ship` subcommands.

### Operational rules established

- **Determinism contract** in `native/zero-rs/clippy.toml` AND runtime-
  verified by `xtask check-determinism`. Banned: `HashMap`, `HashSet`,
  `Instant::now`, `SystemTime::now`, `temp_dir`, `process::id`.
- **Normalization** lives in `xtask/src/normalize.rs`. Any new JSON
  field added to a `--json` output must be classified `stable` or
  `masked` in the same PR.
- **Differential tests** spawn `bin/zero` (or `$ZERO_BIN`). Skip if
  unbuilt; `ZERO_DIFFERENTIAL_REQUIRED=1` fails instead.
- **`ZERO_BIN` swap**: works across `conformance/run.mjs`,
  `scripts/test-native.sh`, all `scripts/*-smoke.mjs`,
  `snapshot-command-contracts.mjs`, `zls.mjs`, `agent-repair-demo.mjs`.

### Anti-patterns learned this session

- **Don't pre-add C dumper flags speculatively.** Recon revealed
  `tokens --json` / `parse --json` already existed.
- **Don't assume EOF semantics.** C emits the EOF token in
  `tokens --json`; the differential test caught the mismatch when
  Rust filtered it.
- **Don't byte-compare formatted JSON.** Compare `serde_json::Value`s
  structurally.
- **Don't lookahead-detect assigns inside expressions.** In the parser,
  `let x = y` was misread as Let+Assign because `x =` matched the
  assign pattern; restrict the check to statement starts.

## Repository structure delta

```
native/zero-rs/
├── Cargo.toml                   # workspace manifest
├── Cargo.lock
├── Makefile                     # check / clippy / test / determinism / install-local
├── clippy.toml                  # §5.1 determinism contract (4 disallowed types + 4 methods)
├── .gitignore
├── crates/
│   ├── zero-ast/                # ✅ Expr/FieldInit + Stmt with body/expr fields + Program
│   ├── zero-checker/            # stub (Phase 4 — checker stub in zero-cli)
│   ├── zero-cli/                # ✅ 10 working subcommands; rest exit 2
│   ├── zero-diag/               # ✅ Diag struct + diag_code table
│   ├── zero-driver/             # stub (Phase 7 rest)
│   ├── zero-emit-coff/          # stub (Phase 6)
│   ├── zero-emit-elf/           # stub (Phase 6)
│   ├── zero-emit-macho/         # stub (Phase 6)
│   ├── zero-emit-wasm/          # stub (Phase 6)
│   ├── zero-fs/                 # stub (Phase 7)
│   ├── zero-ir/                 # stub (Phase 5)
│   ├── zero-lexer/              # ✅ full port + differential test
│   ├── zero-parser/             # ✅ summary parser + expression parser + differential test
│   └── zero-target/             # ✅ manifest parse + per-target derivation + differential test
└── xtask/                       # ✅ normalize + real check-determinism

.zero/bin/
└── zero-rs                      # ✅ Rust binary, 10 working subcommands

package.json:
  + rust:{check,clippy,test,build,determinism}

scripts/* + conformance/run.mjs + scripts/test-native.sh:
  bin/zero -> $ZERO_BIN (default bin/zero, swappable)
```

## Commits this session

```
9f09307 zero-ast: extend Stmt with expression / body fields (Phase 4 scaffold)
574ef9f Phase 7 (continued): add doctor, skills, routes subcommands
6eed952 Phase 3.5: full expression parser (parse_primary through parse_binary)
2b18a05 Phase 7 (continued): add `clean` subcommand + status update
9736404 xtask check-determinism: real implementation (was Phase 0 stub)
aff6d11 Phase 4 (parser-only stub) + Phase 7 (explain stub) in zero-cli
0693fdb Phase 7 (partial): zero-cli with real subcommands
694e92a Phase 3 (summary scope): port parser + AST for parse --json parity
25804b0 Phase 1 (partial): port targets.manifest parser and diagnostic codes
60bd2bf Phase 0.5: ZERO_BIN env var for compiler swap in test harness
(plus Phase 0 scaffold, Phase 1.5 derive, plan docs)
```
