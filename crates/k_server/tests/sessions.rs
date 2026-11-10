//! Sessions, rate limits and OAuth state in the app's database
//! (`docs/design/26-auth-and-sessions.md`, `docs/compiler/server.md` §9):
//! what a request script can't show, because it needs a restart or two
//! replicas. Each test serves `tests/projects/sessions-db` on a fresh
//! SQLite file, from one or two servers at once.

// Tests fail by panicking.
#![allow(clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, ConfigValue, resolve};
use k_server::testing::{HttpResponse, send};
use k_server::{JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions};

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/projects/sessions-db")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    for e in std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
    {
        let path = e.path();
        if path.is_dir() {
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        out.push((
            ProjectPath::new(&relative).expect("a project path"),
            std::fs::read_to_string(&path).expect("text"),
        ));
    }
}

fn build() -> Output {
    let dir = project();
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment: Environment::Development,
            entry: Entry::App,
        },
    )
    .expect("generates")
}

/// A fresh SQLite file, removed afterwards.
struct TempDb(PathBuf);

impl TempDb {
    fn new(tag: &str) -> TempDb {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("k_sessions_{tag}_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDb(dir)
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.0.join("app.db").display())
    }

    /// The rows of `sql`, as text.
    fn rows(&self, sql: &str) -> Vec<Vec<String>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let db = k_db::Database::connect(&self.url(), &k_db::DatabaseOptions::default())
                .await
                .expect("connects");
            db.query(sql, &[])
                .await
                .expect("queries")
                .into_iter()
                .map(|r| {
                    r.values
                        .into_iter()
                        .map(|v| match v {
                            k_db::Value::Text(t) => t,
                            other => format!("{other:?}"),
                        })
                        .collect()
                })
                .collect()
        })
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn start(output: &Output, db: &TempDb) -> Server {
    start_with(output, db, &[])
}

/// [`start`] with extra configuration, as `.env` lines would give it.
fn start_with(output: &Output, db: &TempDb, extra: &[(&str, &str)]) -> Server {
    try_start_with(output, db, extra).expect("starts")
}

/// [`start_with`], returning why the server didn't start.
fn try_start_with(output: &Output, db: &TempDb, extra: &[(&str, &str)]) -> Result<Server, String> {
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().expect("an entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 2;
    o.shutdown_grace = Duration::from_secs(5);
    o.log = LogOutput::Capture;
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    let env = k_server::config::DevEnvironment {
        root: project(),
        dotenv: [("DATABASE_URL".to_owned(), db.url())]
            .into_iter()
            .chain(
                extra
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
            )
            .collect(),
        with_process: false,
    };
    o.config = resolve(&fields, &env).expect("valid configuration");
    assert!(matches!(
        o.config.get("DATABASE_URL"),
        Some(ConfigValue::String(_))
    ));
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    assert!(o.database.is_some(), "the project has a database");
    Server::start(o).map_err(|e| e.to_string())
}

/// A browser: its cookie jar (the session cookie and the guest cookie),
/// against one server or another, and the `X-Forwarded-For` header the
/// proxy in front of it adds, if any.
#[derive(Default)]
struct Browser {
    cookie: Option<String>,
    guest: Option<String>,
    forwarded_for: Option<String>,
}

impl Browser {
    fn rpc(&mut self, server: &Server, method: &str, body: &str) -> HttpResponse {
        let r = self.request(server, method, body);
        self.take(&r);
        r
    }

    /// Sends rpc `method` with the jar's cookies, leaving the jar as it is.
    fn request(&self, server: &Server, method: &str, body: &str) -> HttpResponse {
        let mut headers = vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("connect-protocol-version".to_owned(), "1".to_owned()),
            ("origin".to_owned(), "http://localhost".to_owned()),
        ];
        let jar: Vec<String> = [("k_session", &self.cookie), ("k_guest", &self.guest)]
            .into_iter()
            .filter_map(|(n, v)| v.as_ref().map(|v| format!("{n}={v}")))
            .collect();
        if !jar.is_empty() {
            headers.push(("cookie".to_owned(), jar.join("; ")));
        }
        if let Some(f) = &self.forwarded_for {
            headers.push(("x-forwarded-for".to_owned(), f.clone()));
        }
        send(
            server.local_addr(),
            "POST",
            &format!("/sdb.accounts.AuthProtocol/{method}"),
            &headers,
            body.as_bytes(),
        )
        .expect("answers")
    }

    /// Stores the cookies response `r` sets, as a browser does when it
    /// arrives.
    fn take(&mut self, r: &HttpResponse) {
        for (_, v) in r.headers.iter().filter(|(n, _)| n == "set-cookie") {
            let Some((name, value)) = v.split(';').next().and_then(|p| p.split_once('=')) else {
                continue;
            };
            let value = (!value.is_empty()).then(|| value.to_owned());
            match name {
                "k_session" => self.cookie = value,
                "k_guest" => self.guest = value,
                _ => {}
            }
        }
    }

    fn value(&mut self, server: &Server, method: &str, body: &str) -> serde_json::Value {
        let r = self.rpc(server, method, body);
        assert_eq!(
            r.status,
            200,
            "{method}: {}",
            String::from_utf8_lossy(&r.body)
        );
        let j: serde_json::Value = serde_json::from_slice(&r.body).expect("JSON");
        j.get("value").cloned().unwrap_or(serde_json::Value::Null)
    }
}

#[test]
fn a_session_survives_a_restart() {
    let output = build();
    let db = TempDb::new("restart");
    let mut ada = Browser::default();
    let first = start(&output, &db);
    ada.value(&first, "SignIn", r#"{"name": "ada"}"#);
    assert_eq!(ada.value(&first, "Status", "{}"), "signed in as ada");
    first.shutdown().expect("stops");
    let second = start(&output, &db);
    assert_eq!(
        ada.value(&second, "Status", "{}"),
        "signed in as ada",
        "the new process reads the session from k_session"
    );
    second.shutdown().expect("stops");
}

#[test]
fn replicas_share_sessions_and_revocation() {
    let output = build();
    let db = TempDb::new("replicas");
    let (a, b) = (start(&output, &db), start(&output, &db));
    let mut ada = Browser::default();
    ada.value(&a, "SignIn", r#"{"name": "ada"}"#);
    assert_eq!(ada.value(&b, "Status", "{}"), "signed in as ada");
    // A second browser of the same user, signed in on the other replica.
    let mut phone = Browser::default();
    phone.value(&b, "SignIn", r#"{"name": "ada"}"#);
    assert_eq!(phone.value(&a, "Status", "{}"), "signed in as ada");
    // Signing out everywhere on one replica ends both on the other.
    ada.value(&b, "SignOutEverywhere", "{}");
    assert_eq!(ada.value(&a, "Status", "{}"), "guest");
    assert_eq!(phone.value(&a, "Status", "{}"), "guest");
    // Signing out ends a session for every replica.
    let mut bob = Browser::default();
    bob.value(&a, "SignIn", r#"{"name": "bob"}"#);
    let token = bob.cookie.clone().expect("a cookie");
    bob.value(&b, "SignOut", "{}");
    let mut replay = Browser {
        cookie: Some(token),
        ..Browser::default()
    };
    assert_eq!(
        replay.value(&a, "Status", "{}"),
        "guest",
        "revocation is immediate everywhere"
    );
    a.shutdown().expect("stops");
    b.shutdown().expect("stops");
}

#[test]
fn the_table_holds_hashes_never_the_cookie_token() {
    let output = build();
    let db = TempDb::new("hashed");
    let server = start(&output, &db);
    let mut ada = Browser::default();
    ada.value(&server, "SignIn", r#"{"name": "ada"}"#);
    let token = ada.cookie.clone().expect("a cookie");
    let rows = db.rows("SELECT id, user_json, roles FROM k_session");
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0][0], token);
    assert_eq!(rows[0][0].len(), 64, "a hex SHA-256");
    assert_eq!(rows[0][1], r#"{"name":"ada"}"#);
    assert_eq!(rows[0][2], "[]");
    server.shutdown().expect("stops");
}

#[test]
fn rate_limits_count_across_replicas() {
    let output = build();
    let db = TempDb::new("rate");
    let (a, b) = (start(&output, &db), start(&output, &db));
    let mut guest = Browser::default();
    // 4 per minute per client address, whichever replica answers.
    for server in [&a, &b, &a, &b] {
        assert_eq!(guest.value(server, "Limited", "{}"), "ok");
    }
    let refused = guest.rpc(&a, "Limited", "{}");
    assert_eq!(refused.status, 429);
    let wait: u64 = refused
        .header("retry-after")
        .and_then(|v| v.parse().ok())
        .expect("Retry-After");
    assert!((1..=60).contains(&wait), "{wait}");
    assert_eq!(guest.rpc(&b, "Limited", "{}").status, 429);
    a.shutdown().expect("stops");
    b.shutdown().expect("stops");
}

/// A browser behind a proxy that names it `client` in `X-Forwarded-For`.
fn behind_proxy(client: &str) -> Browser {
    Browser {
        forwarded_for: Some(client.to_owned()),
        ..Browser::default()
    }
}

#[test]
fn clients_behind_a_trusted_proxy_get_their_own_rate_limits() {
    // The test connects from 127.0.0.1, which plays the trusted proxy.
    let output = build();
    let db = TempDb::new("proxy_trusted");
    let server = start_with(
        &output,
        &db,
        &[("K_TRUSTED_PROXIES", "127.0.0.1/32,::1/128")],
    );
    // A client that forges a first hop is still counted by the hop the
    // proxy appended.
    let mut alice = behind_proxy("6.6.6.6, 203.0.113.1");
    let mut bob = behind_proxy("203.0.113.2");
    for _ in 0..4 {
        assert_eq!(alice.value(&server, "Limited", "{}"), "ok");
    }
    assert_eq!(alice.rpc(&server, "Limited", "{}").status, 429);
    // Changing the forged hop doesn't give alice a new bucket.
    let mut alice_again = behind_proxy("7.7.7.7, 203.0.113.1");
    assert_eq!(alice_again.rpc(&server, "Limited", "{}").status, 429);
    // Bob, behind the same proxy, has his own limit.
    for _ in 0..4 {
        assert_eq!(bob.value(&server, "Limited", "{}"), "ok");
    }
    assert_eq!(bob.rpc(&server, "Limited", "{}").status, 429);
    server.shutdown().expect("stops");
}

#[test]
fn forwarding_headers_from_an_untrusted_peer_are_ignored() {
    for trusted in [None, Some("10.0.0.0/8")] {
        let output = build();
        let db = TempDb::new("proxy_untrusted");
        let extra: Vec<(&str, &str)> = trusted
            .map(|t| ("K_TRUSTED_PROXIES", t))
            .into_iter()
            .collect();
        let server = start_with(&output, &db, &extra);
        // Every request claims another address; all share the socket's.
        for i in 0..4 {
            let mut spoofer = behind_proxy(&format!("203.0.113.{i}"));
            assert_eq!(spoofer.value(&server, "Limited", "{}"), "ok", "{trusted:?}");
        }
        let mut spoofer = behind_proxy("198.51.100.99");
        assert_eq!(
            spoofer.rpc(&server, "Limited", "{}").status,
            429,
            "{trusted:?}: a forged X-Forwarded-For doesn't open a new bucket"
        );
        server.shutdown().expect("stops");
    }
}

#[test]
fn invalid_trusted_proxies_stop_the_server_from_starting() {
    let output = build();
    let db = TempDb::new("proxy_invalid");
    let Err(err) = try_start_with(&output, &db, &[("K_TRUSTED_PROXIES", "caddy")]) else {
        panic!("a proxy list it can't read must stop the server");
    };
    assert!(err.contains("K_TRUSTED_PROXIES"), "{err}");
    assert!(err.contains("caddy"), "{err}");
}

#[test]
fn oauth_state_is_bound_to_the_session_and_single_use() {
    let output = build();
    let db = TempDb::new("oauth");
    let (a, b) = (start(&output, &db), start(&output, &db));
    // A visitor without a session gets a guest session to bind it to.
    let mut visitor = Browser::default();
    let state = visitor.value(&a, "StartOAuth", "{}");
    let state = state.as_str().expect("a state").to_owned();
    assert_eq!(state.len(), 43);
    let guest = visitor.guest.clone().expect("a guest session cookie");
    assert_eq!(visitor.cookie, None, "a guest's cookie of its own");
    assert_eq!(visitor.value(&a, "Status", "{}"), "guest");
    // Another browser can't use it, nor can a wrong state.
    let mut attacker = Browser::default();
    let body = serde_json::json!({ "state": state }).to_string();
    assert_eq!(attacker.value(&b, "FinishOAuth", &body), false);
    assert_eq!(
        visitor.value(&b, "FinishOAuth", r#"{"state": "forged"}"#),
        false,
        "a wrong state also clears the bound one"
    );
    // Issued again, it is accepted once, on any replica.
    let state = visitor.value(&a, "StartOAuth", "{}");
    let body = serde_json::json!({ "state": state }).to_string();
    assert_eq!(visitor.value(&b, "FinishOAuth", &body), true);
    assert_eq!(visitor.value(&a, "FinishOAuth", &body), false, "single use");
    assert_eq!(
        visitor.guest.as_deref(),
        Some(guest.as_str()),
        "the same guest session"
    );
    // Signing in replaces the guest session: no fixation.
    visitor.value(&a, "SignIn", r#"{"name": "ada"}"#);
    assert_ne!(visitor.cookie.as_deref(), Some(guest.as_str()));
    let mut planted = Browser {
        cookie: Some(guest),
        ..Browser::default()
    };
    assert_eq!(planted.value(&b, "Status", "{}"), "guest");
    assert_eq!(planted.value(&b, "FinishOAuth", &body), false);
    a.shutdown().expect("stops");
    b.shutdown().expect("stops");
}

/// A first-time visitor's request that issues a guest session (an OAuth
/// state here, a passkey challenge alike) and a sign-in sent before its
/// answer each set a cookie, and the browser keeps whichever arrives last:
/// in either order it stays signed in and keeps the guest's state
/// (26-auth-and-sessions.md, "Guest sessions").
#[test]
fn a_guest_session_issued_beside_a_sign_in_never_signs_it_out() {
    let output = build();
    let db = TempDb::new("race");
    let server = start(&output, &db);
    for guest_last in [true, false] {
        let mut browser = Browser::default();
        let (issued, signed) = std::thread::scope(|s| {
            let issued = s.spawn(|| browser.request(&server, "StartOAuth", "{}"));
            let signed = s.spawn(|| browser.request(&server, "SignIn", r#"{"name": "ada"}"#));
            (
                issued.join().expect("answers"),
                signed.join().expect("answers"),
            )
        });
        assert_eq!(issued.status, 200);
        assert_eq!(signed.status, 200);
        if guest_last {
            browser.take(&signed);
            browser.take(&issued);
        } else {
            browser.take(&issued);
            browser.take(&signed);
        }
        assert_eq!(
            browser.value(&server, "Status", "{}"),
            "signed in as ada",
            "guest answer last: {guest_last}"
        );
        let j: serde_json::Value = serde_json::from_slice(&issued.body).expect("JSON");
        let body = serde_json::json!({ "state": j["value"] }).to_string();
        assert_eq!(
            browser.value(&server, "FinishOAuth", &body),
            true,
            "the guest's state survives: {guest_last}"
        );
    }
    server.shutdown().expect("stops");
}
