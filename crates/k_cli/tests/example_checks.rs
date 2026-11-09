//! Checks of the examples that need no browser: how they start, what their
//! setup scripts and configuration files say, and what their builds ship.
//! The real-browser flows are in `chrome.rs`.
//!
//! Tests marked `#[ignore = "red: <defect>"]` reproduce a defect found by
//! running the examples in real Chrome that is not fixed yet; the fix
//! removes the `ignore` (`docs/compiler/testing.md`, "Red tests").
//!
//! ```text
//! cargo test -p k_cli --test example_checks
//! cargo test -p k_cli --test example_checks -- --ignored   # the red ones
//! ```

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used)]

#[path = "support/apps.rs"]
mod apps;
#[path = "support/cdp.rs"]
#[allow(dead_code)]
mod cdp;
#[path = "support/static_host.rs"]
mod static_host;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

use apps::{Proc, Scratch, copy_example, repo};

/// `GET path` on `port` with `Connection: close`: the status and body.
fn get(port: u16, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, body.to_owned())
}

/// `K_DEV_FETCH_ORIGINS` sends the server's outgoing requests to a fixed
/// third-party origin to a local fake instead, under `k run` and `k dev`
/// only, so an end-to-end test of the weather example never reaches
/// Open-Meteo while the server renders a city.
#[test]
fn k_run_sends_server_fetches_for_a_replaced_origin_to_the_fake() {
    use k_server::fakes::{FakeHttp, FakeRoute};
    let dir = Scratch::new("fetch-origins");
    copy_example("weather", &dir.0);
    let fake = FakeHttp::serve(
        cdp::listener(),
        vec![
            FakeRoute::json(
                "GET",
                "/v1/get",
                &serde_json::json!({ "id": 2950159, "name": "Berlin", "latitude": 52.52, "longitude": 13.41, "country": "Germany", "admin1": "Land Berlin" }),
            ),
            FakeRoute::text("GET", "/v1/forecast", 503, "{}"),
            FakeRoute::text("GET", "/v1/air-quality", 503, "{}"),
        ],
    )
    .unwrap();
    let origins = [
        "https://geocoding-api.open-meteo.com",
        "https://api.open-meteo.com",
        "https://air-quality-api.open-meteo.com",
    ]
    .map(|o| format!("{o}={}", fake.origin()))
    .join(",");
    let port = cdp::free_port();
    let mut k = Proc::start(
        &["run", ".", "--port", &port.to_string()],
        &dir.0,
        &[("K_DEV_FETCH_ORIGINS", origins)],
        &[],
    );
    k.wait_port(port, Duration::from_secs(120));
    let (status, body) = get(port, "/city-weather/2950159");
    // The place came from the fake; the forecast's 503 shows as the
    // dashboard's failed block, which the server renders with 200.
    assert!(
        body.contains("Weather unavailable") || body.contains("Berlin, Land Berlin, Germany"),
        "{status}: {body}"
    );
    let paths: Vec<String> = fake.requests().into_iter().map(|r| r.path).collect();
    assert!(
        paths.iter().any(|p| p.starts_with("/v1/get?id=2950159")),
        "the place was looked up at the fake: {paths:?}"
    );
}

// --------------------------------------------------------------- shop

/// `scripts/init-secrets.sh` on a machine with neither `argon2` nor
/// Docker must fail, not write an empty admin hash and exit 0
/// (shop-D1, stack-shop.md).
#[test]
fn init_secrets_fails_without_argon2_or_docker() {
    let dir = Scratch::new("init-secrets");
    let out = Command::new("bash")
        .arg(repo().join("examples/shop/scripts/init-secrets.sh"))
        .arg(&dir.0)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let hash = std::fs::read_to_string(dir.0.join("admin_password_hash.txt")).unwrap_or_default();
    assert!(
        !out.status.success() || hash.starts_with("$argon2id$"),
        "exit {:?}, hash {hash:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A config field's `@Matches` is checked at startup like its other
/// constraints: the shop with a placeholder admin hash exits 78
/// (shop-D1, `docs/design/25-config-and-secrets.md`).
#[test]
fn shop_refuses_to_start_with_a_placeholder_admin_hash() {
    let dir = Scratch::new("shop-placeholder");
    copy_example("shop", &dir.0);
    // `.env` points the hash at the secret file, as the example's
    // commented line shows; without it `k run` would generate a valid
    // development hash (`[dev.secrets]`).
    std::fs::write(
        dir.0.join("shop/.env"),
        "SHOP_ADMIN_PASSWORD_HASH_FILE=../secrets/admin_password_hash.txt\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.0.join("secrets")).unwrap();
    std::fs::write(dir.0.join("secrets/admin_password_hash.txt"), "placeholder").unwrap();
    let port = cdp::free_port();
    let mut k = Proc::start(
        &["run", ".", "--port", &port.to_string()],
        &dir.0.join("shop"),
        &[],
        &[
            "DATABASE_URL",
            "SHOP_ADMIN_PASSWORD_HASH",
            "SHOP_ADMIN_PASSWORD_HASH_FILE",
        ],
    );
    let exit = k.wait_exit(Duration::from_secs(90));
    assert_eq!(
        exit,
        Some(Some(78)),
        "the shop refuses the placeholder hash:\n{}",
        k.log()
    );
    assert!(
        k.log().contains(
            "SHOP_ADMIN_PASSWORD_HASH  Secret(•••) must be an Argon2id hash in PHC format"
        ),
        "{}",
        k.log()
    );
}

/// `shop/.env.example` shows how to send `k dev`'s confirmation emails
/// to Mailpit at `localhost:1025` instead of the built-in mail catcher,
/// so the compose file publishes that port (shop-D2).
#[test]
fn shop_compose_publishes_mailpits_smtp_port_for_development() {
    let text = std::fs::read_to_string(repo().join("examples/shop/docker-compose.yml")).unwrap();
    let docs = yaml_rust2::YamlLoader::load_from_str(&text).unwrap();
    let ports: Vec<String> = docs[0]["services"]["mail"]["ports"]
        .as_vec()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p.as_str().map(str::to_owned))
        .collect();
    let env = std::fs::read_to_string(repo().join("examples/shop/shop/.env.example")).unwrap();
    let smtp = env
        .lines()
        .find_map(|l| l.trim_start_matches("# ").strip_prefix("SMTP_PORT="))
        .unwrap()
        .trim()
        .to_owned();
    assert!(
        ports
            .iter()
            .any(|p| p.contains(&format!(":{smtp}:")) || p.ends_with(&format!(":{smtp}"))),
        "the mail service publishes SMTP_PORT={smtp}: {ports:?}"
    );
}

/// `k dev` at a workspace root restarts when a member's `.env` changes,
/// as the README's "a change anywhere rebuilds" says (shop-D3b).
#[test]
fn workspace_dev_restarts_when_a_members_env_changes() {
    let dir = Scratch::new("stack-env-watch");
    copy_example("stack", &dir.0);
    let port = cdp::free_ports(2);
    let mut k = Proc::start(
        &["dev", ".", "--port", &port.to_string()],
        &dir.0,
        &[],
        &["PAYBOX_URL", "SHOP_URL", "K_SERVICE_TOKEN"],
    );
    k.wait_port(port, Duration::from_secs(120));
    k.wait_port(port + 1, Duration::from_secs(60));
    let before = k.log();
    std::fs::write(dir.0.join("shop/.env"), "EXTRA_SETTING=1\n").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if k.log().len() > before.len() && k.log()[before.len()..].contains("change detected") {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no restart after shop/.env changed:\n{}", k.log());
}

/// The stack's `.env.example` files agree with its README: paybox on
/// 8080, the shop on 8081, each pointing at the other (shop-D7).
#[test]
fn stack_env_examples_point_each_app_at_the_other() {
    let read = |member: &str| {
        std::fs::read_to_string(
            repo()
                .join("examples/stack")
                .join(member)
                .join(".env.example"),
        )
        .unwrap()
    };
    let value = |text: &str, key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let paybox = read("paybox");
    let shop = read("shop");
    assert_eq!(value(&paybox, "K_PUBLIC_ORIGIN"), "http://localhost:8080");
    assert_eq!(value(&paybox, "SHOP_URL"), "http://localhost:8081");
    assert_eq!(value(&shop, "K_PUBLIC_ORIGIN"), "http://localhost:8081");
    assert_eq!(value(&shop, "PAYBOX_URL"), "http://localhost:8080");
}

/// The browser bundle has no clients for protocols the browser can't
/// call: a `remote(...)` protocol's client holds a service token and runs
/// on the server only (shop-D6).
#[test]
fn stack_shop_client_bundle_leaves_out_server_only_rpc_clients() {
    let dir = Scratch::new("stack-bundle");
    let out = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(repo().join("examples/stack"))
        .arg("--out")
        .arg(dir.0.join("dist"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut offenders = Vec::new();
    let mut stack = vec![dir.0.join("dist/shop/public/assets")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "js") {
                let text = std::fs::read_to_string(&p).unwrap();
                for needle in ["Payments$client", "PaymentEvents$client"] {
                    if text.contains(needle) {
                        offenders.push(format!("{}: {needle}", p.display()));
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

// --------------------------------------------------------------- notes

/// Following the notes README leaves no untracked files: `.env` and the
/// development database are git-ignored, as the README says (notes-5).
#[test]
fn notes_env_and_database_are_git_ignored() {
    for path in ["examples/notes/.env", "examples/notes/.k/notes.db"] {
        let out = Command::new("git")
            .args(["check-ignore", "--quiet", "--no-index", path])
            .current_dir(repo())
            .status()
            .unwrap();
        assert!(out.success(), "{path} is git-ignored");
    }
}

// --------------------------------------------------------------- docs

/// No example README still says the toolchain doesn't exist: every
/// example compiles, builds and runs (todos, weather and notes reports).
#[test]
fn example_readmes_do_not_say_the_compiler_is_missing() {
    let stale = [
        "hasn't been compiled",
        "doesn't exist yet",
        "still being built",
        "will exist once the toolchain ships",
    ];
    let mut found = Vec::new();
    for example in ["todos", "kanban", "weather", "notes", "shop", "stack"] {
        let text = std::fs::read_to_string(repo().join("examples").join(example).join("README.md"))
            .unwrap();
        for s in stale {
            if text.contains(s) {
                found.push(format!("{example}: {s:?}"));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}
