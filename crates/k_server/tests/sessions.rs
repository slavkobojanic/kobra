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
    Server::start(o).expect("starts")
}

/// A browser: its cookie jar, against one server or another.
#[derive(Default)]
struct Browser {
    cookie: Option<String>,
}

impl Browser {
    fn rpc(&mut self, server: &Server, method: &str, body: &str) -> HttpResponse {
        let mut headers = vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("connect-protocol-version".to_owned(), "1".to_owned()),
            ("origin".to_owned(), "http://localhost".to_owned()),
        ];
        if let Some(c) = &self.cookie {
            headers.push(("cookie".to_owned(), format!("k_session={c}")));
        }
        let r = send(
            server.local_addr(),
            "POST",
            &format!("/sdb.accounts.AuthProtocol/{method}"),
            &headers,
            body.as_bytes(),
        )
        .expect("answers");
        if let Some(v) = r.header("set-cookie") {
            let value = v
                .split(';')
                .next()
                .and_then(|p| p.split_once('='))
                .map(|(_, v)| v.to_owned())
                .unwrap_or_default();
            self.cookie = (!value.is_empty()).then_some(value);
        }
        r
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
    let guest = visitor.cookie.clone().expect("a guest session cookie");
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
        visitor.cookie.as_deref(),
        Some(guest.as_str()),
        "the same guest session"
    );
    // Signing in replaces the guest session: no fixation.
    visitor.value(&a, "SignIn", r#"{"name": "ada"}"#);
    assert_ne!(visitor.cookie.as_deref(), Some(guest.as_str()));
    let mut planted = Browser {
        cookie: Some(guest),
    };
    assert_eq!(planted.value(&b, "Status", "{}"), "guest");
    assert_eq!(planted.value(&b, "FinishOAuth", &body), false);
    a.shutdown().expect("stops");
    b.shutdown().expect("stops");
}
