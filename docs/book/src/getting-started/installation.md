# Installation

Kobra ships as **one self-contained binary, `k`**. It contains the compiler, the test runner, the development
server, the migration tool and the server runtime. Installing Kobra needs no Node.js, no npm and no other
JavaScript tooling, and a Kobra app never depends on them at build time or at runtime.

## Installing Kobra

On Linux (x86_64 or arm64, with glibc 2.31 or newer) and macOS, in a terminal:

```sh
curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://slavkobojanic.github.io/kobra/install.ps1 | iex
```

The script downloads the newest release from
[GitHub Releases](https://github.com/slavkobojanic/kobra/releases), picks the archive for your machine,
checks it against the release's `SHA256SUMS`, unpacks it into `~/.k/toolchains/<version>/` and makes it
the default, `~/.k/bin/k`, and unpacks the release's book into `~/.k/docs/<version>/`. It prints the line
that puts `~/.k/bin` on your `PATH`; it never edits your shell's profile itself. Running it again changes
nothing, and `--version 0.1.0` (or `K_VERSION=0.1.0`) installs that version instead of the newest one:

```sh
curl -fsSL https://slavkobojanic.github.io/kobra/install.sh | sh -s -- --version 0.1.0
```

The releases are public, so you need no GitHub account, no [GitHub CLI](https://cli.github.com) and no
token. Until the first stable release is out, the script installs the newest release candidate and says
so, reading the list of releases from this site (`index.json`). Linux distributions built on musl, such
as Alpine, aren't supported: use a glibc distribution, or the toolchain container image,
`ghcr.io/slavkobojanic/kobra`. Then check that it works:

```bash
k --version
```

From then on, `k` keeps itself and your projects' toolchains up to date:

```bash
k self update               # the newest release, made the default
k toolchain install         # the toolchain the project in this directory needs
k toolchain list            # what is installed, the default, and this project's
```

These read the releases through GitHub's API, which allows 60 requests an hour without a token; an
install makes about six. If you hit the limit, set `GH_TOKEN` to a GitHub token, or sign in with the
GitHub CLI (`gh auth login`): `k` then sends the token, to GitHub's API only.
[The k command](../reference/cli.md#toolchains) lists every toolchain command.

## Reading the book

This book is online at [slavkobojanic.github.io/kobra](https://slavkobojanic.github.io/kobra/), one copy per
release (the version menu switches between them), and it is installed with every toolchain, so it also
reads offline. `k docs` opens the copy that matches your `k` in your browser, and `k docs --path` prints
where it is:

```bash
k docs
k docs --path
```

In a checkout of the repository, with [mdBook](https://rust-lang.github.io/mdBook/) installed,
`mdbook serve docs/book` serves the book from its source as you edit it.

## Building from source

To build `k` yourself, for an unsupported platform or to work on the toolchain, you need a stable Rust
toolchain from [rustup.rs](https://rustup.rs):

```bash
git clone https://github.com/slavkobojanic/kobra
cd kobra
cargo install --path crates/k_cli --locked
```

`cargo install` builds the `k` binary and places it in `~/.cargo/bin`, which rustup already put on
your `PATH`. To work on the toolchain itself rather than with it, see the repository's `README.md`,
which explains how to build, test and lint the compiler.

## Editor support

The language server is built into the same binary: `k lsp`. It reuses the compiler's own analysis, so
your editor can never disagree with the compiler. It shows each effect's reactive dependencies on hover,
each page's URL inline, autocomplete after `.` that lists only valid members, and quick fixes for compiler
errors. [Editor setup](editor-setup.md) explains how to connect VS Code, Neovim, Vim or any other editor.

From the command line, `k check` reports every problem, and `k dev` tells you as you save when the
build has errors. Every
diagnostic is also available as JSON (`k check --format json`), which is how agents consume them.

## What's in the box

| Part | What it does |
|---|---|
| Compiler | Type-checks `.k` files and emits JavaScript, CSS and the server bundle |
| Dev server | `k dev`: builds, serves and rebuilds your app as you edit |
| Test runner | `k test`: unit, component and integration tests |
| Migration tool | `k migrate`: generates and applies database migrations |
| Server runtime | A JavaScript runtime (V8) embedded in the binary, used by `k dev` and by every built app |
| Formatter | `k fmt`: rewrites code into the one canonical layout, also on save in your editor |
| Language server | `k lsp`: diagnostics, quick fixes, navigation, rename and completion in your editor |

The same runtime runs your server code in development and in production, so "works on my machine"
means "works in production".
