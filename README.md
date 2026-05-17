# zerolang

`zerolang` is a systems programming language designed for agents: small native tools, explicit effects, predictable memory, and structured compiler output.

This repository contains the language compiler (C and partial Rust port), standard library examples, documentation site, conformance suite, and editor tooling.

> Version 1.0. The C compiler is the released binary; the Rust port (`native/zero-rs/`) handles 11 subcommands today and is on its way to full parity — see `docs/RUST_PORT_PLAN.md`.

## Quick Start

Install the latest release:

```bash
curl -fsSL https://zerolang.ai/install.sh | bash
export PATH="$HOME/.zero/bin:$PATH"
zero --version
```

Check a program:

```bash
zero check examples/hello.0
```

Run a small executable:

```bash
zero run examples/add.0
```

Expected output:

```text
math works
```

## Learn the Language

- `docs-site/articles/getting-started.md` — build the compiler and run your first program.
- `docs-site/articles/learn-zero.md` — a practical tour of the language.
- `docs-site/articles/language-reference.md` — syntax and behavior reference.
- `examples/README.md` — examples grouped by concept.

Run the documentation site locally:

```bash
npm run docs:dev
```

## Common Commands

```bash
zero check examples/hello.0
zero run examples/add.0
zero build --emit exe --target linux-musl-x64 examples/add.0 --out .zero/out/add
zero graph --json examples/systems-package
zero size --json examples/point.0
zero routes --json examples/web/hello
zero skills get zero --full
zero doctor --json
```

## Validation

```bash
npm run docs:test
npm run conformance
npm run native:test
npm run command-contracts
```

Benchmarks run locally by default:

```bash
npm run bench
```

## Repository Layout

- `native/zero-c/` — native compiler implementation.
- `compiler-zero/` — Zero-authored compiler sources.
- `examples/` — runnable Zero source examples.
- `conformance/` — language and CLI behavior fixtures.
- `docs-site/` — documentation site.
- `tests/` — TypeScript tests for CLI behavior.
- `extensions/vscode/` — editor syntax highlighting for `.0` files.

## License

See repository for license details.
