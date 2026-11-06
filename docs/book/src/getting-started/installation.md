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

> **Planned.** The K language server isn't in the first compiler releases. It will be built into the
> same `k` binary and reuse the compiler's own queries, so your editor can never disagree with the compiler. It
> will show each effect's reactive dependencies on hover, each page's URL inline, autocomplete after
> `.` that lists only valid members, and quick fixes for compiler errors.

Until then, `k check` reports every problem from the command line, and `k dev` reports them as you
save. Every diagnostic is also available as JSON (`k check --format json`), which is how agents and
editor integrations consume them.

## What's in the box

| Part | What it does |
|---|---|
| Compiler | Type-checks `.k` files and emits JavaScript, CSS and the server bundle |
| Dev server | `k dev`: builds, serves and rebuilds your app as you edit |
| Test runner | `k test`: unit, component and integration tests |
| Migration tool | `k migrate`: generates and applies database migrations |
| Server runtime | A JavaScript runtime (V8) embedded in the binary, used by `k dev` and by every built app |
| Formatter | `k fmt` (**Planned**) |
| Language server | (**Planned**) |

The same runtime runs your server code in development and in production, so "works on my machine"
means "works in production".
