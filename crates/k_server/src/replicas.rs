//! Broadcasts, presence and session ends between replicas over Postgres
//! (`docs/design/44-channels.md`, "Server isolates and replicas";
//! `docs/compiler/server.md` §20, "Presence and replicas").
//!
//! Replicas behind a load balancer share the app's database and nothing
//! else. With Postgres, [`crate::process::ProcessState::start_replicas`]
//! starts this module, and then:
//!
//! - **A publish** is delivered in its own process at once and sent to the
//!   others with `NOTIFY k_broadcast` on the statement connection: one
//!   channel per app (each app has its own database). The payload is
//!   `{"p": tag, "k": key, "v": element}`; one over [`NOTIFY_INLINE_LIMIT`]
//!   bytes is stored in `k_broadcast_spill` and the notification carries
//!   `{"p": tag, "s": id}` instead. Spilled rows older than
//!   [`SPILL_LIFETIME_MS`] are swept.
//! - **A listening connection** of its own (`LISTEN k_broadcast`,
//!   [`k_db::PostgresListener`]) delivers the other replicas' elements to
//!   this process's subscribers, in the order they arrive, skipping its
//!   own. A lost connection is opened again after
//!   [`ReplicaTimings::reconnect`]; what was notified meanwhile is lost,
//!   which channels already tolerate (a restart brings a snapshot).
//! - **Presence leases** live in `k_presence`, one row per lease with its
//!   process's tag and an expiry. Each process renews its own every
//!   [`ReplicaTimings::renew`]; every process deletes the leases not
//!   renewed for [`ReplicaTimings::expiry`] (their process crashed) and
//!   announces the members that left. Changes of one key are serialized
//!   across replicas with an advisory lock, so a member's first lease
//!   announces `joined` and its last `left` exactly once.
//! - **A session ended** on one replica (sign-out, `endAll`, expiry) closes
//!   its streams and channels on the others: `{"p": tag, "ended":
//!   [token hashes]}`. Only hashes travel, never a cookie token.
//!
//! Every database call runs on the `k-db` runtime ([`ProcessDatabase`]),
//! detached from the calling isolate: one process's publishes and
//! presence changes keep their order, because detached work starts in
//! the order it was spawned and takes the connection's lease in that
//! order. Failures are logged and dropped: fan-out is best-effort, as
//! `NOTIFY` itself is.

use std::collections::BTreeSet;
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::db::ProcessDatabase;
use crate::process::{ProcessState, presence_change};

/// The `LISTEN`/`NOTIFY` channel of every app's broadcasts.
pub const NOTIFY_CHANNEL: &str = "k_broadcast";

/// The largest notification payload sent inline, in bytes, under
/// Postgres's 8,000-byte limit; a larger one is spilled.
pub const NOTIFY_INLINE_LIMIT: usize = 7000;

/// How long [`Replicas::start`] waits for the first `LISTEN`.
pub const FIRST_LISTEN_WAIT: Duration = Duration::from_secs(5);

/// How long a spilled payload is kept: a minute.
pub const SPILL_LIFETIME_MS: i64 = 60_000;

/// The intervals of presence and the listening connection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReplicaTimings {
    /// How often a process renews its presence leases (10 s).
    pub renew: Duration,
    /// How long a lease lives after its last renewal (30 s).
    pub expiry: Duration,
    /// How often each process deletes expired leases and old spills (10 s).
    pub sweep: Duration,
    /// The wait before opening a lost listening connection again (1 s).
    pub reconnect: Duration,
}

impl Default for ReplicaTimings {
    fn default() -> ReplicaTimings {
        ReplicaTimings {
            renew: Duration::from_secs(10),
            expiry: Duration::from_secs(30),
            sweep: Duration::from_secs(10),
            reconnect: Duration::from_secs(1),
        }
    }
}

/// The fan-out of one process to the replicas sharing its Postgres
/// database.
pub struct Replicas {
    /// The process's database.
    db: Arc<ProcessDatabase>,
    /// The process whose subscribers this delivers to.
    process: Weak<ProcessState>,
    /// This process's tag: in its notifications (so it skips its own) and
    /// on its presence rows (so it renews only its own).
    tag: String,
    /// The intervals.
    timings: ReplicaTimings,
    /// The last spill id handed out.
    next_spill: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for Replicas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Replicas({})", self.tag)
    }
}

/// Milliseconds of `d`, for the database's `BIGINT` times.
fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Logs a failure of the fan-out (`k_db` errors never carry the URL).
fn warn(what: &str, error: &dyn std::fmt::Display) {
    eprintln!(
        "{}",
        serde_json::json!({"level": "warn", "message": format!("replicas: {what}: {error}")})
    );
}

impl Replicas {
    /// The fan-out of `process` through `db`, not started yet.
    #[must_use]
    pub fn new(
        db: Arc<ProcessDatabase>,
        process: Weak<ProcessState>,
        timings: ReplicaTimings,
    ) -> Arc<Replicas> {
        let tag = crate::process::random_token()
            .map(|t| t[..16].to_owned())
            .unwrap_or_else(|_| format!("p{}", std::process::id()));
        Arc::new(Replicas {
            db,
            process,
            tag,
            timings,
            next_spill: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// This process's tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Starts the listening connection and the presence timers on the
    /// database runtime, and waits (at most [`FIRST_LISTEN_WAIT`]) until
    /// the first `LISTEN` succeeded or failed, so a replica that starts
    /// serving already hears the others. The tasks hold the fan-out
    /// weakly, so they stop when the process state (and with it the
    /// database runtime) goes. Never call it on the database runtime.
    pub fn start(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        self.db.spawn_detached(listen(me.clone(), Some(ready_tx)));
        self.db.spawn_detached(maintain(me));
        let _ = ready_rx.recv_timeout(FIRST_LISTEN_WAIT);
    }

    /// Sends `json`, published on `key` here, to the other replicas.
    pub fn publish(&self, key: &str, json: &str) {
        let payload = serde_json::json!({"p": self.tag, "k": key, "v": json}).to_string();
        self.notify(payload);
    }

    /// Tells the other replicas that the sessions with these token hashes
    /// ended.
    pub fn sessions_ended(&self, hashes: Vec<String>) {
        if hashes.is_empty() {
            return;
        }
        let payload = serde_json::json!({"p": self.tag, "ended": hashes}).to_string();
        self.notify(payload);
    }

    /// `NOTIFY`s `payload`, spilling it when it is too large.
    fn notify(&self, payload: String) {
        let now = crate::jobs::now_ms();
        let (payload, spill) = if payload.len() > NOTIFY_INLINE_LIMIT {
            let n = self
                .next_spill
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let id = format!("{}.{n}", self.tag);
            let small = serde_json::json!({"p": self.tag, "s": id}).to_string();
            (small, Some((id, payload)))
        } else {
            (payload, None)
        };
        let db = Arc::clone(&self.db);
        self.db.spawn_detached(async move {
            if let Err(e) = db.notify(NOTIFY_CHANNEL, payload, spill, now).await {
                warn("NOTIFY failed", &e);
            }
        });
    }

    /// The text of lease `id` in `k_presence`: unique across replicas.
    fn lease(&self, id: u64) -> String {
        format!("{}.{id}", self.tag)
    }

    /// Stores lease `id` and announces `joined` when it is the member's
    /// first.
    pub fn presence_join(&self, id: u64, key: &str, member: &str) {
        let db = Arc::clone(&self.db);
        let process = self.process.clone();
        let (lease, key, member, tag) = (
            self.lease(id),
            key.to_owned(),
            member.to_owned(),
            self.tag.clone(),
        );
        let expiry = ms(self.timings.expiry);
        self.db.spawn_detached(async move {
            let now = crate::jobs::now_ms();
            match db
                .presence_join(&lease, &key, &member, &tag, now, now + expiry)
                .await
            {
                Ok(true) => {
                    if let Some(p) = process.upgrade() {
                        p.publish(&key, &presence_change("joined", &member));
                    }
                }
                Ok(false) => {}
                Err(e) => warn("storing a presence lease failed", &e),
            }
        });
    }

    /// Replaces lease `id`'s member and announces the change.
    pub fn presence_update(&self, id: u64, member: &str) {
        let db = Arc::clone(&self.db);
        let process = self.process.clone();
        let (lease, member) = (self.lease(id), member.to_owned());
        self.db.spawn_detached(async move {
            let now = crate::jobs::now_ms();
            let moved = match db.presence_update(&lease, &member, now).await {
                Ok(Some(m)) => m,
                Ok(None) => return,
                Err(e) => return warn("updating a presence lease failed", &e),
            };
            if moved.old == member {
                return;
            }
            let change = match (moved.old_left == 0, moved.new_count == 1) {
                (true, true) => Some(presence_change("updated", &member)),
                (true, false) => Some(presence_change("left", &moved.old)),
                (false, true) => Some(presence_change("joined", &member)),
                (false, false) => None,
            };
            if let (Some(c), Some(p)) = (change, process.upgrade()) {
                p.publish(&moved.key, &c);
            }
        });
    }

    /// Ends lease `id` and announces `left` when it was the member's last.
    pub fn presence_leave(&self, id: u64) {
        let db = Arc::clone(&self.db);
        let process = self.process.clone();
        let lease = self.lease(id);
        self.db.spawn_detached(async move {
            match db.presence_leave(&lease, crate::jobs::now_ms()).await {
                Ok(Some((key, member))) => {
                    if let Some(p) = process.upgrade() {
                        p.publish(&key, &presence_change("left", &member));
                    }
                }
                Ok(None) => {}
                Err(e) => warn("ending a presence lease failed", &e),
            }
        });
    }

    /// The members under `key` on every replica, in arrival order. It
    /// queues behind this process's earlier presence changes, so a join
    /// just made is in it.
    pub async fn presence_members(&self, key: &str) -> Vec<String> {
        let db = Arc::clone(&self.db);
        let key = key.to_owned();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.db.spawn_detached(async move {
            let members = db.presence_members(&key, crate::jobs::now_ms()).await;
            let _ = tx.send(members);
        });
        match rx.await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                warn("reading presence failed", &e);
                Vec::new()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Handles one notification: another replica's element, spilled
    /// element or ended sessions. Its own are skipped.
    async fn receive(&self, payload: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        if v["p"].as_str() == Some(self.tag.as_str()) {
            return;
        }
        let Some(process) = self.process.upgrade() else {
            return;
        };
        if let Some(hashes) = v["ended"].as_array() {
            let hashes: BTreeSet<String> = hashes
                .iter()
                .filter_map(|h| h.as_str().map(str::to_owned))
                .collect();
            process.close_sessions_here(&hashes);
            return;
        }
        let full;
        let v = match v["s"].as_str() {
            Some(id) => match self.db.spilled(id).await {
                Ok(Some(text)) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(f) => {
                        full = f;
                        &full
                    }
                    Err(_) => return,
                },
                Ok(None) => return,
                Err(e) => return warn("reading a spilled broadcast failed", &e),
            },
            None => &v,
        };
        if let (Some(key), Some(json)) = (v["k"].as_str(), v["v"].as_str()) {
            process.deliver(key, json);
        }
    }
}

/// The listening connection: delivers other replicas' notifications until
/// the fan-out is dropped, reconnecting after a lost connection. `ready`
/// hears when the first attempt finished.
async fn listen(me: Weak<Replicas>, mut ready: Option<std::sync::mpsc::Sender<()>>) {
    loop {
        let Some(r) = me.upgrade() else { return };
        let reconnect = r.timings.reconnect;
        let listener = r.db.listen(NOTIFY_CHANNEL).await;
        drop(r);
        if let Some(tx) = ready.take() {
            let _ = tx.send(());
        }
        match listener {
            Ok(mut l) => {
                while let Some(payload) = l.next().await {
                    let Some(r) = me.upgrade() else { return };
                    r.receive(&payload).await;
                }
            }
            Err(e) => warn("LISTEN failed", &e),
        }
        tokio::time::sleep(reconnect).await;
    }
}

/// Renews this process's presence leases and sweeps expired ones (and old
/// spills), on their intervals, until the fan-out is dropped.
async fn maintain(me: Weak<Replicas>) {
    let Some(r) = me.upgrade() else { return };
    let (renew, sweep) = (r.timings.renew, r.timings.sweep);
    drop(r);
    let mut renew_at = tokio::time::Instant::now() + renew;
    let mut sweep_at = tokio::time::Instant::now() + sweep;
    loop {
        tokio::time::sleep_until(renew_at.min(sweep_at)).await;
        let Some(r) = me.upgrade() else { return };
        let now = tokio::time::Instant::now();
        if now >= renew_at {
            renew_at = now + renew;
            let expires = crate::jobs::now_ms() + ms(r.timings.expiry);
            if let Err(e) = r.db.presence_renew(&r.tag, expires).await {
                warn("renewing presence leases failed", &e);
            }
        }
        if now >= sweep_at {
            sweep_at = now + sweep;
            let at = crate::jobs::now_ms();
            match r.db.presence_sweep(at, at - SPILL_LIFETIME_MS).await {
                Ok(left) => {
                    if let Some(p) = r.process.upgrade() {
                        for (key, member) in left {
                            p.publish(&key, &presence_change("left", &member));
                        }
                    }
                }
                Err(e) => warn("sweeping presence leases failed", &e),
            }
        }
    }
}
