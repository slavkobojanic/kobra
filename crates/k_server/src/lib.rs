//! The K server runtime: V8 through `deno_core`, HTTP through `hyper`.
//!
//! * [`engine`]: one isolate with the embedded runtime, an in-memory module
//!   map and K's [`ops`]. Used by the server's workers, the execution
//!   corpus and the runtime's JS unit tests.
//! * [`http`]: the HTTP front: accepts connections, serves hashed static
//!   assets, enforces body limits and the RPC and request-forgery
//!   preconditions ([`front`]), and hands every other request to a worker
//!   isolate running the app's server bundle.
//! * [`channels`]: the frames of `k.channel.v1`, the channels' WebSocket
//!   protocol, as pure JSON and Protobuf encode and decode.
//! * [`sockets`]: the channels' WebSocket front at `/_k/channels`: the
//!   handshake checks, heartbeats, limits, resume and the hand-over of
//!   channel events to the worker isolates.
//! * [`config`]: reading `config` declarations from the environment and
//!   `NAME_FILE` secret files, validated all at once and redacted.
//! * [`process`]: the state every worker of a server process shares
//!   (sessions, rate limits, `.server` state, broadcasts, presence, the
//!   database).
//! * [`replicas`]: broadcasts, presence and session ends between replicas
//!   over Postgres `LISTEN`/`NOTIFY`.
//! * [`db`]: the process's database: migrations at startup, statements by
//!   id, request-bound transactions.
//! * [`jobs`]: the background jobs worker (claims, runs, retries, schedules).
//! * [`app`]: the `dist/app` archive (server bundle, assets, migrations)
//!   appended to the runtime executable.
//! * [`outbound`]: outgoing HTTP (`HttpClient`) and SMTP (`EmailProvider`)
//!   on behalf of K code.
//! * [`websocket`]: outgoing WebSockets (`WebSocketClient`) on behalf of K
//!   code.
//! * [`proxy`]: the client address behind the proxies `K_TRUSTED_PROXIES`
//!   trusts (`X-Forwarded-For`).
//! * [`fakes`]: fake third parties for tests (an HTTP API, an SMTP server,
//!   a WebSocket echo server).
//! * [`testing`]: the HTTP request scripts of the server end-to-end tests.
//!
//! The design is `docs/compiler/server.md`; what is v0 and what is deferred
//! is marked there.
//!
//! # Examples
//!
//! Run JavaScript in an isolate with the K runtime and read back the value
//! it reported through `op_k_set_result`:
//!
//! ```
//! use k_server::{Engine, EngineOptions};
//!
//! let mut engine = Engine::new(EngineOptions::default())?;
//! let result = engine.run_main("Deno.core.ops.op_k_set_result(String(6 * 7));")?;
//! assert_eq!(result.as_deref(), Some("42"));
//! # Ok::<(), k_server::EngineError>(())
//! ```
//!
//! Serve a hand-written server bundle on an ephemeral loopback port and
//! send it a request ([`testing::send`]):
//!
//! ```
//! use k_server::{JsModule, Output, Server, ServerBundle, ServerOptions};
//!
//! let entry = "k:/app/server.js".to_owned();
//! let bundle = ServerBundle {
//!     modules: vec![JsModule {
//!         specifier: entry.clone(),
//!         source: r#"
//!             import { serve } from "k:runtime/server";
//!             await serve(async (request) => ({
//!                 status: 200,
//!                 headers: [["content-type", "text/plain"]],
//!                 body: `hello from ${request.path}`,
//!             }));
//!         "#
//!         .to_owned(),
//!     }],
//!     entry,
//!     rpc_paths: Default::default(),
//! };
//! let mut options = ServerOptions::new("127.0.0.1:0".parse()?, bundle);
//! options.workers = 1;
//! options.log = Output::Capture;
//! let server = Server::start(options)?;
//!
//! let response = k_server::testing::send(server.local_addr(), "GET", "/hi", &[], b"")?;
//! assert_eq!(response.status, 200);
//! assert_eq!(response.body, b"hello from /hi");
//! server.shutdown()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(clippy::missing_docs_in_private_items)]

pub mod app;
pub mod channels;
pub mod config;
pub mod db;
pub mod engine;
pub mod fakes;
pub mod front;
pub mod http;
pub mod inspector;
pub mod jobs;
pub mod ops;
pub mod outbound;
pub mod process;
pub mod proxy;
pub mod replicas;
pub mod sockets;
pub mod stack;
pub mod testing;
pub mod websocket;

pub use engine::{Engine, EngineError, EngineOptions, JsModule, LogLine, Output};
pub use http::{Server, ServerBundle, ServerError, ServerOptions};
