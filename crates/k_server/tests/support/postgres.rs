//! A real Postgres server for the tests that need one: replicas sharing a
//! database (`replicas.rs` here, huddle's replicas in `k_cli`'s
//! `examples.rs`, which includes this file).
//!
//! [`TestPostgres::start`] uses the server `K_TEST_POSTGRES_URL` names
//! (`k_db::testing::POSTGRES_URL_ENV`) when it is set; otherwise it starts
//! a throwaway `postgres:17-alpine` container with Docker on a loopback
//! port in 5200–5999, trusting local connections, and removes it when
//! dropped. Without Docker the test is skipped (it returns `None` and says
//! so), unless `K_REQUIRE_DOCKER=1`, which turns a missing Docker into a
//! failure, for CI. Each test gets a database of its own
//! ([`TestPostgres::database`]).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The variable that makes a missing Docker a failure.
pub const REQUIRE_DOCKER: &str = "K_REQUIRE_DOCKER";

/// The variable naming an existing server to use instead of Docker.
pub const POSTGRES_URL: &str = "K_TEST_POSTGRES_URL";

/// The image of the throwaway server.
const IMAGE: &str = "postgres:17-alpine";

/// A Postgres server for one test.
pub struct TestPostgres {
    /// A `postgres://` URL of the server's maintenance database.
    pub url: String,
    /// The container this started, removed on drop.
    container: Option<String>,
}

impl TestPostgres {
    /// The server to test against, or `None` when there is none and the
    /// test should be skipped (see the module docs).
    ///
    /// # Panics
    ///
    /// With `K_REQUIRE_DOCKER=1` and no Docker, or when the container
    /// doesn't become ready within a minute.
    #[must_use]
    pub fn start(tag: &str) -> Option<TestPostgres> {
        if let Ok(url) = std::env::var(POSTGRES_URL) {
            return Some(TestPostgres {
                url,
                container: None,
            });
        }
        let required = std::env::var(REQUIRE_DOCKER).is_ok_and(|v| v == "1");
        let docker = Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !docker {
            assert!(
                !required,
                "{REQUIRE_DOCKER}=1, but Docker isn't available to start Postgres"
            );
            eprintln!(
                "skipped: no Docker to start Postgres (set {POSTGRES_URL} or {REQUIRE_DOCKER}=1)"
            );
            return None;
        }
        let name = format!(
            "k-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        // A port in the tests' range, chosen from the process id and
        // retried on a clash.
        let mut started = false;
        let mut port = 0;
        for attempt in 0..20u32 {
            port = 5200 + (std::process::id() + attempt * 37) % 800;
            let out = Command::new("docker")
                .args([
                    "run",
                    "--detach",
                    "--rm",
                    "--name",
                    &name,
                    "--publish",
                    &format!("127.0.0.1:{port}:5432"),
                    "--env",
                    "POSTGRES_HOST_AUTH_METHOD=trust",
                    "--env",
                    "POSTGRES_USER=k_test",
                    "--env",
                    "POSTGRES_DB=k_test",
                    "--tmpfs",
                    "/var/lib/postgresql/data",
                    IMAGE,
                ])
                .output()
                .expect("docker runs");
            if out.status.success() {
                started = true;
                break;
            }
            let _ = Command::new("docker")
                .args(["rm", "--force", &name])
                .output();
        }
        assert!(started, "a Postgres container starts");
        let server = TestPostgres {
            url: format!("postgres://k_test@127.0.0.1:{port}/k_test"),
            container: Some(name.clone()),
        };
        // Ready when it answers a query from outside the container (the
        // entrypoint restarts the server once after initializing it).
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if query_ok(&server.url) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Postgres is ready within a minute"
            );
            std::thread::sleep(Duration::from_millis(250));
        }
        Some(server)
    }

    /// Creates a fresh database on the server and returns its URL.
    ///
    /// # Panics
    ///
    /// When the server refuses.
    #[must_use]
    pub fn database(&self, tag: &str) -> String {
        let name = format!(
            "k_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        execute(&self.url, &format!("CREATE DATABASE {name}"));
        with_database(&self.url, &name)
    }
}

impl Drop for TestPostgres {
    fn drop(&mut self) {
        if let Some(name) = &self.container {
            let _ = Command::new("docker")
                .args(["rm", "--force", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Runs `f` on a fresh current-thread runtime.
fn block_on<T>(f: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(f)
}

/// `true` when the server at `url` answers `SELECT 1` within 2 seconds.
fn query_ok(url: &str) -> bool {
    block_on(async {
        let Ok(Ok(db)) = tokio::time::timeout(
            Duration::from_secs(2),
            k_db::Database::connect(url, &k_db::DatabaseOptions::default()),
        )
        .await
        else {
            return false;
        };
        db.ping(Duration::from_secs(2)).await
    })
}

/// Runs `sql` on the server at `url`.
fn execute(url: &str, sql: &str) {
    block_on(async {
        let db = k_db::Database::connect(url, &k_db::DatabaseOptions::default())
            .await
            .expect("connects");
        db.execute(sql, &[]).await.expect("executes");
    });
}

/// `url` naming database `name` instead of its own.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = url
        .split_once('?')
        .map_or((url, None), |(b, q)| (b, Some(q)));
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let path = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    let mut out = format!("{}/{name}", &base[..path]);
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}
