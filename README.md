# K

K is a strongly typed language for full-stack web applications that compiles ahead of time to
JavaScript. Everything ships as one binary, `k`: compiler, test runner, development server, migration
tool and server runtime.

- Language design: [docs/design](docs/design/README.md)
- Compiler architecture: [docs/compiler/architecture.md](docs/compiler/architecture.md)
- Grammar: [docs/compiler/grammar.md](docs/compiler/grammar.md)
- Diagnostics and codes: [docs/compiler/diagnostics.md](docs/compiler/diagnostics.md)
- Testing guide: [docs/compiler/testing.md](docs/compiler/testing.md)

## Repository layout

```text
crates/
  k_diagnostics   diagnostics: codes, spans, labels, quick fixes, human + JSON rendering
  k_syntax        tokens, lexer, parser, lossless syntax tree, typed AST, golden dumps
  k_sema          semantic analysis (planned)
  k_codegen       JavaScript and CSS generation (planned)
  k_runtime       embedded JavaScript runtime (planned)
  k_server        server runtime: V8, HTTP, SSR, RPC (planned)
  k_db            persistence: schema, query IR, SQL, migrations, drivers (planned)
  k_cli           the `k` binary
  k_corpus        golden corpus test harness
tests/corpus/     golden corpus (lexer, parser, ...)
fuzz/             cargo-fuzz targets (nightly)
docs/             language design and compiler documentation
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
