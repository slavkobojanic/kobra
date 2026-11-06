//! Code generation for K: JavaScript and CSS from the checked program.
//!
//! `k_codegen` lowers the HIR produced by `k_sema` into:
//!
//! * **client JavaScript** (ES2022 modules): components compiled to
//!   fine-grained DOM code over the signal runtime of `k_runtime`, with
//!   static template cloning, per-binding effects and no virtual DOM; the
//!   generated `Page` enum, router table, typed RPC client stubs and the
//!   hydration entry points;
//! * **server JavaScript**: the same components compiled to streaming string
//!   renderers for server-side rendering, services with request scoping, RPC
//!   handlers, repositories and the dependency-injection wiring;
//! * **CSS**: one stylesheet of scoped (hashed) class rules per component,
//!   theme tokens as custom properties on `:root`, and variant classes;
//! * **source maps** for both bundles.
//!
//! The compiler enforces the client/server split here: anything reachable
//! from a server-only declaration never reaches the client bundle.
//!
//! The design is in `docs/compiler/architecture.md`, "Code generation".
//!
//! # Status
//!
//! Not implemented yet.
