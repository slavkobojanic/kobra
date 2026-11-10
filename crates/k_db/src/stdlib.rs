//! The tables the standard library keeps in the app's own database,
//! outside `schema.lock` (`docs/compiler/db.md` §12):
//!
//! | Table | For | Design |
//! |---|---|---|
//! | `k_job` | queued, running and dead background jobs | 30-background-jobs.md |
//! | `k_job_schedule` | the last due time each `@Schedule` job was enqueued for | 30-background-jobs.md, "Schedules" |
//! | `k_session` | sessions: user, roles, timestamps, OAuth state | 26-auth-and-sessions.md |
//! | `k_rate_limit` | per-window hit counts of `RateLimit` guards | 26-auth-and-sessions.md |
//! | `k_presence` | presence leases with Postgres: key, member, process, expiry | 44-channels.md, "Presence" |
//! | `k_broadcast_spill` | broadcast payloads over `NOTIFY`'s limit, for a minute | 44-channels.md, "Server isolates and replicas" |
//!
//! [`tables`] creates them idempotently (`CREATE … IF NOT EXISTS`) after
//! the app's migrations; they are never recorded in `k_migrations`, so the
//! app's migration history stays the app's. A later incompatible change to
//! one of them gets a new table name.
//!
//! This module holds what is the same on every server: the SQL (written
//! once with `$n` placeholders, [`sql`] adapts it to SQLite), the job
//! specs and their schedules ([`JobSpec`], [`Schedule`]), and the backoff
//! of failed attempts ([`backoff_ms`]). Executing it is the server's
//! (`k_server::db`), which owns the connection and its transactions.
//!
//! Every timestamp is milliseconds since the Unix epoch, UTC, as `BIGINT`.

use serde::{Deserialize, Serialize};

use crate::sql::Dialect;

/// The statements that create the stdlib's tables, in order. Each is
/// idempotent; the same text runs on both dialects.
#[must_use]
pub fn tables() -> &'static [&'static str] {
    &[
        "CREATE TABLE IF NOT EXISTS k_job (\
         id TEXT PRIMARY KEY, \
         name TEXT NOT NULL, \
         payload TEXT NOT NULL, \
         status TEXT NOT NULL, \
         attempts BIGINT NOT NULL, \
         run_after BIGINT NOT NULL, \
         locked_until BIGINT, \
         last_error TEXT, \
         created_at BIGINT NOT NULL, \
         updated_at BIGINT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS k_job_due ON k_job (status, run_after)",
        "CREATE TABLE IF NOT EXISTS k_job_schedule (\
         name TEXT PRIMARY KEY, \
         last_due BIGINT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS k_session (\
         id TEXT PRIMARY KEY, \
         user_json TEXT, \
         user_key TEXT, \
         roles TEXT NOT NULL, \
         created_at BIGINT NOT NULL, \
         last_seen BIGINT NOT NULL, \
         oauth_state TEXT, \
         oauth_state_expires BIGINT)",
        "CREATE INDEX IF NOT EXISTS k_session_user ON k_session (user_key)",
        "CREATE TABLE IF NOT EXISTS k_rate_limit (\
         key TEXT NOT NULL, \
         window_start BIGINT NOT NULL, \
         hits BIGINT NOT NULL, \
         PRIMARY KEY (key, window_start))",
        "CREATE TABLE IF NOT EXISTS k_presence (\
         lease TEXT PRIMARY KEY, \
         key TEXT NOT NULL, \
         member TEXT NOT NULL, \
         process TEXT NOT NULL, \
         joined_at BIGINT NOT NULL, \
         expires_at BIGINT NOT NULL)",
        "CREATE INDEX IF NOT EXISTS k_presence_member ON k_presence (key, member)",
        "CREATE INDEX IF NOT EXISTS k_presence_process ON k_presence (process)",
        "CREATE TABLE IF NOT EXISTS k_broadcast_spill (\
         id TEXT PRIMARY KEY, \
         payload TEXT NOT NULL, \
         created_at BIGINT NOT NULL)",
    ]
}

/// `text` (written with Postgres's `$1`, `$2`, … placeholders) for
/// `dialect`: SQLite's `?1`, `?2`, …
#[must_use]
pub fn sql(dialect: Dialect, text: &str) -> String {
    match dialect {
        Dialect::Postgres => text.to_owned(),
        Dialect::Sqlite => {
            let mut out = String::with_capacity(text.len());
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '$' && chars.peek().is_some_and(char::is_ascii_digit) {
                    out.push('?');
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

/// The status of a `k_job` row.
pub mod status {
    /// Waiting for its `run_after`.
    pub const QUEUED: &str = "queued";
    /// Claimed by a worker until `locked_until`.
    pub const RUNNING: &str = "running";
    /// Failed permanently or out of attempts; kept for `jobs list --dead`.
    pub const DEAD: &str = "dead";
}

/// The default attempts of a job without `@Retry`.
pub const DEFAULT_ATTEMPTS: u32 = 5;

/// The default first backoff of a job without `@Retry`: 10 seconds.
pub const DEFAULT_BACKOFF_MS: u64 = 10_000;

/// The longest any backoff grows to: one day.
pub const MAX_BACKOFF_MS: u64 = 24 * 3600 * 1000;

/// One job of the build: its name in the queue, its retry policy and its
/// schedule (from the build manifest).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct JobSpec {
    /// The name its rows carry: `@JobName`, else the name recorded in
    /// `storage.lock`, else derived from the service's name.
    pub name: String,
    /// The service's qualified name, for messages.
    pub declaration: String,
    /// Runs in total, at least 1 (`@Retry(attempts:)`).
    pub attempts: u32,
    /// The wait after the first failed attempt, doubling after each
    /// (`@Retry(backoff:)`), in milliseconds.
    pub backoff_ms: u64,
    /// When it runs on its own, if it has `@Schedule`.
    #[serde(default)]
    pub schedule: Option<Schedule>,
}

/// A typed `@Schedule` (30-background-jobs.md, "Schedules"). Times are
/// UTC.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Schedule {
    /// `.every(1.hour)`: due at every multiple of the interval since the
    /// epoch.
    Every {
        /// The interval in milliseconds (at least one second).
        interval_ms: u64,
    },
    /// `.daily(at: TimeOfDay("03:00"))`.
    Daily {
        /// Minutes after midnight, `0..1440`.
        minute: u32,
    },
    /// `.weekly(on: .monday, at: TimeOfDay("03:00"))`.
    Weekly {
        /// `0` for Monday through `6` for Sunday.
        weekday: u32,
        /// Minutes after midnight, `0..1440`.
        minute: u32,
    },
}

/// One minute in milliseconds.
const MINUTE_MS: i64 = 60_000;
/// One day in milliseconds (schedules are in UTC, which has no DST).
const DAY_MS: i64 = 24 * 60 * MINUTE_MS;
/// One week in milliseconds.
const WEEK_MS: i64 = 7 * DAY_MS;
/// The Unix epoch was a Thursday: Monday 1970-01-05 is 4 days later.
const FIRST_MONDAY_MS: i64 = 4 * DAY_MS;

impl Schedule {
    /// The latest due time at or before `now_ms`. A due time is enqueued
    /// at most once across all replicas (`k_job_schedule`), and is skipped
    /// while an earlier run of the job is unfinished.
    #[must_use]
    pub fn latest_due(self, now_ms: i64) -> i64 {
        match self {
            Schedule::Every { interval_ms } => {
                let i = i64::try_from(interval_ms.max(1000)).unwrap_or(i64::MAX);
                now_ms.div_euclid(i) * i
            }
            Schedule::Daily { minute } => {
                let offset = i64::from(minute.min(1439)) * MINUTE_MS;
                (now_ms - offset).div_euclid(DAY_MS) * DAY_MS + offset
            }
            Schedule::Weekly { weekday, minute } => {
                let offset = FIRST_MONDAY_MS
                    + i64::from(weekday.min(6)) * DAY_MS
                    + i64::from(minute.min(1439)) * MINUTE_MS;
                (now_ms - offset).div_euclid(WEEK_MS) * WEEK_MS + offset
            }
        }
    }
}

/// The wait before attempt `attempt + 1` after attempt `attempt` (1-based)
/// failed.
///
/// It is `base` doubled `attempt - 1` times, capped at
/// [`MAX_BACKOFF_MS`], then scaled by `1 + jitter` where `jitter` is in
/// `-0.2..=0.2` (callers pass a random value), so retries of jobs that
/// failed together spread out.
#[must_use]
pub fn backoff_ms(base: u64, attempt: u32, jitter: f64) -> u64 {
    let doubled = base.saturating_mul(1u64 << attempt.saturating_sub(1).min(32));
    let capped = doubled.min(MAX_BACKOFF_MS);
    let jitter = if jitter.is_finite() {
        jitter.clamp(-0.2, 0.2)
    } else {
        0.0
    };
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let scaled = (capped as f64 * (1.0 + jitter)).round() as u64;
    scaled.max(1)
}

/// Statements on `k_job`, with `$n` placeholders ([`sql`]).
pub mod job_sql {
    /// `$1` id, `$2` name, `$3` payload, `$4` run after, `$5` now.
    pub const INSERT: &str = "INSERT INTO k_job (id, name, payload, status, attempts, run_after, locked_until, last_error, created_at, updated_at) \
         VALUES ($1, $2, $3, 'queued', 0, $4, NULL, NULL, $5, $5)";
    /// Candidates due at `$1` (queued, or running with an expired lease),
    /// at most `$2`.
    pub const DUE: &str = "SELECT id FROM k_job \
         WHERE (status = 'queued' AND run_after <= $1) OR (status = 'running' AND locked_until < $1) \
         ORDER BY run_after, id LIMIT $2";
    /// Claims `$1` until `$2` at `$3` when it is still claimable; returns
    /// the claimed row.
    pub const CLAIM: &str = "UPDATE k_job SET status = 'running', attempts = attempts + 1, locked_until = $2, updated_at = $3 \
         WHERE id = $1 AND ((status = 'queued' AND run_after <= $3) OR (status = 'running' AND locked_until < $3)) \
         RETURNING id, name, payload, attempts";
    /// The job ran: `$1` id.
    pub const DONE: &str = "DELETE FROM k_job WHERE id = $1";
    /// Back to the queue: `$1` id, `$2` run after, `$3` error, `$4` now.
    pub const RETRY: &str = "UPDATE k_job SET status = 'queued', run_after = $2, locked_until = NULL, last_error = $3, updated_at = $4 WHERE id = $1";
    /// Dead: `$1` id, `$2` error, `$3` now.
    pub const BURY: &str = "UPDATE k_job SET status = 'dead', locked_until = NULL, last_error = $2, updated_at = $3 WHERE id = $1";
    /// An unfinished run at shutdown: back to the queue at once, the
    /// attempt not counted. `$1` id, `$2` now.
    pub const RELEASE: &str = "UPDATE k_job SET status = 'queued', attempts = attempts - 1, run_after = $2, locked_until = NULL, updated_at = $2 WHERE id = $1 AND status = 'running'";
    /// Every dead job, oldest first.
    pub const DEAD: &str = "SELECT id, name, attempts, last_error, updated_at FROM k_job WHERE status = 'dead' ORDER BY updated_at, id";
    /// Every job, by status then due time.
    pub const ALL: &str = "SELECT id, name, status, attempts, run_after, last_error FROM k_job ORDER BY status, run_after, id";
    /// A dead job back to the queue with fresh attempts: `$1` id, `$2` now.
    pub const REQUEUE: &str = "UPDATE k_job SET status = 'queued', attempts = 0, run_after = $2, locked_until = NULL, updated_at = $2 WHERE id = $1 AND status = 'dead'";
    /// Records schedule `$1` (never due before).
    pub const SCHEDULE_INIT: &str =
        "INSERT INTO k_job_schedule (name, last_due) VALUES ($1, -1) ON CONFLICT (name) DO NOTHING";
    /// Claims due time `$2` of schedule `$1`: one replica's update matches.
    pub const SCHEDULE_CLAIM: &str =
        "UPDATE k_job_schedule SET last_due = $2 WHERE name = $1 AND last_due < $2";
    /// The unfinished runs of job `$1` (queued, retrying or running), which
    /// make its schedule skip a due time instead of overlapping itself.
    pub const SCHEDULE_ACTIVE: &str =
        "SELECT COUNT(*) FROM k_job WHERE name = $1 AND status IN ('queued', 'running')";
}

/// Statements on `k_session`, with `$n` placeholders ([`sql`]). Ids are
/// stored hashed (the server hashes the cookie's token), so the table
/// alone never holds a usable session.
pub mod session_sql {
    /// `$1` id hash, `$2` user JSON (`NULL` for a guest), `$3` user key,
    /// `$4` roles JSON, `$5` now.
    pub const INSERT: &str = "INSERT INTO k_session (id, user_json, user_key, roles, created_at, last_seen, oauth_state, oauth_state_expires) \
         VALUES ($1, $2, $3, $4, $5, $5, NULL, NULL)";
    /// The row of `$1`.
    pub const GET: &str =
        "SELECT user_json, roles, created_at, last_seen FROM k_session WHERE id = $1";
    /// Marks `$1` seen at `$2`.
    pub const TOUCH: &str = "UPDATE k_session SET last_seen = $2 WHERE id = $1";
    /// Deletes `$1`.
    pub const DELETE: &str = "DELETE FROM k_session WHERE id = $1";
    /// The ids of user key `$1`'s sessions.
    pub const OF_USER: &str = "SELECT id FROM k_session WHERE user_key = $1";
    /// Deletes user key `$1`'s sessions.
    pub const DELETE_USER: &str = "DELETE FROM k_session WHERE user_key = $1";
    /// Binds OAuth state hash `$2`, valid until `$3`, to `$1`.
    pub const SET_STATE: &str =
        "UPDATE k_session SET oauth_state = $2, oauth_state_expires = $3 WHERE id = $1";
    /// The OAuth state of `$1`.
    pub const GET_STATE: &str =
        "SELECT oauth_state, oauth_state_expires FROM k_session WHERE id = $1";
    /// Clears `$1`'s OAuth state if it is still `$2` (single use).
    pub const TAKE_STATE: &str = "UPDATE k_session SET oauth_state = NULL, oauth_state_expires = NULL WHERE id = $1 AND oauth_state = $2";
    /// Deletes sessions idle since before `$1` or created before `$2`.
    pub const EXPIRE: &str = "DELETE FROM k_session WHERE last_seen < $1 OR created_at < $2";
}

/// Statements on `k_rate_limit`, with `$n` placeholders ([`sql`]).
pub mod rate_sql {
    /// The hits of key `$1` in windows `$2` (the previous) and `$3` (the
    /// current).
    pub const WINDOWS: &str =
        "SELECT window_start, hits FROM k_rate_limit WHERE key = $1 AND window_start IN ($2, $3)";
    /// One hit of key `$1` in window `$2`.
    pub const HIT: &str = "INSERT INTO k_rate_limit (key, window_start, hits) VALUES ($1, $2, 1) \
         ON CONFLICT (key, window_start) DO UPDATE SET hits = k_rate_limit.hits + 1";
    /// Forgets key `$1`'s windows before `$2`.
    pub const PRUNE: &str = "DELETE FROM k_rate_limit WHERE key = $1 AND window_start < $2";
}

/// The decision of a sliding-window rate limit from the hit counts of the
/// previous and the current fixed window.
///
/// This is the "sliding window counter" approximation, which needs two
/// rows per key instead of one per call: the estimate is `previous × (1 − elapsed/window) + current`. Returns
/// `0` when one more call is allowed, else the whole seconds (at least 1)
/// until the estimate drops below `limit`.
#[must_use]
pub fn rate_decision(
    limit: u32,
    window_ms: i64,
    elapsed_ms: i64,
    previous: i64,
    current: i64,
) -> u64 {
    let window = window_ms.max(1);
    let elapsed = elapsed_ms.clamp(0, window);
    let limit = i64::from(limit);
    #[allow(clippy::cast_precision_loss)]
    let estimate = previous as f64 * (1.0 - elapsed as f64 / window as f64) + current as f64;
    #[allow(clippy::cast_precision_loss)]
    if estimate < limit as f64 {
        return 0;
    }
    // When the current window alone is full, wait for the next one, whose
    // "previous" is this one: then wait until it has decayed enough.
    let wait_ms = if current >= limit || previous == 0 {
        let rest = window - elapsed;
        #[allow(clippy::cast_precision_loss)]
        let decay = if current == 0 {
            0.0
        } else {
            (1.0 - limit as f64 / current as f64).max(0.0) * window as f64
        };
        #[allow(clippy::cast_possible_truncation)]
        let decay = decay.ceil() as i64;
        rest + decay
    } else {
        #[allow(clippy::cast_precision_loss)]
        let needed = (1.0 - (limit - current) as f64 / previous as f64) * window as f64;
        #[allow(clippy::cast_possible_truncation)]
        let needed = needed.ceil() as i64;
        needed - elapsed
    };
    let secs = (wait_ms.max(1) + 999) / 1000;
    u64::try_from(secs.max(1)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_placeholders_replace_postgres_ones() {
        assert_eq!(
            sql(Dialect::Sqlite, "SELECT $1, '$x', $12"),
            "SELECT ?1, '$x', ?12"
        );
        assert_eq!(sql(Dialect::Postgres, "SELECT $1"), "SELECT $1");
    }

    #[test]
    fn schedules_are_due_at_their_times_in_utc() {
        // 2026-11-07T14:40:00Z (a Saturday).
        let now = 1_794_062_400_000;
        let hour = Schedule::Every {
            interval_ms: 3_600_000,
        };
        assert_eq!(hour.latest_due(now), 1_794_060_000_000);
        let daily = Schedule::Daily { minute: 3 * 60 };
        // 03:00 the same day.
        assert_eq!(daily.latest_due(now), 1_794_020_400_000);
        // Just before 03:00 the next day it is still today's.
        assert_eq!(
            daily.latest_due(1_794_020_400_000 + DAY_MS - 1),
            1_794_020_400_000
        );
        // Monday 03:00 before Saturday 2026-11-07: 2026-11-02.
        let weekly = Schedule::Weekly {
            weekday: 0,
            minute: 3 * 60,
        };
        let due = weekly.latest_due(now);
        assert_eq!(due, 1_794_020_400_000 - 5 * DAY_MS);
        assert_eq!(
            (due - FIRST_MONDAY_MS).rem_euclid(WEEK_MS),
            3 * 60 * MINUTE_MS
        );
    }

    #[test]
    fn backoff_doubles_with_bounded_jitter() {
        assert_eq!(backoff_ms(10_000, 1, 0.0), 10_000);
        assert_eq!(backoff_ms(10_000, 2, 0.0), 20_000);
        assert_eq!(backoff_ms(10_000, 4, 0.0), 80_000);
        assert_eq!(
            backoff_ms(10_000, 3, 0.5),
            48_000,
            "jitter is clamped to 20%"
        );
        assert_eq!(backoff_ms(10_000, 3, -0.2), 32_000);
        assert_eq!(backoff_ms(10_000, 64, 0.0), MAX_BACKOFF_MS);
        assert_eq!(backoff_ms(0, 1, f64::NAN), 1);
    }

    #[test]
    fn the_rate_window_slides_over_two_counters() {
        // Fresh key: allowed until the limit.
        assert_eq!(rate_decision(2, 60_000, 0, 0, 0), 0);
        assert_eq!(rate_decision(2, 60_000, 1000, 0, 1), 0);
        let wait = rate_decision(2, 60_000, 2000, 0, 2);
        assert!((58..=60).contains(&wait), "{wait}");
        // Half a window later the previous window counts half.
        assert_eq!(rate_decision(4, 60_000, 30_000, 4, 1), 0);
        assert!(rate_decision(4, 60_000, 30_000, 4, 2) >= 1);
        assert_eq!(rate_decision(0, 60_000, 0, 0, 0), 60);
    }
}
