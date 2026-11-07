# Installation

K ships as **one static binary, `k`**. It contains the compiler, the test runner, the development
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

## Editor support

The language server is built into the same binary: `k lsp`. It reuses the compiler's own analysis, so
your editor can never disagree with the compiler. It shows each effect's reactive dependencies on hover,
each page's URL inline, autocomplete after `.` that lists only valid members, and quick fixes for compiler
errors. [Editor setup](editor-setup.md) explains how to connect VS Code, Neovim, Vim or any other editor.

From the command line, `k check` reports every problem, and `k dev` reports them as you save. Every
diagnostic is also available as JSON (`k check --format json`), which is how agents consume them.

## What's in the box

| Part | What it does |
|---|---|
| Compiler | Type-checks `.k` files and emits JavaScript, CSS and the server bundle |
| Dev server | `k dev`: builds, serves and rebuilds your app as you edit |
| Test runner | `k test`: unit, component and integration tests |
| Migration tool | `k migrate`: generates and applies database migrations |
| Server runtime | A JavaScript runtime (V8) embedded in the binary, used by `k dev` and by every built app |
| Formatter | `k fmt` (**Planned**) |
| Language server | `k lsp`: diagnostics, quick fixes, navigation, rename and completion in your editor |

The same runtime runs your server code in development and in production, so "works on my machine"
means "works in production".
