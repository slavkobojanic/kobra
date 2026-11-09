//! State shared by every worker isolate of one server process
//! (`docs/compiler/server.md`, "Process model" and "Sessions, auth and
//! request forgery").
//!
//! Isolates share no JavaScript state, so everything that must be the same
//! for every worker lives here, behind ops:
//!
//! | State | Ops | Design |
//! |---|---|---|
//! | the resolved configuration and the public origin | `op_k_server_info` | 25-config-and-secrets.md |
//! | sessions: id → user, roles, timestamps | `op_k_session_*` | 26-auth-and-sessions.md |
//! | rate-limit windows: key → recent calls | `op_k_rate_limit` | 26-auth-and-sessions.md |
//! | `@State` of `@Scope(.server)` services, as JSON | `op_k_shared_get` / `op_k_shared_set` | 10-services.md |
//! | keyed broadcasts (`Broadcast<K, V>`) | `op_k_broadcast_*` | 29-server-streaming.md |
//! | presence leases (`Presence<K, M>`) | `op_k_presence_*` | 44-channels.md |
//!
//! **Broadcasts and presence reach other replicas** through
//! [`crate::replicas`] when the database is Postgres ([`ProcessState::start_replicas`]):
//! a publish is delivered here at once and sent to the others with
//! `NOTIFY`, and presence leases live in `k_presence` instead of
//! [`PresenceTable`].
//!
//! **Sessions and rate-limit windows** live in the app's database when the
//! process has one (the stdlib's `k_session` and `k_rate_limit` tables,
//! `k_db::stdlib`), so they survive restarts and are shared by replicas;
//! without a database they live here, in memory. The store's contract
//! (fresh ids, sliding idle timeout, absolute lifetime, immediate
//! revocation, single-use OAuth state bound to the session) is the same
//! either way; the `session_*` and `rate_hit` methods choose.
//!
//! Every lock is held only for a map operation, never across an `await`,
//! and a poisoned lock is recovered (the maps stay consistent because each
//! operation is a single insert, remove or lookup).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::ConfigValue;
use crate::db::ProcessDatabase;

/// How long a session lasts without a request (`K_SESSION_IDLE`).
pub const DEFAULT_SESSION_IDLE: Duration = Duration::from_secs(7 * 24 * 3600);

/// How long a session lasts at most after `signIn` (`K_SESSION_MAX_AGE`).
pub const DEFAULT_SESSION_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// How long an OAuth state issued by `SessionManager.issueState` stays
/// valid: ten minutes.
pub const OAUTH_STATE_LIFETIME: Duration = Duration::from_secs(600);

/// A stored session is marked seen at most this often (ms), so reads
/// don't each write.
pub const SESSION_TOUCH_MS: i64 = 60_000;

/// The hex SHA-256 of a cookie token or an OAuth state: what the store
/// keeps, so the `k_session` table alone never holds a usable session.
#[must_use]
pub fn token_hash(token: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Compares two byte strings in time independent of where they differ.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// How many undelivered elements a broadcast subscriber may fall behind
/// before it is closed (29-server-streaming.md, "Back-pressure").
pub const BROADCAST_BACKLOG: usize = 1000;

/// One session: signed in, or a guest's (created only to bind OAuth
/// state, design 26).
#[derive(Clone, Debug)]
struct SessionRow {
    /// The user id, as the JSON the app's `AuthUserId` type encodes to;
    /// `null` for a guest.
    user: serde_json::Value,
    /// A stable text form of `user`, for `endAll`; `None` for a guest.
    user_key: Option<String>,
    /// The hash of the OAuth state bound to the session and when it
    /// expires.
    oauth: Option<(String, Instant)>,
    /// The role case names.
    roles: Vec<String>,
    /// When `signIn` created the session.
    created: Instant,
    /// The last request that used it.
    last_seen: Instant,
}

/// A session as JavaScript receives it.
#[derive(Clone, PartialEq, Debug, serde::Serialize)]
pub struct SessionView {
    /// The user id as JSON; `null` for a guest session.
    pub user: serde_json::Value,
    /// The role case names.
    pub roles: Vec<String>,
}

/// One subscriber of a keyed broadcast.
#[derive(Debug, Default)]
struct Subscriber {
    /// The broadcast key it listens to.
    key: String,
    /// The session it was opened under, if any (closed by `signOut`).
    session: Option<String>,
    /// Elements not yet taken, as JSON.
    queue: VecDeque<String>,
    /// `true` once closed: by falling too far behind, by its session
    /// ending, or by the stream being dropped.
    closed: bool,
    /// Wakes the task waiting in `op_k_broadcast_next`.
    notify: Arc<tokio::sync::Notify>,
}

/// State shared by every worker of a server process. Cheap to share
/// (`Arc`); every method takes `&self`.
#[derive(Debug)]
pub struct ProcessState {
    /// `K_PUBLIC_ORIGIN`, serialized (`https://shop.example`).
    pub public_origin: String,
    /// The validated configuration, by variable name.
    pub config: BTreeMap<String, ConfigValue>,
    /// The idle timeout of sessions.
    pub session_idle: Duration,
    /// The absolute lifetime of sessions.
    pub session_max_age: Duration,
    /// The sessions, by session id (the in-memory store of v0).
    sessions: Mutex<HashMap<String, SessionRow>>,
    /// The in-memory rate limits: each key's calls within its window, oldest
    /// first.
    rate: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// The pairing of a test's virtual time with an instant, set by the
    /// first rate-limit count on virtual time ([`Self::rate_hit`]).
    virtual_origin: Mutex<Option<(i64, Instant)>>,
    /// The `.server` state shared by every worker, by key, as JSON.
    shared: Mutex<HashMap<String, String>>,
    /// The open broadcast subscribers, by id.
    subscribers: Mutex<HashMap<u64, Subscriber>>,
    /// The last subscriber id this state handed out.
    next_subscriber: Mutex<u64>,
    /// The app's database, when the build has one and `DATABASE_URL`
    /// opened it ([`crate::db`]).
    database: Option<Arc<ProcessDatabase>>,
    /// [`crate::ServerOptions::fetch_origins`].
    fetch_origins: Vec<(String, String)>,
    /// [`crate::ServerOptions::dev_errors`]: a failed request's 500 shows
    /// its stack trace, mapped to `.k` lines.
    pub dev_errors: bool,
    /// The open channels and their sockets ([`crate::sockets`]).
    channels: Arc<crate::sockets::Hub>,
    /// The presence leases of this process, when it has no Postgres.
    presence: Mutex<PresenceTable>,
    /// The last presence lease id this state handed out.
    next_lease: std::sync::atomic::AtomicU64,
    /// Fan-out to the other replicas, once [`ProcessState::start_replicas`]
    /// found a Postgres database.
    replicas: std::sync::OnceLock<Arc<crate::replicas::Replicas>>,
}

/// The presence leases of one process (44-channels.md, "Presence"), when
/// they aren't in Postgres: per key, the members in the order they
/// arrived, each with how many leases hold it.
#[derive(Debug, Default)]
struct PresenceTable {
    /// Each lease's key and member (as its codec JSON).
    leases: HashMap<u64, (String, String)>,
    /// Per key, the members present and their lease counts, in arrival
    /// order.
    keys: HashMap<String, Vec<(String, usize)>>,
}

/// One presence key with its members (codec JSON) and their lease counts,
/// as `k dev`'s inspector lists it ([`ProcessState::presence_snapshot`]).
pub type PresenceKey = (String, Vec<(String, usize)>);

/// A change of presence, as `PresenceChange`'s codec writes it
/// (`{"case": "joined", "_0": member}`); `member` is JSON text.
#[must_use]
pub fn presence_change(case: &str, member: &str) -> String {
    format!("{{\"case\":\"{case}\",\"_0\":{member}}}")
}

impl PresenceTable {
    /// Adds lease `id`; `true` when `member` wasn't present under `key`.
    fn join(&mut self, id: u64, key: &str, member: &str) -> bool {
        self.leases.insert(id, (key.to_owned(), member.to_owned()));
        let members = self.keys.entry(key.to_owned()).or_default();
        if let Some(m) = members.iter_mut().find(|(m, _)| m == member) {
            m.1 += 1;
            false
        } else {
            members.push((member.to_owned(), 1));
            true
        }
    }

    /// Ends lease `id`: its key and member when that was the member's last
    /// lease under the key.
    fn leave(&mut self, id: u64) -> Option<(String, String)> {
        let (key, member) = self.leases.remove(&id)?;
        let members = self.keys.get_mut(&key)?;
        let i = members.iter().position(|(m, _)| *m == member)?;
        members[i].1 -= 1;
        if members[i].1 > 0 {
            return None;
        }
        members.remove(i);
        if members.is_empty() {
            self.keys.remove(&key);
        }
        Some((key, member))
    }

    /// Replaces lease `id`'s member: the key and the change to announce,
    /// as [`presence_change`] arguments.
    fn update(&mut self, id: u64, member: &str) -> Option<(String, &'static str, String)> {
        let (key, old) = self.leases.get(&id).cloned()?;
        if old == member {
            return None;
        }
        self.leases.insert(id, (key.clone(), member.to_owned()));
        let members = self.keys.get_mut(&key)?;
        let i = members.iter().position(|(m, _)| *m == old)?;
        let exists = members.iter().position(|(m, _)| m == member);
        members[i].1 -= 1;
        let old_gone = members[i].1 == 0;
        match (old_gone, exists) {
            (true, None) => {
                // The member changed in place: it keeps its arrival order.
                members[i] = (member.to_owned(), 1);
                Some((key, "updated", member.to_owned()))
            }
            (true, Some(j)) => {
                members[j].1 += 1;
                members.remove(i);
                Some((key, "left", old))
            }
            (false, None) => {
                members.push((member.to_owned(), 1));
                Some((key, "joined", member.to_owned()))
            }
            (false, Some(j)) => {
                members[j].1 += 1;
                None
            }
        }
    }

    /// The members under `key` in arrival order.
    fn members(&self, key: &str) -> Vec<String> {
        self.keys
            .get(key)
            .map(|ms| ms.iter().map(|(m, _)| m.clone()).collect())
            .unwrap_or_default()
    }
}

impl Default for ProcessState {
    fn default() -> ProcessState {
        ProcessState::new("http://localhost".to_owned(), BTreeMap::new())
    }
}

/// The next broadcast subscriber id, shared by every [`ProcessState`] of
/// the program (ids start at 1).
static NEXT_SUBSCRIBER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Locks `m`, recovering the data if a panicking thread poisoned it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 32 bytes from the operating system's CSPRNG as base64url without
/// padding (43 characters): a session id or an OAuth state.
///
/// # Errors
///
/// The operating system refused to provide randomness (a host failure the
/// caller turns into a panic of the request).
pub fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(base64url(&bytes))
}

/// Encodes bytes as base64url without padding.
#[must_use]
pub fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..chars {
            let index = (n >> (18 - 6 * i)) & 63;
            out.push(char::from(ALPHABET[index as usize]));
        }
    }
    out
}

impl ProcessState {
    /// State for a process serving `public_origin` with `config`. The
    /// session lifetimes are `K_SESSION_IDLE` and `K_SESSION_MAX_AGE` from
    /// `config` (the stdlib's `SessionConfig`, resolved at startup) when
    /// present, else the defaults ([`DEFAULT_SESSION_IDLE`],
    /// [`DEFAULT_SESSION_MAX_AGE`]).
    #[must_use]
    pub fn new(public_origin: String, config: BTreeMap<String, ConfigValue>) -> ProcessState {
        let lifetime = |name: &str, default: Duration| match config.get(name) {
            Some(ConfigValue::Duration(ms)) if ms.is_finite() && *ms > 0.0 => {
                Duration::from_secs_f64(ms / 1000.0)
            }
            _ => default,
        };
        let session_idle = lifetime("K_SESSION_IDLE", DEFAULT_SESSION_IDLE);
        let session_max_age = lifetime("K_SESSION_MAX_AGE", DEFAULT_SESSION_MAX_AGE);
        ProcessState {
            public_origin,
            config,
            session_idle,
            session_max_age,
            sessions: Mutex::new(HashMap::new()),
            rate: Mutex::new(HashMap::new()),
            virtual_origin: Mutex::new(None),
            shared: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber: Mutex::new(0),
            database: None,
            fetch_origins: Vec::new(),
            dev_errors: false,
            channels: Arc::new(crate::sockets::Hub::default()),
            presence: Mutex::new(PresenceTable::default()),
            next_lease: std::sync::atomic::AtomicU64::new(1),
            replicas: std::sync::OnceLock::new(),
        }
    }

    /// Starts fan-out to the other replicas when the process's database is
    /// Postgres ([`crate::replicas`]): broadcasts and presence then reach
    /// every replica sharing the database. Without Postgres it does
    /// nothing, and both stay in this process. Idempotent.
    pub fn start_replicas(self: &Arc<Self>, timings: crate::replicas::ReplicaTimings) {
        let Some(db) = self.database().filter(|d| d.is_postgres()) else {
            return;
        };
        if self.replicas.get().is_some() {
            return;
        }
        let replicas = crate::replicas::Replicas::new(db, Arc::downgrade(self), timings);
        if self.replicas.set(Arc::clone(&replicas)).is_ok() {
            replicas.start();
        }
    }

    /// The fan-out to other replicas, once started.
    #[must_use]
    pub fn replicas(&self) -> Option<Arc<crate::replicas::Replicas>> {
        self.replicas.get().cloned()
    }

    /// This state with `database` as the process's database.
    #[must_use]
    pub fn with_database(mut self, database: Arc<ProcessDatabase>) -> ProcessState {
        self.database = Some(database);
        self
    }

    /// This state with `origins` replacing the origins of outgoing
    /// requests ([`crate::ServerOptions::fetch_origins`]).
    #[must_use]
    pub fn with_fetch_origins(mut self, origins: Vec<(String, String)>) -> ProcessState {
        self.fetch_origins = origins;
        self
    }

    /// Enforces `limits` on channels ([`crate::sockets::ChannelLimits`]).
    #[must_use]
    pub fn with_channel_limits(mut self, limits: crate::sockets::ChannelLimits) -> ProcessState {
        self.channels = Arc::new(crate::sockets::Hub::new(limits));
        self
    }

    /// The process's channels ([`crate::sockets::Hub`]).
    pub fn channels(&self) -> Arc<crate::sockets::Hub> {
        Arc::clone(&self.channels)
    }

    /// The process's metrics ([`crate::metrics`]), which its channels'
    /// hub records into.
    #[must_use]
    pub fn metrics(&self) -> Arc<crate::metrics::Metrics> {
        Arc::clone(&self.channels.metrics)
    }

    /// Every presence key of this process with its members (codec JSON)
    /// and their lease counts, sorted by key, for `k dev`'s inspector;
    /// `None` when presence lives in Postgres.
    #[must_use]
    pub fn presence_snapshot(&self) -> Option<Vec<PresenceKey>> {
        if self.replicas.get().is_some() {
            return None;
        }
        let table = lock(&self.presence);
        let mut keys: Vec<PresenceKey> = table
            .keys
            .iter()
            .filter(|(_, m)| !m.is_empty())
            .map(|(k, m)| (k.clone(), m.clone()))
            .collect();
        keys.sort();
        Some(keys)
    }

    /// Shows failed requests' stack traces on their 500 page
    /// ([`crate::ServerOptions::dev_errors`]).
    #[must_use]
    pub fn with_dev_errors(mut self, on: bool) -> ProcessState {
        self.dev_errors = on;
        self
    }

    /// The `(origin, replacement)` pairs outgoing requests are sent
    /// through ([`crate::outbound::replace_origin`]); empty in production.
    #[must_use]
    pub fn fetch_origins(&self) -> &[(String, String)] {
        &self.fetch_origins
    }

    /// The process's database, if it has one.
    #[must_use]
    pub fn database(&self) -> Option<Arc<ProcessDatabase>> {
        self.database.clone()
    }

    // ------------------------------------------------------------ sessions

    /// The session with `id`, if it exists and is neither idle past the
    /// timeout nor past its lifetime (an expired one is deleted). Marks it
    /// as seen now.
    pub fn session(&self, id: &str) -> Option<SessionView> {
        self.session_at(id, Instant::now())
    }

    /// [`ProcessState::session`] at the instant `now` (tests pass their own
    /// clock).
    fn session_at(&self, id: &str, now: Instant) -> Option<SessionView> {
        let mut sessions = lock(&self.sessions);
        let row = sessions.get_mut(id)?;
        let idle = now.saturating_duration_since(row.last_seen);
        let age = now.saturating_duration_since(row.created);
        if idle > self.session_idle || age > self.session_max_age {
            sessions.remove(id);
            drop(sessions);
            self.close_session_streams(id);
            return None;
        }
        row.last_seen = now;
        Some(SessionView {
            user: row.user.clone(),
            roles: row.roles.clone(),
        })
    }

    /// Starts a session for `user` and returns its new id. Never reuses an
    /// id (`signIn` always issues a fresh one, so a planted id is never
    /// adopted).
    ///
    /// # Errors
    ///
    /// No randomness was available.
    pub fn create_session(
        &self,
        user: serde_json::Value,
        roles: Vec<String>,
    ) -> Result<String, String> {
        let user_key = (!user.is_null()).then(|| user.to_string());
        let now = Instant::now();
        let mut sessions = lock(&self.sessions);
        loop {
            let id = random_token()?;
            if sessions.contains_key(&id) {
                continue;
            }
            sessions.insert(
                id.clone(),
                SessionRow {
                    user,
                    user_key,
                    oauth: None,
                    roles,
                    created: now,
                    last_seen: now,
                },
            );
            return Ok(id);
        }
    }

    /// Deletes the session `id` and closes the streams opened under it.
    /// Does nothing for an unknown id.
    pub fn delete_session(&self, id: &str) {
        lock(&self.sessions).remove(id);
        self.close_session_streams(id);
    }

    /// Deletes every session of the user whose id encodes to `user`.
    /// Returns how many were deleted.
    pub fn end_all_sessions(&self, user: &serde_json::Value) -> usize {
        let key = user.to_string();
        let ended: Vec<String> = {
            let mut sessions = lock(&self.sessions);
            let ids: Vec<String> = sessions
                .iter()
                .filter(|(_, r)| r.user_key.as_deref() == Some(key.as_str()))
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                sessions.remove(id);
            }
            ids
        };
        for id in &ended {
            self.close_session_streams(id);
        }
        ended.len()
    }

    /// Binds a fresh OAuth state to session `id` (memory store) and
    /// returns it, or `None` when there is no such session.
    ///
    /// # Errors
    ///
    /// No randomness was available.
    fn issue_state_memory(&self, id: &str) -> Result<Option<String>, String> {
        let state = random_token()?;
        let mut sessions = lock(&self.sessions);
        let Some(row) = sessions.get_mut(id) else {
            return Ok(None);
        };
        row.oauth = Some((token_hash(&state), Instant::now() + OAUTH_STATE_LIFETIME));
        Ok(Some(state))
    }

    /// Takes the OAuth state of session `id` (memory store): `true` once
    /// for the state last issued to it, within its lifetime.
    fn consume_state_memory(&self, id: &str, state: &str) -> bool {
        let mut sessions = lock(&self.sessions);
        let Some(row) = sessions.get_mut(id) else {
            return false;
        };
        let Some((hash, expires)) = row.oauth.take() else {
            return false;
        };
        constant_time_eq(hash.as_bytes(), token_hash(state).as_bytes()) && Instant::now() <= expires
    }

    // ------------------------------------------------------- the store

    /// The session of cookie token `token`: from the `k_session` table when
    /// the process has a database (shared by replicas and kept across
    /// restarts), else from memory. Expired sessions are deleted; a live
    /// one is marked seen (in the table at most once a minute).
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_load(&self, token: &str) -> Result<Option<SessionView>, String> {
        let Some(db) = self.database() else {
            return Ok(self.session(token));
        };
        let id = token_hash(token);
        let Some((user, roles, created, seen)) =
            db.session_row(&id).await.map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let now = crate::jobs::now_ms();
        let idle = i64::try_from(self.session_idle.as_millis()).unwrap_or(i64::MAX);
        let max_age = i64::try_from(self.session_max_age.as_millis()).unwrap_or(i64::MAX);
        if now.saturating_sub(seen) > idle || now.saturating_sub(created) > max_age {
            db.session_statement(
                k_db::stdlib::session_sql::DELETE,
                vec![k_db::Value::Text(id)],
            )
            .await
            .map_err(|e| e.to_string())?;
            self.close_session_streams(token);
            return Ok(None);
        }
        if now.saturating_sub(seen) >= SESSION_TOUCH_MS {
            db.session_statement(
                k_db::stdlib::session_sql::TOUCH,
                vec![k_db::Value::Text(id), k_db::Value::Int(now)],
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        let user = match user {
            Some(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        };
        let roles = serde_json::from_str(&roles).unwrap_or_default();
        Ok(Some(SessionView { user, roles }))
    }

    /// Starts a session for `user` (`null`: a guest session) and returns
    /// its new cookie token. Never reuses a token.
    ///
    /// # Errors
    ///
    /// No randomness, or a database failure.
    pub async fn session_create(
        &self,
        user: serde_json::Value,
        roles: Vec<String>,
    ) -> Result<String, String> {
        let Some(db) = self.database() else {
            return self.create_session(user, roles);
        };
        let token = random_token()?;
        let who = (!user.is_null()).then(|| (user.to_string(), user.to_string()));
        let roles = serde_json::to_string(&roles).map_err(|e| e.to_string())?;
        db.session_insert(&token_hash(&token), who, roles, crate::jobs::now_ms())
            .await
            .map_err(|e| e.to_string())?;
        Ok(token)
    }

    /// Deletes the session of `token` and closes the streams opened under
    /// it.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_delete(&self, token: &str) -> Result<(), String> {
        if let Some(db) = self.database() {
            db.session_statement(
                k_db::stdlib::session_sql::DELETE,
                vec![k_db::Value::Text(token_hash(token))],
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        self.delete_session(token);
        Ok(())
    }

    /// Deletes every session of the user whose id encodes to `user`, on
    /// every replica, and closes their streams here. Returns how many.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_end_all(&self, user: &serde_json::Value) -> Result<usize, String> {
        let Some(db) = self.database() else {
            return Ok(self.end_all_sessions(user));
        };
        let key = k_db::Value::Text(user.to_string());
        let ids: BTreeSet<String> = db
            .session_query(k_db::stdlib::session_sql::OF_USER, vec![key.clone()])
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|r| match r.into_iter().next() {
                Some(k_db::Value::Text(t)) => Some(t),
                _ => None,
            })
            .collect();
        db.session_statement(k_db::stdlib::session_sql::DELETE_USER, vec![key])
            .await
            .map_err(|e| e.to_string())?;
        self.close_sessions_here(&ids);
        if let Some(r) = self.replicas.get() {
            r.sessions_ended(ids.iter().cloned().collect());
        }
        Ok(ids.len())
    }

    /// Binds a fresh OAuth state to the session of `token` and returns it
    /// (`SessionManager.issueState`), or `None` without such a session.
    /// It is single-use and expires after [`OAUTH_STATE_LIFETIME`].
    ///
    /// # Errors
    ///
    /// No randomness, or a database failure.
    pub async fn session_issue_state(&self, token: &str) -> Result<Option<String>, String> {
        let Some(db) = self.database() else {
            return self.issue_state_memory(token);
        };
        let state = random_token()?;
        let expires = crate::jobs::now_ms()
            + i64::try_from(OAUTH_STATE_LIFETIME.as_millis()).unwrap_or(i64::MAX);
        let n = db
            .session_statement(
                k_db::stdlib::session_sql::SET_STATE,
                vec![
                    k_db::Value::Text(token_hash(token)),
                    k_db::Value::Text(token_hash(&state)),
                    k_db::Value::Int(expires),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok((n > 0).then_some(state))
    }

    /// `SessionManager.consumeState`: `true` exactly once for the state
    /// last issued to the session of `token`, before it expires; it is
    /// cleared either way.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_consume_state(&self, token: &str, state: &str) -> Result<bool, String> {
        let Some(db) = self.database() else {
            return Ok(self.consume_state_memory(token, state));
        };
        let id = token_hash(token);
        let rows = db
            .session_query(
                k_db::stdlib::session_sql::GET_STATE,
                vec![k_db::Value::Text(id.clone())],
            )
            .await
            .map_err(|e| e.to_string())?;
        let Some((stored, expires)) = rows.into_iter().next().and_then(|r| match r.as_slice() {
            [k_db::Value::Text(h), k_db::Value::Int(e)] => Some((h.clone(), *e)),
            _ => None,
        }) else {
            return Ok(false);
        };
        let taken = db
            .session_statement(
                k_db::stdlib::session_sql::TAKE_STATE,
                vec![k_db::Value::Text(id), k_db::Value::Text(stored.clone())],
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(taken > 0
            && constant_time_eq(stored.as_bytes(), token_hash(state).as_bytes())
            && crate::jobs::now_ms() <= expires)
    }

    /// Counts one call against `key` (`RateLimit`): in `k_rate_limit` when
    /// the process has a database, so every replica counts against the
    /// same window ([`k_db::stdlib::rate_decision`]), else in memory.
    /// `virtual_now` is a test's virtual time in Unix milliseconds (its
    /// `TestClock`), counted exactly in memory; `None` for the system time.
    /// Returns `0` if allowed, or the whole seconds until a call would be.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn rate_hit(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
        virtual_now: Option<i64>,
    ) -> Result<u64, String> {
        // A test is one process on its virtual clock: its calls are counted
        // exactly, in memory (the database's two weighted windows exist so
        // replicas share a limit; a test has none).
        if let Some(ms) = virtual_now {
            return Ok(self.rate_limit_at(key, limit, window, self.virtual_instant(ms)));
        }
        let Some(db) = self.database() else {
            return Ok(self.rate_limit(key, limit, window));
        };
        let w = i64::try_from(window.as_millis()).unwrap_or(i64::MAX).max(1);
        let now = crate::jobs::now_ms();
        let current = now.div_euclid(w) * w;
        let previous = current - w;
        let key_v = k_db::Value::Text(key.to_owned());
        let rows = db
            .session_query(
                k_db::stdlib::rate_sql::WINDOWS,
                vec![
                    key_v.clone(),
                    k_db::Value::Int(previous),
                    k_db::Value::Int(current),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
        let (mut prev_hits, mut cur_hits) = (0, 0);
        for r in rows {
            if let [k_db::Value::Int(start), k_db::Value::Int(hits)] = r.as_slice() {
                if *start == current {
                    cur_hits = *hits;
                } else {
                    prev_hits = *hits;
                }
            }
        }
        let wait = k_db::stdlib::rate_decision(limit, w, now - current, prev_hits, cur_hits);
        if wait == 0 {
            db.session_statement(
                k_db::stdlib::rate_sql::HIT,
                vec![key_v.clone(), k_db::Value::Int(current)],
            )
            .await
            .map_err(|e| e.to_string())?;
            db.session_statement(
                k_db::stdlib::rate_sql::PRUNE,
                vec![key_v, k_db::Value::Int(previous)],
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(wait)
    }

    // --------------------------------------------------------- rate limits

    /// Counts one call against `key`, allowing `limit` calls in any sliding
    /// `window`. Returns `0` if this call is allowed, or the whole seconds
    /// (at least 1) until the next call would be.
    pub fn rate_limit(&self, key: &str, limit: u32, window: Duration) -> u64 {
        self.rate_limit_at(key, limit, window, Instant::now())
    }

    /// [`ProcessState::rate_limit`] at the instant `now` (tests pass their
    /// own clock).
    fn rate_limit_at(&self, key: &str, limit: u32, window: Duration, now: Instant) -> u64 {
        let mut rate = lock(&self.rate);
        let calls = rate.entry(key.to_owned()).or_default();
        while calls
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= window)
        {
            calls.pop_front();
        }
        if calls.len() < limit as usize {
            calls.push_back(now);
            return 0;
        }
        let oldest = calls.front().copied().unwrap_or(now);
        let wait = window.saturating_sub(now.saturating_duration_since(oldest));
        wait.as_secs_f64().ceil().max(1.0) as u64
    }

    /// The instant of virtual time `ms` (a test's clock, Unix milliseconds):
    /// the first virtual time seen is now, later ones are offset from it,
    /// so durations between virtual times are kept.
    fn virtual_instant(&self, ms: i64) -> Instant {
        let mut origin = lock(&self.virtual_origin);
        let (base_ms, base) = *origin.get_or_insert((ms, Instant::now()));
        let delta = Duration::from_millis(ms.abs_diff(base_ms));
        if ms >= base_ms {
            base + delta
        } else {
            base.checked_sub(delta).unwrap_or(base)
        }
    }

    // ------------------------------------------------- `.server` state

    /// The JSON stored under `key`, if any.
    pub fn shared_get(&self, key: &str) -> Option<String> {
        lock(&self.shared).get(key).cloned()
    }

    /// Stores `json` under `key`.
    pub fn shared_set(&self, key: &str, json: String) {
        lock(&self.shared).insert(key.to_owned(), json);
    }

    // ---------------------------------------------------------- broadcasts

    /// Subscribes to broadcast `key`; the subscription closes when the
    /// session `session` ends. Returns the subscriber id and its wake-up
    /// handle.
    pub fn subscribe(
        &self,
        key: String,
        session: Option<String>,
    ) -> (u64, Arc<tokio::sync::Notify>) {
        // Unique across every process state of the program, not only this
        // one: a stream a test of `k test` left open keeps reading after
        // the next test's fresh state replaced this one, and must never
        // read a subscription of that test.
        let id = NEXT_SUBSCRIBER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *lock(&self.next_subscriber) = id;
        let notify = Arc::new(tokio::sync::Notify::new());
        lock(&self.subscribers).insert(
            id,
            Subscriber {
                key,
                session,
                queue: VecDeque::new(),
                closed: false,
                notify: Arc::clone(&notify),
            },
        );
        (id, notify)
    }

    /// Publishes `json` to every subscriber of `key`: here at once, and on
    /// the other replicas through [`crate::replicas`] when it runs.
    pub fn publish(&self, key: &str, json: &str) {
        self.deliver(key, json);
        if let Some(r) = self.replicas.get() {
            r.publish(key, json);
        }
    }

    /// Delivers `json` to every subscriber of `key` in this process. A
    /// subscriber with [`BROADCAST_BACKLOG`] undelivered elements is
    /// closed instead.
    pub fn deliver(&self, key: &str, json: &str) {
        let mut subscribers = lock(&self.subscribers);
        for s in subscribers
            .values_mut()
            .filter(|s| s.key == key && !s.closed)
        {
            if s.queue.len() >= BROADCAST_BACKLOG {
                s.closed = true;
                s.queue.clear();
            } else {
                s.queue.push_back(json.to_owned());
            }
            s.notify.notify_one();
        }
    }

    /// Takes the next element of subscriber `id`: `Some(Some(json))` for an
    /// element, `Some(None)` once it is closed, `None` when nothing is
    /// queued yet (wait on the notify handle and ask again).
    pub fn take(&self, id: u64) -> Option<Option<String>> {
        let mut subscribers = lock(&self.subscribers);
        let Some(s) = subscribers.get_mut(&id) else {
            return Some(None);
        };
        if s.closed {
            subscribers.remove(&id);
            return Some(None);
        }
        s.queue.pop_front().map(Some)
    }

    /// The wake-up handle of subscriber `id`.
    pub fn notifier(&self, id: u64) -> Option<Arc<tokio::sync::Notify>> {
        lock(&self.subscribers)
            .get(&id)
            .map(|s| Arc::clone(&s.notify))
    }

    /// Removes subscriber `id` (its stream was dropped).
    pub fn unsubscribe(&self, id: u64) {
        lock(&self.subscribers).remove(&id);
    }

    /// Closes every broadcast subscriber opened under `session` and wakes it,
    /// so its stream ends (the session ended or expired), here and on the
    /// other replicas.
    fn close_session_streams(&self, session: &str) {
        let mut subscribers = lock(&self.subscribers);
        for s in subscribers
            .values_mut()
            .filter(|s| s.session.as_deref() == Some(session))
        {
            s.closed = true;
            s.queue.clear();
            s.notify.notify_one();
        }
        drop(subscribers);
        if let Some(r) = self.replicas.get() {
            r.sessions_ended(vec![token_hash(session)]);
        }
    }

    /// Closes the subscribers opened under the sessions whose token hashes
    /// are `hashes`, in this process only: another replica ended them.
    pub fn close_sessions_here(&self, hashes: &BTreeSet<String>) {
        let mut subscribers = lock(&self.subscribers);
        for s in subscribers.values_mut().filter(|s| {
            s.session
                .as_deref()
                .is_some_and(|t| hashes.contains(&token_hash(t)))
        }) {
            s.closed = true;
            s.queue.clear();
            s.notify.notify_one();
        }
    }

    // ------------------------------------------------------------ presence

    /// Adds `member` (its codec JSON) under `key` and returns the lease's
    /// id; the member's first lease announces `joined` on `key`'s
    /// broadcast. With Postgres the lease is stored by the database task,
    /// in the order of the calls, and announced from there.
    pub fn presence_join(&self, key: &str, member: &str) -> u64 {
        let id = self
            .next_lease
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(r) = self.replicas.get() {
            r.presence_join(id, key, member);
            return id;
        }
        let joined = lock(&self.presence).join(id, key, member);
        if joined {
            self.publish(key, &presence_change("joined", member));
        }
        id
    }

    /// Replaces lease `id`'s member and announces the change.
    pub fn presence_update(&self, id: u64, member: &str) {
        if let Some(r) = self.replicas.get() {
            r.presence_update(id, member);
            return;
        }
        let change = lock(&self.presence).update(id, member);
        if let Some((key, case, m)) = change {
            self.publish(&key, &presence_change(case, &m));
        }
    }

    /// Ends lease `id`; the member's last lease announces `left`.
    /// Idempotent.
    pub fn presence_leave(&self, id: u64) {
        if let Some(r) = self.replicas.get() {
            r.presence_leave(id);
            return;
        }
        let left = lock(&self.presence).leave(id);
        if let Some((key, member)) = left {
            self.publish(&key, &presence_change("left", &member));
        }
    }

    /// The members under `key` (codec JSON), each once, in arrival order.
    pub async fn presence_members(&self, key: &str) -> Vec<String> {
        if let Some(r) = self.replicas.get() {
            return r.presence_members(key).await;
        }
        lock(&self.presence).members(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_presence_snapshot_lists_each_key_with_its_members_and_leases() {
        let p = ProcessState::new("http://localhost".to_owned(), BTreeMap::new());
        let first = p.presence_join("room|b", "\"ana\"");
        p.presence_join("room|b", "\"ana\"");
        p.presence_join("room|a", "\"bo\"");
        let gone = p.presence_join("room|c", "\"cy\"");
        p.presence_leave(first);
        p.presence_leave(gone);
        assert_eq!(
            p.presence_snapshot(),
            Some(vec![
                ("room|a".to_owned(), vec![("\"bo\"".to_owned(), 1)]),
                ("room|b".to_owned(), vec![("\"ana\"".to_owned(), 1)]),
            ])
        );
    }

    #[test]
    fn subscriber_ids_differ_across_process_states() {
        let a = ProcessState::default();
        let b = ProcessState::default();
        let (x, _) = a.subscribe("k".into(), None);
        let (y, _) = b.subscribe("k".into(), None);
        assert_ne!(x, y, "a fresh state never reuses another state's id");
        b.publish("k", "1");
        assert_eq!(
            a.take(x),
            None,
            "the old state's subscriber sees nothing of the new one"
        );
    }

    #[test]
    fn tokens_are_43_base64url_characters_and_differ() {
        let a = random_token().expect("randomness");
        let b = random_token().expect("randomness");
        assert_eq!(a.len(), 43);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_ne!(a, b);
    }

    #[test]
    fn base64url_matches_the_rfc_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn sessions_are_created_read_and_revoked() {
        let p = ProcessState::default();
        let user = serde_json::json!({"name": "ada"});
        let id = p
            .create_session(user.clone(), vec!["admin".into()])
            .expect("created");
        let s = p.session(&id).expect("exists");
        assert_eq!(s.user, user);
        assert_eq!(s.roles, vec!["admin".to_owned()]);
        assert!(p.session("forged").is_none());
        p.delete_session(&id);
        assert!(p.session(&id).is_none());
    }

    #[test]
    fn end_all_ends_every_session_of_the_user_only() {
        let p = ProcessState::default();
        let ada = serde_json::json!({"name": "ada"});
        let a = p.create_session(ada.clone(), vec![]).expect("a");
        let b = p.create_session(ada.clone(), vec![]).expect("b");
        let c = p
            .create_session(serde_json::json!({"name": "bob"}), vec![])
            .expect("c");
        assert_eq!(p.end_all_sessions(&ada), 2);
        assert!(p.session(&a).is_none() && p.session(&b).is_none());
        assert!(p.session(&c).is_some());
    }

    #[test]
    fn idle_and_absolute_timeouts_expire_sessions() {
        let p = ProcessState {
            session_idle: Duration::from_secs(10),
            session_max_age: Duration::from_secs(25),
            ..ProcessState::default()
        };
        let id = p
            .create_session(serde_json::json!(1), vec![])
            .expect("created");
        let start = Instant::now();
        assert!(p.session_at(&id, start + Duration::from_secs(9)).is_some());
        assert!(p.session_at(&id, start + Duration::from_secs(18)).is_some());
        assert!(p.session_at(&id, start + Duration::from_secs(26)).is_none());
        let id = p
            .create_session(serde_json::json!(2), vec![])
            .expect("created");
        let start = Instant::now();
        assert!(p.session_at(&id, start + Duration::from_secs(11)).is_none());
    }

    #[test]
    fn rate_limits_slide_and_report_retry_after() {
        let p = ProcessState::default();
        let w = Duration::from_secs(60);
        let t = Instant::now();
        assert_eq!(p.rate_limit_at("k", 2, w, t), 0);
        assert_eq!(p.rate_limit_at("k", 2, w, t + Duration::from_secs(1)), 0);
        let wait = p.rate_limit_at("k", 2, w, t + Duration::from_secs(2));
        assert!((58..=60).contains(&wait), "{wait}");
        assert_eq!(p.rate_limit_at("other", 2, w, t), 0);
        assert_eq!(p.rate_limit_at("k", 2, w, t + Duration::from_secs(61)), 0);
        assert_eq!(p.rate_limit_at("z", 0, w, t), 60);
    }

    #[test]
    fn broadcasts_deliver_in_order_and_close_slow_subscribers() {
        let p = ProcessState::default();
        let (a, _) = p.subscribe("room".into(), None);
        let (b, _) = p.subscribe("other".into(), Some("sid".into()));
        p.publish("room", "1");
        p.publish("room", "2");
        p.publish("other", "x");
        assert_eq!(p.take(a), Some(Some("1".into())));
        assert_eq!(p.take(a), Some(Some("2".into())));
        assert_eq!(p.take(a), None);
        p.delete_session("sid");
        assert_eq!(p.take(b), Some(None));
        for i in 0..=BROADCAST_BACKLOG {
            p.publish("room", &i.to_string());
        }
        assert_eq!(p.take(a), Some(None));
    }

    #[test]
    fn shared_state_round_trips() {
        let p = ProcessState::default();
        assert_eq!(p.shared_get("a"), None);
        p.shared_set("a", "1".into());
        assert_eq!(p.shared_get("a").as_deref(), Some("1"));
    }

    #[test]
    fn oauth_state_in_memory_is_bound_and_single_use() {
        let p = ProcessState::default();
        let guest = p
            .create_session(serde_json::Value::Null, vec![])
            .expect("guest");
        assert_eq!(
            p.session(&guest).map(|s| s.user),
            Some(serde_json::Value::Null)
        );
        let state = p
            .issue_state_memory(&guest)
            .expect("random")
            .expect("a session");
        assert!(!p.consume_state_memory("other", &state));
        assert!(p.consume_state_memory(&guest, &state));
        assert!(!p.consume_state_memory(&guest, &state), "single use");
        let state = p
            .issue_state_memory(&guest)
            .expect("random")
            .expect("a session");
        assert!(!p.consume_state_memory(&guest, "forged"));
        assert!(
            !p.consume_state_memory(&guest, &state),
            "a wrong guess clears it"
        );
        assert_eq!(p.issue_state_memory("gone").expect("random"), None);
        // A guest session is nobody's: `endAll` never ends it.
        assert_eq!(p.end_all_sessions(&serde_json::Value::Null), 0);
        assert!(p.session(&guest).is_some());
    }

    #[test]
    fn token_hashes_are_hex_sha256_and_comparisons_constant_time() {
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"sama"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
