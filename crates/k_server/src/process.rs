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
//!
//! **v0:** sessions live in memory, so they last as long as the process and
//! are not shared between replicas; the `k_session` table of server.md §9
//! replaces this store once the database ops are wired into startup. The
//! store's contract (fresh ids, sliding idle timeout, absolute lifetime,
//! immediate revocation) is the same either way.
//!
//! Every lock is held only for a map operation, never across an `await`,
//! and a poisoned lock is recovered (the maps stay consistent because each
//! operation is a single insert, remove or lookup).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::ConfigValue;
use crate::db::ProcessDatabase;

/// How long a session lasts without a request (`K_SESSION_IDLE`).
pub const DEFAULT_SESSION_IDLE: Duration = Duration::from_secs(7 * 24 * 3600);

/// How long a session lasts at most after `signIn` (`K_SESSION_MAX_AGE`).
pub const DEFAULT_SESSION_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// How many undelivered elements a broadcast subscriber may fall behind
/// before it is closed (29-server-streaming.md, "Back-pressure").
pub const BROADCAST_BACKLOG: usize = 1000;

/// One signed-in session.
#[derive(Clone, Debug)]
struct SessionRow {
    /// The user id, as the JSON the app's `AuthUserId` type encodes to.
    user: serde_json::Value,
    /// A stable text form of `user`, for `endAll`.
    user_key: String,
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
    /// The user id as JSON.
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
    sessions: Mutex<HashMap<String, SessionRow>>,
    rate: Mutex<HashMap<String, VecDeque<Instant>>>,
    shared: Mutex<HashMap<String, String>>,
    subscribers: Mutex<HashMap<u64, Subscriber>>,
    next_subscriber: Mutex<u64>,
    /// The app's database, when the build has one and `DATABASE_URL`
    /// opened it ([`crate::db`]).
    database: Option<Arc<ProcessDatabase>>,
}

impl Default for ProcessState {
    fn default() -> ProcessState {
        ProcessState::new("http://localhost".to_owned(), BTreeMap::new())
    }
}

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
            shared: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber: Mutex::new(0),
            database: None,
        }
    }

    /// This state with `database` as the process's database.
    #[must_use]
    pub fn with_database(mut self, database: Arc<ProcessDatabase>) -> ProcessState {
        self.database = Some(database);
        self
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
        let user_key = user.to_string();
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
                .filter(|(_, r)| r.user_key == key)
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

    // --------------------------------------------------------- rate limits

    /// Counts one call against `key`, allowing `limit` calls in any sliding
    /// `window`. Returns `0` if this call is allowed, or the whole seconds
    /// (at least 1) until the next call would be.
    pub fn rate_limit(&self, key: &str, limit: u32, window: Duration) -> u64 {
        self.rate_limit_at(key, limit, window, Instant::now())
    }

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
        let id = {
            let mut n = lock(&self.next_subscriber);
            *n += 1;
            *n
        };
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

    /// Publishes `json` to every subscriber of `key`. A subscriber with
    /// [`BROADCAST_BACKLOG`] undelivered elements is closed instead.
    pub fn publish(&self, key: &str, json: &str) {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
