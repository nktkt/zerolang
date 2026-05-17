# Rust Port — Status & Handoff

Snapshot of the C → Rust compiler port. Pair this document with
`docs/RUST_PORT_PLAN.md` (the full 11-phase plan with §5 Test
Integrity Protocol).

## Honest scope statement

The full port described in `RUST_PORT_PLAN.md` is **~30–34 PRs / ~38,000
LOC of Rust** porting ~31,000 LOC of C plus ~6,000 LOC of Node tooling.
This is multi-team-weeks of work, not a single-session deliverable.

What landed across this work: the **load-bearing foundation** plus
working implementations of every compiler stage that doesn't require
type/borrow/effect analysis, plus a real WebAssembly backend (from
W3C spec) that compiles a useful subset of Zero programs to valid
`.wasm` modules end-to-end. The Rust binary at `.zero/bin/zero-rs`
handles 11 subcommands today and can build project directories.

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
| 4 — Checker | 🟡 partial real | parser + name resolution catches undefined refs; type/borrow/effect/match-exhaustiveness still pending |
| 5 — IR lowering | ⛔ not started | depends on Phase 4 completion |
| 6 — Object emitters (×5) | 🟡 WASM only | from-spec WASM emitter: i32 + arithmetic + comparisons + let + call + if/else/while + break/continue. ELF/Mach-O/COFF not started |
| 7 — Driver + CLI | 🟡 partial (~55%) | 11 working subcommands incl. `build --emit wasm` end-to-end; run/ship/test/fmt/doc/graph/size/mem/dev/time/abi/fix still return exit 2 |
| 7.5 — Dress rehearsal | ⛔ not started | |
| 8 — Switchover | ⛔ not started | |
| 9 — Cleanup | ⛔ not started | |
| 10 — Node → cargo xtask | 🟡 minimal | xtask exists with normalize + real check-determinism; conformance/bench/etc port still pending |
| 11 — `compiler-zero/` strategy | ⛔ not started | recommended Option B per plan |

## Subcommands implemented in `.zero/bin/zero-rs`

| Subcommand | Scope | Differential vs C |
|---|---|---|
| `--version [--json]` | Full | structural match |
| `tokens --json <file>` | Full | byte-identical token stream across 8 corpus files |
| `parse --json <file>` | Summary | structural match across 4 corpus files |
| `targets --json` | Full | structural match (10 targets) |
| `check [--json] <file>` | Lex + parse + name resolution | catches lex/parse errors and undefined name refs; type/borrow/effect NOT yet checked |
| `explain [--json] <code>` | Stub | knows code→string mapping only |
| `clean` | Full | removes `.zero/` cache dir |
| `doctor [--json]` | Minimal | host + cc/zig PATH; full audit pending |
| `skills [list/get/path] [--json]` | Stub shell | JSON shape only; rich data not yet ported |
| `routes [--json] <project>` | Real | parses each `.0` file under `src/routes/` (or `routes/`, or project root); reports every `pub fun` with file/line/column/paramCount/returnType/raises. `#[route(...)]` attribute extraction still pending |
| `build [--emit wasm] [--out <file>] <file.0\|project-dir>` | Real | pipeline: lex → full parse → name-resolve → WASM emit. Single file or recursive .0 collection from a directory |

Subcommands that return exit 2 with directive: `build --emit exe|obj`,
run, ship, test, fmt, new, doc, graph, size, mem, dev, time, abi, fix.

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
| zero-parser (full statement) | 3 | hello.0/add.0/branch.0 full AST |
| zero-checker (name resolution) | 11 | undefined refs, scope rules, std prefix, host capability |
| zero-emit-wasm | 17 | const/params/let/call/comparisons/if-else/while/break-continue, rejection paths, LEB128 round-trips |
| **Total** | **96 tests** | clippy clean, determinism contract active |

Additional runtime checks:
- `cargo run -p xtask -- check-determinism`: 91 runs across 65 files,
  zero non-determinism (real §5.1 gate active)
- `npm run rust:{check,clippy,test,build,determinism}` all green
- CI workflow integration: `.github/workflows/ci.yml` runs Rust
  workspace check/clippy/test/determinism on every PR

End-to-end verified:
```bash
.zero/bin/zero-rs --version                  # zero 0.1.1 / commit / host
.zero/bin/zero-rs check examples/hello.0     # check ok (lex + parse + name-res)
.zero/bin/zero-rs targets --json             # 10 targets, structural match C
.zero/bin/zero-rs routes examples/web/hello  # "1 route ... pub fun GET ..."
.zero/bin/zero-rs build --emit wasm examples/add.0 --out /tmp/out
                                              # writes /tmp/out.wasm
.zero/bin/zero-rs build --emit wasm /path/to/project --out /tmp/proj
                                              # recursive .0 collection + WASM
ZERO_BIN=.zero/bin/zero-rs <script>          # drop-in for 11 subcommands
```

## Lines of code

| Component | Rust LOC |
|---|---|
| zero-rs Cargo workspace scaffold | ~150 |
| zero-diag (Diag struct + code table) | ~115 |
| zero-target (manifest + derive) | ~560 |
| zero-lexer (impl + tests) | ~470 |
| zero-ast (with extended Stmt + Expr) | ~230 |
| zero-parser summary + expression + full-stmt (impl + tests) | ~1,400 |
| zero-checker (name resolution + 11 tests) | ~330 |
| zero-emit-wasm (impl + tests incl. control flow) | ~960 |
| zero-cli (11 subcommands incl. build pipeline + project dir) | ~720 |
| xtask (normalize + check-determinism) | ~340 |
| Differential tests (3 files) | ~330 |
| **Total Rust** | **~5,600 LOC** |

## CI integration (committed)

`.github/workflows/ci.yml` now runs five Rust steps on every PR after
the C compiler build:

- `dtolnay/rust-toolchain@stable` toolchain install
- `actions/cache@v4` for `~/.cargo` and `native/zero-rs/target`,
  keyed on `Cargo.lock`
- `npm run rust:check` — `cargo check --workspace --all-targets`
- `npm run rust:clippy` — workspace clippy with the §5.1
  disallowed-types/methods determinism contract
- `npm run rust:test` with `ZERO_DIFFERENTIAL_REQUIRED=1` — three
  differential tests against `bin/zero` (tokens/parse/targets) fail
  rather than skip
- `npm run rust:determinism` — `xtask check-determinism` runs the
  Rust binary twice on the examples corpus, fails on byte diffs

## How to resume

### Immediate next steps (in order)

1. **Phase 4 (full) — type/borrow/effect checker.** The single largest
   port (~4,700 LOC C → ~6,000 LOC Rust). Slice along natural seams
   (type → effects → borrow → meta → match); pre-acknowledge that
   borrow + type likely fold into one slice.

2. **Phase 5 — IR Lowering** (~3,200 LOC). Depends on Phase 4 output
   shape.

3. **Phase 6 — additional emitters** in this order: ELF64 x86_64
   → Mach-O → ELF aarch64 → COFF. Each ~1,500–2,000 LOC from public
   format specs (System V ABI / Apple Mach-O / Microsoft PE-COFF).
   Gate on run-equality per §5, not byte-equality.

4. **Phase 7 rest** — `run`, `ship`, `test`, `fmt` subcommands once
   backends are available.

5. **Future WASM emitter extensions** — memory section + data
   segments + load/store opcodes (W3C spec) enable string/array
   support; required before Phase 4 stdlib functions can compile.

### Operational rules established

- **Determinism contract** in `native/zero-rs/clippy.toml` AND
  runtime-verified by `xtask check-determinism`. Banned: `HashMap`,
  `HashSet`, `Instant::now`, `SystemTime::now`, `temp_dir`,
  `process::id`.
- **Normalization** lives in `xtask/src/normalize.rs`. Any new JSON
  field added to a `--json` output must be classified `stable` or
  `masked` in the same PR.
- **Differential tests** spawn `bin/zero` (or `$ZERO_BIN`). Skip
  unbuilt; `ZERO_DIFFERENTIAL_REQUIRED=1` fails instead.
- **`ZERO_BIN` swap**: works across `conformance/run.mjs`,
  `scripts/test-native.sh`, all `scripts/*-smoke.mjs`,
  `snapshot-command-contracts.mjs`, `zls.mjs`,
  `agent-repair-demo.mjs`.
- **CI gate**: Rust workspace check/clippy/test/determinism now run
  on every PR via `.github/workflows/ci.yml`.

### Anti-patterns observed and avoided

- **Don't pre-add C dumper flags speculatively.** Recon revealed
  `tokens --json` / `parse --json` already existed.
- **Don't assume EOF semantics.** C emits the EOF token in
  `tokens --json`; the differential test caught the mismatch when
  Rust filtered it.
- **Don't byte-compare formatted JSON.** Compare `serde_json::Value`s
  structurally.
- **Don't lookahead-detect assigns inside expressions.** In the
  summary parser, `let x = y` was misread as Let+Assign because
  `x =` matched the assign pattern; restrict the check to statement
  starts.
- **Don't emit dead infrastructure.** Memory opcode constants without
  language integration produce only unused-warning noise; defer the
  emitter slice until the language work that uses it.

## Repository structure delta

```
native/zero-rs/
├── Cargo.toml                   # workspace manifest
├── Cargo.lock
├── Makefile                     # check / clippy / test / determinism / install-local
├── clippy.toml                  # §5.1 determinism contract
├── .gitignore
├── crates/
│   ├── zero-ast/                # ✅ Expr/FieldInit + Stmt with body/expr fields + Program
│   ├── zero-checker/            # ✅ name resolution checker (11 tests)
│   ├── zero-cli/                # ✅ 11 working subcommands incl. build pipeline + project dirs
│   ├── zero-diag/               # ✅ Diag struct + diag_code table
│   ├── zero-driver/             # stub (Phase 7 rest)
│   ├── zero-emit-coff/          # stub (Phase 6 rest)
│   ├── zero-emit-elf/           # stub (Phase 6 rest)
│   ├── zero-emit-macho/         # stub (Phase 6 rest)
│   ├── zero-emit-wasm/          # ✅ from-spec emitter: i32/arithmetic/let/call/if-else/while/break-continue
│   ├── zero-fs/                 # stub (Phase 7)
│   ├── zero-ir/                 # stub (Phase 5)
│   ├── zero-lexer/              # ✅ full port + differential test
│   ├── zero-parser/             # ✅ summary + expression + full-statement parsers + differential test
│   └── zero-target/             # ✅ manifest parse + per-target derivation + differential test
└── xtask/                       # ✅ normalize + real check-determinism

.zero/bin/
└── zero-rs                      # ✅ Rust binary, 11 working subcommands

.github/workflows/ci.yml         # ✅ Rust workspace gates added

package.json:
  + rust:{check,clippy,test,build,determinism}

scripts/* + conformance/run.mjs + scripts/test-native.sh:
  bin/zero -> $ZERO_BIN (default bin/zero, swappable)
```
