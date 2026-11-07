//! The K server runtime: V8 through `deno_core`, HTTP through `hyper`.
//!
//! * [`engine`]: one isolate with the embedded runtime, an in-memory module
//!   map and K's [`ops`]. Used by the server's workers, the execution
//!   corpus and the runtime's JS unit tests.
//! * [`http`]: the HTTP front: accepts connections, serves hashed static
//!   assets, enforces body limits and the RPC and request-forgery
//!   preconditions ([`front`]), and hands every other request to a worker
//!   isolate running the app's server bundle.
//! * [`config`]: reading `config` declarations from the environment and
//!   `NAME_FILE` secret files, validated all at once and redacted.
//! * [`process`]: the state every worker of a server process shares
//!   (sessions, rate limits, `.server` state, broadcasts, the database).
//! * [`db`]: the process's database: migrations at startup, statements by
//!   id, request-bound transactions.
//! * [`app`]: the `dist/app` archive (server bundle, assets, migrations)
//!   appended to the runtime executable.
//! * [`testing`]: the HTTP request scripts of the server end-to-end tests.
//!
//! The design is `docs/compiler/server.md`; what is v0 and what is deferred
//! is marked there.

pub mod app;
pub mod config;
pub mod db;
pub mod engine;
pub mod front;
pub mod http;
pub mod ops;
pub mod process;
pub mod testing;

pub use engine::{Engine, EngineError, EngineOptions, JsModule, LogLine, Output};
pub use http::{Server, ServerBundle, ServerError, ServerOptions};
