## Examples

The examples directory is the best hands-on path through Zero. Each example is
small, deterministic, and intended to be checked or built from the repository
root.

Start here:

```sh
bin/zero check examples/hello.0
bin/zero build --emit exe --target linux-musl-x64 examples/add.0 --out .zero/out/add
./.zero/out/add
bin/zero build --emit wasm --target wasm32-wasi examples/direct-wasm-add.0 --out .zero/out/direct-wasm-add.wasm
```

Tiny profile smoke:

```sh
bin/zero build --release tiny --target linux-musl-x64 examples/hello.0 --out .zero/out/hello-tiny
bin/zero size --json --release tiny --target linux-musl-x64 --out .zero/out/hello-tiny examples/hello.0
```

The current compiler can produce a tiny hosted/musl-style artifact for hello
world. This is not a complete no-libc runtime.

Build profiles:

```sh
bin/zero build --json --profile debug --target linux-musl-x64 examples/hello.0 --out .zero/out/hello-debug
bin/zero build --json --profile fast --target linux-musl-x64 examples/hello.0 --out .zero/out/hello-fast
bin/zero build --json --profile small --target linux-musl-x64 examples/hello.0 --out .zero/out/hello-small
bin/zero size --json --profile tiny --target linux-musl-x64 examples/fixed-vec.0
```

Build JSON reports `profileSemantics` and `profileBudget`. Size JSON adds
`sizeBreakdown`, `retentionReasons`, and `optimizationHints`.

Direct helper audit:

```sh
bin/zero size --json --target linux-musl-x64 examples/fixed-vec.0
```

The direct size and graph reports show which helpers are retained for
fixed-capacity shapes. They also make the absence of vtables, method registries,
generic registries, hidden allocator machinery, and reflection visible.

Cross-compile smoke:

```sh
bin/zero build --release tiny --target linux-musl-x64 examples/hello.0 --out .zero/out/hello-linux-musl
bin/zero build --release tiny --target win32-x64.exe examples/hello.0 --out .zero/out/hello-win32
bin/zero size --json --release tiny --target linux-musl-x64 --out .zero/out/hello-linux-musl examples/hello.0
```

On macOS hosts these commands produce Linux and Windows-style output artifacts
through direct emitters. `zero size --json` provides the target report and
artifact size without executing the foreign binary.

Core examples:

- `examples/hello.0`: `World`, stdout, `check`, and `raises`.
- `examples/direct-wasm-add.0`: experimental direct wasm output for exported primitive integer arithmetic.
- `examples/point.0`: shapes, literals, fields, and helper functions.
- `examples/result-choice.0`: enums, choices, payload binding, and `match`.
- `examples/fixed-vec.0`: field defaults, static value parameters, constructor-style methods, and receiver calls.
- `examples/fallibility.0`: named errors and explicit error sets.
- `examples/memory-primitives.0`: spans, fixed buffers, references, `Maybe<T>`, and allocator vocabulary.
- `examples/allocator-collections.0`: `NullAlloc`, fixed-buffer allocation, `Vec`, empty map/set metadata, and `zero mem --json` allocator budget reporting.
- `examples/compile-time-v1.0`: bounded `meta`, target/type reflection facts, Bool and enum static values, and compile-time JSON metadata.
- `examples/ownership-cleanup.0`: `owned<T>` cleanup, canonical `drop`, and `defer` at lexical scope exit.
- `examples/std-path-io.0`: fixed-buffer `std.path` helpers and caller-owned `std.io` buffers.
- `examples/std-data-formats.0`: `std.codec` encoders and basic explicit-allocator `std.json` helpers.
- `examples/std-platform.0`: `std.time`, `std.rand`, `std.proc`, and `std.crypto` capability-shaped helpers.
- `examples/cli-file.0`: args, env, file writes, stdout, and stderr.
- `examples/file-copy.0`: `Fs`, `owned<File>`, read/write resource capabilities, and automatic close.
- `examples/zero-hash/`: file checksum CLI with args fallback, fixed buffers, hosted reads, and CRC-32 over bytes.
- `examples/readall-cli/`: fixed-buffer allocator use and owned byte-buffer reads.
- `examples/resource-cli/`: package-local modules, resource cleanup, and hosted filesystem capability use.
- `examples/memory-package/`: target-neutral package helper checks without hosted filesystem dependencies.
- `examples/error-tour/`: broken examples, explanations, and canonical repairs for common diagnostics.
- `examples/agent-repair-demo/`: a scripted agent loop that checks JSON diagnostics, explains the code, plans a repair, applies the edit, and re-runs check.
- `examples/web/hello/`: route manifest and web bundle audit metadata for web projects.

Use the index in `examples/README.md` for the full learning order and copyable commands.

Native Workflow Coverage:

- arguments and environment: `examples/cli-file.0` reads `std.args` and `std.env`.
- filesystem resources: `examples/zero-hash/` seeds and reads a hosted file through explicit capabilities.
- deterministic exit status: `examples/direct-exe-return.0` builds to a tiny direct native executable that returns `42`.
- unhandled error exit path: `examples/direct-unhandled-error-exit.0` keeps the fallible exit route visible.

## Larger CLI: `zero-hash`

`examples/zero-hash/` is a larger CLI example. It exercises:

- args and hosted filesystem access
- fixed-buffer allocation
- memory helpers
- codec/hash-style APIs
- fallibility and owned resources

It stays useful without a large standard library.

Build command:

```sh
bin/zero build --emit wasm --target wasm32-wasi examples/zero-hash --out .zero/out/zero-hash
```

Run command:

```sh
npm run native:test
```

Expected output:

```text
zero-hash ok
```

Size output:

```sh
bin/zero size --json --target wasm32-wasi examples/zero-hash --out .zero/out/zero-hash-size.json
```

Inspect metadata:

```sh
bin/zero graph --json examples/zero-hash
```

The graph and size reports show the helper use behind `zero-hash`: args,
fixed-buffer allocation, CRC-32 bytes, and hosted reads. They also show that the
program does not retain hidden runtime machinery such as global allocation,
registries, reflection, or heap allocation.

Benchmark case:

```sh
ZERO_BENCH_RUNS=1 npm run bench
```

The benchmark report includes the Zero-only `zero-hash` case with expected output checking.

Cross-target status:

```sh
bin/zero check --json --target wasm32-web examples/zero-hash
```

`zero-hash` intentionally uses hosted filesystem APIs. WASI provides the required
filesystem capability for the direct harness. Browser-style `wasm32-web` checks
report `TAR002`.

Use `examples/memory-package/` for target-neutral cross-target direct builds.

## Web Routes: `web/hello`

Route map:

```sh
bin/zero routes --json examples/web/hello
```

The packet reports the runtime target (`wasm32-web`), route count, route files,
artifact metadata, web surfaces, and web bundle audit facts.

The route packet also reports `localRuntime` for the portable browser-worker
path:

- explicit imports
- denied filesystem/process access
- `frameworkTaxBytes: 0`
- `providerSpecificDeployment: false`

Use `npm run wasm:runtime:smoke` to run the local WASI/browser import harness.
