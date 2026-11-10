//! Workspaces end to end (`docs/design/40-workspaces-and-services.md`):
//! `k check`, `k test` and `k build` at a workspace root and in a member,
//! `rpc.lock` written by `k build` and enforced by `k check`, two built
//! services calling each other with signed service tokens, and `k dev` at
//! the root running every app with its remotes wired.
//!
//! Every test works on a copy of `examples/stack` or `examples/shop` (or a
//! workspace written here) in a temporary directory, so the repository is
//! never written.

// The helpers below fail the calling test with a message.
#![allow(clippy::unwrap_used, clippy::panic)]

use crate::children::{ChildGuard, SpawnGuarded};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A directory removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("k_ws_{tag}_{}_{n}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `k` with `args` in `dir`.
fn k(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TEST_FILTER")
        .output()
        .expect("run k")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Writes `text` to `dir/relative`, creating directories.
fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Copies `examples/stack` to `dir`, without build output or local state.
fn copy_stack(dir: &Path) {
    copy_example("stack", dir);
}

/// Copies `examples/<name>` to `dir`, without build output, local state or
/// secrets.
fn copy_example(example: &str, dir: &Path) {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "dist" || name == ".k" || name == ".env" || name == "secrets" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(&*name));
            } else {
                fs::copy(&path, to.join(&*name)).unwrap();
            }
        }
    }
    copy(&repo().join("examples").join(example), dir);
}

fn package(name: &str, deps: &[&str]) -> String {
    let mut s = format!("[package]\nname = \"{name}\"\nk = \"0.1\"\n");
    if !deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        for d in deps {
            s.push_str(&format!("{d} = {{ workspace = true }}\n"));
        }
    }
    s
}

#[test]
fn check_at_the_root_checks_every_member_and_the_stack_is_clean() {
    let out = k(&["check", "--color", "never", "examples/stack"], &repo());
    let err = text(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains(": no problems"), "{err}");
}

#[test]
fn check_in_a_member_mounts_the_libraries_it_depends_on() {
    for member in [
        "examples/stack/shop",
        "examples/stack/paybox/src",
        "examples/stack/pay-contracts",
    ] {
        let out = k(&["check", "--color", "never", member], &repo());
        assert!(out.status.success(), "{member}: {}", text(&out.stderr));
    }
}

#[test]
fn check_reports_manifest_problems_and_library_errors_once_with_real_paths() {
    let dir = TempDir::new("cycle");
    write(
        &dir.0,
        "k.toml",
        "[workspace]\nmembers = [\"a\", \"b\", \"app\"]\n",
    );
    write(&dir.0, "a/k.toml", &package("a", &["b"]));
    write(
        &dir.0,
        "a/src/a/A.k",
        "/// A.\npublic struct A {\n    let x: Missing\n}\n",
    );
    write(&dir.0, "b/k.toml", &package("b", &["a"]));
    write(&dir.0, "app/k.toml", &package("app", &["a"]));
    write(&dir.0, "app/App.k", "public app App {}\n");
    let out = k(
        &["check", "--color", "never", "--format", "json", "."],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(1));
    let lines: Vec<serde_json::Value> = text(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let found: Vec<(String, String)> = lines
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap_or_default().to_owned(),
                d["span"]["file"]
                    .as_str()
                    .or(d["file"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect();
    let has = |code: &str, file: &str| found.iter().any(|(c, f)| c == code && f.ends_with(file));
    assert!(has("K8104", "a/k.toml"), "{found:?}");
    assert!(has("K8104", "b/k.toml"), "{found:?}");
    // The library's error is reported once, at the library's own file,
    // though both `a` and `app` analyse it.
    let missing: Vec<_> = found.iter().filter(|(c, _)| c == "K1202").collect();
    assert_eq!(missing.len(), 1, "{found:?}");
    assert!(missing[0].1.ends_with("a/src/a/A.k"), "{found:?}");
}

#[test]
fn test_at_the_root_runs_every_members_tests() {
    let out = k(&["test", "--color", "never", "examples/stack"], &repo());
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", text(&out.stderr));
    for name in [
        "PaymentsApi › a charge is recorded even though shop is a TestRemote that is offline",
        "Checkout › a payment answers paybox's charge",
        "CheckoutOffline › without a fake, paybox is a TestRemote, which is offline",
    ] {
        assert!(
            stdout.contains(&format!("PASS  {name}")),
            "{name}:\n{stdout}"
        );
    }
    assert!(stdout.contains("== paybox"), "{stdout}");
    assert!(stdout.contains("== shop"), "{stdout}");
    assert!(!stdout.contains("FAIL"), "{stdout}");
}

#[test]
fn build_at_the_root_writes_every_app_to_dist_name_and_records_rpc_lock() {
    let dir = TempDir::new("build");
    copy_stack(&dir.0);
    let out = k(&["build", "."], &dir.0);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(dir.0.join("dist/paybox/app").is_file());
    assert!(dir.0.join("dist/shop/app").is_file());
    assert!(
        !dir.0.join("dist/pay-contracts").exists(),
        "a library builds nothing"
    );
    let lock = fs::read_to_string(dir.0.join("paybox/rpc.lock")).unwrap();
    assert!(
        lock.contains("[protocols.\"pay.Payments\".rpcs.charge]"),
        "{lock}"
    );
    assert!(
        lock.contains("{ name = \"cents\", type = \"Int\", number = 2 }"),
        "{lock}"
    );
    let shop_lock = fs::read_to_string(dir.0.join("shop/rpc.lock")).unwrap();
    assert!(
        shop_lock.contains("[protocols.\"pay.PaymentEvents\"]"),
        "{shop_lock}"
    );
    assert!(
        shop_lock.contains("[protocols.\"checkout.CheckoutApi\"]"),
        "{shop_lock}"
    );
    assert!(!dir.0.join("pay-contracts/rpc.lock").exists());
    // The committed example's locks are what a build writes.
    for member in ["paybox", "shop"] {
        let committed =
            fs::read_to_string(repo().join("examples/stack").join(member).join("rpc.lock"))
                .unwrap_or_default();
        let written = fs::read_to_string(dir.0.join(member).join("rpc.lock")).unwrap();
        assert_eq!(
            committed, written,
            "examples/stack/{member}/rpc.lock is stale"
        );
    }

    // A breaking change to the contract now fails `k check`, until it is
    // marked `@Breaking`.
    let charge = dir.0.join("pay-contracts/src/pay/Charge.k");
    let source = fs::read_to_string(&charge).unwrap();
    fs::write(
        &charge,
        source.replace("    let caller: String\n", "    let caller: String?\n"),
    )
    .unwrap();
    let out = k(&["check", "--color", "never", "."], &dir.0);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("K8001"), "{err}");
    assert!(
        err.contains("field `caller` of `pay.Charge` was `String` and is now `String?`"),
        "{err}"
    );
    // `Charge` is carried by both protocols, so both record the change.
    assert!(err.contains("`pay.Payments` changed"), "{err}");
    assert!(err.contains("`pay.PaymentEvents` changed"), "{err}");
    for protocol in ["Payments", "PaymentEvents"] {
        let file = dir.0.join(format!("pay-contracts/src/pay/{protocol}.k"));
        let source = fs::read_to_string(&file).unwrap();
        fs::write(
            &file,
            source.replace(
                &format!("public protocol {protocol} "),
                &format!("@Breaking(\"no caller is deployed yet\")\npublic protocol {protocol} "),
            ),
        )
        .unwrap();
    }
    let out = k(&["check", "--color", "never", "."], &dir.0);
    assert!(out.status.success(), "{}", text(&out.stderr));
    // The next build records the new shape, keeping the field's number.
    let out = k(&["build", "."], &dir.0);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let lock = fs::read_to_string(dir.0.join("paybox/rpc.lock")).unwrap();
    assert!(
        lock.contains("{ name = \"caller\", type = \"String?\", number = 4 }"),
        "{lock}"
    );
}

#[test]
fn build_in_a_member_writes_its_own_dist() {
    let dir = TempDir::new("member");
    copy_stack(&dir.0);
    let out = k(&["build", "shop"], &dir.0);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(dir.0.join("shop/dist/app").is_file());
}

/// A free local port in 5200–5999 ([`crate::cdp::free_port`]).
fn free_port() -> u16 {
    crate::cdp::free_port()
}

/// Two consecutive free local ports in 5200–5999 (a workspace run serves
/// its apps on `port` and `port + 1`); the first of them. Nothing holds
/// either on any address, and no other test process has them
/// ([`crate::cdp::free_ports`]).
fn free_pair() -> u16 {
    crate::cdp::free_ports(2)
}

/// One HTTP/1.1 POST of an rpc with `Connection: close`: the status and body.
fn rpc(port: u16, path: &str, body: &str) -> Option<(u16, String)> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(20))).ok()?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).ok()?;
    let raw = text(&raw);
    let (head, rest) = raw.split_once("\r\n\r\n")?;
    let status = head.lines().next()?.split(' ').nth(1)?.parse().ok()?;
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut s = rest;
        while let Some((size, r)) = s.split_once("\r\n") {
            let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
                break;
            };
            if n == 0 || r.len() < n {
                break;
            }
            out.push_str(&r[..n]);
            s = r[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        rest.to_owned()
    };
    Some((status, body))
}

/// Waits until `port` accepts connections.
fn wait_for(port: u16, child: &mut ChildGuard, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            child.try_wait().ok().flatten().is_none(),
            "{what} exited before listening"
        );
        assert!(Instant::now() < deadline, "{what} listens within 30 s");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn stop(mut child: ChildGuard) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

/// Starts a built app from `dist/<name>/app` with `env`.
fn serve(dist: &Path, name: &str, port: u16, env: &[(&str, String)]) -> ChildGuard {
    let mut cmd = Command::new(dist.join(name).join("app"));
    cmd.arg("serve")
        .env_remove("DATABASE_URL")
        .env("PORT", port.to_string())
        .env("K_PUBLIC_ORIGIN", format!("http://localhost:{port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn_guarded().unwrap();
    wait_for(port, &mut child, name);
    child
}

#[test]
fn two_built_services_call_each_other_with_signed_tokens() {
    let dir = TempDir::new("serve");
    copy_stack(&dir.0);
    let out = k(&["build", "."], &dir.0);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let dist = dir.0.join("dist");
    let (shop_port, paybox_port) = (free_port(), free_port());
    let secret = "test-only-shared-secret".to_owned();
    let paybox = serve(
        &dist,
        "paybox",
        paybox_port,
        &[
            ("K_SERVICE_TOKEN", secret.clone()),
            ("SHOP_URL", format!("http://127.0.0.1:{shop_port}")),
            ("SHOP_TOKEN", secret.clone()),
        ],
    );
    let shop = serve(
        &dist,
        "shop",
        shop_port,
        &[
            ("K_SERVICE_TOKEN", secret.clone()),
            ("PAYBOX_URL", format!("http://127.0.0.1:{paybox_port}")),
            ("PAYBOX_TOKEN", secret.clone()),
        ],
    );
    // The browser's rpc reaches the shop, which calls paybox, which tells
    // the shop about the charge before answering.
    let (status, body) = rpc(
        shop_port,
        "/shop.checkout.CheckoutApi/Pay",
        r#"{"orderId":"o1","cents":990}"#,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let receipt: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(receipt["chargeId"], "ch_o1");
    let (status, body) = rpc(shop_port, "/shop.checkout.CheckoutApi/PaidCount", "{}").unwrap();
    assert_eq!((status, body.as_str()), (200, r#"{"value":1}"#));
    // A declined charge arrives as paybox's typed error, mapped by the shop.
    let (status, body) = rpc(
        shop_port,
        "/shop.checkout.CheckoutApi/Pay",
        r#"{"orderId":"o2","cents":0}"#,
    )
    .unwrap();
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("CheckoutError.declined"), "{body}");
    // Paybox's rpc path comes from the library, and a call without a token
    // is refused.
    let (status, _) = rpc(
        paybox_port,
        "/pay_contracts.pay.Payments/Charge",
        r#"{"request":{"orderId":"x","cents":1}}"#,
    )
    .unwrap();
    assert_eq!(status, 401);
    stop(shop);
    stop(paybox);

    // With the wrong secret, paybox refuses the shop, which reports payments
    // as unavailable.
    let (shop_port, paybox_port) = (free_port(), free_port());
    let paybox = serve(
        &dist,
        "paybox",
        paybox_port,
        &[
            ("K_SERVICE_TOKEN", "another-secret".to_owned()),
            ("SHOP_URL", format!("http://127.0.0.1:{shop_port}")),
            ("SHOP_TOKEN", secret.clone()),
        ],
    );
    let shop = serve(
        &dist,
        "shop",
        shop_port,
        &[
            ("K_SERVICE_TOKEN", secret.clone()),
            ("PAYBOX_URL", format!("http://127.0.0.1:{paybox_port}")),
            ("PAYBOX_TOKEN", secret),
        ],
    );
    let (status, body) = rpc(
        shop_port,
        "/shop.checkout.CheckoutApi/Pay",
        r#"{"orderId":"o3","cents":5}"#,
    )
    .unwrap();
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("CheckoutError.paymentsUnavailable"), "{body}");
    stop(shop);
    stop(paybox);
}

#[test]
fn dev_at_the_root_runs_every_app_with_its_remotes_wired() {
    let dir = TempDir::new("dev");
    copy_stack(&dir.0);
    let port = free_pair();
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", ".", "--port", &port.to_string()])
        .current_dir(&dir.0)
        .env_remove("PAYBOX_URL")
        .env_remove("SHOP_URL")
        .env_remove("K_SERVICE_TOKEN")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    // `paybox serving on http://localhost:<port>` and the shop's, in any order.
    let mut ports = std::collections::BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    while ports.len() < 2 {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = rx
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("both apps serve: {seen:?}"));
        // `paybox | serving on http://localhost:<port>`: each app's lines
        // start with its name.
        if let Some((name, rest)) = line.split_once(" | serving on http://localhost:") {
            ports.insert(name.trim().to_owned(), rest.trim().parse::<u16>().unwrap());
        }
        seen.push(line);
    }
    let shop_port = ports["shop"];
    assert!(ports.contains_key("paybox"), "{seen:?}");
    let (status, body) = rpc(
        shop_port,
        "/shop.checkout.CheckoutApi/Pay",
        r#"{"orderId":"dev","cents":990}"#,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}\n{seen:?}");
    let (status, body) = rpc(shop_port, "/shop.checkout.CheckoutApi/PaidCount", "{}").unwrap();
    assert_eq!((status, body.as_str()), (200, r#"{"value":1}"#), "{seen:?}");
    stop(child);
}

/// Collects a child's output lines (`stdout` and `stderr`) as they come.
fn lines_of(child: &mut ChildGuard) -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if tx2.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Waits for a line containing `needle`, keeping every line in `seen`.
fn wait_line(
    rx: &std::sync::mpsc::Receiver<String>,
    seen: &mut Vec<String>,
    needle: &str,
) -> String {
    if let Some(l) = seen.iter().find(|l| l.contains(needle)) {
        return l.clone();
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = rx
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("a line with {needle:?}: {seen:#?}"));
        seen.push(line.clone());
        if line.contains(needle) {
            return line;
        }
    }
}

#[test]
fn dev_at_the_root_of_the_shop_runs_with_no_setup() {
    // `examples/shop` as its README runs it: `k dev` at the root and
    // nothing else. No `.env`, no secret files, no Docker: paybox and the
    // shop are wired together, each has its own SQLite database below its
    // own directory, the shop's is seeded with the demo catalog, the admin
    // password is generated and printed, and confirmation email lands in
    // the built-in mail catcher.
    let dir = TempDir::new("shop-dev");
    copy_example("shop", &dir.0);
    let paybox = free_pair();
    let shop = paybox + 1;
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", ".", "--port", &paybox.to_string()])
        .current_dir(&dir.0)
        .env_remove("DATABASE_URL")
        .env_remove("PAYBOX_URL")
        .env_remove("PAYBOX_CALLBACK_URL")
        .env_remove("K_SERVICE_TOKEN")
        .env_remove("SMTP_HOST")
        .env_remove("SHOP_ADMIN_PASSWORD_HASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let rx = lines_of(&mut child);
    let mut seen = Vec::new();
    wait_line(
        &rx,
        &mut seen,
        &format!("paybox | serving on http://localhost:{paybox}"),
    );
    wait_line(
        &rx,
        &mut seen,
        &format!("shop   | serving on http://localhost:{shop}"),
    );
    wait_line(&rx, &mut seen, "shop   | seeded the new database from");
    let note = wait_line(&rx, &mut seen, "the hash of the development password ");
    let password = note
        .split("development password ")
        .nth(1)
        .and_then(|r| r.split(' ').next())
        .unwrap()
        .to_owned();
    for member in ["paybox", "shop"] {
        assert!(
            dir.0.join(member).join(format!(".k/{member}.db")).is_file(),
            "{member} has its own database"
        );
    }
    assert!(!dir.0.join(".k").exists(), "nothing at the root");
    let (status, body) = rpc(
        shop,
        "/shop.admin.AdminProtocol/SignIn",
        &format!(r#"{{"request":{{"email":"admin@shop.example","password":"{password}"}}}}"#),
    )
    .unwrap();
    assert_eq!(status, 200, "the printed admin password signs in: {body}");
    let (status, body) = rpc(
        shop,
        "/shop.store.StoreProtocol/PlaceOrder",
        r#"{"request":{"productSlug":"k-mug","quantity":1,"email":"ana@example.com"}}"#,
    )
    .unwrap();
    assert_eq!(status, 200, "the demo catalog is there: {body}");
    let order = body
        .split("\"orderId\":\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .unwrap()
        .to_owned();
    let (status, body) = rpc(
        shop,
        "/shop.paybox.CardProtocol/Pay",
        &format!(r#"{{"orderId":"{order}","card":{{"number":"4242 4242 4242 4242"}}}}"#),
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    // Paybox's event, from its jobs worker, reaches the shop at its own
    // address (`PAYBOX_CALLBACK_URL` from `[dev.env]`), which marks the
    // order paid and emails the confirmation to the catcher.
    let mail = wait_line(&rx, &mut seen, "shop   | mail from");
    assert!(mail.contains("to ana@example.com"), "{mail}");
    wait_line(&rx, &mut seen, &format!("http://localhost:{shop}/"));
    let kept = fs::read_dir(dir.0.join("shop/.k/mail")).unwrap().count();
    assert_eq!(kept, 1, "the message is kept as an .eml file");
    stop(child);
}

#[test]
fn run_at_the_root_serves_every_app_wired_together() {
    // `k run` at a workspace root starts every app like `k dev`, without
    // watching, and Ctrl-C or SIGTERM stops them all.
    let dir = TempDir::new("run-root");
    copy_stack(&dir.0);
    let port = free_pair();
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["run", ".", "--port", &port.to_string()])
        .current_dir(&dir.0)
        .env_remove("PAYBOX_URL")
        .env_remove("SHOP_URL")
        .env_remove("K_SERVICE_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let rx = lines_of(&mut child);
    let mut seen = Vec::new();
    let line = wait_line(&rx, &mut seen, "shop   | serving on http://localhost:");
    let shop: u16 = line.rsplit(':').next().unwrap().trim().parse().unwrap();
    wait_line(&rx, &mut seen, "paybox | serving on http://localhost:");
    let (status, body) = rpc(
        shop,
        "/shop.checkout.CheckoutApi/Pay",
        r#"{"orderId":"run","cents":990}"#,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}\n{seen:?}");
    let started = Instant::now();
    stop(child);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "both apps stop promptly: {:?}",
        started.elapsed()
    );
}

#[test]
fn the_notes_example_runs_with_no_database_url() {
    // A development run needs no `.env`: `DATABASE_URL` defaults to a
    // SQLite file below the project's `.k/`, created and migrated at start.
    let dir = TempDir::new("notes-run");
    copy_example("notes", &dir.0);
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["run", ".", "--port", &port.to_string()])
        .current_dir(&dir.0)
        .env_remove("DATABASE_URL")
        .env_remove("DATABASE_PASSWORD")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let rx = lines_of(&mut child);
    let mut seen = Vec::new();
    wait_line(
        &rx,
        &mut seen,
        &format!("serving on http://localhost:{port}"),
    );
    assert!(dir.0.join(".k/notes.db").is_file(), "{seen:?}");
    let (status, _) = rpc(port, "/", "").unwrap_or((0, String::new()));
    assert_ne!(status, 0);
    stop(child);
}
