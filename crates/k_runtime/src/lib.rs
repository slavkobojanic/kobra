//! The JavaScript runtime of K, embedded in the `k` binary.
//!
//! The runtime is hand-written JavaScript (no npm, no bundler) stored under
//! this crate and embedded with `include_str!`. Generated code imports it.
//! It provides:
//!
//! * **signals**: `signal`, `computed`, `effect`, `batch`, `untrack` and
//!   `onCleanup`, with an owner tree for disposal and automatic cancellation
//!   of async effects;
//! * **path stores** for fine-grained updates of nested `@State` structs;
//! * the **DOM renderer**: template cloning, text/attribute/class/event
//!   bindings, keyed lists (`for ... key`), conditionals and `match`;
//! * **hydration** of server-rendered HTML;
//! * the **router**: History API navigation, typed URL building and parsing,
//!   layouts, guards and redirects;
//! * the **RPC client**: Connect-compatible JSON over HTTP, typed errors and
//!   cancellation.
//!
//! The design and its size budget are in `docs/compiler/architecture.md`,
//! "JavaScript runtime".
//!
//! # Status
//!
//! Not implemented yet.
