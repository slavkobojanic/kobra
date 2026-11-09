//! The call example's deployment (`examples/call/compose.yaml`,
//! `docs/design/46-media-and-rtc.md`, "Example apps"): the Compose file,
//! `dist/app config` and coturn's flags must agree, like the shop's
//! (`deploy.rs`).
//!
//! The example is built with `k build` (in a copy, so the committed
//! `rpc.lock` is compared with what a build writes), and the test checks
//! that
//!
//! - the app's container sets only variables its `dist/app` reads, every
//!   required one, and its secret only as `K_TURN_SECRET_FILE` naming a
//!   mounted secret whose file in `secrets/` has a committed `.example`;
//! - coturn reads the same secret, with `--use-auth-secret`, so the
//!   credentials `IceServers` signs are the ones it accepts;
//! - `K_TURN_URLS` are `turn:` URLs on coturn's listening port and realm,
//!   over UDP and TCP;
//! - coturn denies relaying to every private range and loopback, and to
//!   multicast, and has a relay port range.
//!
//! It needs no Docker: `chrome::call_relays_through_coturn_in_chrome` runs
//! coturn itself.
//!
//! ```text
//! cargo test -p k_cli --test it -- call_deploy::
//! ```

// The helpers below fail the calling test with a message.
#![allow(clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `examples/call` project.
fn call() -> PathBuf {
    repo().join("examples/call")
}

/// A temporary directory, removed afterwards.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("k_call_{tag}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copies the sources at `from` to `to`, without build output, local state
/// or secrets.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("readable").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "dist" | ".k" | "secrets" | ".env") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(&name));
        } else {
            std::fs::copy(&path, to.join(&name)).expect("copied");
        }
    }
}

/// `dist/app config` of the example, by variable: `(required, secret)`.
fn config() -> BTreeMap<String, (bool, bool)> {
    let dir = TempDir::new("build");
    let copy = dir.0.join("call");
    copy_tree(&call(), &copy);
    let out = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(&copy)
        .arg("--out")
        .arg(dir.0.join("dist"))
        .output()
        .expect("k build runs");
    assert!(
        out.status.success(),
        "k build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let committed = std::fs::read_to_string(call().join("rpc.lock")).unwrap_or_default();
    let written = std::fs::read_to_string(copy.join("rpc.lock")).expect("written");
    assert_eq!(committed, written, "examples/call/rpc.lock is stale");
    let listed = Command::new(dir.0.join("dist").join("app"))
        .arg("config")
        .output()
        .expect("dist/app config runs");
    assert!(listed.status.success(), "dist/app config");
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            (
                words[0].to_owned(),
                (words[2] == "required", words.last() == Some(&"secret")),
            )
        })
        .collect()
}

/// The Compose file, parsed.
fn compose() -> Yaml {
    let path = call().join("compose.yaml");
    let text = std::fs::read_to_string(&path).expect("compose.yaml");
    YamlLoader::load_from_str(&text)
        .unwrap_or_else(|e| panic!("compose.yaml: {e}"))
        .remove(0)
}

/// A string-valued mapping of the Compose file as a map.
fn strings(y: &Yaml) -> BTreeMap<String, String> {
    y.as_hash()
        .map(|h| {
            h.iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// The secrets a service mounts, by name.
fn mounted(service: &Yaml) -> Vec<String> {
    service["secrets"]
        .as_vec()
        .map(|v| {
            v.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The value of `--name=value` in coturn's command line.
fn flag(command: &str, name: &str) -> Option<String> {
    command
        .split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("--{name}=")))
        .map(|v| v.trim_matches('"').to_owned())
}

#[test]
fn compose_dist_app_config_and_coturn_agree() {
    let vars = config();
    for name in ["K_TURN_URLS", "K_TURN_SECRET"] {
        assert!(
            vars.contains_key(name),
            "dist/app config lists {name}: {vars:?}"
        );
    }
    assert!(vars["K_TURN_SECRET"].1, "K_TURN_SECRET is a secret");
    let doc = compose();
    let services = &doc["services"];
    let app = &services["app"];
    let env = strings(&app["environment"]);

    // The app sets only what it reads, its secret only as a file.
    let mut problems = Vec::new();
    for name in env.keys() {
        match (
            vars.get(name),
            name.strip_suffix("_FILE").and_then(|n| vars.get(n)),
        ) {
            (Some((_, true)), _) => {
                problems.push(format!("{name} is a secret; give it as {name}_FILE"))
            }
            (Some(_), _) | (None, Some((_, true))) => {}
            (None, Some(_)) => problems.push(format!("{name}: only secrets can come from a file")),
            (None, None) => problems.push(format!("{name} is not a variable the app reads")),
        }
    }
    for (name, (required, _)) in &vars {
        if *required && !env.contains_key(name) && !env.contains_key(&format!("{name}_FILE")) {
            problems.push(format!("{name} is required but not set"));
        }
    }
    assert!(
        problems.is_empty(),
        "compose.yaml app:\n{}",
        problems.join("\n")
    );

    // One secret, mounted by both, from a committed example.
    let secret_path = &env["K_TURN_SECRET_FILE"];
    let secret = secret_path
        .strip_prefix("/run/secrets/")
        .unwrap_or_else(|| panic!("K_TURN_SECRET_FILE={secret_path} is not a Compose secret"));
    let turn = &services["turn"];
    assert!(
        mounted(app).contains(&secret.to_owned()),
        "the app mounts {secret}"
    );
    assert!(
        mounted(turn).contains(&secret.to_owned()),
        "coturn mounts {secret}"
    );
    let file = doc["secrets"][secret]["file"]
        .as_str()
        .unwrap_or_else(|| panic!("secrets.{secret}.file"));
    let example = call().join(format!("{}.example", file.trim_start_matches("./")));
    assert!(example.is_file(), "{} is committed", example.display());
    let ignored = std::fs::read_to_string(call().join(".gitignore")).unwrap_or_default();
    assert!(
        ignored.contains("secrets/*"),
        "real secrets are git-ignored"
    );

    // coturn checks the credentials IceServers signs with that secret.
    let command = turn["command"]
        .as_vec()
        .and_then(|v| v.first())
        .and_then(Yaml::as_str)
        .unwrap_or_else(|| panic!("services.turn.command"))
        .to_owned();
    assert!(command.contains("turnserver"), "{command}");
    assert!(
        command.contains("--use-auth-secret"),
        "coturn uses the TURN REST scheme: {command}"
    );
    assert!(
        command.contains(&format!("--static-auth-secret=\"$$(cat {secret_path})\"")),
        "coturn reads the app's secret file: {command}"
    );
    assert!(
        !command.contains("--lt-cred-mech"),
        "no static users: {command}"
    );

    // The URLs browsers get name coturn's port and realm, over both transports.
    let port = flag(&command, "listening-port").expect("--listening-port");
    let realm = flag(&command, "realm").expect("--realm");
    let urls: Vec<&str> = env["K_TURN_URLS"].split(',').map(str::trim).collect();
    for url in &urls {
        let rest = url
            .strip_prefix("turn:")
            .unwrap_or_else(|| panic!("{url} is a turn: URL"));
        let (authority, _) = rest.split_once('?').unwrap_or((rest, ""));
        assert_eq!(authority, format!("{realm}:{port}"), "{url} names coturn");
    }
    for transport in ["udp", "tcp"] {
        assert!(
            urls.iter()
                .any(|u| u.ends_with(&format!("transport={transport}"))),
            "a {transport} URL: {urls:?}"
        );
    }
    let origin = &env["K_PUBLIC_ORIGIN"];
    assert!(
        origin.ends_with(&realm),
        "{origin} and the realm {realm} are one host"
    );

    // No relaying into the deployment's own networks.
    for range in [
        "10.0.0.0-10.255.255.255",
        "172.16.0.0-172.31.255.255",
        "192.168.0.0-192.168.255.255",
        "127.0.0.0-127.255.255.255",
    ] {
        assert!(
            command.contains(&format!("--denied-peer-ip={range}")),
            "coturn denies {range}: {command}"
        );
    }
    assert!(command.contains("--no-multicast-peers"), "{command}");
    let min: u32 = flag(&command, "min-port")
        .expect("--min-port")
        .parse()
        .expect("a port");
    let max: u32 = flag(&command, "max-port")
        .expect("--max-port")
        .parse()
        .expect("a port");
    assert!(min < max, "a relay range: {min}-{max}");
    assert_eq!(
        turn["network_mode"].as_str(),
        Some("host"),
        "the relay range needs host networking"
    );
}
