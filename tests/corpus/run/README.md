# The `run` corpus

Programs compiled by `k_codegen` and executed in V8; each case pins what a program logs (`.stdout`) and
what `main` returns, throws or panics with (`.result`). Formats: `docs/compiler/testing.md` §3.10. Run with
`cargo test -p k_server --test corpus_run`.

Every case is `.pending` until code generation lowers what it uses.
