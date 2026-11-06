//! Semantic analysis of K programs.
//!
//! `k_sema` turns syntax trees into a checked, resolved program. It owns:
//!
//! * the **project model**: modules from directories, files, imports, the
//!   one-public-declaration-per-file rule and visibility;
//! * **name resolution**: imports, declarations, members, implicit member
//!   expressions (`.name` against the expected type), the generated `Page`
//!   enum and theme tokens;
//! * the **HIR** (high-level intermediate representation): a desugared,
//!   arena-allocated, id-based form of each declaration body, independent of
//!   trivia and syntax, which later stages consume;
//! * **type checking and inference**: structs, enums, classes, interfaces,
//!   generics with `where` constraints, optionals, typed throws and
//!   exhaustive `match`/`catch`, units and number extensions;
//! * the **language rules** that make K's guarantees hold: reactivity
//!   (`@State`/`@Derived`/`@Effect` and their annotation/mutability pairing),
//!   components (props, bindings, slots, chained optional props), styles and
//!   class references, dependency injection (the compile-time DI graph,
//!   scopes, cycles), layering (who may inject what), the server/client
//!   boundary, routing (`requires`, `at`, `@Param`), protocols, persistence
//!   queries and tests;
//! * the **incremental query database** shared by the compiler and the
//!   language server.
//!
//! The design is in `docs/compiler/architecture.md`, "Semantic analysis" and
//! "Incremental queries".
//!
//! # Status
//!
//! Not implemented yet. This crate exists so the workspace layout matches the
//! architecture.
