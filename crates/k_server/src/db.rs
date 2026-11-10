//! The database of a server process (`docs/compiler/db.md` §6 and §9).
//!
//! [`ProcessDatabase::open`] connects to `DATABASE_URL`, applies the
//! build's pending migrations and loads its statement table for the
//! database's dialect; [`crate::http::Server::start`] does that before any
//! worker starts or the port opens, so a failed migration keeps the old
//! release serving elsewhere and this one from starting.
//!
//! **One runtime for the database.** Workers are threads with their own
//! single-threaded runtimes, while a Postgres connection is a task that
//! must keep running between requests. The database therefore lives on a
//! runtime of its own, on the `k-db` thread, and every operation is spawned
//! there; the ops await the result from any worker.
//!
//! **Transactions are bound to a request.** v0 has one connection per
//! process (the pool is planned, db.md §6), shared by every worker. A
//! statement outside a transaction takes the connection's *lease* for the
//! statement only; `BEGIN` takes it until `COMMIT` or `ROLLBACK`, so other
//! requests wait instead of running inside someone else's transaction.
//! Waiting for the lease is bounded ([`LEASE_TIMEOUT`]): a transaction left
//! open for that long makes the waiting statement fail (a panic of its
//! request) rather than hang the server.
//!
//! Statements are referred to by their number in the build's
//! [`DbManifest`]; SQL text never comes from JavaScript. Error messages
//! never contain bound values or the connection URL.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use k_db::manifest::DbManifest;
use k_db::{Database, DatabaseOptions, DbError, Dialect, Value};
use tokio::sync::{OwnedMutexGuard, oneshot};

/// How long a statement waits for the connection while another request's
/// transaction holds it.
pub const LEASE_TIMEOUT: Duration = Duration::from_secs(30);

/// What a server needs to open its database.
#[derive(Clone, Debug, Default)]
pub struct DatabaseSetup {
    /// `DATABASE_URL`: `sqlite:<path>`, `sqlite::memory:` or `postgres://…`.
    pub url: String,
    /// `DATABASE_PASSWORD`, overriding a password in the URL.
    pub password: Option<String>,
    /// The build's statements, migrations and unique indexes.
    pub manifest: DbManifest,
}

impl DatabaseSetup {
    /// The setup of a build's database from its validated configuration:
    /// `DATABASE_URL` and the optional `DATABASE_PASSWORD` secret. `None`
    /// when the build has no database or the app reads no `DATABASE_URL`
    /// (an app with entities but no `Database` configuration panics on its
    /// first query instead).
    #[must_use]
    pub fn from_config(
        manifest: Option<DbManifest>,
        config: &std::collections::BTreeMap<String, crate::config::ConfigValue>,
    ) -> Option<DatabaseSetup> {
        use crate::config::ConfigValue;
        let manifest = manifest?;
        let text = |v: &ConfigValue| -> Option<String> {
            match v {
                ConfigValue::String(s) => Some(s.clone()),
                ConfigValue::Secret(inner) => match inner.as_ref() {
                    ConfigValue::String(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            }
        };
        let url = config.get("DATABASE_URL").and_then(text)?;
        let password = config.get("DATABASE_PASSWORD").and_then(text);
        Some(DatabaseSetup {
            url,
            password,
            manifest,
        })
    }

    /// For development (`k run`, `k dev`, `k migrate`): a relative
    /// `sqlite:` path in the URL is made relative to `project` instead of
    /// the working directory, like a relative `NAME_FILE` path, so each
    /// member of a workspace run from its root keeps its own database; and
    /// the file's directory is created. Other URLs are kept.
    ///
    /// # Errors
    ///
    /// The directory couldn't be created.
    pub fn rooted_at(mut self, project: &std::path::Path) -> std::io::Result<DatabaseSetup> {
        let Some(path) = self.url.strip_prefix("sqlite:") else {
            return Ok(self);
        };
        if path == ":memory:" || path.is_empty() {
            return Ok(self);
        }
        let path = std::path::Path::new(path);
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            project.join(path)
        };
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.url = format!("sqlite:{}", full.display());
        Ok(self)
    }
}

/// The statements of a SQL script, trimmed, without the `;` that ends
/// each: a `;` inside a quoted string or identifier (`'…'`, `"…"`) or a
/// comment (`-- …`, `/* … */`) doesn't end a statement, and empty
/// statements (comments only) are left out.
#[must_use]
pub fn split_sql(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut code = false;
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                code = true;
                current.push(c);
                while let Some(d) = chars.next() {
                    current.push(d);
                    if d == c {
                        // A doubled quote is an escaped one.
                        if chars.peek() == Some(&c) {
                            current.extend(chars.next());
                        } else {
                            break;
                        }
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        current.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                current.push_str("/*");
                chars.next();
                let mut last = ' ';
                for d in chars.by_ref() {
                    current.push(d);
                    if last == '*' && d == '/' {
                        break;
                    }
                    last = d;
                }
            }
            ';' => {
                if code {
                    out.push(current.trim().to_owned());
                }
                current.clear();
                code = false;
            }
            _ => {
                if !c.is_whitespace() {
                    code = true;
                }
                current.push(c);
            }
        }
    }
    if code {
        out.push(current.trim().to_owned());
    }
    out
}

/// The outcome of a write.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum WriteOutcome {
    /// The number of rows affected.
    Count(u64),
    /// A `@Unique` field's index was violated: the field, which names the
    /// case of the entity's `UniqueViolation`.
    Unique(String),
}

/// A job claimed for a run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClaimedJob {
    /// The row's id.
    pub id: String,
    /// The job's name.
    pub name: String,
    /// The payload JSON.
    pub payload: String,
    /// The attempt this run is, from 1.
    pub attempt: u32,
}

/// What [`ProcessDatabase::claim_schedule`] did with a due time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScheduleClaim {
    /// This caller claimed it and enqueued its run.
    Enqueued,
    /// This caller claimed it, but an earlier run of the job is still
    /// queued, retrying or running, so the due time was skipped (a
    /// scheduled job never overlaps itself).
    Skipped,
    /// Another caller (or replica) claimed it already, or a later one.
    Taken,
}

/// What happens to a claimed job after its run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum JobFate {
    /// It ran: the row is deleted.
    Done,
    /// It failed and has attempts left: queued again at `run_after`.
    Retry {
        /// Epoch ms.
        run_after: i64,
        /// The failure, for `jobs list`.
        error: String,
    },
    /// It failed permanently or ran out of attempts.
    Dead {
        /// The last failure.
        error: String,
    },
    /// The run didn't finish (shutdown): queued again at once, the attempt
    /// not counted.
    Release,
}

/// A row of `k_job`, for `dist/app jobs list`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct JobRow {
    /// The job's id (16 random bytes, base64url).
    pub id: String,
    /// The job's name.
    pub name: String,
    /// `queued`, `running` or `dead`.
    pub status: String,
    /// Attempts so far.
    pub attempts: i64,
    /// When it is due (queued) or died (dead), epoch ms.
    pub at: i64,
    /// The last failure, if any.
    pub last_error: Option<String>,
}

/// A savepoint operation inside a transaction (db.md §9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Savepoint {
    /// `SAVEPOINT k_sp_<n>`: a nested `transaction { … }` starts.
    Begin,
    /// `RELEASE SAVEPOINT k_sp_<n>`: the nested block returned.
    Release,
    /// `ROLLBACK TO SAVEPOINT k_sp_<n>` then `RELEASE`: the block threw.
    Rollback,
}

/// The database every worker of a server process shares.
pub struct ProcessDatabase {
    /// The runtime of the `k-db` thread, where every database operation is
    /// spawned ([`ProcessDatabase::on_db`]).
    handle: tokio::runtime::Handle,
    /// Dropping it ends the `k-db` thread.
    _stop: oneshot::Sender<()>,
    /// The one connection to `DATABASE_URL`.
    db: Arc<Database>,
    /// The connection's dialect, which picks the statement texts.
    dialect: Dialect,
    /// The statement texts for [`Self::dialect`], by id.
    statements: Arc<Vec<String>>,
    /// The build's database manifest: statements, unique indexes and the schema
    /// used to name the field of a unique violation.
    manifest: Arc<DbManifest>,
    /// The connection's lease: held for one statement outside a transaction,
    /// and from `BEGIN` to `COMMIT` or `ROLLBACK` by a transaction.
    lease: Arc<tokio::sync::Mutex<()>>,
    /// The open transactions by id, each holding the lease until it ends.
    transactions: Arc<Mutex<HashMap<u32, OwnedMutexGuard<()>>>>,
    /// The id the next `BEGIN` hands out; ids start at 1 because 0 means "no
    /// transaction" in the ops.
    next_transaction: AtomicU32,
    /// `DATABASE_URL` and `DATABASE_PASSWORD` of a Postgres database, for
    /// the listening connection of [`crate::replicas`]; `None` for SQLite.
    postgres: Option<(String, Option<String>)>,
}

impl std::fmt::Debug for ProcessDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProcessDatabase({:?})", self.dialect)
    }
}

/// Locks `m`, recovering the data if a panicking thread poisoned it (the
/// maps it guards stay consistent between statements).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// An internal failure of the process database (its thread or task
/// stopped), reported as [`DbError::Connection`].
fn internal(message: impl Into<String>) -> DbError {
    DbError::Connection(message.into())
}

impl ProcessDatabase {
    /// Starts the `k-db` thread, connects, applies the pending migrations
    /// and returns the database with the names of the migrations it
    /// applied. Blocks the calling thread until then; never call it from
    /// a runtime worker that the database work itself needs.
    ///
    /// # Errors
    ///
    /// An invalid URL, a failed connection or a failed migration
    /// ([`DbError::Migration`], rolled back), none of which echo the URL
    /// or the password.
    pub fn open(setup: DatabaseSetup) -> Result<(Arc<ProcessDatabase>, Vec<String>), DbError> {
        ProcessDatabase::open_with(setup, true)
    }

    /// [`Self::open`], applying the pending migrations only when
    /// `migrate` (`dist/app jobs …` reads the queue without migrating). The
    /// stdlib's tables are always ensured.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_with(
        setup: DatabaseSetup,
        migrate: bool,
    ) -> Result<(Arc<ProcessDatabase>, Vec<String>), DbError> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        std::thread::Builder::new()
            .name("k-db".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = handle_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = handle_tx.send(Ok(runtime.handle().clone()));
                // Runs the database tasks until the database is dropped.
                runtime.block_on(async move {
                    let _ = stop_rx.await;
                });
            })
            .map_err(|e| internal(format!("cannot start the database thread: {e}")))?;
        let handle = handle_rx
            .recv()
            .map_err(|_| internal("the database thread exited while starting"))?
            .map_err(|e| internal(format!("cannot start the database runtime: {e}")))?;
        let DatabaseSetup {
            url,
            password,
            manifest,
        } = setup;
        let migrations = manifest.migrations.clone();
        let postgres = (url.starts_with("postgres://") || url.starts_with("postgresql://"))
            .then(|| (url.clone(), password.clone()));
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        handle.spawn(async move {
            let result = async {
                let db = Database::connect(&url, &DatabaseOptions { password }).await?;
                // On Postgres, replicas starting together take turns under
                // the migrators' advisory lock for the stdlib's tables too:
                // two concurrent `CREATE TABLE IF NOT EXISTS` of one table
                // race, and the loser fails on `pg_type`'s unique index
                // (found by the examples' Compose smoke tests). The lock is
                // reentrant, so `apply` takes it again inside.
                let lock = db.dialect() == Dialect::Postgres;
                if lock {
                    db.execute(
                        &format!(
                            "SELECT pg_advisory_lock({})",
                            k_db::migrate::ADVISORY_LOCK_KEY
                        ),
                        &[],
                    )
                    .await?;
                }
                let prepared = async {
                    let applied = if migrate {
                        k_db::migrate::apply(&db, &migrations).await?
                    } else {
                        Vec::new()
                    };
                    // The stdlib's own tables (jobs, sessions, rate limits),
                    // idempotently and outside `k_migrations` (db.md §12).
                    for statement in k_db::stdlib::tables() {
                        db.execute(statement, &[]).await?;
                    }
                    Ok::<_, DbError>(applied)
                }
                .await;
                if lock {
                    db.execute(
                        &format!(
                            "SELECT pg_advisory_unlock({})",
                            k_db::migrate::ADVISORY_LOCK_KEY
                        ),
                        &[],
                    )
                    .await?;
                }
                Ok::<_, DbError>((db, prepared?))
            }
            .await;
            let _ = done_tx.send(result);
        });
        let (db, applied) = done_rx
            .recv()
            .map_err(|_| internal("the database thread stopped while connecting"))??;
        let dialect = db.dialect();
        let statements = manifest
            .statements
            .iter()
            .map(|s| s.text(dialect).to_owned())
            .collect();
        Ok((
            Arc::new(ProcessDatabase {
                handle,
                _stop: stop_tx,
                db: Arc::new(db),
                dialect,
                statements: Arc::new(statements),
                manifest: Arc::new(manifest),
                lease: Arc::new(tokio::sync::Mutex::new(())),
                transactions: Arc::new(Mutex::new(HashMap::new())),
                next_transaction: AtomicU32::new(1),
                postgres,
            }),
            applied,
        ))
    }

    /// The dialect of `DATABASE_URL`.
    #[must_use]
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Runs `work` on the database runtime and awaits it from anywhere.
    ///
    /// # Errors
    ///
    /// `work`'s own error, or [`DbError::Connection`] if the task panicked or
    /// the database runtime is gone.
    async fn on_db<T: Send + 'static>(
        &self,
        work: impl Future<Output = Result<T, DbError>> + Send + 'static,
    ) -> Result<T, DbError> {
        self.handle
            .spawn(work)
            .await
            .map_err(|_| internal("the database task failed"))?
    }

    /// The text of statement `id` in this database's dialect.
    ///
    /// # Errors
    ///
    /// [`DbError::Statement`] when the build has no statement with that id (a
    /// mismatch between the server bundle and the manifest).
    fn statement(&self, id: u32) -> Result<String, DbError> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.statements.get(i))
            .cloned()
            .ok_or_else(|| DbError::Statement(format!("the build has no statement {id}")))
    }

    /// Checks that `tx` (0: none) is an open transaction.
    ///
    /// # Errors
    ///
    /// [`DbError::Statement`] when `tx` was committed, rolled back or never
    /// begun.
    fn check_transaction(&self, tx: u32) -> Result<(), DbError> {
        if tx != 0 && !lock(&self.transactions).contains_key(&tx) {
            return Err(DbError::Statement(format!(
                "transaction {tx} is not open (it was committed or rolled back)"
            )));
        }
        Ok(())
    }

    /// Takes the connection's lease, bounded by [`LEASE_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// [`DbError::Connection`] when [`LEASE_TIMEOUT`] passes first (another
    /// request's transaction holds the connection).
    async fn lease(lease: Arc<tokio::sync::Mutex<()>>) -> Result<OwnedMutexGuard<()>, DbError> {
        tokio::time::timeout(LEASE_TIMEOUT, lease.lock_owned())
            .await
            .map_err(|_| {
                internal(
                    "timed out waiting for the database connection: another request's transaction held it too long",
                )
            })
    }

    /// Runs statement `id` and returns its rows, each a list of values in
    /// select order. `tx` is the request's transaction, 0 for none.
    ///
    /// # Errors
    ///
    /// Any [`DbError`]: an unknown statement or transaction, a database
    /// failure.
    pub async fn query(
        &self,
        id: u32,
        params: Vec<Value>,
        tx: u32,
    ) -> Result<Vec<Vec<Value>>, DbError> {
        let sql = self.statement(id)?;
        self.check_transaction(tx)?;
        let db = Arc::clone(&self.db);
        let lease = (tx == 0).then(|| Arc::clone(&self.lease));
        self.on_db(async move {
            let _held = match lease {
                Some(l) => Some(Self::lease(l).await?),
                None => None,
            };
            let rows = db.query(&sql, &params).await?;
            Ok(rows.into_iter().map(|r| r.values).collect())
        })
        .await
    }

    /// Runs write statement `id` and returns the rows affected, or the
    /// `@Unique` field whose index it violated.
    ///
    /// # Errors
    ///
    /// As [`Self::query`]; a unique violation that isn't a `@Unique`
    /// field's (a primary key) is an error too.
    pub async fn execute(
        &self,
        id: u32,
        params: Vec<Value>,
        tx: u32,
    ) -> Result<WriteOutcome, DbError> {
        let sql = self.statement(id)?;
        self.check_transaction(tx)?;
        let db = Arc::clone(&self.db);
        let manifest = Arc::clone(&self.manifest);
        let lease = (tx == 0).then(|| Arc::clone(&self.lease));
        self.on_db(async move {
            let _held = match lease {
                Some(l) => Some(Self::lease(l).await?),
                None => None,
            };
            match db.execute(&sql, &params).await {
                Ok(n) => Ok(WriteOutcome::Count(n)),
                Err(e) => match manifest.unique_field(&e) {
                    Some(field) => Ok(WriteOutcome::Unique(field.to_owned())),
                    None => Err(e),
                },
            }
        })
        .await
    }

    /// Begins a transaction and returns its id. It holds the connection
    /// until [`Self::finish`].
    ///
    /// # Errors
    ///
    /// The lease timed out, or `BEGIN` failed.
    pub async fn begin(&self) -> Result<u32, DbError> {
        let db = Arc::clone(&self.db);
        let lease = Arc::clone(&self.lease);
        let begin = match self.dialect {
            // Writers serialize up front instead of failing with
            // SQLITE_BUSY when a read turns into a write.
            Dialect::Sqlite => "BEGIN IMMEDIATE",
            Dialect::Postgres => "BEGIN ISOLATION LEVEL READ COMMITTED",
        };
        let guard = self
            .on_db(async move {
                let guard = Self::lease(lease).await?;
                db.execute(begin, &[]).await?;
                Ok(guard)
            })
            .await?;
        let mut id = self.next_transaction.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            id = self.next_transaction.fetch_add(1, Ordering::Relaxed);
        }
        lock(&self.transactions).insert(id, guard);
        Ok(id)
    }

    /// Commits (`commit`) or rolls back transaction `tx` and releases the
    /// connection. A failed commit is rolled back.
    ///
    /// # Errors
    ///
    /// `tx` isn't open, or the database failed.
    pub async fn finish(&self, tx: u32, commit: bool) -> Result<(), DbError> {
        let Some(guard) = lock(&self.transactions).remove(&tx) else {
            return Err(DbError::Statement(format!("transaction {tx} is not open")));
        };
        let db = Arc::clone(&self.db);
        self.on_db(async move {
            let _guard = guard;
            if commit {
                if let Err(e) = db.execute("COMMIT", &[]).await {
                    let _ = db.execute("ROLLBACK", &[]).await;
                    return Err(e);
                }
                Ok(())
            } else {
                db.execute("ROLLBACK", &[]).await.map(|_| ())
            }
        })
        .await
    }

    /// Runs savepoint operation `op` for nesting depth `depth` inside
    /// transaction `tx`.
    ///
    /// # Errors
    ///
    /// `tx` isn't open, or the database failed.
    pub async fn savepoint(&self, tx: u32, depth: u32, op: Savepoint) -> Result<(), DbError> {
        if tx == 0 {
            return Err(DbError::Statement(
                "a savepoint needs a transaction".to_owned(),
            ));
        }
        self.check_transaction(tx)?;
        let name = k_db::sql::quote_identifier(&format!("k_sp_{depth}"));
        let statements = match op {
            Savepoint::Begin => vec![format!("SAVEPOINT {name}")],
            Savepoint::Release => vec![format!("RELEASE SAVEPOINT {name}")],
            Savepoint::Rollback => vec![
                format!("ROLLBACK TO SAVEPOINT {name}"),
                format!("RELEASE SAVEPOINT {name}"),
            ],
        };
        let db = Arc::clone(&self.db);
        self.on_db(async move {
            for s in statements {
                db.execute(&s, &[]).await?;
            }
            Ok(())
        })
        .await
    }

    /// `SELECT 1` within `timeout`; never fails (`database.ping`). It
    /// doesn't wait for the lease, so a long transaction doesn't make a
    /// health check fail.
    pub async fn ping(&self, timeout: Duration) -> bool {
        let db = Arc::clone(&self.db);
        self.on_db(async move { Ok(db.ping(timeout).await) })
            .await
            .unwrap_or(false)
    }

    /// Runs one of the stdlib's statements (`k_db::stdlib`, written with
    /// `$n` placeholders) and returns the rows affected, under the same
    /// lease and transaction rules as [`Self::execute`].
    ///
    /// # Errors
    ///
    /// As [`Self::execute`]: a transaction that isn't open, a lease timeout,
    /// or the statement's failure.
    async fn stdlib_execute(
        &self,
        text: &str,
        params: Vec<Value>,
        tx: u32,
    ) -> Result<u64, DbError> {
        self.check_transaction(tx)?;
        let sql = k_db::stdlib::sql(self.dialect, text);
        let db = Arc::clone(&self.db);
        let lease = (tx == 0).then(|| Arc::clone(&self.lease));
        self.on_db(async move {
            let _held = match lease {
                Some(l) => Some(Self::lease(l).await?),
                None => None,
            };
            db.execute(&sql, &params).await
        })
        .await
    }

    /// Runs one of the stdlib's queries and returns its rows.
    ///
    /// # Errors
    ///
    /// As [`Self::stdlib_execute`].
    async fn stdlib_query(
        &self,
        text: &str,
        params: Vec<Value>,
        tx: u32,
    ) -> Result<Vec<Vec<Value>>, DbError> {
        self.check_transaction(tx)?;
        let sql = k_db::stdlib::sql(self.dialect, text);
        let db = Arc::clone(&self.db);
        let lease = (tx == 0).then(|| Arc::clone(&self.lease));
        self.on_db(async move {
            let _held = match lease {
                Some(l) => Some(Self::lease(l).await?),
                None => None,
            };
            Ok(db
                .query(&sql, &params)
                .await?
                .into_iter()
                .map(|r| r.values)
                .collect())
        })
        .await
    }

    // ------------------------------------------------------------ jobs

    /// Stores a job `name` with `payload` (JSON) to run at `run_after`
    /// (epoch ms), inside transaction `tx` (0: on its own), and returns
    /// its id. Inside a transaction the row commits or rolls back with it.
    ///
    /// # Errors
    ///
    /// A database failure, or `tx` isn't open.
    pub async fn enqueue_job(
        &self,
        id: String,
        name: &str,
        payload: String,
        run_after: i64,
        now: i64,
        tx: u32,
    ) -> Result<String, DbError> {
        self.stdlib_execute(
            k_db::stdlib::job_sql::INSERT,
            vec![
                Value::Text(id.clone()),
                Value::Text(name.to_owned()),
                Value::Text(payload),
                Value::Int(run_after),
                Value::Int(now),
            ],
            tx,
        )
        .await?;
        Ok(id)
    }

    /// Claims up to `limit` jobs due at `now`, each until `now + lease_ms`.
    /// A job is claimed by one caller only: the claim is a conditional
    /// update, so replicas racing for a row see one of them win.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn claim_jobs(
        &self,
        now: i64,
        limit: usize,
        lease_ms: i64,
    ) -> Result<Vec<ClaimedJob>, DbError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let due = self
            .stdlib_query(
                k_db::stdlib::job_sql::DUE,
                vec![Value::Int(now), Value::Int(limit)],
                0,
            )
            .await?;
        let mut out = Vec::new();
        for row in due {
            let Some(Value::Text(id)) = row.into_iter().next() else {
                continue;
            };
            let claimed = self
                .stdlib_query(
                    k_db::stdlib::job_sql::CLAIM,
                    vec![Value::Text(id), Value::Int(now + lease_ms), Value::Int(now)],
                    0,
                )
                .await?;
            for r in claimed {
                if let [
                    Value::Text(id),
                    Value::Text(name),
                    Value::Text(payload),
                    Value::Int(attempt),
                ] = r.as_slice()
                {
                    out.push(ClaimedJob {
                        id: id.clone(),
                        name: name.clone(),
                        payload: payload.clone(),
                        attempt: u32::try_from(*attempt).unwrap_or(u32::MAX),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Records the outcome of a claimed job's run (see [`JobFate`]).
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn finish_job(&self, id: &str, fate: JobFate, now: i64) -> Result<(), DbError> {
        let id = Value::Text(id.to_owned());
        let (sql, params) = match fate {
            JobFate::Done => (k_db::stdlib::job_sql::DONE, vec![id]),
            JobFate::Retry { run_after, error } => (
                k_db::stdlib::job_sql::RETRY,
                vec![
                    id,
                    Value::Int(run_after),
                    Value::Text(error),
                    Value::Int(now),
                ],
            ),
            JobFate::Dead { error } => (
                k_db::stdlib::job_sql::BURY,
                vec![id, Value::Text(error), Value::Int(now)],
            ),
            JobFate::Release => (k_db::stdlib::job_sql::RELEASE, vec![id, Value::Int(now)]),
        };
        self.stdlib_execute(sql, params, 0).await.map(|_| ())
    }

    /// Every job (`all`) or the dead ones, for `dist/app jobs list`.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn list_jobs(&self, dead_only: bool) -> Result<Vec<JobRow>, DbError> {
        let rows = if dead_only {
            self.stdlib_query(k_db::stdlib::job_sql::DEAD, vec![], 0)
                .await?
        } else {
            self.stdlib_query(k_db::stdlib::job_sql::ALL, vec![], 0)
                .await?
        };
        let text = |v: &Value| match v {
            Value::Text(t) => t.clone(),
            _ => String::new(),
        };
        let int = |v: &Value| match v {
            Value::Int(i) => *i,
            _ => 0,
        };
        Ok(rows
            .into_iter()
            .map(|r| {
                if dead_only {
                    JobRow {
                        id: r.first().map(text).unwrap_or_default(),
                        name: r.get(1).map(text).unwrap_or_default(),
                        status: k_db::stdlib::status::DEAD.to_owned(),
                        attempts: r.get(2).map_or(0, int),
                        at: r.get(4).map_or(0, int),
                        last_error: r.get(3).map(text).filter(|t| !t.is_empty()),
                    }
                } else {
                    JobRow {
                        id: r.first().map(text).unwrap_or_default(),
                        name: r.get(1).map(text).unwrap_or_default(),
                        status: r.get(2).map(text).unwrap_or_default(),
                        attempts: r.get(3).map_or(0, int),
                        at: r.get(4).map_or(0, int),
                        last_error: r.get(5).map(text).filter(|t| !t.is_empty()),
                    }
                }
            })
            .collect())
    }

    /// Puts dead job `id` back in the queue with fresh attempts; `false`
    /// when there is no dead job with that id.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn requeue_job(&self, id: &str, now: i64) -> Result<bool, DbError> {
        let n = self
            .stdlib_execute(
                k_db::stdlib::job_sql::REQUEUE,
                vec![Value::Text(id.to_owned()), Value::Int(now)],
                0,
            )
            .await?;
        Ok(n > 0)
    }

    /// Claims due time `due` of schedule `name` and, unless an earlier run
    /// of the job is still unfinished, enqueues its run as job `id` (due at
    /// `due`, created at `now`). Exactly one caller across every replica
    /// gets [`ScheduleClaim::Enqueued`] or [`ScheduleClaim::Skipped`]; a
    /// skipped due time still moves `last_due` forward, so missed runs are
    /// coalesced rather than queued up.
    ///
    /// The claim, the check and the insert are one transaction: on Postgres
    /// a second replica's claim of the next due time waits on the
    /// `k_job_schedule` row until this one commits, then sees its run; on
    /// SQLite writers serialize (`BEGIN IMMEDIATE`).
    ///
    /// # Errors
    ///
    /// A database failure (the transaction is rolled back).
    pub async fn claim_schedule(
        &self,
        name: &str,
        due: i64,
        id: String,
        now: i64,
    ) -> Result<ScheduleClaim, DbError> {
        self.stdlib_execute(
            k_db::stdlib::job_sql::SCHEDULE_INIT,
            vec![Value::Text(name.to_owned())],
            0,
        )
        .await?;
        let tx = self.begin().await?;
        let outcome = self.claim_schedule_in(name, due, id, now, tx).await;
        let commit = matches!(
            outcome,
            Ok(ScheduleClaim::Enqueued | ScheduleClaim::Skipped)
        );
        let finished = self.finish(tx, commit).await;
        let outcome = outcome?;
        finished?;
        Ok(outcome)
    }

    /// [`Self::claim_schedule`]'s statements inside transaction `tx`.
    async fn claim_schedule_in(
        &self,
        name: &str,
        due: i64,
        id: String,
        now: i64,
        tx: u32,
    ) -> Result<ScheduleClaim, DbError> {
        let claimed = self
            .stdlib_execute(
                k_db::stdlib::job_sql::SCHEDULE_CLAIM,
                vec![Value::Text(name.to_owned()), Value::Int(due)],
                tx,
            )
            .await?;
        if claimed == 0 {
            return Ok(ScheduleClaim::Taken);
        }
        let active = self
            .stdlib_query(
                k_db::stdlib::job_sql::SCHEDULE_ACTIVE,
                vec![Value::Text(name.to_owned())],
                tx,
            )
            .await?;
        if matches!(active.first().and_then(|r| r.first()), Some(Value::Int(n)) if *n > 0) {
            return Ok(ScheduleClaim::Skipped);
        }
        self.enqueue_job(id, name, "{}".into(), due, now, tx)
            .await?;
        Ok(ScheduleClaim::Enqueued)
    }

    // -------------------------------------------------------- sessions

    /// Inserts a session row (`k_db::stdlib::session_sql::INSERT`).
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_insert(
        &self,
        id_hash: &str,
        user: Option<(String, String)>,
        roles_json: String,
        now: i64,
    ) -> Result<(), DbError> {
        let (user_json, user_key) = match user {
            Some((json, key)) => (Value::Text(json), Value::Text(key)),
            None => (Value::Null, Value::Null),
        };
        self.stdlib_execute(
            k_db::stdlib::session_sql::INSERT,
            vec![
                Value::Text(id_hash.to_owned()),
                user_json,
                user_key,
                Value::Text(roles_json),
                Value::Int(now),
            ],
            0,
        )
        .await
        .map(|_| ())
    }

    /// The row of a session: `(user JSON or None for a guest, roles JSON,
    /// created, last seen)`.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_row(
        &self,
        id_hash: &str,
    ) -> Result<Option<(Option<String>, String, i64, i64)>, DbError> {
        let rows = self
            .stdlib_query(
                k_db::stdlib::session_sql::GET,
                vec![Value::Text(id_hash.to_owned())],
                0,
            )
            .await?;
        Ok(rows.into_iter().next().and_then(|r| match r.as_slice() {
            [
                user,
                Value::Text(roles),
                Value::Int(created),
                Value::Int(seen),
            ] => Some((
                match user {
                    Value::Text(u) => Some(u.clone()),
                    _ => None,
                },
                roles.clone(),
                *created,
                *seen,
            )),
            _ => None,
        }))
    }

    /// Runs a session statement whose parameters are all given.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_statement(&self, text: &str, params: Vec<Value>) -> Result<u64, DbError> {
        self.stdlib_execute(text, params, 0).await
    }

    /// Runs a session or rate-limit query whose parameters are all given.
    ///
    /// # Errors
    ///
    /// A database failure.
    pub async fn session_query(
        &self,
        text: &str,
        params: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, DbError> {
        self.stdlib_query(text, params, 0).await
    }

    /// Runs a SQL script (several statements separated by `;`, as
    /// [`split_sql`] reads them) in one transaction, rolled back on the
    /// first failure; returns how many statements ran. `k dev` seeds a new
    /// development database with it (`[dev] seed` in `k.toml`).
    ///
    /// # Errors
    ///
    /// The failing statement's error (the transaction is rolled back), or a
    /// lease timeout.
    pub async fn run_script(&self, script: &str) -> Result<usize, DbError> {
        let statements = split_sql(script);
        let db = Arc::clone(&self.db);
        let lease = Arc::clone(&self.lease);
        self.on_db(async move {
            let _held = Self::lease(lease).await?;
            db.execute("BEGIN", &[]).await?;
            for statement in &statements {
                if let Err(e) = db.execute(statement, &[]).await {
                    let _ = db.execute("ROLLBACK", &[]).await;
                    return Err(e);
                }
            }
            db.execute("COMMIT", &[]).await?;
            Ok(statements.len())
        })
        .await
    }

    /// The number of open transactions (for tests and diagnostics).
    #[must_use]
    pub fn open_transactions(&self) -> usize {
        lock(&self.transactions).len()
    }

    // -------------------------------------------------------- replicas
    //
    // Broadcasts and presence between replicas (`crate::replicas`,
    // 44-channels.md "Server isolates and replicas"). Postgres only: every
    // statement below is Postgres SQL, and `crate::replicas` starts only
    // for a Postgres database.

    /// `true` when the database is Postgres, whose `LISTEN`/`NOTIFY`
    /// carries broadcasts and presence between replicas.
    #[must_use]
    pub fn is_postgres(&self) -> bool {
        self.postgres.is_some()
    }

    /// Runs `work` on the database runtime without waiting for it. Work
    /// spawned from one thread starts in the order it was spawned, and
    /// statements take the connection's lease in that order, so one
    /// process's notifications go out in the order they were published.
    pub fn spawn_detached(&self, work: impl Future<Output = ()> + Send + 'static) {
        drop(self.handle.spawn(work));
    }

    /// Opens a listening connection on `channel` (`LISTEN`), on the database
    /// runtime.
    ///
    /// # Errors
    ///
    /// Not Postgres, or connecting or `LISTEN` failed.
    pub async fn listen(&self, channel: &str) -> Result<k_db::PostgresListener, DbError> {
        let Some((url, password)) = self.postgres.clone() else {
            return Err(DbError::Statement("LISTEN needs Postgres".to_owned()));
        };
        let channel = channel.to_owned();
        self.on_db(async move {
            k_db::PostgresListener::connect(&url, &DatabaseOptions { password }, &channel).await
        })
        .await
    }

    /// Runs `statements` (SQL with their parameters) in one transaction of
    /// their own, holding the connection's lease, and returns each one's
    /// rows. A failure rolls back.
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    async fn in_transaction(
        &self,
        work: Vec<(&'static str, Vec<Value>)>,
    ) -> Result<Vec<Vec<Vec<Value>>>, DbError> {
        let db = Arc::clone(&self.db);
        let lease = Arc::clone(&self.lease);
        self.on_db(async move {
            let _held = Self::lease(lease).await?;
            db.execute("BEGIN", &[]).await?;
            let mut out = Vec::with_capacity(work.len());
            for (sql, params) in work {
                match db.query(sql, &params).await {
                    Ok(rows) => out.push(rows.into_iter().map(|r| r.values).collect()),
                    Err(e) => {
                        let _ = db.execute("ROLLBACK", &[]).await;
                        return Err(e);
                    }
                }
            }
            db.execute("COMMIT", &[]).await?;
            Ok(out)
        })
        .await
    }

    /// Runs one statement outside any transaction (taking the lease for it)
    /// and returns its rows.
    ///
    /// # Errors
    ///
    /// A lease timeout or the statement's failure.
    async fn replica_query(
        &self,
        sql: &'static str,
        params: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, DbError> {
        let db = Arc::clone(&self.db);
        let lease = Arc::clone(&self.lease);
        self.on_db(async move {
            let _held = Self::lease(lease).await?;
            Ok(db
                .query(sql, &params)
                .await?
                .into_iter()
                .map(|r| r.values)
                .collect())
        })
        .await
    }

    /// `NOTIFY channel, payload`, after first storing `spill` (an id and
    /// the full payload, which `payload` then names) in
    /// `k_broadcast_spill` when the element was too large to notify.
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    pub async fn notify(
        &self,
        channel: &str,
        payload: String,
        spill: Option<(String, String)>,
        now: i64,
    ) -> Result<(), DbError> {
        let db = Arc::clone(&self.db);
        let lease = Arc::clone(&self.lease);
        let channel = channel.to_owned();
        self.on_db(async move {
            let _held = Self::lease(lease).await?;
            if let Some((id, full)) = spill {
                db.execute(
                    "INSERT INTO k_broadcast_spill (id, payload, created_at) VALUES ($1, $2, $3)",
                    &[Value::Text(id), Value::Text(full), Value::Int(now)],
                )
                .await?;
            }
            db.query(
                "SELECT 1 FROM (SELECT pg_notify($1, $2)) AS n",
                &[Value::Text(channel), Value::Text(payload)],
            )
            .await?;
            Ok(())
        })
        .await
    }

    /// The payload stored as spill `id`, if it is still there.
    ///
    /// # Errors
    ///
    /// A lease timeout or the query's failure.
    pub async fn spilled(&self, id: &str) -> Result<Option<String>, DbError> {
        let rows = self
            .replica_query(
                "SELECT payload FROM k_broadcast_spill WHERE id = $1",
                vec![Value::Text(id.to_owned())],
            )
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|r| match r.into_iter().next() {
                Some(Value::Text(t)) => Some(t),
                _ => None,
            }))
    }

    /// Stores lease `lease` of `member` under `key`, held by process
    /// `process` until `expires`; `true` when it is the member's only live
    /// lease under the key (the member joined).
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    pub async fn presence_join(
        &self,
        lease: &str,
        key: &str,
        member: &str,
        process: &str,
        now: i64,
        expires: i64,
    ) -> Result<bool, DbError> {
        let k = Value::Text(key.to_owned());
        let m = Value::Text(member.to_owned());
        let out = self
            .in_transaction(vec![
                (PRESENCE_LOCK, vec![k.clone()]),
                (
                    "INSERT INTO k_presence (lease, key, member, process, joined_at, expires_at) \
                     VALUES ($1, $2, $3, $4, $5, $6) RETURNING 1",
                    vec![
                        Value::Text(lease.to_owned()),
                        k.clone(),
                        m.clone(),
                        Value::Text(process.to_owned()),
                        Value::Int(now),
                        Value::Int(expires),
                    ],
                ),
                (PRESENCE_COUNT, vec![k, m, Value::Int(now)]),
            ])
            .await?;
        Ok(count(out.get(2)) == 1)
    }

    /// Replaces the member of lease `lease` with `member`: the lease's key
    /// and old member, with how many live leases each member has after the
    /// change (`None` for an unknown lease).
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    pub async fn presence_update(
        &self,
        lease: &str,
        member: &str,
        now: i64,
    ) -> Result<Option<PresenceMove>, DbError> {
        let Some((key, old)) = self.presence_lease(lease).await? else {
            return Ok(None);
        };
        let k = Value::Text(key.clone());
        let out = self
            .in_transaction(vec![
                (PRESENCE_LOCK, vec![k.clone()]),
                (
                    "UPDATE k_presence SET member = $2 WHERE lease = $1 RETURNING 1",
                    vec![
                        Value::Text(lease.to_owned()),
                        Value::Text(member.to_owned()),
                    ],
                ),
                (
                    PRESENCE_COUNT,
                    vec![k.clone(), Value::Text(old.clone()), Value::Int(now)],
                ),
                (
                    PRESENCE_COUNT,
                    vec![k, Value::Text(member.to_owned()), Value::Int(now)],
                ),
            ])
            .await?;
        if out.get(1).is_none_or(Vec::is_empty) {
            return Ok(None);
        }
        Ok(Some(PresenceMove {
            key,
            old,
            old_left: count(out.get(2)),
            new_count: count(out.get(3)),
        }))
    }

    /// Ends lease `lease`: its key and member when that was the member's
    /// last live lease under the key (the member left).
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    pub async fn presence_leave(
        &self,
        lease: &str,
        now: i64,
    ) -> Result<Option<(String, String)>, DbError> {
        let Some((key, member)) = self.presence_lease(lease).await? else {
            return Ok(None);
        };
        let k = Value::Text(key.clone());
        let m = Value::Text(member.clone());
        let out = self
            .in_transaction(vec![
                (PRESENCE_LOCK, vec![k.clone()]),
                (
                    "DELETE FROM k_presence WHERE lease = $1 RETURNING 1",
                    vec![Value::Text(lease.to_owned())],
                ),
                (PRESENCE_COUNT, vec![k, m, Value::Int(now)]),
            ])
            .await?;
        let deleted = out.get(1).is_some_and(|r| !r.is_empty());
        Ok((deleted && count(out.get(2)) == 0).then_some((key, member)))
    }

    /// The key and member of lease `lease`, if it exists.
    async fn presence_lease(&self, lease: &str) -> Result<Option<(String, String)>, DbError> {
        let rows = self
            .replica_query(
                "SELECT key, member FROM k_presence WHERE lease = $1",
                vec![Value::Text(lease.to_owned())],
            )
            .await?;
        Ok(rows.into_iter().next().and_then(|r| {
            let mut it = r.into_iter();
            match (it.next(), it.next()) {
                (Some(Value::Text(k)), Some(Value::Text(m))) => Some((k, m)),
                _ => None,
            }
        }))
    }

    /// The members with a live lease under `key`, each once, in the order
    /// of their first lease.
    ///
    /// # Errors
    ///
    /// A lease timeout or the query's failure.
    pub async fn presence_members(&self, key: &str, now: i64) -> Result<Vec<String>, DbError> {
        let rows = self
            .replica_query(
                "SELECT member, MIN(joined_at) AS first FROM k_presence \
                 WHERE key = $1 AND expires_at > $2 GROUP BY member ORDER BY first, member",
                vec![Value::Text(key.to_owned()), Value::Int(now)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| match r.into_iter().next() {
                Some(Value::Text(m)) => Some(m),
                _ => None,
            })
            .collect())
    }

    /// Renews every lease of process `process` until `expires`.
    ///
    /// # Errors
    ///
    /// A lease timeout or the statement's failure.
    pub async fn presence_renew(&self, process: &str, expires: i64) -> Result<(), DbError> {
        self.replica_query(
            "UPDATE k_presence SET expires_at = $1 WHERE process = $2 RETURNING 1",
            vec![Value::Int(expires), Value::Text(process.to_owned())],
        )
        .await?;
        Ok(())
    }

    /// Deletes the leases that expired by `now` (their process stopped
    /// renewing them: it crashed) and the spilled payloads older than
    /// `spill_before`; returns each key and member that has no live lease
    /// left, so the caller announces that it left.
    ///
    /// # Errors
    ///
    /// A lease timeout or a statement's failure.
    pub async fn presence_sweep(
        &self,
        now: i64,
        spill_before: i64,
    ) -> Result<Vec<(String, String)>, DbError> {
        self.replica_query(
            "DELETE FROM k_broadcast_spill WHERE created_at < $1 RETURNING 1",
            vec![Value::Int(spill_before)],
        )
        .await?;
        let rows = self
            .replica_query(
                "DELETE FROM k_presence WHERE expires_at <= $1 RETURNING key, member",
                vec![Value::Int(now)],
            )
            .await?;
        let mut gone: Vec<(String, String)> = Vec::new();
        for r in rows {
            let mut it = r.into_iter();
            if let (Some(Value::Text(k)), Some(Value::Text(m))) = (it.next(), it.next())
                && !gone.contains(&(k.clone(), m.clone()))
            {
                gone.push((k, m));
            }
        }
        let mut left = Vec::new();
        for (key, member) in gone {
            let out = self
                .in_transaction(vec![
                    (PRESENCE_LOCK, vec![Value::Text(key.clone())]),
                    (
                        PRESENCE_COUNT,
                        vec![
                            Value::Text(key.clone()),
                            Value::Text(member.clone()),
                            Value::Int(now),
                        ],
                    ),
                ])
                .await?;
            if count(out.get(1)) == 0 {
                left.push((key, member));
            }
        }
        Ok(left)
    }
}

/// Serializes presence changes of one key across replicas, for the rest of
/// the transaction.
const PRESENCE_LOCK: &str = "SELECT 1 FROM (SELECT pg_advisory_xact_lock(hashtext($1))) AS l";

/// The live leases of one member under one key.
const PRESENCE_COUNT: &str =
    "SELECT COUNT(*) FROM k_presence WHERE key = $1 AND member = $2 AND expires_at > $3";

/// The number in the first column of the first row of `rows` (a `COUNT`).
fn count(rows: Option<&Vec<Vec<Value>>>) -> i64 {
    match rows.and_then(|r| r.first()).and_then(|r| r.first()) {
        Some(Value::Int(n)) => *n,
        _ => 0,
    }
}

/// What [`ProcessDatabase::presence_update`] changed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PresenceMove {
    /// The lease's key.
    pub key: String,
    /// The member the lease had before.
    pub old: String,
    /// The old member's live leases after the change.
    pub old_left: i64,
    /// The new member's live leases after the change.
    pub new_count: i64,
}

/// Converts a statement parameter from JavaScript to a [`Value`].
///
/// Accepted are `null`, booleans, integers (`Int`), other numbers
/// (`Double`) and strings. Anything else (an
/// object, an array) is an error, so a generated encoder that forgot a
/// conversion fails loudly instead of binding garbage.
///
/// # Errors
///
/// A value that isn't one of those.
pub fn param_from_json(v: serde_json::Value) -> Result<Value, String> {
    Ok(match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Double(n.as_f64().ok_or("a number out of range")?),
        },
        serde_json::Value::String(s) => Value::Text(s),
        serde_json::Value::Array(_) => return Err("an array is not a parameter".to_owned()),
        serde_json::Value::Object(_) => return Err("an object is not a parameter".to_owned()),
    })
}

/// Converts a column value for JavaScript: bytes become an array of
/// numbers, a non-finite double `null`.
#[must_use]
pub fn value_to_json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::from(i),
        Value::Double(d) => serde_json::Number::from_f64(d)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Value::Text(s) => serde_json::Value::String(s),
        Value::Bytes(b) => {
            serde_json::Value::Array(b.into_iter().map(serde_json::Value::from).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_db::manifest::{StatementSql, UniqueIndex};
    use k_db::migrate::Migration;

    fn setup_at(url: &str) -> DatabaseSetup {
        DatabaseSetup {
            url: url.to_owned(),
            password: None,
            manifest: DbManifest::default(),
        }
    }

    #[test]
    fn a_relative_sqlite_path_is_rooted_at_the_project_and_its_directory_created() {
        let project = std::env::temp_dir().join(format!("k_db_rooted_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let rooted = setup_at("sqlite:.k/dev.sqlite")
            .rooted_at(&project)
            .expect("rooted");
        assert_eq!(
            rooted.url,
            format!("sqlite:{}", project.join(".k/dev.sqlite").display())
        );
        assert!(project.join(".k").is_dir(), "the directory is created");
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn absolute_memory_and_postgres_urls_are_kept() {
        let project = std::path::Path::new("/nowhere/project");
        let absolute = std::env::temp_dir().join("k_db_absolute.sqlite");
        let url = format!("sqlite:{}", absolute.display());
        for url in [url.as_str(), "sqlite::memory:", "postgres://k@db:5432/k"] {
            assert_eq!(setup_at(url).rooted_at(project).expect("kept").url, url);
        }
    }

    fn statement(sql: &str) -> StatementSql {
        StatementSql {
            sqlite: sql.to_owned(),
            postgres: sql.to_owned(),
        }
    }

    fn setup() -> DatabaseSetup {
        DatabaseSetup {
            url: "sqlite::memory:".into(),
            password: None,
            manifest: DbManifest {
                statements: vec![
                    statement("SELECT \"id\", \"email\" FROM \"account\" ORDER BY \"id\""),
                    statement("INSERT INTO \"account\" (\"id\", \"email\") VALUES (?1, ?2)"),
                    statement("SELECT COUNT(*) FROM \"account\""),
                ],
                migrations: vec![Migration {
                    name: "0001_init".into(),
                    sqlite: vec![
                        "CREATE TABLE \"account\" (\"id\" TEXT NOT NULL PRIMARY KEY, \"email\" TEXT NOT NULL)".into(),
                        "CREATE UNIQUE INDEX \"account_email_key\" ON \"account\" (\"email\")".into(),
                    ],
                    postgres: vec![],
                }],
                unique: vec![UniqueIndex {
                    table: "account".into(),
                    columns: vec!["email".into()],
                    index: "account_email_key".into(),
                    field: "email".into(),
                }],
            },
        }
    }

    fn text(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    fn block_on<T>(f: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(f)
    }

    #[test]
    fn sql_scripts_split_on_semicolons_outside_quotes_and_comments() {
        assert_eq!(
            split_sql(
                "-- a comment; still a comment\nINSERT INTO t VALUES ('a;b', \"c;\");\n/* x; */ DELETE FROM t ;\n\n  ;"
            ),
            vec![
                "INSERT INTO t VALUES ('a;b', \"c;\")",
                "/* x; */ DELETE FROM t",
            ]
        );
        assert_eq!(split_sql("SELECT 'it''s'"), vec!["SELECT 'it''s'"]);
        assert!(split_sql("-- only a comment\n").is_empty());
    }

    #[test]
    fn a_seed_script_runs_in_one_transaction() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            let script = "INSERT INTO account (id, email) VALUES ('a', 'a@x');\nINSERT INTO account (id, email) VALUES ('b', 'b@x');";
            assert_eq!(db.run_script(script).await, Ok(2));
            // The second statement fails: the first is rolled back with it.
            let failing = "INSERT INTO account (id, email) VALUES ('c', 'c@x');\nINSERT INTO account (id, email) VALUES ('d', 'a@x');";
            assert!(db.run_script(failing).await.is_err());
            assert_eq!(db.query(2, vec![], 0).await, Ok(vec![vec![Value::Int(2)]]));
        });
    }

    #[test]
    fn opens_migrates_and_runs_statements_by_id() {
        let (db, applied) = ProcessDatabase::open(setup()).expect("opens");
        assert_eq!(applied, ["0001_init"]);
        block_on(async {
            assert_eq!(
                db.execute(1, vec![text("a"), text("a@example.com")], 0)
                    .await,
                Ok(WriteOutcome::Count(1))
            );
            assert_eq!(
                db.execute(1, vec![text("b"), text("a@example.com")], 0)
                    .await,
                Ok(WriteOutcome::Unique("email".into()))
            );
            // A primary-key violation isn't a typed outcome.
            assert!(
                db.execute(1, vec![text("a"), text("c@example.com")], 0)
                    .await
                    .is_err()
            );
            assert_eq!(
                db.query(0, vec![], 0).await,
                Ok(vec![vec![text("a"), text("a@example.com")]])
            );
            assert!(db.query(99, vec![], 0).await.is_err());
            assert!(db.ping(Duration::from_secs(2)).await);
        });
    }

    #[test]
    fn transactions_commit_roll_back_and_nest() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            let tx = db.begin().await.expect("begins");
            db.execute(1, vec![text("a"), text("a@x")], tx)
                .await
                .expect("inserts");
            db.savepoint(tx, 1, Savepoint::Begin)
                .await
                .expect("savepoint");
            db.execute(1, vec![text("b"), text("b@x")], tx)
                .await
                .expect("inserts");
            db.savepoint(tx, 1, Savepoint::Rollback)
                .await
                .expect("rolls back");
            db.finish(tx, true).await.expect("commits");
            assert_eq!(db.query(2, vec![], 0).await, Ok(vec![vec![Value::Int(1)]]));
            assert_eq!(db.open_transactions(), 0);

            let tx = db.begin().await.expect("begins");
            db.execute(1, vec![text("c"), text("c@x")], tx)
                .await
                .expect("inserts");
            db.finish(tx, false).await.expect("rolls back");
            assert_eq!(db.query(2, vec![], 0).await, Ok(vec![vec![Value::Int(1)]]));
            // A finished transaction can't be used or finished again.
            assert!(db.query(2, vec![], tx).await.is_err());
            assert!(db.finish(tx, true).await.is_err());
            assert!(db.savepoint(0, 1, Savepoint::Begin).await.is_err());
        });
    }

    #[test]
    fn a_statement_outside_waits_for_the_transaction() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            let tx = db.begin().await.expect("begins");
            db.execute(1, vec![text("a"), text("a@x")], tx)
                .await
                .expect("inserts");
            let outside = {
                let db = Arc::clone(&db);
                tokio::spawn(async move { db.query(2, vec![], 0).await })
            };
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(!outside.is_finished(), "waits for the lease");
            db.finish(tx, true).await.expect("commits");
            assert_eq!(
                outside.await.expect("joins"),
                Ok(vec![vec![Value::Int(1)]]),
                "sees the committed row, never the open transaction"
            );
        });
    }

    #[test]
    fn a_failing_migration_is_an_error_without_secrets() {
        let mut s = setup();
        s.manifest.migrations[0]
            .sqlite
            .push("CREATE TABLE broken (".into());
        let err = ProcessDatabase::open(s).expect_err("fails");
        assert!(
            matches!(err, DbError::Migration { ref name, .. } if name == "0001_init"),
            "{err}"
        );
        let mut s = setup();
        s.url = "postgres://user:hunter2@127.0.0.1:1/x".into();
        let err = ProcessDatabase::open(s).expect_err("fails").to_string();
        assert!(!err.contains("hunter2"), "{err}");
        let mut s = setup();
        s.url = "mysql://x".into();
        assert!(matches!(
            ProcessDatabase::open(s),
            Err(DbError::InvalidUrl(_))
        ));
    }

    #[test]
    fn json_parameters_and_values_convert() {
        assert_eq!(param_from_json(serde_json::json!(null)), Ok(Value::Null));
        assert_eq!(
            param_from_json(serde_json::json!(true)),
            Ok(Value::Bool(true))
        );
        assert_eq!(param_from_json(serde_json::json!(42)), Ok(Value::Int(42)));
        assert_eq!(
            param_from_json(serde_json::json!(1.5)),
            Ok(Value::Double(1.5))
        );
        assert_eq!(param_from_json(serde_json::json!("x")), Ok(text("x")));
        assert!(param_from_json(serde_json::json!([1])).is_err());
        assert!(param_from_json(serde_json::json!({"a": 1})).is_err());
        assert_eq!(
            value_to_json(Value::Double(f64::NAN)),
            serde_json::Value::Null
        );
        assert_eq!(
            value_to_json(Value::Bytes(vec![1, 2])),
            serde_json::json!([1, 2])
        );
        assert_eq!(value_to_json(Value::Int(-3)), serde_json::json!(-3));
    }

    #[test]
    fn jobs_are_claimed_once_retried_buried_and_requeued() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            // A job in a rolled-back transaction never becomes visible.
            let tx = db.begin().await.expect("begins");
            db.enqueue_job("gone".into(), "mail", "{}".into(), 0, 0, tx)
                .await
                .expect("enqueues");
            db.finish(tx, false).await.expect("rolls back");
            db.enqueue_job("a".into(), "mail", r#"{"to":1}"#.into(), 100, 0, 0)
                .await
                .expect("enqueues");
            assert!(
                db.claim_jobs(99, 10, 1000)
                    .await
                    .expect("claims")
                    .is_empty(),
                "not due yet"
            );
            let claimed = db.claim_jobs(100, 10, 1000).await.expect("claims");
            assert_eq!(
                claimed,
                vec![ClaimedJob {
                    id: "a".into(),
                    name: "mail".into(),
                    payload: r#"{"to":1}"#.into(),
                    attempt: 1
                }]
            );
            assert!(
                db.claim_jobs(200, 10, 1000)
                    .await
                    .expect("claims")
                    .is_empty(),
                "leased"
            );
            // An expired lease makes it claimable again (a dead worker).
            assert_eq!(
                db.claim_jobs(1101, 10, 1000).await.expect("claims")[0].attempt,
                2
            );
            db.finish_job(
                "a",
                JobFate::Retry {
                    run_after: 5000,
                    error: "down".into(),
                },
                1200,
            )
            .await
            .expect("retries");
            let rows = db.list_jobs(false).await.expect("lists");
            assert_eq!(rows[0].status, "queued");
            assert_eq!(rows[0].last_error.as_deref(), Some("down"));
            assert_eq!(
                db.claim_jobs(5000, 10, 1000).await.expect("claims")[0].attempt,
                3
            );
            db.finish_job(
                "a",
                JobFate::Dead {
                    error: "still down".into(),
                },
                5100,
            )
            .await
            .expect("buries");
            assert!(
                db.claim_jobs(1_000_000, 10, 1000)
                    .await
                    .expect("claims")
                    .is_empty()
            );
            let dead = db.list_jobs(true).await.expect("lists");
            assert_eq!(dead.len(), 1);
            assert_eq!((dead[0].attempts, dead[0].at), (3, 5100));
            assert!(!db.requeue_job("nope", 6000).await.expect("requeues"));
            assert!(db.requeue_job("a", 6000).await.expect("requeues"));
            let again = db.claim_jobs(6000, 10, 1000).await.expect("claims");
            assert_eq!(again[0].attempt, 1, "fresh attempts");
            // A run unfinished at shutdown goes back without counting.
            db.finish_job("a", JobFate::Release, 6100)
                .await
                .expect("releases");
            assert_eq!(
                db.claim_jobs(6100, 10, 1000).await.expect("claims")[0].attempt,
                1
            );
            db.finish_job("a", JobFate::Done, 6200).await.expect("done");
            assert!(db.list_jobs(false).await.expect("lists").is_empty());
        });
    }

    #[test]
    fn a_schedule_due_time_is_claimed_once() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            let claim = |name: &'static str, due: i64| {
                let db = Arc::clone(&db);
                async move {
                    db.claim_schedule(name, due, format!("{name}-{due}"), due)
                        .await
                        .expect("claims")
                }
            };
            assert_eq!(claim("tick", 1000).await, ScheduleClaim::Enqueued);
            assert_eq!(claim("tick", 1000).await, ScheduleClaim::Taken);
            assert_eq!(claim("tick", 999).await, ScheduleClaim::Taken);
            assert_eq!(claim("other", 1000).await, ScheduleClaim::Enqueued);
            let rows = db.list_jobs(false).await.expect("lists");
            assert_eq!(rows.len(), 2, "one run per schedule: {rows:?}");
        });
    }

    /// The ids of the unfinished runs of `name`.
    async fn runs(db: &ProcessDatabase, name: &str) -> Vec<String> {
        db.list_jobs(false)
            .await
            .expect("lists")
            .into_iter()
            .filter(|r| r.name == name && r.status != "dead")
            .map(|r| r.id)
            .collect()
    }

    #[test]
    fn a_due_time_while_the_previous_run_is_queued_is_skipped() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            assert_eq!(
                db.claim_schedule("tick", 1000, "a".into(), 1000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Enqueued
            );
            // Nobody ran it yet: the next two due times coalesce into it.
            for due in [2000, 3000] {
                assert_eq!(
                    db.claim_schedule("tick", due, format!("t{due}"), due)
                        .await
                        .expect("claims"),
                    ScheduleClaim::Skipped
                );
            }
            assert_eq!(runs(&db, "tick").await, vec!["a"]);
            // A skipped due time still moved `last_due` forward.
            assert_eq!(
                db.claim_schedule("tick", 3000, "b".into(), 3000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Taken
            );
            // A retry waiting for its backoff is unfinished too.
            assert_eq!(
                db.claim_jobs(3000, 10, 1000).await.expect("claims").len(),
                1
            );
            db.finish_job(
                "a",
                JobFate::Retry {
                    run_after: 9000,
                    error: "down".into(),
                },
                3100,
            )
            .await
            .expect("retries");
            assert_eq!(
                db.claim_schedule("tick", 4000, "c".into(), 4000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Skipped
            );
            assert_eq!(runs(&db, "tick").await, vec!["a"]);
        });
    }

    #[test]
    fn a_due_time_while_the_previous_run_is_running_is_skipped() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            db.claim_schedule("tick", 1000, "a".into(), 1000)
                .await
                .expect("claims");
            assert_eq!(
                db.claim_jobs(1000, 10, 60_000).await.expect("claims").len(),
                1
            );
            assert_eq!(
                db.claim_schedule("tick", 2000, "b".into(), 2000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Skipped
            );
            // The worker that ran it died: its lease recovers the run, and
            // the schedule still waits for it.
            let again = db.claim_jobs(70_000, 10, 60_000).await.expect("claims");
            assert_eq!((again[0].id.as_str(), again[0].attempt), ("a", 2));
            assert_eq!(
                db.claim_schedule("tick", 70_000, "c".into(), 70_000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Skipped
            );
            assert_eq!(runs(&db, "tick").await, vec!["a"]);
        });
    }

    #[test]
    fn a_due_time_after_the_previous_run_finished_is_enqueued() {
        let (db, _) = ProcessDatabase::open(setup()).expect("opens");
        block_on(async {
            db.claim_schedule("tick", 1000, "a".into(), 1000)
                .await
                .expect("claims");
            db.claim_jobs(1000, 10, 60_000).await.expect("claims");
            db.finish_job("a", JobFate::Done, 1500).await.expect("done");
            assert_eq!(
                db.claim_schedule("tick", 2000, "b".into(), 2000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Enqueued
            );
            // A dead run doesn't hold the schedule back either.
            db.claim_jobs(2000, 10, 60_000).await.expect("claims");
            db.finish_job("b", JobFate::Dead { error: "no".into() }, 2500)
                .await
                .expect("buries");
            assert_eq!(
                db.claim_schedule("tick", 3000, "c".into(), 3000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Enqueued
            );
            assert_eq!(runs(&db, "tick").await, vec!["c"]);
        });
    }

    #[test]
    fn two_replicas_never_overlap_a_schedule() {
        let dir = std::env::temp_dir().join(format!(
            "k_db_schedule_replicas_{}_{}",
            std::process::id(),
            jobs_now_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("a directory");
        let url = format!("sqlite:{}", dir.join("app.sqlite").display());
        let open = || {
            let mut s = setup();
            s.url.clone_from(&url);
            ProcessDatabase::open(s).expect("opens").0
        };
        let (a, b) = (open(), open());
        block_on(async {
            // Both replicas see due time 1000: one enqueues it.
            let first = [
                a.claim_schedule("tick", 1000, "a1".into(), 1000)
                    .await
                    .expect("claims"),
                b.claim_schedule("tick", 1000, "b1".into(), 1000)
                    .await
                    .expect("claims"),
            ];
            assert_eq!(first, [ScheduleClaim::Enqueued, ScheduleClaim::Taken]);
            // Replica b runs it; at the next due time neither enqueues.
            assert_eq!(
                b.claim_jobs(1000, 10, 60_000).await.expect("claims").len(),
                1
            );
            let second = [
                b.claim_schedule("tick", 2000, "b2".into(), 2000)
                    .await
                    .expect("claims"),
                a.claim_schedule("tick", 2000, "a2".into(), 2000)
                    .await
                    .expect("claims"),
            ];
            assert_eq!(second, [ScheduleClaim::Skipped, ScheduleClaim::Taken]);
            b.finish_job("a1", JobFate::Done, 2500).await.expect("done");
            assert_eq!(
                a.claim_schedule("tick", 3000, "a3".into(), 3000)
                    .await
                    .expect("claims"),
                ScheduleClaim::Enqueued
            );
            assert_eq!(runs(&b, "tick").await, vec!["a3"]);
        });
        drop((a, b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn jobs_now_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    }
}
