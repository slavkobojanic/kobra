//! The example apps end to end (`docs/compiler/testing.md`, "Examples"):
//! each one in `examples/` is built with `k build`, its `dist/app` is
//! started on a free port with a fresh in-memory SQLite database, and its
//! main page and one rpc (or, for the apps without rpcs, one hashed asset)
//! must answer. `SIGTERM` must then shut it down cleanly.
//!
//! The configuration is the minimum each app requires, with placeholder
//! values; nothing reaches an outside service.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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
    /// Builds `examples/<name>` into a temporary `dist/` and starts it.
    fn start(name: &str, out: &Path, env: &[(&str, &str)]) -> App {
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
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .expect("a free port");
        let mut cmd = Command::new(&app);
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

fn json(r: &Response) -> serde_json::Value {
    serde_json::from_str(&r.body).expect("a JSON body")
}

#[test]
fn todos_serves_its_client_rendered_page() {
    let dir = TempDir::new("todos");
    let app = App::start("todos", &dir.0, &[]);
    let home = app.get("/");
    assert_eq!(home.status, 200);
    // Rendering is `.client`: the server sends the empty shell only.
    assert!(
        home.body.contains("<div id=\"k-app\"></div>"),
        "{}",
        home.body
    );
    app.serves_its_assets(&home);
    app.stop();
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
fn shop_renders_the_storefront_and_lists_products_from_the_database() {
    let dir = TempDir::new("shop");
    let app = App::start(
        "shop",
        &dir.0,
        &[
            ("DATABASE_URL", "sqlite::memory:"),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", "placeholder"),
            ("STRIPE_API_KEY", "sk_test_placeholder_SECRET"),
            ("STRIPE_WEBHOOK_SECRET", "whsec_placeholder_SECRET"),
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
