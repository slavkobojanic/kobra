//! The example apps end to end (`docs/compiler/testing.md`, "Examples"):
//! each one in `examples/` is built with `k build`, its `dist/app` is
//! started on a free port with a fresh in-memory SQLite database, and its
//! main page and one rpc (or, for the apps without rpcs, one hashed asset)
//! must answer. `SIGTERM` must then shut it down cleanly.
//!
//! A static-only app (`todos`, `kanban`; `docs/design/38-deployment.md`,
//! "Static-only apps") has no `dist/app`: its `dist/public` is served by
//! the tests' static host, as the examples' nginx images serve it, and
//! every URL must answer its `index.html`.
//!
//! The configuration is the minimum each app requires, with placeholder
//! values; nothing reaches an outside service.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A well-formed Argon2id hash (PHC format) that matches no password: the
/// shop refuses to start without one, and these tests never sign in.
const UNUSABLE_ADMIN_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

use crate::static_host;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("k_examples_{tag}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A response: status, headers (names lower case) and body.
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// One HTTP/1.1 exchange with `Connection: close`.
fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Option<Response> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        req.push_str(&format!("{n}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, rest) = text.split_once("\r\n\r\n")?;
    let mut lines = head.lines();
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(n, v)| n == "transfer-encoding" && v.contains("chunked"));
    let body = if chunked {
        dechunk(rest)
    } else {
        rest.to_owned()
    };
    Some(Response {
        status,
        headers,
        body,
    })
}

fn dechunk(mut s: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = s.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if n == 0 || rest.len() < n {
            break;
        }
        out.push_str(&rest[..n]);
        s = rest[n..].trim_start_matches("\r\n");
    }
    out
}

fn rpc(port: u16, path: &str, body: &str) -> Response {
    http(
        port,
        "POST",
        path,
        &[
            ("content-type", "application/json"),
            ("connect-protocol-version", "1"),
        ],
        body,
    )
    .expect("the rpc answers")
}

/// A running `dist/app`, stopped with `SIGTERM` by [`App::stop`].
struct App {
    child: Child,
    port: u16,
    name: String,
}

impl App {
    /// Builds `examples/<name>` (a project, or a workspace member such as
    /// `shop/paybox`) into a temporary `dist/` and starts it.
    fn start(name: &str, out: &Path, env: &[(&str, &str)]) -> App {
        App::serve(name, &App::build(name, out), env)
    }

    /// Builds `examples/<name>` into `out/dist` and returns its `dist/app`.
    fn build(name: &str, out: &Path) -> PathBuf {
        let project = repo().join("examples").join(name);
        let dist = out.join("dist");
        let built = Command::new(env!("CARGO_BIN_EXE_k"))
            .arg("build")
            .arg(&project)
            .arg("--out")
            .arg(&dist)
            .output()
            .expect("k build runs");
        assert!(
            built.status.success(),
            "k build {name}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let app = dist.join("app");
        assert!(app.is_file(), "{name}: dist/app exists");
        app
    }

    /// Starts the built `app` of `examples/<name>` on a free port.
    fn serve(name: &str, app: &Path, env: &[(&str, &str)]) -> App {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .expect("a free port");
        App::serve_on(name, app, env, port)
    }

    /// Starts the built `app` of `examples/<name>` on `port`.
    fn serve_on(name: &str, app: &Path, env: &[(&str, &str)], port: u16) -> App {
        let mut cmd = Command::new(app);
        cmd.arg("serve")
            .env_remove("DATABASE_URL")
            .env_remove("DATABASE_PASSWORD")
            .env("PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("dist/app starts");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            let exited = child.try_wait().ok().flatten();
            let mut err = String::new();
            if exited.is_some() {
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
            }
            assert!(
                exited.is_none(),
                "{name}: dist/app exited ({exited:?}) before listening: {err}"
            );
            assert!(
                Instant::now() < deadline,
                "{name}: dist/app listens within 20 s"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        App {
            child,
            port,
            name: name.to_owned(),
        }
    }

    fn get(&self, path: &str) -> Response {
        http(self.port, "GET", path, &[("accept", "text/html")], "").expect("GET answers")
    }

    /// Asserts that the first hashed script of `page` is served.
    fn serves_its_assets(&self, page: &Response) {
        let start = page
            .body
            .find("/assets/")
            .expect("the page links its assets");
        let rest = &page.body[start..];
        let end = rest.find('"').unwrap_or(rest.len());
        let asset = self.get(&rest[..end]);
        assert_eq!(asset.status, 200, "{}: {}", self.name, &rest[..end]);
        assert!(
            asset
                .header("cache-control")
                .is_some_and(|c| c.contains("immutable")),
            "{}: hashed assets are immutable",
            self.name
        );
    }

    fn stop(mut self) {
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let status = self.child.wait().expect("dist/app exits");
        let mut err = String::new();
        if let Some(mut e) = self.child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        #[cfg(unix)]
        assert_eq!(
            status.code(),
            Some(0),
            "{}: SIGTERM shuts down cleanly: {err}",
            self.name
        );
        assert!(
            !err.contains("\"level\":\"error\""),
            "{}: no errors were logged: {err}",
            self.name
        );
    }
}

/// A static-only app's `dist/public`, served by [`static_host`].
struct Static {
    host: static_host::StaticHost,
    name: String,
}

impl Static {
    /// Builds `examples/<name>` into a temporary `dist/`, checks that it
    /// is static files only, and serves `dist/public`.
    fn build(name: &str, out: &Path) -> Static {
        let dist = out.join("dist");
        let built = Command::new(env!("CARGO_BIN_EXE_k"))
            .arg("build")
            .arg(repo().join("examples").join(name))
            .arg("--out")
            .arg(&dist)
            .output()
            .expect("k build runs");
        assert!(
            built.status.success(),
            "k build {name}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert!(
            !dist.join("app").exists(),
            "{name}: static files only, no dist/app"
        );
        assert!(
            dist.join("public/index.html").is_file(),
            "{name}: dist/public/index.html"
        );
        Static {
            host: static_host::StaticHost::start(&dist.join("public")),
            name: name.to_owned(),
        }
    }

    fn get(&self, path: &str) -> Response {
        http(
            self.host.addr.port(),
            "GET",
            path,
            &[("accept", "text/html")],
            "",
        )
        .expect("GET answers")
    }

    /// Asserts that every hashed file `page` links is served, immutable.
    fn serves_its_assets(&self, page: &Response) {
        let mut linked = 0;
        for rest in page.body.split("\"/assets/").skip(1) {
            let path = format!("/assets/{}", rest.split('"').next().unwrap_or_default());
            let asset = self.get(&path);
            assert_eq!(asset.status, 200, "{}: {path}", self.name);
            assert!(
                asset
                    .header("cache-control")
                    .is_some_and(|c| c.contains("immutable")),
                "{}: hashed assets are immutable",
                self.name
            );
            linked += 1;
        }
        assert!(
            linked >= 2,
            "{}: the page links its script and stylesheet",
            self.name
        );
    }
}

fn json(r: &Response) -> serde_json::Value {
    serde_json::from_str(&r.body).expect("a JSON body")
}

#[test]
fn todos_serves_its_client_rendered_page() {
    let dir = TempDir::new("todos");
    let site = Static::build("todos", &dir.0);
    let home = site.get("/");
    assert_eq!(home.status, 200);
    // Rendering is `.client`: the document is the empty shell only.
    assert!(
        home.body.contains("<div id=\"k-app\"></div>"),
        "{}",
        home.body
    );
    assert!(
        home.header("cache-control") == Some("no-cache"),
        "the document is revalidated, so a deploy is seen at once"
    );
    site.serves_its_assets(&home);
}

#[test]
fn kanban_serves_its_document_for_every_url_and_its_assets() {
    let dir = TempDir::new("kanban");
    let site = Static::build("kanban", &dir.0);
    let home = site.get("/");
    assert_eq!(home.status, 200);
    assert!(
        home.body.contains("<div id=\"k-app\"></div>"),
        "{}",
        home.body
    );
    assert!(home.body.contains("<title>Kanban</title>"), "{}", home.body);
    // A deep link reaches the client router through the fallback.
    let deep = site.get("/boards/7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e6f");
    assert_eq!(deep.status, 200);
    assert_eq!(deep.body, home.body);
    assert_eq!(
        site.get("/assets/missing.0123abcd.js").status,
        404,
        "no fallback for assets"
    );
    site.serves_its_assets(&home);
}

#[test]
fn weather_renders_its_home_page_on_the_server() {
    let dir = TempDir::new("weather");
    let app = App::start("weather", &dir.0, &[]);
    let home = app.get("/");
    assert_eq!(home.status, 200);
    assert!(
        home.body.contains("<title>Weather</title>"),
        "{}",
        home.body
    );
    assert!(home.body.contains("Featured cities"), "{}", home.body);
    assert!(
        home.body.contains("href=\"/city-weather/2950159\""),
        "{}",
        home.body
    );
    app.serves_its_assets(&home);
    app.stop();
}

#[test]
fn notes_migrates_redirects_to_sign_in_and_answers_an_rpc_from_the_database() {
    let dir = TempDir::new("notes");
    let app = App::start("notes", &dir.0, &[("DATABASE_URL", "sqlite::memory:")]);
    let home = app.get("/");
    assert_eq!(home.status, 303, "the notes page requires a session");
    assert!(
        home.header("location")
            .is_some_and(|l| l.ends_with("/sign-in")),
        "{:?}",
        home.headers
    );
    let sign_in = app.get("/sign-in");
    assert_eq!(sign_in.status, 200, "{}", sign_in.body);
    assert!(
        sign_in.body.contains("<h1>Sign in</h1>"),
        "{}",
        sign_in.body
    );
    assert!(
        sign_in.body.contains("<title>Sign in · K Notes</title>"),
        "{}",
        sign_in.body
    );
    app.serves_its_assets(&sign_in);
    // A query on the migrated `account` table.
    let available = rpc(
        app.port,
        "/notes.accounts.AuthProtocol/IsEmailAvailable",
        "{\"email\": \"Ada@Example.com\"}",
    );
    assert_eq!(available.status, 200, "{}", available.body);
    assert_eq!(json(&available), serde_json::json!({"value": true}));
    app.stop();
}

#[test]
fn huddle_signs_in_and_talks_in_a_room_over_its_channel() {
    use tungstenite::Message;
    use tungstenite::client::IntoClientRequest;

    let dir = TempDir::new("huddle");
    let app = App::start("huddle", &dir.0, &[("DATABASE_URL", "sqlite::memory:")]);
    let home = app.get("/");
    assert_eq!(home.status, 303, "the lobby requires a session");
    let sign_in = app.get("/sign-in");
    assert_eq!(sign_in.status, 200, "{}", sign_in.body);
    app.serves_its_assets(&sign_in);
    let signed = rpc(
        app.port,
        "/huddle.accounts.AuthProtocol/SignIn",
        "{\"name\": \"ana\"}",
    );
    assert_eq!(signed.status, 200, "{}", signed.body);
    let cookie = signed
        .header("set-cookie")
        .and_then(|c| c.split(';').next())
        .expect("a session cookie")
        .to_owned();
    // The tab's one socket (44-channels.md): the handshake needs the
    // public origin and the `k.channel.v1` subprotocol.
    let stream = TcpStream::connect(("127.0.0.1", app.port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a timeout");
    let mut request = format!("ws://127.0.0.1:{}/_k/channels", app.port)
        .into_client_request()
        .expect("a request");
    let h = request.headers_mut();
    let origin = format!("http://localhost:{}", app.port);
    h.insert("origin", origin.parse().expect("a header"));
    h.insert(
        "sec-websocket-protocol",
        "k.channel.v1".parse().expect("a header"),
    );
    h.insert("cookie", cookie.parse().expect("a header"));
    let (mut ws, _) = tungstenite::client::client(request, stream).expect("upgrades");
    let next = |ws: &mut tungstenite::WebSocket<TcpStream>| -> serde_json::Value {
        loop {
            match ws.read().expect("a frame") {
                Message::Text(t) => {
                    let v: serde_json::Value = serde_json::from_str(t.as_str()).expect("JSON");
                    if v.get("beat").is_none() {
                        return v;
                    }
                }
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    };
    let open = serde_json::json!({"open": {"ch": 1, "method": "/huddle.chat.ChatProtocol/Room", "request": {"name": "general"}}});
    ws.send(Message::text(open.to_string())).expect("sends");
    let opened = next(&mut ws);
    assert!(opened["opened"]["resume"].is_string(), "{opened}");
    let snapshot = next(&mut ws);
    assert_eq!(snapshot["msg"]["value"]["case"], "snapshot", "{snapshot}");
    let post = serde_json::json!({"msg": {"ch": 1, "seq": 1, "value": {"case": "post", "text": "hello", "clientId": "6f1c2a2e-0000-4000-8000-000000000001"}}});
    ws.send(Message::text(post.to_string())).expect("sends");
    // The echo and the acknowledgement, in either order.
    // Ana's own arrival in the room's presence comes first or in between.
    let mut ack = None;
    let mut echo = None;
    while ack.is_none() || echo.is_none() {
        let v = next(&mut ws);
        if v.get("ack").is_some() {
            ack = Some(v);
        } else if v["msg"]["value"]["case"] == "posted" {
            echo = Some(v);
        } else {
            assert_eq!(v["msg"]["value"]["case"], "presence", "{v}");
        }
    }
    let (ack, echo) = (ack.unwrap_or_default(), echo.unwrap_or_default());
    assert_eq!(ack, serde_json::json!({"ack": {"ch": 1, "seq": 1}}));
    assert_eq!(echo["msg"]["value"]["case"], "posted", "{echo}");
    assert_eq!(echo["msg"]["value"]["message"]["author"], "ana", "{echo}");
    // A refusal is a typed error on the message; the channel stays open.
    let blank = serde_json::json!({"msg": {"ch": 1, "seq": 2, "value": {"case": "post", "text": " ", "clientId": "6f1c2a2e-0000-4000-8000-000000000002"}}});
    ws.send(Message::text(blank.to_string())).expect("sends");
    let refused = next(&mut ws);
    assert_eq!(
        refused["ack"]["error"]["code"], "failed_precondition",
        "{refused}"
    );
    // `k dev`'s channels inspector is never in a build: with a channel
    // open, `dist/app` answers its URL as any unknown page.
    let inspector = app.get("/_k/dev/channels");
    assert_ne!(inspector.status, 200, "{}", inspector.body);
    assert!(
        !inspector.body.contains("Channels · k dev"),
        "{}",
        inspector.body
    );
    let _ = ws.close(None);
    app.stop();
}

/// A free port in 5200–5999, reserved for this process.
fn port_in_range() -> u16 {
    (5200..6000u16)
        .find(|&port| {
            crate::ports::reserve(port) && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
        })
        .expect("a free port in 5200–5999")
}

/// A raw channels socket (`/_k/channels`) to `port`, presenting `origin`
/// and the session `cookie`.
fn channels_socket(port: u16, origin: &str, cookie: &str) -> tungstenite::WebSocket<TcpStream> {
    use tungstenite::client::IntoClientRequest;
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("a timeout");
    let mut request = format!("ws://127.0.0.1:{port}/_k/channels")
        .into_client_request()
        .expect("a request");
    let h = request.headers_mut();
    h.insert("origin", origin.parse().expect("a header"));
    h.insert(
        "sec-websocket-protocol",
        "k.channel.v1".parse().expect("a header"),
    );
    h.insert("cookie", cookie.parse().expect("a header"));
    tungstenite::client::client(request, stream)
        .expect("upgrades")
        .0
}

/// Reads frames from `ws`, skipping heartbeats, until one satisfies
/// `want`, which it returns.
fn frame_until(
    ws: &mut tungstenite::WebSocket<TcpStream>,
    what: &str,
    want: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "{what}; saw {seen:?}");
        let read = ws.read();
        assert!(
            read.is_ok(),
            "{what}: the socket failed: {:?}; saw {seen:?}",
            read.as_ref().err()
        );
        if let Ok(tungstenite::Message::Text(t)) = read {
            let v: serde_json::Value = serde_json::from_str(t.as_str()).expect("JSON");
            if v.get("beat").is_some() {
                continue;
            }
            if want(&v) {
                return v;
            }
            seen.push(v);
        }
    }
}

/// Signs `name` in through the replica on `port` and returns the session
/// cookie (`name=value`).
fn huddle_sign_in(port: u16, name: &str) -> String {
    let signed = rpc(
        port,
        "/huddle.accounts.AuthProtocol/SignIn",
        &format!("{{\"name\": \"{name}\"}}"),
    );
    assert_eq!(signed.status, 200, "{}", signed.body);
    signed
        .header("set-cookie")
        .and_then(|c| c.split(';').next())
        .expect("a session cookie")
        .to_owned()
}

#[test]
fn huddle_replicas_share_messages_and_presence_over_postgres() {
    // Two replicas of huddle's dist/app on one Postgres, as in its
    // docker-compose.yml without nginx in front (44-channels.md, "Example
    // app"): a person connected to replica A and one connected to replica
    // B see each other in the room, and a message posted through A
    // reaches B's socket.
    use tungstenite::Message;

    let Some(pg) = crate::postgres::TestPostgres::start("huddle") else {
        return;
    };
    let url = pg.database("huddle");
    let dir = TempDir::new("huddle-replicas");
    let app = App::build("huddle", &dir.0);
    let origin = "http://localhost:8080";
    let env = [("DATABASE_URL", url.as_str()), ("K_PUBLIC_ORIGIN", origin)];
    let a = App::serve_on("huddle", &app, &env, port_in_range());
    let b = App::serve_on("huddle", &app, &env, port_in_range());

    // Sessions live in the shared database: either replica may sign in.
    let ana = huddle_sign_in(a.port, "ana");
    let bob = huddle_sign_in(b.port, "bob");
    let open = serde_json::json!({"open": {"ch": 1, "method": "/huddle.chat.ChatProtocol/Room", "request": {"name": "general"}}});
    let is_msg =
        |case: &'static str| move |v: &serde_json::Value| v["msg"]["value"]["case"] == case;
    let presence = |case: &'static str, name: &'static str| {
        move |v: &serde_json::Value| {
            v["msg"]["value"]["case"] == "presence"
                && v["msg"]["value"]["_0"]["case"] == case
                && v["msg"]["value"]["_0"]["_0"]["name"] == name
        }
    };

    let mut on_b = channels_socket(b.port, origin, &bob);
    on_b.send(Message::text(open.to_string())).expect("sends");
    let snapshot = frame_until(&mut on_b, "bob's snapshot", is_msg("snapshot"));
    assert_eq!(
        snapshot["msg"]["value"]["present"],
        serde_json::json!([{"name": "bob"}]),
        "{snapshot}"
    );

    let mut on_a = channels_socket(a.port, origin, &ana);
    on_a.send(Message::text(open.to_string())).expect("sends");
    let snapshot = frame_until(&mut on_a, "ana's snapshot", is_msg("snapshot"));
    assert_eq!(
        snapshot["msg"]["value"]["present"],
        serde_json::json!([{"name": "bob"}, {"name": "ana"}]),
        "presence spans both replicas: {snapshot}"
    );
    frame_until(
        &mut on_b,
        "ana's arrival reaches replica B",
        presence("joined", "ana"),
    );

    let post = serde_json::json!({"msg": {"ch": 1, "seq": 1, "value": {"case": "post", "text": "across", "clientId": "6f1c2a2e-0000-4000-8000-000000000001"}}});
    on_a.send(Message::text(post.to_string())).expect("sends");
    let posted = frame_until(
        &mut on_b,
        "a message posted through A reaches B",
        is_msg("posted"),
    );
    assert_eq!(
        posted["msg"]["value"]["message"]["author"], "ana",
        "{posted}"
    );
    assert_eq!(
        posted["msg"]["value"]["message"]["text"], "across",
        "{posted}"
    );
    frame_until(&mut on_a, "the acknowledgement on A", |v| {
        v == &serde_json::json!({"ack": {"ch": 1, "seq": 1}})
    });

    let typing = serde_json::json!({"msg": {"ch": 1, "seq": 2, "value": "typing"}});
    on_a.send(Message::text(typing.to_string())).expect("sends");
    let typed = frame_until(&mut on_b, "typing reaches B", is_msg("typing"));
    assert_eq!(typed["msg"]["value"]["name"], "ana", "{typed}");

    // Ana leaves the room: her lease ends on A, and B hears it.
    let close = serde_json::json!({"close": {"ch": 1, "reason": "byClient"}});
    on_a.send(Message::text(close.to_string())).expect("sends");
    frame_until(&mut on_b, "ana leaving reaches B", presence("left", "ana"));

    let _ = on_a.close(None);
    let _ = on_b.close(None);
    a.stop();
    b.stop();
}

#[test]
fn huddle_deploys_two_replicas_behind_nginx_with_postgres() {
    // The compose file and nginx configuration that 44-channels.md's
    // "Example app" describes: Postgres, two separately restartable
    // replicas, and nginx passing WebSocket upgrades with round robin.
    let dir = repo().join("examples/huddle");
    let compose = std::fs::read_to_string(dir.join("docker-compose.yml")).expect("compose");
    for needle in [
        "image: postgres:",
        "  app1:\n",
        "  app2:\n",
        "DATABASE_URL: postgres://huddle@db:5432/huddle",
        "K_PUBLIC_ORIGIN: http://localhost:8080",
        "image: nginx:",
        "./deploy/nginx.conf:/etc/nginx/conf.d/default.conf:ro",
        "dockerfile: examples/huddle/Dockerfile",
    ] {
        assert!(compose.contains(needle), "docker-compose.yml: {needle}");
    }
    let nginx = std::fs::read_to_string(dir.join("deploy/nginx.conf")).expect("nginx.conf");
    for needle in [
        "server app1:8080;",
        "server app2:8080;",
        "proxy_http_version 1.1;",
        "proxy_set_header Upgrade $http_upgrade;",
        "proxy_set_header Connection \"upgrade\";",
        "proxy_read_timeout 75s;",
    ] {
        assert!(nginx.contains(needle), "nginx.conf: {needle}");
    }
    assert!(
        !nginx.lines().any(|l| {
            let l = l.trim_start();
            l.starts_with("ip_hash") || l.starts_with("hash ") || l.starts_with("sticky")
        }),
        "no sticky sessions"
    );
    let dockerfile = std::fs::read_to_string(dir.join("Dockerfile")).expect("Dockerfile");
    for needle in [
        "RUN k check",
        "RUN k build --out /dist",
        "COPY --from=build /dist/app /app",
    ] {
        assert!(dockerfile.contains(needle), "Dockerfile: {needle}");
    }
}

#[test]
fn shop_renders_the_storefront_and_lists_products_from_the_database() {
    let dir = TempDir::new("shop");
    // The shop is the `shop` member of the `examples/shop` workspace. Its
    // production build pays through paybox, which isn't needed to render
    // the storefront.
    let app = App::start(
        "shop/shop",
        &dir.0,
        &[
            ("DATABASE_URL", "sqlite::memory:"),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", UNUSABLE_ADMIN_HASH),
            ("K_SERVICE_TOKEN", "shop_service_SECRET"),
            ("PAYBOX_URL", "http://127.0.0.1:9"),
            ("PAYBOX_TOKEN", "paybox_service_SECRET"),
            ("PAYBOX_CALLBACK_URL", "http://127.0.0.1:9"),
            // The order confirmation job sends email over SMTP, so a
            // production server requires `SmtpConfig` (31-email.md).
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let home = app.get("/");
    assert_eq!(home.status, 200, "{}", home.body);
    assert!(
        home.body.contains("<h1>New this season</h1>"),
        "{}",
        home.body
    );
    assert!(home.body.contains("The shelves are empty"), "{}", home.body);
    assert!(!home.body.contains("SECRET"), "secrets never reach a page");
    app.serves_its_assets(&home);
    let products = rpc(app.port, "/shop.store.StoreProtocol/Products", "{}");
    assert_eq!(products.status, 200, "{}", products.body);
    assert_eq!(json(&products), serde_json::json!({"value": []}));
    let missing = rpc(
        app.port,
        "/shop.store.StoreProtocol/Product",
        "{\"slug\": \"nope\"}",
    );
    assert_eq!(missing.status, 400, "{}", missing.body);
    assert!(
        missing.body.contains("\"case\":\"notFound\""),
        "{}",
        missing.body
    );
    app.stop();
}

#[test]
fn paybox_answers_its_health_check_and_refuses_calls_without_a_service_token() {
    let dir = TempDir::new("paybox");
    let app = App::start(
        "shop/paybox",
        &dir.0,
        &[
            ("DATABASE_URL", "sqlite::memory:"),
            ("K_SERVICE_TOKEN", "paybox_service_SECRET"),
        ],
    );
    let health = app.get("/health");
    assert_eq!(health.status, 200, "{}", health.body);
    assert_eq!(health.body, "ok");
    // `Payments` requires `ServiceAuthenticated`: a call without a token
    // (a browser, or a misconfigured service) is refused before the handler.
    let charge = rpc(
        app.port,
        "/pay_contracts.pay.Payments/RegisterMerchant",
        "{\"callbackUrl\": \"http://shop.internal\"}",
    );
    assert_eq!(charge.status, 401, "{}", charge.body);
    assert!(charge.body.contains("unauthenticated"), "{}", charge.body);
    assert!(!charge.body.contains("SECRET"), "{}", charge.body);
    app.stop();
}

#[test]
fn an_app_refuses_to_start_when_its_database_cannot_be_opened() {
    let dir = TempDir::new("notes_bad_url");
    let project = repo().join("examples/notes");
    let dist = dir.0.join("dist");
    let built = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(&project)
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(built.status.success());
    // A directory can't be opened as a SQLite file: the database fails
    // before the port opens, with exit 1 and no password in the message.
    let out = Command::new(dist.join("app"))
        .arg("serve")
        .env("PORT", "0")
        .env("DATABASE_URL", format!("sqlite:{}", dir.0.display()))
        .env("DATABASE_PASSWORD", "hunter2-SECRET")
        .output()
        .expect("dist/app runs");
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("database"), "{err}");
    assert!(!err.contains("hunter2"), "{err}");
}

/// The kanban image's nginx serves `dist/public` as the tests' static host
/// does (`support/static_host.rs`): a year of immutable caching for the
/// hashed assets and a 404 for a missing one, `no-cache` and the
/// single-page-app fallback for everything else; and the image copies
/// `dist/public` only, with no `dist/app` to run.
#[test]
fn kanban_image_serves_dist_public_like_the_static_host() {
    let dir = repo().join("examples/kanban");
    let conf = std::fs::read_to_string(dir.join("nginx.conf")).expect("nginx.conf");
    let assets = conf
        .split("location /assets/ {")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("a location for the hashed assets");
    assert!(assets.contains("try_files $uri =404;"), "{assets}");
    assert!(
        assets.contains("Cache-Control \"public, max-age=31536000, immutable\""),
        "{assets}"
    );
    let document = conf
        .split("location / {")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("a location for the document");
    assert!(
        document.contains("try_files $uri /index.html;"),
        "{document}"
    );
    assert!(
        document.contains("Cache-Control \"no-cache\""),
        "{document}"
    );
    let dockerfile = std::fs::read_to_string(dir.join("Dockerfile")).expect("Dockerfile");
    assert!(dockerfile.contains("RUN k check"), "{dockerfile}");
    assert!(dockerfile.contains("RUN k build"), "{dockerfile}");
    assert!(
        dockerfile.contains("COPY --from=build /src/dist/public /usr/share/nginx/html"),
        "{dockerfile}"
    );
    assert!(
        !dockerfile.contains("/src/dist/app"),
        "a static-only image copies no dist/app: {dockerfile}"
    );
}

// --------------------------------------------------------------- learn

/// The projects of the book's Learn pages: every directory with a `k.toml`
/// below `examples/learn/` (`docs/design/49-documentation.md`, "Tutorial
/// projects"), sorted.
fn learn_projects() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![repo().join("examples/learn")];
    while let Some(dir) = stack.pop() {
        if dir.join("k.toml").is_file() {
            found.push(dir);
            continue;
        }
        for entry in std::fs::read_dir(&dir).expect("examples/learn").flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            }
        }
    }
    found.sort();
    found
}

/// Copies the sources and lockfiles of `from` to `to`, leaving out build
/// output and local state.
fn copy_project(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the copy");
    for entry in std::fs::read_dir(from).expect("read the project").flatten() {
        let name = entry.file_name();
        if matches!(name.to_str(), Some("dist" | ".k" | ".env")) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_project(&path, &to.join(&name));
        } else {
            std::fs::copy(&path, to.join(&name)).expect("copy a file");
        }
    }
}

/// The `*.lock` files of `dir`, by name, with their text.
fn lockfiles(dir: &Path) -> Vec<(String, String)> {
    let mut locks: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("read the project")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "lock"))
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path()).expect("read a lockfile"),
            )
        })
        .collect();
    locks.sort();
    locks
}

/// Every Learn project passes `k check` with at least one test run, and
/// `k build` succeeds without changing its committed lockfiles.
#[test]
fn learn_projects_check_build_and_match_their_lockfiles() {
    let projects = learn_projects();
    assert!(
        projects.iter().any(|p| p.ends_with("learn/quick-start")),
        "{projects:?}"
    );
    for project in projects {
        let shown = project.display().to_string();
        let check = Command::new(env!("CARGO_BIN_EXE_k"))
            .args(["check", "--color", "never"])
            .arg(&project)
            .env_remove("K_TEST_FILTER")
            .output()
            .expect("k check runs");
        let err = String::from_utf8_lossy(&check.stderr);
        assert!(check.status.success(), "k check {shown}:\n{err}");
        assert!(
            err.lines()
                .last()
                .is_some_and(|l| l.contains(", ran ") && l.ends_with(": no problems")),
            "{shown}: its tests ran:\n{err}"
        );

        let dir = TempDir::new("learn");
        let copy = dir.0.join("project");
        copy_project(&project, &copy);
        let built = Command::new(env!("CARGO_BIN_EXE_k"))
            .arg("build")
            .arg(&copy)
            .output()
            .expect("k build runs");
        assert!(
            built.status.success(),
            "k build {shown}:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert_eq!(
            lockfiles(&copy),
            lockfiles(&project),
            "{shown}: `k build` changed the lockfiles; commit what it writes"
        );
    }
}

/// The quick start's tip splitter (`learn/quick-start.md`) renders its
/// page on the server, with the default tip selected.
#[test]
fn quick_start_tip_splitter_renders_its_page() {
    let dir = TempDir::new("quick_start");
    let app = App::start("learn/quick-start", &dir.0, &[]);
    let home = app.get("/");
    assert_eq!(home.status, 200);
    assert!(home.body.contains("<title>Tip</title>"), "{}", home.body);
    assert!(home.body.contains("Tip splitter"), "{}", home.body);
    assert!(
        home.body.contains("Each person pays $0.00"),
        "{}",
        home.body
    );
    app.serves_its_assets(&home);
    app.stop();
}
