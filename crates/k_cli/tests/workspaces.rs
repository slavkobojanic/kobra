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

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
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

/// A free local port.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap()
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
fn wait_for(port: u16, child: &mut Child, what: &str) {
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

fn stop(mut child: Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

/// Starts a built app from `dist/<name>/app` with `env`.
fn serve(dist: &Path, name: &str, port: u16, env: &[(&str, String)]) -> Child {
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
    let mut child = cmd.spawn().unwrap();
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
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", ".", "--port", &port.to_string()])
        .current_dir(&dir.0)
        .env_remove("PAYBOX_URL")
        .env_remove("SHOP_URL")
        .env_remove("K_SERVICE_TOKEN")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
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
        if let Some((name, rest)) = line.split_once(" serving on http://localhost:") {
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

#[test]
fn dev_at_the_root_of_the_shop_pays_an_order_through_paybox_with_a_database_per_member() {
    // `examples/shop` as its README runs it: each member's `.env` from its
    // `.env.example`, the secret files `scripts/init-secrets.sh` writes, and
    // `k dev` at the root. Each member's relative `sqlite:` database is its
    // own, below its own directory, created on first start.
    let dir = TempDir::new("shop-dev");
    copy_example("shop", &dir.0);
    // Paybox serves on `--port`, the shop on the next one; the examples
    // name the default ports, 8080 and 8081.
    let paybox = free_port();
    let shop = paybox + 1;
    for member in ["paybox", "shop"] {
        let example = fs::read_to_string(dir.0.join(member).join(".env.example")).unwrap();
        if member == "shop" {
            assert!(
                example.contains("PAYBOX_CALLBACK_URL=http://127.0.0.1:8081\n"),
                "{example}"
            );
        }
        let env = example.replace(
            "PAYBOX_CALLBACK_URL=http://127.0.0.1:8081",
            &format!("PAYBOX_CALLBACK_URL=http://127.0.0.1:{shop}"),
        );
        write(&dir.0, &format!("{member}/.env"), &env);
    }
    write(
        &dir.0,
        "secrets/admin_password_hash.txt",
        "$argon2id$v=19$m=65536,t=3,p=4$aLYLgjk75OnGzmfOgi+w+g$6DSzMJriTe+IRcfo7YcTibzOSpaPcEMeYxWRXNtBCFY",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", ".", "--port", &paybox.to_string()])
        .current_dir(&dir.0)
        .env_remove("DATABASE_URL")
        .env_remove("PAYBOX_URL")
        .env_remove("PAYBOX_CALLBACK_URL")
        .env_remove("K_SERVICE_TOKEN")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(paybox, &mut child, "paybox");
    wait_for(shop, &mut child, "the shop");
    for member in ["paybox", "shop"] {
        assert!(
            dir.0.join(member).join(".k/dev.sqlite").is_file(),
            "{member} has its own database"
        );
    }
    assert!(!dir.0.join(".k").exists(), "no database at the root");
    let url = format!("sqlite:{}", dir.0.join("shop/.k/dev.sqlite").display());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let catalog = fs::read_to_string(dir.0.join("shop/scripts/demo-catalog.sql")).unwrap();
    let execute = |sql: &str| -> Vec<Vec<k_db::Value>> {
        runtime.block_on(async {
            let db = k_db::Database::connect(&url, &k_db::DatabaseOptions::default())
                .await
                .unwrap();
            if sql.trim_start().starts_with("SELECT") {
                db.query(sql, &[])
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|r| r.values)
                    .collect()
            } else {
                db.execute(sql, &[]).await.unwrap();
                Vec::new()
            }
        })
    };
    execute(&catalog);
    let (status, body) = rpc(
        shop,
        "/shop.store.StoreProtocol/PlaceOrder",
        r#"{"request":{"productSlug":"k-mug","quantity":1,"email":"ana@example.com"}}"#,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
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
    // Paybox's event, from its jobs worker, marks the order paid: the two
    // apps call each other with the run's one development secret.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let rows = execute("SELECT status FROM \"order\"");
        if rows[0][0] == k_db::Value::Text("paid".into()) {
            break;
        }
        assert!(Instant::now() < deadline, "the order is paid: {rows:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    stop(child);
}
