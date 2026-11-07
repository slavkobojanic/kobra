//! The background jobs worker (`docs/design/30-background-jobs.md`,
//! `docs/compiler/server.md` §11).
//!
//! The worker runs next to the HTTP server, on a thread of its own
//! (`k-jobs`), whenever the build has jobs and a database:
//!
//! 1. Every [`JobWorkerOptions::poll`] it enqueues the due runs of
//!    `@Schedule` jobs (each due time at most once across all replicas,
//!    through `k_job_schedule`), then claims as many due jobs as it has
//!    free slots ([`JobWorkerOptions::concurrency`], `K_JOB_CONCURRENCY`).
//! 2. Each claimed job runs in a worker isolate, delivered through the
//!    same queues as HTTP requests but marked as a job ([`JobRun`]), so the
//!    isolate runs it in a fresh container with no session. The isolate
//!    answers its outcome as JSON: `{"outcome": "done"}`, or `"retryable"`,
//!    `"permanent"` or `"panic"` with a `reason`.
//! 3. The outcome decides the row's fate ([`JobFate`]): deleted when done;
//!    queued again after a backoff ([`k_db::stdlib::backoff_ms`]) for a
//!    retryable failure or a panic with attempts left; kept as dead, and
//!    logged at error level, for a permanent failure or the last attempt.
//!
//! A claim is a lease ([`JobWorkerOptions::lease`]): a job whose worker
//! died is claimable again once its lease expires, so delivery is at
//! least once. On shutdown the worker stops claiming, waits for running
//! jobs up to the grace period, and returns unfinished ones to the queue
//! without counting the attempt.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use k_db::stdlib::JobSpec;
use tokio::sync::{oneshot, watch};

use crate::db::{ClaimedJob, JobFate, ProcessDatabase};
use crate::engine::Output;
use crate::ops::{JobRun, PendingRequest};

/// How the worker runs.
#[derive(Clone, Debug)]
pub struct JobWorkerOptions {
    /// The build's jobs.
    pub specs: Vec<JobSpec>,
    /// Jobs run at once (`K_JOB_CONCURRENCY`, 1 to 256).
    pub concurrency: usize,
    /// How often it looks for due jobs.
    pub poll: Duration,
    /// How long a claim lasts before another worker may take the job.
    pub lease: Duration,
    /// How long shutdown waits for running jobs.
    pub grace: Duration,
    /// Where its log lines go.
    pub log: Output,
}

impl JobWorkerOptions {
    /// The defaults for `specs`: 4 at once, polling every 250 ms, a
    /// 15-minute lease, a 25-second grace.
    #[must_use]
    pub fn new(specs: Vec<JobSpec>) -> JobWorkerOptions {
        JobWorkerOptions {
            specs,
            concurrency: 4,
            poll: Duration::from_millis(250),
            lease: Duration::from_secs(15 * 60),
            grace: Duration::from_secs(25),
            log: Output::Capture,
        }
    }
}

/// Hands a job run to a worker isolate; `false` when no worker took it.
pub type Dispatch = Arc<dyn Fn(PendingRequest) -> bool + Send + Sync>;

/// A running jobs worker. Dropping it without [`JobWorker::stop`] stops it
/// too.
pub struct JobWorker {
    stop: Option<watch::Sender<bool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Epoch milliseconds now.
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A random value in `-0.2..=0.2` for the backoff's jitter.
fn jitter() -> f64 {
    let mut b = [0u8; 2];
    if getrandom::fill(&mut b).is_err() {
        return 0.0;
    }
    f64::from(u16::from_le_bytes(b)) / f64::from(u16::MAX) * 0.4 - 0.2
}

/// A fresh job id: 16 random bytes, base64url.
///
/// # Errors
///
/// The operating system refused randomness.
pub fn job_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    Ok(crate::process::base64url(&b))
}

fn log(mode: Output, level: &str, message: &str) {
    if mode == Output::JsonLines {
        let line = serde_json::json!({ "level": level, "message": message });
        println!("{line}");
    }
}

/// What a run's outcome means for its row.
#[must_use]
pub fn fate(
    spec: Option<&JobSpec>,
    job: &ClaimedJob,
    outcome: &serde_json::Value,
    now: i64,
) -> JobFate {
    let Some(spec) = spec else {
        return JobFate::Dead {
            error: format!("the build has no job named `{}`", job.name),
        };
    };
    let reason = outcome["reason"].as_str().unwrap_or("").to_owned();
    match outcome["outcome"].as_str() {
        Some("done") => JobFate::Done,
        Some("permanent") => JobFate::Dead { error: reason },
        // A retryable failure, a panic, or an answer that isn't an outcome.
        other => {
            let error = match other {
                Some("panic") => format!("panic: {reason}"),
                Some("retryable") => reason,
                _ => "the run produced no outcome".to_owned(),
            };
            if job.attempt >= spec.attempts.max(1) {
                JobFate::Dead {
                    error: format!("{error} (after {} attempts)", job.attempt),
                }
            } else {
                let wait = k_db::stdlib::backoff_ms(spec.backoff_ms, job.attempt, jitter());
                JobFate::Retry {
                    run_after: now.saturating_add(i64::try_from(wait).unwrap_or(i64::MAX)),
                    error,
                }
            }
        }
    }
}

impl JobWorker {
    /// Starts the worker on its own thread.
    ///
    /// # Errors
    ///
    /// The thread couldn't be started.
    pub fn start(
        db: Arc<ProcessDatabase>,
        options: JobWorkerOptions,
        dispatch: Dispatch,
    ) -> std::io::Result<JobWorker> {
        let (stop_tx, stop_rx) = watch::channel(false);
        let thread = std::thread::Builder::new()
            .name("k-jobs".to_owned())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let local = tokio::task::LocalSet::new();
                local.block_on(&runtime, run(db, options, dispatch, stop_rx));
            })?;
        Ok(JobWorker {
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// Stops claiming, waits for running jobs (up to the grace period),
    /// returns unfinished ones to the queue and joins the thread.
    pub fn stop(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(true);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for JobWorker {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

async fn run(
    db: Arc<ProcessDatabase>,
    options: JobWorkerOptions,
    dispatch: Dispatch,
    mut stop: watch::Receiver<bool>,
) {
    let specs: Rc<BTreeMap<String, JobSpec>> = Rc::new(
        options
            .specs
            .iter()
            .map(|s| (s.name.clone(), s.clone()))
            .collect(),
    );
    let running: Rc<RefCell<BTreeSet<String>>> = Rc::new(RefCell::new(BTreeSet::new()));
    let mut scheduled: BTreeMap<String, i64> = BTreeMap::new();
    let lease = i64::try_from(options.lease.as_millis()).unwrap_or(i64::MAX);
    let concurrency = options.concurrency.clamp(1, 256);
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            () = tokio::time::sleep(options.poll) => {}
        }
        if *stop.borrow() {
            break;
        }
        let now = now_ms();
        for spec in specs.values() {
            let Some(schedule) = spec.schedule else {
                continue;
            };
            let due = schedule.latest_due(now);
            if scheduled.get(&spec.name) == Some(&due) {
                continue;
            }
            match db.claim_schedule(&spec.name, due).await {
                Ok(true) => {
                    let id = match job_id() {
                        Ok(id) => id,
                        Err(e) => {
                            log(options.log, "error", &format!("jobs: {e}"));
                            continue;
                        }
                    };
                    if let Err(e) = db
                        .enqueue_job(id, &spec.name, "{}".into(), due, now, 0)
                        .await
                    {
                        log(
                            options.log,
                            "error",
                            &format!("jobs: scheduling {}: {e}", spec.name),
                        );
                        continue;
                    }
                    scheduled.insert(spec.name.clone(), due);
                }
                Ok(false) => {
                    scheduled.insert(spec.name.clone(), due);
                }
                Err(e) => log(
                    options.log,
                    "error",
                    &format!("jobs: scheduling {}: {e}", spec.name),
                ),
            }
        }
        let free = concurrency.saturating_sub(running.borrow().len());
        if free == 0 {
            continue;
        }
        let claimed = match db.claim_jobs(now, free, lease).await {
            Ok(c) => c,
            Err(e) => {
                log(options.log, "error", &format!("jobs: claiming: {e}"));
                continue;
            }
        };
        for job in claimed {
            running.borrow_mut().insert(job.id.clone());
            let (db, specs, running, dispatch) = (
                Arc::clone(&db),
                Rc::clone(&specs),
                Rc::clone(&running),
                Arc::clone(&dispatch),
            );
            let mode = options.log;
            tokio::task::spawn_local(async move {
                let outcome = run_one(&dispatch, &job).await;
                let now = now_ms();
                let fate = fate(specs.get(&job.name), &job, &outcome, now);
                match &fate {
                    JobFate::Dead { error } => log(
                        mode,
                        "error",
                        &format!("job {} ({}) is dead: {error}", job.name, job.id),
                    ),
                    JobFate::Retry { error, .. } => log(
                        mode,
                        "warning",
                        &format!(
                            "job {} ({}) attempt {} failed: {error}",
                            job.name, job.id, job.attempt
                        ),
                    ),
                    JobFate::Done | JobFate::Release => {}
                }
                if let Err(e) = db.finish_job(&job.id, fate, now).await {
                    log(mode, "error", &format!("jobs: recording {}: {e}", job.id));
                }
                running.borrow_mut().remove(&job.id);
            });
        }
    }
    // Shutdown: wait for running jobs, then give back the unfinished.
    let deadline = Instant::now() + options.grace;
    while !running.borrow().is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let unfinished: Vec<String> = running.borrow().iter().cloned().collect();
    for id in unfinished {
        let _ = db.finish_job(&id, JobFate::Release, now_ms()).await;
    }
}

/// Runs `job` in a worker isolate and returns its outcome JSON (a
/// retryable "no outcome" when no worker took it or it never answered).
async fn run_one(dispatch: &Dispatch, job: &ClaimedJob) -> serde_json::Value {
    let (reply, answer) = oneshot::channel();
    let sent = dispatch(PendingRequest {
        method: "POST".into(),
        path: "/".into(),
        headers: Vec::new(),
        body: job.payload.clone().into_bytes(),
        remote: String::new(),
        job: Some(JobRun {
            name: job.name.clone(),
            attempt: job.attempt,
        }),
        reply,
    });
    if !sent {
        return serde_json::json!({ "outcome": "retryable", "reason": "no worker isolate is available" });
    }
    match answer.await {
        Ok(r) => serde_json::from_slice(&r.body).unwrap_or_else(|_| {
            serde_json::json!({ "outcome": "panic", "reason": "the job's answer was not an outcome" })
        }),
        Err(_) => serde_json::json!({ "outcome": "retryable", "reason": "the worker isolate stopped" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> JobSpec {
        JobSpec {
            name: "mail".into(),
            declaration: "a.MailJob".into(),
            attempts: 3,
            backoff_ms: 1000,
            schedule: None,
        }
    }

    fn job(attempt: u32) -> ClaimedJob {
        ClaimedJob {
            id: "j".into(),
            name: "mail".into(),
            payload: "{}".into(),
            attempt,
        }
    }

    #[test]
    fn outcomes_decide_the_fate() {
        let s = spec();
        let done = serde_json::json!({"outcome": "done"});
        assert_eq!(fate(Some(&s), &job(1), &done, 0), JobFate::Done);
        let retry = serde_json::json!({"outcome": "retryable", "reason": "smtp down"});
        match fate(Some(&s), &job(1), &retry, 10_000) {
            JobFate::Retry { run_after, error } => {
                assert!((10_800..=11_200).contains(&run_after), "{run_after}");
                assert_eq!(error, "smtp down");
            }
            other => panic!("{other:?}"),
        }
        match fate(Some(&s), &job(3), &retry, 0) {
            JobFate::Dead { error } => assert_eq!(error, "smtp down (after 3 attempts)"),
            other => panic!("{other:?}"),
        }
        let permanent = serde_json::json!({"outcome": "permanent", "reason": "no order"});
        assert_eq!(
            fate(Some(&s), &job(1), &permanent, 0),
            JobFate::Dead {
                error: "no order".into()
            }
        );
        let panic = serde_json::json!({"outcome": "panic", "reason": "bug"});
        assert!(
            matches!(fate(Some(&s), &job(1), &panic, 0), JobFate::Retry { error, .. } if error == "panic: bug")
        );
        assert!(matches!(
            fate(None, &job(1), &done, 0),
            JobFate::Dead { .. }
        ));
        assert!(matches!(
            fate(Some(&s), &job(1), &serde_json::json!(42), 0),
            JobFate::Retry { .. }
        ));
    }

    #[test]
    fn jitter_stays_in_bounds() {
        for _ in 0..100 {
            let j = jitter();
            assert!((-0.2..=0.2).contains(&j), "{j}");
        }
    }
}
