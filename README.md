# Kobra

Kobra is a strongly typed language for full-stack web applications that compiles ahead of time to
JavaScript. Source files end in `.k`, and everything ships as one binary, `k`: compiler, test runner,
formatter, development server, migration tool, language server, MCP server and server runtime.

## Install

The repository is private, so Kobra installs for people who have access to it. Install the
[GitHub CLI](https://cli.github.com) and sign in with an account that can read
`slavkobojanic/kobra`:

```sh
gh auth login
```

Then, on Linux (x86_64 or arm64, glibc 2.31 or newer) and macOS:

```sh
gh api repos/slavkobojanic/kobra/contents/scripts/install.sh -H 'Accept: application/vnd.github.raw' | sh
```

On Windows, in PowerShell:

```powershell
gh api repos/slavkobojanic/kobra/contents/scripts/install.ps1 -H 'Accept: application/vnd.github.raw' | Out-String | iex
```

The script downloads the newest release from the repository's
[GitHub Releases](https://github.com/slavkobojanic/kobra/releases) with your sign-in (or `GH_TOKEN`
and curl, without `gh`), verifies it against the release's `SHA256SUMS` and installs it, with its book,
into `~/.k` (until the first stable release, the newest release candidate). Then:

```sh
k new hello && cd hello && k dev
```

`k self update` installs newer releases the same way, through your `gh` sign-in or `GH_TOKEN`.

## AI assistants

`k mcp` is an MCP server that gives AI coding assistants Kobra's documentation, its example apps and the
compiler's checks, all built into `k`. In Claude Code:

```sh
claude mcp add kobra -- k mcp
```

See [AI assistants](docs/book/src/getting-started/ai-assistants.md) in the book, and
[docs/compiler/mcp.md](docs/compiler/mcp.md) for the tools.

## Documentation

The book, with a quick start, a tutorial and the reference, is installed with every toolchain: `k docs`
opens it (`k docs --path` prints where it is). There is no public docs site while the repository is
private. In a checkout, `mdbook serve docs/book` serves it from its source,
[docs/book](docs/book/README.md). For contributors:

- Language design: [docs/design](docs/design/README.md)
- Compiler architecture: [docs/compiler/architecture.md](docs/compiler/architecture.md)
- Grammar: [docs/compiler/grammar.md](docs/compiler/grammar.md)
- Diagnostics and codes: [docs/compiler/diagnostics.md](docs/compiler/diagnostics.md)
- Testing guide: [docs/compiler/testing.md](docs/compiler/testing.md)
- Releasing: [docs/compiler/release.md](docs/compiler/release.md)

## Repository layout

```text
crates/
  k_diagnostics   diagnostics: codes, spans, labels, quick fixes, human + JSON rendering
  k_syntax        tokens, lexer, parser, lossless syntax tree, typed AST
  k_sema          semantic analysis and the embedded standard library
  k_codegen       JavaScript and CSS generation
  k_runtime       the JavaScript runtime embedded in every app
  k_server        server runtime: V8, HTTP, SSR, RPC
  k_db            persistence: schema, query IR, SQL, migrations, drivers
  k_fmt           the formatter (`k fmt`)
  k_lsp           the language server (`k lsp`)
  k_mcp           the MCP server for AI assistants (`k mcp`)
  k_cli           the `k` binary
  k_corpus        golden corpus test harness
  xtask           release automation (`cargo xtask`), never shipped
editors/          VS Code, JetBrains (IntelliJ), Neovim and Vim support
examples/         example apps
tests/corpus/     golden corpus (lexer, parser, ...)
fuzz/             cargo-fuzz targets (nightly)
docs/             the book, language design and compiler documentation
scripts/          install.sh, install.ps1 and the CI helpers
```

## Building and testing

The toolchain is written in Rust (stable). Run `. "$HOME/.cargo/env"` first if cargo is not on your path.

```sh
cargo build --workspace                  # build everything, including the `k` binary
cargo test --workspace                   # unit, integration, property, corpus and CLI tests
cargo clippy --workspace --all-targets   # lints (must be clean)
cargo fmt --all --check                  # formatting (must be clean)
cargo bench -p k_syntax                  # benchmarks
cargo run -q -p k_cli -- check src/      # run `k check` on a directory
```

How to run each kind of test and how to add golden corpus cases is described in
[docs/compiler/testing.md](docs/compiler/testing.md).

## Licence

Kobra is dual-licensed under either of

- the MIT licence ([LICENSE-MIT](LICENSE-MIT)), or
- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)),

at your option. Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion
in Kobra, as defined in the Apache-2.0 licence, is dual-licensed as above, without any additional terms or
conditions.
