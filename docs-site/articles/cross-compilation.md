## Cross-Compilation Guide

Zero target names are explicit and target facts are visible through `zero targets`.

```sh
bin/zero targets
bin/zero check --target linux-musl-x64 examples/memory-package
bin/zero build --target linux-musl-x64 examples/memory-package --out .zero/out/memory-package
```

The compiler separates checking from executable linking. Target-neutral code can
check for non-host targets, while hosted APIs such as `std.fs` are rejected when
the target does not provide that capability.

```sh
bin/zero check --json --target wasm32-web conformance/native/fail/std-fs-target-unsupported.0
```

## Direct Artifacts

Supported executable builds use Zero's direct target emitters. Unsupported
targets or language features report diagnostics rather than silently choosing an
external backend.

```sh
bin/zero build --emit exe --target linux-musl-x64 examples/direct-exe-return.0 --out .zero/out/direct-exe-return
bin/zero build --emit wasm --target wasm32-web examples/direct-wasm-add.0 --out .zero/out/direct-wasm-add
```

Use JSON modes to inspect target support, required capabilities, selected
emitters, and artifact facts:

```sh
bin/zero build --json --emit exe --target linux-musl-x64 examples/direct-exe-return.0
bin/zero graph --json --target wasm32-web examples/memory-package
bin/zero size --json --target wasm32-web examples/direct-wasm-add.0
```

## Sysroots And C Boundaries

Zero reports sysroot and C ABI facts in JSON so cross-target builds do not
silently reuse host SDK paths. When a target requires an explicit SDK/sysroot,
use the environment variable named by `zero targets --json`.

C interop is still early. Keep C-facing code small, inspect `zero abi --json`
where applicable, and prefer examples that make target assumptions explicit.

## Wasm And Local Web Runtime

The direct WebAssembly path supports small browser and WASI artifacts for the
documented subset:

```sh
bin/zero build --emit wasm --target wasm32-wasi examples/direct-wasm-add.0 --out .zero/out/direct-wasm-add.wasm
bin/zero build --emit wasm --target wasm32-web examples/direct-array-sum.0 --out .zero/out/direct-array-sum
```

Route metadata is available through:

```sh
bin/zero routes --json examples/web/hello
```

This reports route maps, local-runtime facts, explicit imports, and capability
restrictions. Hosted deployment adapters are not the focus of the current public
preview.
