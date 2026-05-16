# Contributor Notes

Zero is the programming language for agents. Keep public-facing changes honest
about what works today without weakening that positioning.

## Development

- Build the local compiler with `make -C native/zero-c`.
- Use `bin/zero` for focused checks.
- Keep examples runnable and docs copyable.
- Prefer small, direct changes over broad refactors.
- Use direct emitters for compiler output.

## Useful Checks

```sh
npm run docs:test
npm run conformance
npm run native:test
npm run command-contracts
```

For focused compiler work:

```sh
bin/zero check --json <file-or-package>
bin/zero graph --json <file-or-package>
bin/zero size --json <file-or-package>
bin/zero explain <diagnostic-code>
bin/zero fix --plan --json <file-or-package>
```

## Project Layout

- `native/zero-c/`: native compiler implementation.
- `compiler-zero/`: Zero-authored compiler sources.
- `examples/`: small runnable programs and packages.
- `conformance/`: language and CLI fixtures.
- `docs-site/`: public documentation site.
- `scripts/`: validation and release support tooling.

## Public Docs Policy

Docs should describe current user-facing behavior, not internal development
history. Avoid release-planning language, validation-report narratives, and
implementation diary details in pages intended for external readers.

## Releasing

Releases are manual, single-branch affairs. The maintainer controls the
changelog voice and format.

To prepare a release:

1. Create a release branch, such as `ctate/v0.1.1`.
2. Bump the release version in `package.json`, `package-lock.json`,
   `docs-site/package.json`, `extensions/vscode/package.json`, and
   `native/zero-c/src/main.c`.
3. Update command-contract expectations that assert the compiler version.
4. Write the `CHANGELOG.md` entry for the new version, wrapped in
   `<!-- release:start -->` and `<!-- release:end -->` markers.
5. Remove the release markers from the previous release entry. Only the latest
   release entry should have markers.
6. Include a `### Contributors` section in the marked changelog entry. Derive
   contributors from commit authors and `Co-authored-by` trailers since the
   previous tag.
7. Run focused checks before opening or updating the PR:

```sh
make -C native/zero-c
bin/zero --version --json
npm run test:zero
npm run command-contracts:local
npm run docs:test
```

The release workflow reads the version from `package.json`, builds release
assets, and uses the content between the changelog markers as the GitHub
release body.
