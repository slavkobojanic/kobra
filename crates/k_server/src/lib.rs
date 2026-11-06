//! The K server runtime.
//!
//! `k_server` runs the server bundle produced by `k_codegen`. It embeds V8
//! through `deno_core`, serves HTTP with `hyper`, and exposes only
//! web-standard APIs (fetch, streams, Web Crypto) plus K's own ops to the
//! JavaScript side. It provides:
//!
//! * server-side rendering of pages with streamed HTML, serialized `@Load`
//!   results and hydration data;
//! * RPC dispatch: Connect-compatible JSON over HTTP, with request-scoped
//!   service instances, `requires` guards and `Validatable` request checks;
//! * static, hashed client assets with long-lived caching;
//! * database access through Rust ops backed by `k_db`;
//! * secrets and configuration from the environment.
//!
//! `k build` produces `dist/app`: this runtime plus the application bundle in
//! one executable. The design is in `docs/compiler/architecture.md`, "Server
//! runtime".
//!
//! # Status
//!
//! Not implemented yet.
