# Installation

K ships as **one self-contained binary, `k`**. It contains the compiler, the test runner, the development
server, the migration tool and the server runtime. Installing K needs no Node.js, no npm and no other
JavaScript tooling, and a K app never depends on them at build time or at runtime.

## Building from source

Prebuilt releases aren't published yet, so today you build `k` from its repository. The toolchain is
written in Rust, so you need a stable Rust toolchain from [rustup.rs](https://rustup.rs).

```bash
git clone <the K repository> k
cd k
cargo install --path crates/k_cli --locked
```

`cargo install` builds the `k` binary and places it in `~/.cargo/bin`, which rustup already put on
your `PATH`. Check that it works:

```bash
k --help
```

To work on the toolchain itself rather than with it, see the repository's `README.md`, which explains
how to build, test and lint the compiler.

## Installing a release

When releases are published, the docs site serves an install script, and installing K is one line. On
Linux (x86_64 or arm64, with glibc 2.31 or newer) and macOS:

```sh
curl -fsSL https://<docs site>/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://<docs site>/install.ps1 | iex
```

The script picks the archive for your machine, checks it against the release's `SHA256SUMS`, unpacks
it into `~/.k/toolchains/<version>/` and makes it the default, `~/.k/bin/k`. It prints the line that
puts `~/.k/bin` on your `PATH`; it never edits your shell's profile itself. Running it again changes
nothing, and `--version 0.1.0` (or `K_VERSION=0.1.0`) installs that version instead of the newest one.
Linux distributions built on musl, such as Alpine, aren't supported: use a glibc distribution, or the
toolchain container image.

From then on, `k` keeps itself and your projects' toolchains up to date:

```bash
k self update               # the newest release, made the default
k toolchain install         # the toolchain the project in this directory needs
k toolchain list            # what is installed, the default, and this project's
```

[The k command](../reference/cli.md#toolchains) lists every toolchain command.

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
