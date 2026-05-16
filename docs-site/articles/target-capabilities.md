## What Targets Mean Today

The current compiler has a small explicit target table. Inspect it with:

```sh
zero targets
```

The JSON includes:

- `schemaVersion`
- current `host`
- each target's `hosted` flag
- aliases
- mapped C target
- capabilities

## Host Capabilities

Only the current host target exposes hosted process capabilities:

- `args`
- `env`
- `fs`
- `memory`
- `stdio`

Non-host targets currently expose the target-neutral subset:

- `memory`
- `stdio`

This means hosted `std.fs` examples are valid on the host target. Memory-only
packages can still build for target-neutral outputs.

## Hosted File I/O

This succeeds on the host target:

```sh
zero check examples/resource-cli
```

The same hosted filesystem surface fails clearly on a non-host target:

```sh
zero check --json --target linux-musl-x64 conformance/native/fail/std-fs-target-unsupported.0
```

The diagnostic is `TAR002` with repair id `remove-hosted-fs-or-use-host-target`.

## Target-Neutral Memory

`std.mem.copy` and `std.mem.fill` do not require hosted filesystem support:

```sh
zero build --target linux-musl-x64 examples/memory-package --out .zero/out/memory-package
```

Use graph and size JSON to inspect target facts:

```sh
zero graph --json --target linux-musl-x64 examples/memory-package
zero size --json --target linux-musl-x64 examples/memory-package
```

Both outputs include `requiresCapabilities`, `targetSupport`, and `stdlibHelpers`.

## Repair Commands

Use `zero explain` for human and JSON explanations:

```sh
zero explain TAR002
zero explain --json TAR002
```

Use fix-plan mode to inspect the canonical repair without editing files:

```sh
zero fix --plan --json --target linux-musl-x64 conformance/native/fail/std-fs-target-unsupported.0
```
