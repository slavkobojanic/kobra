//! The examples' Docker Compose stacks, run for real (`docs/compiler/testing.md`,
//! "Compose smoke tests").
//!
//! `deploy.rs`, `call_deploy.rs` and `examples.rs` check the Compose files
//! against the builds without Docker; these tests start each stack the way
//! its README does, `docker compose up --build --wait`, and use it over
//! HTTP: `/_k/health`, the home page, and one real flow per app.
//!
//! | Stack | Flow |
//! |---|---|
//! | `examples/shop` | an order placed and paid through paybox, confirmed by paybox's event, emailed to Mailpit; in Chrome, a mug added to the cart and bought |
//! | `examples/kanban` | nginx's caching headers; in Chrome, a board and a card created |
//! | `examples/huddle` | two people signed in through nginx, connected to different replicas, see each other's message over the room's channel (Postgres `NOTIFY`) |
//! | `examples/call` | the page loads, `iceServers` names coturn, and coturn answers a STUN binding request |
//!
//! They are skipped unless `K_COMPOSE=1`; with it, a missing Docker fails
//! them (as `K_REQUIRE_DOCKER=1` does elsewhere). The Chrome parts are
//! skipped without Chrome unless `K_REQUIRE_CHROME=1`.
//!
//! Each run generates throwaway secrets into a scratch directory, from the
//! committed `secrets/*.example` templates (the shop's with
//! `scripts/init-secrets.sh`), and an override file that points the
//! secrets there, gives every published port one in 5200–5999, sets
//! `K_PUBLIC_ORIGIN` to match, and tags the built images
//! `k-compose-test-*:local` so no image of the reader's own is replaced.
//! The project name is unique per run. Dropping the [`Stack`], on success
//! or failure, runs `docker compose down --volumes`, removes those images
//! and the scratch directory; a run killed before that is cleaned up by
//! the next one. Only one stack runs at a time.
//!
//! ```text
//! K_COMPOSE=1 cargo test -p k_cli --test it -- compose:: --test-threads 1
//! ```

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::io::Read;
use std::net::{TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::apps::{Scratch, repo};
use crate::examples::{Response, channels_socket, frame_until, http};
use crate::page;

/// The variable that turns these tests on.
const ENABLE: &str = "K_COMPOSE";

/// The prefix of every project these tests start.
const PROJECT_PREFIX: &str = "k-compose-test-";

/// How long `up --build --wait` may take: the first run compiles the `k`
/// toolchain inside the image (about ten minutes on a laptop).
const UP_TIMEOUT_SECS: u32 = 2400;

/// One stack at a time: each builds images and starts several containers,
/// and the shop's network has a fixed subnet.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Whether the tests run: `K_COMPOSE=1` and Docker with Compose v2. With
/// `K_COMPOSE=1` and no Docker the test fails.
fn enabled() -> bool {
    if std::env::var(ENABLE).as_deref() != Ok("1") {
        eprintln!("skipped: set {ENABLE}=1 to run the examples' Compose stacks (needs Docker)");
        return false;
    }
    let docker = Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        && Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
    assert!(
        docker,
        "{ENABLE}=1, but Docker with Compose v2 isn't available"
    );
    true
}

/// A TCP port in 5200–5999 that is free on loopback, also free for UDP
/// when `udp`, and reserved for this process.
fn free_port(udp: bool) -> u16 {
    (5200..6000u16)
        .find(|&port| {
            crate::ports::reserve(port)
                && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
                && std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
                && (!udp || UdpSocket::bind(("0.0.0.0", port)).is_ok())
        })
        .expect("a free port in 5200–5999")
}

/// 24 random bytes as hex, for a throwaway secret.
fn random_hex() -> String {
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a process with this id is alive.
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Takes down the stacks of earlier runs that were killed before their
/// [`Stack`] was dropped (the process id in the project name is gone).
fn remove_orphans() {
    let Ok(out) = Command::new("docker")
        .args(["compose", "ls", "--all", "--format", "json"])
        .output()
    else {
        return;
    };
    let list: Value = serde_json::from_slice(&out.stdout).unwrap_or(json!([]));
    for project in list.as_array().into_iter().flatten() {
        let Some(name) = project["Name"].as_str() else {
            continue;
        };
        let Some(rest) = name.strip_prefix(PROJECT_PREFIX) else {
            continue;
        };
        let pid = rest.split('-').nth(1).and_then(|p| p.parse::<u32>().ok());
        if pid.is_some_and(|p| p != std::process::id() && !alive(p)) {
            let _ = Command::new("docker")
                .args([
                    "compose",
                    "--project-name",
                    name,
                    "down",
                    "--volumes",
                    "--remove-orphans",
                ])
                .output();
        }
    }
}

/// A Compose stack of one example, taken down when dropped.
struct Stack {
    /// The unique project name.
    project: String,
    /// The example's directory.
    dir: PathBuf,
    /// The example's own Compose file, then the generated override.
    files: Vec<PathBuf>,
    /// The images the override tags, removed when dropped.
    images: Vec<String>,
    /// The override and the generated secrets.
    scratch: Scratch,
    /// The services that run a K app, whose logs must hold no error.
    apps: Vec<&'static str>,
    _one_at_a_time: MutexGuard<'static, ()>,
}

impl Stack {
    /// The stack of `examples/<example>` from `compose_file`, whose K apps
    /// run in the services `apps`. Waits for any other stack of this
    /// process to finish; doesn't start this one ([`Stack::up`]).
    fn new(example: &str, compose_file: &str, apps: Vec<&'static str>) -> Stack {
        let held = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        remove_orphans();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let dir = repo().join("examples").join(example);
        Stack {
            project: format!("{PROJECT_PREFIX}{example}-{}-{n}", std::process::id()),
            files: vec![dir.join(compose_file)],
            dir,
            images: Vec::new(),
            scratch: Scratch::new(&format!("compose-{example}")),
            apps,
            _one_at_a_time: held,
        }
    }

    /// The scratch directory, for secrets and the override.
    fn scratch(&self) -> &Path {
        &self.scratch.0
    }

    /// Writes the override and remembers the images it tags.
    fn set_override(&mut self, yaml: &str, images: &[&str]) {
        let path = self.scratch().join("compose.override.yml");
        std::fs::write(&path, yaml).expect("the override");
        self.files.push(path);
        self.images = images.iter().map(|i| (*i).to_owned()).collect();
    }

    /// `docker compose` with this stack's project and files.
    fn compose(&self) -> Command {
        let mut c = Command::new("docker");
        c.arg("compose").arg("--project-name").arg(&self.project);
        for f in &self.files {
            c.arg("--file").arg(f);
        }
        c.current_dir(&self.dir);
        c
    }

    /// Runs `docker compose <args>`, failing the test on a non-zero exit.
    fn run(&self, args: &[&str]) -> Output {
        let out = self.compose().args(args).output().expect("docker compose");
        assert!(
            out.status.success(),
            "docker compose {}: {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    /// `up --build --wait`: builds the images and waits until every
    /// service with a health check is healthy.
    fn up(&self) {
        let start = Instant::now();
        let timeout = UP_TIMEOUT_SECS.to_string();
        let out = self
            .compose()
            .args([
                "up",
                "--build",
                "--detach",
                "--wait",
                "--wait-timeout",
                &timeout,
            ])
            .output()
            .expect("docker compose up");
        assert!(
            out.status.success(),
            "docker compose up --build --wait failed after {:?}:\n{}\n{}",
            start.elapsed(),
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .rev()
                .take(60)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"),
            self.logs()
        );
        eprintln!(
            "{}: up and healthy after {:.0?}",
            self.project,
            start.elapsed()
        );
    }

    /// The last lines of every service's log.
    fn logs(&self) -> String {
        self.compose()
            .args(["logs", "--no-color", "--tail", "80"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    /// Every container has been running since it was first started (none
    /// crashed and was restarted by `restart: unless-stopped`, which
    /// `--wait` alone doesn't notice), and no K app logged an error.
    fn assert_steady(&self) {
        let ids = String::from_utf8(self.run(&["ps", "--all", "--quiet"]).stdout).unwrap();
        for id in ids.split_whitespace() {
            let out = Command::new("docker")
                .args([
                    "inspect",
                    "--format",
                    "{{.Name}} {{.State.Status}} {{.RestartCount}}",
                    id,
                ])
                .output()
                .expect("docker inspect");
            let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            assert!(
                line.ends_with(" running 0"),
                "{line}: every container runs, never restarted\n{}",
                self.logs()
            );
        }
        for app in &self.apps {
            let out = self.run(&["logs", "--no-color", "--no-log-prefix", app]);
            let log = String::from_utf8_lossy(&out.stdout);
            let errors: Vec<&str> = log
                .lines()
                .filter(|l| l.starts_with("error") || l.contains("\"level\":\"error\""))
                .collect();
            assert!(
                errors.is_empty(),
                "{app} logged errors:\n{}",
                errors.join("\n")
            );
        }
    }

    /// The number of running containers of `service`.
    fn replicas(&self, service: &str) -> usize {
        String::from_utf8(self.run(&["ps", "--quiet", service]).stdout)
            .unwrap()
            .split_whitespace()
            .count()
    }

    /// Runs `psql` in the `db` service as `user` on `database`, with `sql`
    /// on standard input; returns its unaligned, tuples-only output.
    fn psql(&self, user: &str, database: &str, sql: &str) -> String {
        use std::io::Write;
        let mut child = self
            .compose()
            .args([
                "exec",
                "-T",
                "db",
                "psql",
                "--username",
                user,
                "--dbname",
                database,
                "--set",
                "ON_ERROR_STOP=1",
                "--quiet",
                "--tuples-only",
                "--no-align",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("psql");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "psql: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("{}: logs\n{}", self.project, self.logs());
        }
        let _ = self
            .compose()
            .args(["down", "--volumes", "--remove-orphans", "--timeout", "10"])
            .output();
        for image in &self.images {
            let _ = Command::new("docker")
                .args(["image", "rm", "--force", image])
                .output();
        }
        // The secrets are throwaway but secrets still: never left behind,
        // even when the test failed.
        let _ = std::fs::remove_dir_all(&self.scratch.0);
    }
}

/// The `secrets:` of a Compose file: each name and its file, relative to
/// the example.
fn compose_secrets(file: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(file).expect("the Compose file");
    let docs = yaml_rust2::YamlLoader::load_from_str(&text).expect("YAML");
    let secrets = &docs[0]["secrets"];
    secrets
        .as_hash()
        .into_iter()
        .flatten()
        .map(|(name, s)| {
            (
                name.as_str().unwrap().to_owned(),
                s["file"].as_str().expect("a file secret").to_owned(),
            )
        })
        .collect()
}

/// The override's `secrets:` section, pointing each of the Compose file's
/// secrets at the generated file of the same name in `dir`. Every secret
/// must have a committed `.example` template and a generated file.
fn secrets_override(stack: &Stack, dir: &Path) -> String {
    let mut yaml = String::from("secrets:\n");
    for (name, file) in compose_secrets(&stack.files[0]) {
        let template = stack.dir.join(format!("{file}.example"));
        assert!(
            template.is_file(),
            "{}: the secret {name} has a committed template",
            template.display()
        );
        let file_name = Path::new(&file).file_name().unwrap();
        let generated = dir.join(file_name);
        assert!(generated.is_file(), "{} was generated", generated.display());
        yaml.push_str(&format!("  {name}:\n    file: {}\n", generated.display()));
    }
    yaml
}

/// `GET path` on `port`.
fn get(port: u16, path: &str) -> Response {
    http(port, "GET", path, &[], "").unwrap_or_else(|| panic!("GET {path} on port {port} answers"))
}

/// Polls `GET path` until it answers 200: a published port can take a
/// moment to reach the container after its health check passed.
fn wait_ok(port: u16, path: &str) -> Response {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match http(port, "GET", path, &[], "") {
            Some(r) if r.status == 200 => return r,
            other => assert!(
                Instant::now() < deadline,
                "GET {path} on port {port}: {:?}",
                other.map(|r| (r.status, r.body))
            ),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `/_k/health` answers `ready`, answered by the server without JavaScript
/// (38-deployment.md, "Startup and shutdown").
fn assert_healthy(port: u16) {
    let r = wait_ok(port, "/_k/health");
    let v: Value = serde_json::from_str(&r.body).expect("JSON health");
    assert_eq!(v["status"], "ready", "{}", r.body);
}

/// Calls an rpc as the app's pages do (Connect's JSON encoding, the
/// page's `Origin`), with `cookie` when given.
fn rpc(port: u16, origin: &str, path: &str, body: &str, cookie: Option<&str>) -> Response {
    let mut headers = vec![
        ("content-type", "application/json"),
        ("connect-protocol-version", "1"),
        ("origin", origin),
    ];
    if let Some(c) = cookie {
        headers.push(("cookie", c));
    }
    http(port, "POST", path, &headers, body).unwrap_or_else(|| panic!("{path} answers"))
}

/// The session cookie (`name=value`) a response sets.
fn session_cookie(r: &Response) -> String {
    r.headers
        .iter()
        .filter(|(n, _)| n == "set-cookie")
        .filter_map(|(_, v)| v.split(';').next())
        .find(|c| c.starts_with("k_session"))
        .unwrap_or_else(|| panic!("a session cookie in {:?}", r.headers))
        .to_owned()
}

/// A tab in a fresh Chrome, or `None` (said, or failing with
/// `K_REQUIRE_CHROME=1`).
fn chrome(stack: &Stack) -> Option<page::Tab> {
    page::chrome(&stack.scratch().join("chrome"))
}

// ==================================================================== shop

#[test]
fn shop_stack_sells_a_mug_through_paybox_and_mails_the_receipt() {
    if !enabled() {
        return;
    }
    let mut stack = Stack::new("shop", "docker-compose.yml", vec!["shop", "paybox"]);
    let secrets = stack.scratch().join("secrets");
    let init = Command::new(stack.dir.join("scripts/init-secrets.sh"))
        .arg(&secrets)
        .env("K", env!("CARGO_BIN_EXE_k"))
        .output()
        .expect("init-secrets.sh runs");
    assert!(
        init.status.success(),
        "init-secrets.sh: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let web = free_port(false);
    let mail = free_port(false);
    let origin = format!("http://localhost:{web}");
    // A subnet of its own, so a `k-shop` stack someone runs by hand keeps
    // 172.29.89.0/24; Caddy's fixed address moves with it.
    let yaml = format!(
        "services:
  paybox:
    image: k-compose-test-paybox:local
  shop:
    image: k-compose-test-shop:local
    environment:
      K_PUBLIC_ORIGIN: {origin}
      K_TRUSTED_PROXIES: 172.29.189.10/32
  mail:
    ports: !override
      - \"127.0.0.1:{mail}:8025\"
  proxy:
    ports: !override
      - \"127.0.0.1:{web}:8080\"
    networks:
      default:
        ipv4_address: 172.29.189.10
networks:
  default:
    ipam:
      config: !override
        - subnet: 172.29.189.0/24
{}",
        secrets_override(&stack, &secrets)
    );
    stack.set_override(
        &yaml,
        &["k-compose-test-paybox:local", "k-compose-test-shop:local"],
    );
    stack.up();
    assert_eq!(stack.replicas("shop"), 2, "two shop replicas");
    assert_healthy(web);

    // The demo catalog, as the README loads it.
    let catalog = std::fs::read_to_string(stack.dir.join("shop/scripts/demo-catalog.sql")).unwrap();
    stack.psql("shop", "shop", &catalog);
    let home = wait_ok(web, "/");
    assert!(home.body.contains("K Mug"), "{}", home.body);

    // Checkout over HTTP, as the product and card pages call it.
    let customer = "compose-http@example.com";
    let placed = rpc(
        web,
        &origin,
        "/shop.store.StoreProtocol/PlaceOrder",
        &json!({"request": {"productSlug": "k-mug", "quantity": 1, "email": customer}}).to_string(),
        None,
    );
    assert_eq!(placed.status, 200, "placeOrder: {}", placed.body);
    let placed: Value = serde_json::from_str(&placed.body).unwrap();
    let order = placed["orderId"].as_str().unwrap().to_owned();
    assert_eq!(
        placed["paymentUrl"],
        format!("{origin}/payment/{order}"),
        "the card form is the shop's own, at the public origin"
    );
    let paid = rpc(
        web,
        &origin,
        "/shop.paybox.CardProtocol/Pay",
        &json!({"orderId": order, "card": {"number": "4242 4242 4242 4242"}}).to_string(),
        None,
    );
    assert_eq!(paid.status, 200, "pay: {}", paid.body);
    // Paybox's event, a background job calling the shop, marks it paid.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !get(web, &format!("/receipt/{order}"))
        .body
        .contains("Payment received")
    {
        assert!(
            Instant::now() < deadline,
            "the receipt never showed the payment\n{}",
            stack.logs()
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    assert_eq!(mails_to(mail, customer, 1), 1, "one confirmation email");

    // The same purchase in a browser, with the cart.
    if let Some(mut tab) = chrome(&stack) {
        let customer = "compose-chrome@example.com";
        tab.open(&format!("{origin}/"));
        tab.wait_text("Cart: 0 items");
        let mug = tab.by_text("K Mug");
        tab.click(&mug);
        let add = tab.role("button", "Add to cart");
        tab.wait_handles_click(&add);
        tab.click(&add);
        tab.wait_text("Cart: 1 item");
        let field = tab.role("textbox", "Email for your receipt");
        tab.fill(&field, customer);
        let buy = tab.role("button", "Buy now");
        tab.wait_handles_click(&buy);
        tab.click(&buy);
        let card = tab.role("textbox", "Card number");
        tab.fill(&card, "4242 4242 4242 4242");
        let pay = tab.wait("the Pay button", |t| {
            t.roles("button")
                .into_iter()
                .find(|(name, _)| name.starts_with("Pay "))
                .map(|(_, el)| el)
        });
        tab.wait_handles_click(&pay);
        tab.click(&pay);
        tab.wait_text("Payment received");
        assert_eq!(
            mails_to(mail, customer, 1),
            1,
            "the browser's receipt email"
        );
        drop(tab);
    }
    stack.assert_steady();
}

/// Waits until Mailpit holds at least `at_least` messages to `to`, then
/// returns how many it holds.
fn mails_to(port: u16, to: &str, at_least: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let r = get(port, &format!("/api/v1/search?query=to:{to}"));
        let count = serde_json::from_str::<Value>(&r.body)
            .ok()
            .and_then(|v| v["messages_count"].as_u64())
            .unwrap_or(0);
        if count >= at_least {
            return count;
        }
        assert!(
            Instant::now() < deadline,
            "no email to {to} in Mailpit: {}",
            r.body
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

// ================================================================== kanban

#[test]
fn kanban_stack_serves_the_static_build_and_keeps_a_new_card() {
    if !enabled() {
        return;
    }
    let mut stack = Stack::new("kanban", "docker-compose.yml", vec![]);
    let web = free_port(false);
    stack.set_override(
        &format!(
            "services:
  kanban:
    image: k-compose-test-kanban:local
    ports: !override
      - \"127.0.0.1:{web}:8080\"
"
        ),
        &["k-compose-test-kanban:local"],
    );
    stack.up();
    // A static-only app has no server, so no `/_k/health`: the image's
    // health check (`--wait`) is nginx answering the document.
    let home = wait_ok(web, "/");
    assert_eq!(home.header("cache-control"), Some("no-cache"));
    assert!(home.body.contains("id=\"k-app\""), "{}", home.body);
    let deep = get(web, "/boards/some-board");
    assert_eq!(deep.status, 200, "a deep link answers the document");
    let script = home
        .body
        .split('"')
        .find(|s| s.starts_with("/assets/") && s.ends_with(".js"))
        .expect("an entry script");
    let asset = get(web, script);
    assert_eq!(asset.status, 200, "{script}");
    assert_eq!(
        asset.header("cache-control"),
        Some("public, max-age=31536000, immutable"),
        "{script}"
    );
    assert_eq!(get(web, "/assets/missing.js").status, 404);

    // Creating a board and a card needs the browser: they live in its
    // storage. nginx's Content-Security-Policy applies, so a blocked
    // script would fail the tab.
    if let Some(mut tab) = chrome(&stack) {
        tab.open(&format!("http://localhost:{web}/"));
        let field = tab.role("textbox", "New board");
        tab.fill(&field, "Launch plan");
        tab.press("Enter");
        tab.role("heading", "Launch plan");
        let add = tab.role("button", "Add a card to To do");
        tab.click(&add);
        let card = tab.role("textbox", "New card in To do");
        tab.fill(&card, "Ship the stack");
        tab.press("Enter");
        tab.role("heading", "Ship the stack");
        tab.reload();
        tab.role("heading", "Ship the stack");
        drop(tab);
    }
    stack.assert_steady();
}

// ================================================================== huddle

#[test]
fn huddle_stack_carries_a_message_between_replicas_behind_nginx() {
    use tungstenite::Message;

    if !enabled() {
        return;
    }
    let mut stack = Stack::new("huddle", "docker-compose.yml", vec!["app1", "app2"]);
    let web = free_port(false);
    let origin = format!("http://localhost:{web}");
    stack.set_override(
        &format!(
            "services:
  app1:
    image: k-compose-test-huddle:local
    environment:
      K_PUBLIC_ORIGIN: {origin}
  app2:
    image: k-compose-test-huddle:local
    environment:
      K_PUBLIC_ORIGIN: {origin}
  proxy:
    ports: !override
      - \"127.0.0.1:{web}:8080\"
"
        ),
        &["k-compose-test-huddle:local"],
    );
    stack.up();
    assert_healthy(web);
    let home = get(web, "/");
    assert_eq!(home.status, 303, "the lobby needs a session");
    let sign_in = wait_ok(web, "/sign-in");
    assert!(sign_in.body.contains("<title>Sign in · Huddle</title>"));

    let sign = |name: &str| {
        let r = rpc(
            web,
            &origin,
            "/huddle.accounts.AuthProtocol/SignIn",
            &json!({ "name": name }).to_string(),
            None,
        );
        assert_eq!(r.status, 200, "{}", r.body);
        session_cookie(&r)
    };
    let ana = sign("ana");
    let bob = sign("bob");
    let open = json!({"open": {"ch": 1, "method": "/huddle.chat.ChatProtocol/Room", "request": {"name": "general"}}});
    let snapshot = |v: &Value| v["msg"]["value"]["case"] == "snapshot";
    // The replica (the presence lease's `process`) each person's channel
    // is on: nginx's round robin decides, so a second socket is opened
    // until the two are on different replicas.
    let process_of = |name: &str| {
        stack.psql(
            "huddle",
            "huddle",
            &format!(
                "SELECT process FROM k_presence WHERE member LIKE '%\"{name}\"%' \
                 ORDER BY joined_at DESC LIMIT 1;"
            ),
        )
    };
    let mut on_ana = channels_socket(web, &origin, &ana);
    on_ana.send(Message::text(open.to_string())).expect("sends");
    frame_until(&mut on_ana, "ana's snapshot", snapshot);
    let ana_on = process_of("ana");
    let mut attempt = 0;
    let mut on_bob = loop {
        attempt += 1;
        let mut socket = channels_socket(web, &origin, &bob);
        socket.send(Message::text(open.to_string())).expect("sends");
        frame_until(&mut socket, "bob's snapshot", snapshot);
        if process_of("bob") != ana_on {
            break socket;
        }
        assert!(
            attempt < 10,
            "ten of bob's sockets all reached ana's replica: is nginx balancing?"
        );
        let _ = socket.close(None);
        std::thread::sleep(Duration::from_millis(200));
    };
    assert!(!ana_on.is_empty(), "ana's presence lease is in Postgres");

    let post = json!({"msg": {"ch": 1, "seq": 1, "value": {"case": "post", "text": "across the replicas", "clientId": "6f1c2a2e-0000-4000-8000-0000000000c1"}}});
    on_ana.send(Message::text(post.to_string())).expect("sends");
    let posted = frame_until(
        &mut on_bob,
        "ana's message reaches bob on the other replica",
        |v| v["msg"]["value"]["case"] == "posted",
    );
    assert_eq!(posted["msg"]["value"]["message"]["author"], "ana");
    assert_eq!(
        posted["msg"]["value"]["message"]["text"],
        "across the replicas"
    );
    let _ = on_ana.close(None);
    let _ = on_bob.close(None);
    stack.assert_steady();
}

// ==================================================================== call

#[test]
fn call_stack_serves_the_page_and_coturn_answers_stun() {
    if !enabled() {
        return;
    }
    let mut stack = Stack::new("call", "compose.yaml", vec!["app"]);
    let secrets = stack.scratch().join("secrets");
    std::fs::create_dir_all(&secrets).unwrap();
    // From the template: one random value where it says to put one.
    for (_, file) in compose_secrets(&stack.files[0]) {
        let name = Path::new(&file).file_name().unwrap().to_owned();
        std::fs::write(secrets.join(name), random_hex()).unwrap();
    }
    let web = free_port(false);
    let turn = free_port(true);
    // coturn runs with host networking in production (a relay port range
    // is impractical to publish); here it joins the project's network and
    // publishes its listening port in the tests' range. UDP is published
    // on every interface: Docker Desktop doesn't forward UDP published on
    // 127.0.0.1 alone (chrome.rs's coturn says the same).
    let yaml = format!(
        "services:
  app:
    image: k-compose-test-call:local
    ports: !override
      - \"127.0.0.1:{web}:8080\"
    environment:
      K_PUBLIC_ORIGIN: http://localhost:{web}
      K_TURN_URLS: turn:localhost:{turn}?transport=udp,turn:localhost:{turn}?transport=tcp
  turn:
    network_mode: !reset null
    ports:
      - \"{turn}:3478/udp\"
      - \"127.0.0.1:{turn}:3478/tcp\"
    environment:
      EXTERNAL_IP: 127.0.0.1
{}",
        secrets_override(&stack, &secrets)
    );
    stack.set_override(&yaml, &["k-compose-test-call:local"]);
    stack.up();
    assert_healthy(web);
    let origin = format!("http://localhost:{web}");
    let page = wait_ok(web, "/sign-in");
    assert!(
        page.body.contains("<title>Sign in · Call</title>"),
        "{}",
        page.body
    );
    let script = page
        .body
        .split("<link rel=\"modulepreload\" href=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("an entry script");
    assert_eq!(get(web, script).status, 200, "{script}");

    // Signed in, the browser is given coturn's URLs with a credential the
    // app signed with the shared secret.
    let signed = rpc(
        web,
        &origin,
        "/call.accounts.AuthProtocol/SignIn",
        &json!({"name": "ana"}).to_string(),
        None,
    );
    assert_eq!(signed.status, 200, "{}", signed.body);
    let cookie = session_cookie(&signed);
    let ice = rpc(
        web,
        &origin,
        "/call.calls.CallProtocol/IceServers",
        "{}",
        Some(&cookie),
    );
    assert_eq!(ice.status, 200, "{}", ice.body);
    let ice: Value = serde_json::from_str(&ice.body).unwrap();
    let server = &ice["value"][0];
    assert_eq!(
        server["urls"],
        json!([
            format!("turn:localhost:{turn}?transport=udp"),
            format!("turn:localhost:{turn}?transport=tcp")
        ])
    );
    assert!(
        server["username"]
            .as_str()
            .is_some_and(|u| u.ends_with(":ana"))
            && server["credential"].as_str().is_some_and(|c| !c.is_empty()),
        "{server}"
    );

    // A STUN binding request (RFC 8489): coturn answers with a success
    // response carrying the same transaction id.
    let socket = UdpSocket::bind("0.0.0.0:0").expect("a UDP socket");
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut request = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42];
    let id: Vec<u8> = random_hex().as_bytes()[..12].to_vec();
    request.extend(&id);
    let deadline = Instant::now() + Duration::from_secs(30);
    let answer = loop {
        let _ = socket.send_to(&request, ("127.0.0.1", turn));
        let mut answer = [0u8; 512];
        if let Ok((n, _)) = socket.recv_from(&mut answer) {
            break answer[..n].to_vec();
        }
        assert!(
            Instant::now() < deadline,
            "coturn answers STUN on 127.0.0.1:{turn}\n{}",
            stack.logs()
        );
    };
    assert!(answer.len() >= 20, "{answer:?}");
    assert_eq!(&answer[..2], &[0x01, 0x01], "a binding success response");
    assert_eq!(&answer[8..20], &id[..], "the request's transaction id");
    // TCP too, for networks that block UDP: the port accepts connections.
    TcpStream::connect(("127.0.0.1", turn)).expect("coturn listens on TCP");
    stack.assert_steady();
}
