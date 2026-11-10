//! Replicas sharing a Postgres database (`docs/design/44-channels.md`,
//! "Server isolates and replicas"; `k_server::replicas`): broadcasts,
//! spilled payloads, presence leases and session ends reach the other
//! process over `LISTEN`/`NOTIFY`, a crashed process's leases expire, and a
//! schedule never overlaps itself across replicas.
//!
//! Each test runs two [`ProcessState`]s, each with its own database
//! connection and listening connection, against a real Postgres
//! (`support/postgres.rs`: Docker, or `K_TEST_POSTGRES_URL`; skipped
//! without either unless `K_REQUIRE_DOCKER=1`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use k_server::db::{DatabaseSetup, JobFate, ProcessDatabase, ScheduleClaim};
use k_server::process::ProcessState;
use k_server::replicas::{NOTIFY_INLINE_LIMIT, ReplicaTimings};

use crate::postgres::TestPostgres;

/// Short presence intervals, so an expiry takes about a second.
fn timings() -> ReplicaTimings {
    ReplicaTimings {
        renew: Duration::from_millis(200),
        expiry: Duration::from_millis(1200),
        sweep: Duration::from_millis(200),
        reconnect: Duration::from_millis(200),
    }
}

/// A replica: a process state on database `url`, fanning out.
fn replica(url: &str) -> Arc<ProcessState> {
    let (db, _) = ProcessDatabase::open(DatabaseSetup {
        url: url.to_owned(),
        password: None,
        manifest: k_db::manifest::DbManifest::default(),
    })
    .expect("the database opens");
    let p = Arc::new(ProcessState::default().with_database(db));
    p.start_replicas(timings());
    assert!(p.replicas().is_some(), "Postgres starts the fan-out");
    p
}

/// Two replicas on a fresh database, or `None` to skip.
fn two(tag: &str) -> Option<(TestPostgres, Arc<ProcessState>, Arc<ProcessState>)> {
    let server = TestPostgres::start(tag)?;
    let url = server.database(tag);
    let a = replica(&url);
    let b = replica(&url);
    Some((server, a, b))
}

/// The next element of subscriber `id` of `p` within 10 seconds:
/// `Some(json)`, or `None` once the subscription closed.
fn next(p: &ProcessState, id: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = p.take(id) {
            return v;
        }
        assert!(
            Instant::now() < deadline,
            "an element arrives within 10 seconds"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Asserts that subscriber `id` of `p` receives nothing for `wait`.
fn quiet(p: &ProcessState, id: u64, wait: Duration) {
    std::thread::sleep(wait);
    assert_eq!(p.take(id), None, "nothing else arrived");
}

/// `members(key)` of `p`.
fn members(p: &ProcessState, key: &str) -> Vec<String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(p.presence_members(key))
}

#[test]
fn a_publish_reaches_subscribers_on_another_replica_in_order() {
    let Some((_pg, a, b)) = two("fanout") else {
        return;
    };
    let (on_b, _) = b.subscribe("room|\"general\"".into(), None);
    let (on_a, _) = a.subscribe("room|\"general\"".into(), None);
    let (other, _) = b.subscribe("room|\"other\"".into(), None);
    for i in 1..=20 {
        a.publish("room|\"general\"", &i.to_string());
    }
    for i in 1..=20 {
        assert_eq!(next(&b, on_b), Some(i.to_string()), "B gets A's in order");
    }
    for i in 1..=20 {
        assert_eq!(next(&a, on_a), Some(i.to_string()), "A gets its own once");
    }
    quiet(&a, on_a, Duration::from_millis(300));
    quiet(&b, other, Duration::from_millis(0));
    // B measured how late each of A's elements arrived; A's own elements
    // never crossed replicas.
    let lag = b
        .metrics()
        .histogram("k_broadcast_lag_seconds", &[])
        .expect("B measured the lag");
    assert_eq!(lag.count, 20);
    assert!(lag.sum >= 0.0 && lag.sum < 200.0, "{lag:?}");
    assert_eq!(a.metrics().histogram("k_broadcast_lag_seconds", &[]), None);
}

#[test]
fn an_element_over_the_notify_limit_is_spilled_and_still_arrives() {
    let Some((_pg, a, b)) = two("spill") else {
        return;
    };
    let (on_b, _) = b.subscribe("k".into(), None);
    let big = format!("\"{}\"", "x".repeat(NOTIFY_INLINE_LIMIT * 3));
    a.publish("k", "\"before\"");
    a.publish("k", &big);
    a.publish("k", "\"after\"");
    assert_eq!(next(&b, on_b).as_deref(), Some("\"before\""));
    assert_eq!(next(&b, on_b), Some(big), "the spilled element, whole");
    assert_eq!(next(&b, on_b).as_deref(), Some("\"after\""), "and in order");
}

#[test]
fn presence_spans_replicas_and_counts_a_member_once() {
    let Some((_pg, a, b)) = two("presence") else {
        return;
    };
    let key = "presence|\"main\"";
    let (watch, _) = b.subscribe(key.into(), None);
    let ana = "{\"name\":\"ana\"}";
    let bob = "{\"name\":\"bob\"}";
    let on_a = a.presence_join(key, ana);
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"joined\",\"_0\":{\"name\":\"ana\"}}")
    );
    // Ana's second tab, on the other replica: still one member.
    let on_b = b.presence_join(key, ana);
    let bobs = b.presence_join(key, bob);
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"joined\",\"_0\":{\"name\":\"bob\"}}"),
        "no second `joined` for ana"
    );
    assert_eq!(members(&a, key), vec![ana.to_owned(), bob.to_owned()]);
    assert_eq!(members(&b, key), vec![ana.to_owned(), bob.to_owned()]);
    b.presence_update(bobs, "{\"name\":\"bob\",\"away\":true}");
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"updated\",\"_0\":{\"name\":\"bob\",\"away\":true}}")
    );
    a.presence_leave(on_a);
    a.presence_leave(on_a);
    quiet(&b, watch, Duration::from_millis(300));
    b.presence_leave(on_b);
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"left\",\"_0\":{\"name\":\"ana\"}}"),
        "the last lease leaves"
    );
    assert_eq!(
        members(&a, key),
        vec!["{\"name\":\"bob\",\"away\":true}".to_owned()]
    );
}

#[test]
fn a_crashed_replicas_members_leave_once_their_leases_expire() {
    let Some((_pg, a, b)) = two("expiry") else {
        return;
    };
    let key = "presence|\"main\"";
    let (watch, _) = b.subscribe(key.into(), None);
    let _lease = a.presence_join(key, "\"ghost\"");
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"joined\",\"_0\":\"ghost\"}")
    );
    // Renewed while A lives: still there after two expiry periods.
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(members(&b, key), vec!["\"ghost\"".to_owned()]);
    // A crashes: nothing leaves, its renewals stop.
    drop(a);
    let started = Instant::now();
    assert_eq!(
        next(&b, watch).as_deref(),
        Some("{\"case\":\"left\",\"_0\":\"ghost\"}"),
        "the expired lease is swept by the other replica"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(500),
        "not at once"
    );
    assert!(members(&b, key).is_empty());
}

#[test]
fn a_session_ended_on_one_replica_closes_its_streams_on_another() {
    let Some((_pg, a, b)) = two("sessions") else {
        return;
    };
    let token = "a-session-cookie-token";
    let (stream, _) = b.subscribe(format!("k.session|{token}"), Some(token.to_owned()));
    let (other, _) = b.subscribe("k.session|other".into(), Some("other".to_owned()));
    a.delete_session(token);
    assert_eq!(next(&b, stream), None, "the stream on B closed");
    assert_eq!(b.take(other), None, "another session's stream stays open");
}

/// A process database on `url`, without the fan-out.
fn database(url: &str) -> Arc<ProcessDatabase> {
    ProcessDatabase::open(DatabaseSetup {
        url: url.to_owned(),
        password: None,
        manifest: k_db::manifest::DbManifest::default(),
    })
    .expect("the database opens")
    .0
}

/// The ids of `db`'s unfinished runs of `name`.
async fn unfinished(db: &ProcessDatabase, name: &str) -> Vec<String> {
    db.list_jobs(false)
        .await
        .expect("lists")
        .into_iter()
        .filter(|r| r.name == name && r.status != "dead")
        .map(|r| r.id)
        .collect()
}

#[test]
fn a_schedule_never_overlaps_itself_across_replicas() {
    let Some(server) = TestPostgres::start("schedule") else {
        return;
    };
    let url = server.database("schedule");
    let (a, b) = (database(&url), database(&url));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            for round in 1..=30i64 {
                let due = round * 1000;
                // Both replicas race for each due time.
                let (x, y) = tokio::join!(
                    a.claim_schedule("tick", due, format!("a{round}"), due),
                    b.claim_schedule("tick", due, format!("b{round}"), due),
                );
                let claims = [x.expect("claims"), y.expect("claims")];
                assert_eq!(
                    claims
                        .iter()
                        .filter(|c| **c == ScheduleClaim::Taken)
                        .count(),
                    1,
                    "one replica claims {due}: {claims:?}"
                );
                let runs = unfinished(&a, "tick").await;
                assert_eq!(runs.len(), 1, "never two runs at once: {runs:?}");
                // Every third run is still running at the next due time,
                // which is skipped; the others finish first.
                let expected = if round % 3 == 2 {
                    ScheduleClaim::Skipped
                } else {
                    ScheduleClaim::Enqueued
                };
                assert!(claims.contains(&expected), "round {round}: {claims:?}");
                if round % 3 == 1 {
                    assert_eq!(
                        b.claim_jobs(due, 10, 60_000).await.expect("claims").len(),
                        1
                    );
                } else {
                    let _ = a.claim_jobs(due, 10, 60_000).await.expect("claims");
                    for id in runs {
                        a.finish_job(&id, JobFate::Done, due).await.expect("done");
                    }
                }
            }
        });
}
