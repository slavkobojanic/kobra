# The `codegen` corpus

What `k_codegen` emits: exact `.client.js` / `.server.js` / `.css` (rendered and normalized as
`docs/compiler/testing.md` §3.9 specifies) or `.expect` assertions. The lowering rules are
`docs/compiler/codegen.md`. Every case is `.pending` until code generation lowers what it uses; the split
cases (`split/`) pin the guarantee that server-only code and data never reach a client file.
