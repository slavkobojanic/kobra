# `codegen/lang`: lowering of the core language

`.expect` assertions pinning how language constructs are lowered (`docs/compiler/codegen.md` §4–§9):
struct path copies, copy-on-write, equality, typed catches that rethrow panics, the cancellation context,
interpolation by type, string-literal safety, struct methods as module functions, overload names, enum
constructors, Codable codecs and witnesses, hash collections, units and Money. What the generated code
*does* is pinned by the `run` suite (`tests/corpus/run`). Every case is `.pending` until lowering exists.
