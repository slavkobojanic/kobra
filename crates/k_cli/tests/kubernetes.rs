//! The shop's Kubernetes manifests, applied to a real cluster
//! (`docs/compiler/testing.md`, "Kind smoke test").
//!
//! `deploy.rs` checks `examples/shop/deploy/kubernetes` against the build
//! without a cluster; this test runs it on a throwaway [kind] cluster the
//! way the manifests' README does, and uses the shop over HTTP:
//!
//! 1. a cluster from `kind-config.yaml`, with every host port moved into
//!    5200–5999 and one more mapping for a test `NodePort` Service in front
//!    of the shop pods (no ingress controller is installed: that would pull
//!    images from outside the machine);
//! 2. the images built from the shop's Dockerfile, as Compose builds them,
//!    tagged `k-kind-test-*:local` and loaded with `kind load docker-image`
//!    (no registry); Postgres and Mailpit are pulled by the node;
//! 3. throwaway secrets from `scripts/init-secrets.sh <dir>`, made into
//!    Secrets by `create-secrets.sh <dir>`;
//! 4. `kubectl apply --kustomize` of an overlay that only renames the
//!    images, points `K_PUBLIC_ORIGIN` at the test port and adds the
//!    `NodePort` Service; then the migration Jobs complete and the
//!    Deployments become Available;
//! 5. `/_k/health`, the home page with the demo catalog, an order placed
//!    (the shop calling paybox) and paid (paybox's event calling the shop
//!    back), and a `kubectl rollout restart` of the shop during which
//!    `/_k/health`, asked continuously through the Service, never fails.
//!
//! It is skipped unless `K_KIND=1`; with it, a missing Docker, `kind` or
//! `kubectl` fails it. `K_KUBECTL` names the `kubectl` to use. The cluster
//! has its own kubeconfig in the scratch directory, so the reader's
//! contexts are untouched. Dropping the [`Cluster`], also on a panic (after
//! printing the pods, events and logs), deletes the cluster, the test
//! images and the scratch directory with its secrets; a cluster left by a
//! killed run (its process id is gone) is deleted by the next run.
//!
//! ```text
//! K_KIND=1 cargo test -p k_cli --test it -- kubernetes:: --test-threads 1
//! ```
//!
//! [kind]: https://kind.sigs.k8s.io

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

use crate::apps::{Scratch, repo};
use crate::examples::{Response, http};

/// The variable that turns this test on.
const ENABLE: &str = "K_KIND";

/// The prefix of every cluster this test creates: `<prefix><pid>-<ms>`.
const CLUSTER_PREFIX: &str = "k-kind-test-";

/// The images this test builds and loads, by app, and the names the
/// manifests use for them.
const IMAGES: [(&str, &str, &str); 2] = [
    ("paybox", "k-paybox", "k-kind-test-paybox"),
    ("shop", "k-shop", "k-kind-test-shop"),
];

/// The port of the test's `NodePort` Service on the node.
const NODE_PORT: u16 = 30080;

/// The namespace of every resource.
const NAMESPACE: &str = "k-shop";

/// One cluster at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The `kubectl` to run: `K_KUBECTL`, else the one on `PATH`.
fn kubectl_bin() -> String {
    std::env::var("K_KUBECTL").unwrap_or_else(|_| "kubectl".to_owned())
}

/// Whether `program args` runs and succeeds.
fn works(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Whether the test runs: `K_KIND=1`, and then Docker, kind and kubectl
/// must be there.
fn enabled() -> bool {
    if std::env::var(ENABLE).as_deref() != Ok("1") {
        eprintln!(
            "skipped: set {ENABLE}=1 to run the shop's Kubernetes manifests on a kind cluster \
             (needs Docker, kind and kubectl)"
        );
        return false;
    }
    assert!(
        works("docker", &["version", "--format", "{{.Server.Version}}"]),
        "{ENABLE}=1, but Docker isn't running"
    );
    assert!(
        works("kind", &["version"]),
        "{ENABLE}=1, but kind isn't on PATH"
    );
    assert!(
        works(&kubectl_bin(), &["version", "--client"]),
        "{ENABLE}=1, but {} doesn't run (set K_KUBECTL)",
        kubectl_bin()
    );
    true
}

/// A TCP port in 5200–5999 that nothing holds, reserved for this process.
fn free_port() -> u16 {
    (5200..6000u16)
        .find(|&port| crate::ports::take(port))
        .expect("a free port in 5200–5999")
}

/// Whether a process with this id is alive.
fn alive(pid: u32) -> bool {
    works("kill", &["-0", &pid.to_string()])
}

/// Deletes the clusters of earlier runs that were killed before their
/// [`Cluster`] was dropped: the process id in the name is gone. Clusters
/// with other names are never touched.
fn remove_orphans() {
    let Ok(out) = Command::new("kind").args(["get", "clusters"]).output() else {
        return;
    };
    for name in String::from_utf8_lossy(&out.stdout).lines() {
        let Some(rest) = name.strip_prefix(CLUSTER_PREFIX) else {
            continue;
        };
        let mut parts = rest.split('-');
        let (Some(pid), Some(ms), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(pid), true) = (pid.parse::<u32>(), ms.parse::<u128>().is_ok()) else {
            continue;
        };
        if pid != std::process::id() && !alive(pid) {
            eprintln!("deleting {name}, left by a run that is gone");
            let _ = Command::new("kind")
                .args(["delete", "cluster", "--name", name])
                .output();
        }
    }
}

/// Fails the test unless `out` succeeded, naming `what`.
fn check(what: &str, out: Output) -> Output {
    assert!(
        out.status.success(),
        "{what}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// A kind cluster of this test, deleted when dropped.
struct Cluster {
    /// The unique cluster name.
    name: String,
    /// The cluster's own kubeconfig, in the scratch directory.
    kubeconfig: PathBuf,
    /// Whether `kind create cluster` ran, so there is one to delete.
    created: bool,
    /// The images built for it, removed from Docker when dropped.
    images: Vec<String>,
    /// The generated configs, overlay and secrets.
    scratch: Scratch,
    _one_at_a_time: MutexGuard<'static, ()>,
}

impl Cluster {
    /// A cluster name and scratch directory; creates nothing yet.
    fn new() -> Cluster {
        let held = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        remove_orphans();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let scratch = Scratch::new("kind-shop");
        Cluster {
            name: format!("{CLUSTER_PREFIX}{}-{ms}", std::process::id()),
            kubeconfig: scratch.0.join("kubeconfig"),
            created: false,
            images: Vec::new(),
            scratch,
            _one_at_a_time: held,
        }
    }

    fn scratch(&self) -> &Path {
        &self.scratch.0
    }

    /// `kubectl` against this cluster, in the shop's namespace.
    fn kubectl(&self) -> Command {
        let mut c = Command::new(kubectl_bin());
        c.env("KUBECONFIG", &self.kubeconfig)
            .args(["--namespace", NAMESPACE]);
        c
    }

    /// Runs `kubectl <args>`, failing the test on a non-zero exit.
    fn run(&self, args: &[&str]) -> String {
        let out = check(
            &format!("kubectl {}", args.join(" ")),
            self.kubectl().args(args).output().expect("kubectl"),
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Creates the cluster from `config` (a kind config, as YAML).
    fn create(&mut self, config: &str) {
        let path = self.scratch().join("kind-config.yaml");
        std::fs::write(&path, config).expect("the kind config");
        let start = Instant::now();
        // From here on there may be a cluster to delete, even if `create`
        // fails halfway.
        self.created = true;
        check(
            "kind create cluster",
            Command::new("kind")
                .args(["create", "cluster", "--name", &self.name, "--wait", "180s"])
                .arg("--config")
                .arg(&path)
                .arg("--kubeconfig")
                .arg(&self.kubeconfig)
                .output()
                .expect("kind"),
        );
        eprintln!("{}: created in {:.0?}", self.name, start.elapsed());
    }

    /// Builds each app's image with the shop's Dockerfile (as Compose
    /// does), under the test's tag, and loads it into the node.
    fn build_and_load(&mut self) {
        for (app, _, test_image) in IMAGES {
            let tag = format!("{test_image}:local");
            let start = Instant::now();
            self.images.push(tag.clone());
            check(
                &format!("docker build {app}"),
                Command::new("docker")
                    .args(["build", "--quiet", "--file", "examples/shop/Dockerfile"])
                    .args(["--build-arg", &format!("APP={app}"), "--tag", &tag, "."])
                    .current_dir(repo())
                    .output()
                    .expect("docker build"),
            );
            check(
                &format!("kind load docker-image {tag}"),
                Command::new("kind")
                    .args(["load", "docker-image", &tag, "--name", &self.name])
                    .output()
                    .expect("kind load"),
            );
            eprintln!("{tag}: built and loaded in {:.0?}", start.elapsed());
        }
    }

    /// What a failed run needs to be understood: the pods, the events and
    /// every container's last log lines.
    fn diagnostics(&self) -> String {
        let mut out = String::new();
        for args in [
            &["get", "all,ingress,pvc", "--output", "wide"][..],
            &["get", "events", "--sort-by", ".lastTimestamp"][..],
            &[
                "logs",
                "--all-containers",
                "--prefix",
                "--tail",
                "60",
                "--selector",
                "app",
                "--max-log-requests",
                "20",
            ][..],
        ] {
            if let Ok(o) = self.kubectl().args(args).output() {
                out.push_str(&String::from_utf8_lossy(&o.stdout));
                out.push_str(&String::from_utf8_lossy(&o.stderr));
            }
        }
        out
    }

    /// Each pod's name and container restart count.
    fn restarts(&self) -> Vec<(String, u64)> {
        let pods: Value = serde_json::from_str(&self.run(&["get", "pods", "--output", "json"]))
            .expect("pods as JSON");
        pods["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["status"]["phase"] == "Running")
            .map(|p| {
                let restarts = p["status"]["containerStatuses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["restartCount"].as_u64())
                    .sum();
                (p["metadata"]["name"].as_str().unwrap().to_owned(), restarts)
            })
            .collect()
    }

    /// No log line of a running app container is an error.
    fn assert_no_errors(&self) {
        for app in ["shop", "paybox"] {
            let log = self.run(&[
                "logs",
                "--selector",
                &format!("app={app}"),
                "--all-containers",
                "--prefix",
                "--tail",
                "-1",
            ]);
            let errors: Vec<&str> = log
                .lines()
                .filter(|l| l.contains("] error") || l.contains("\"level\":\"error\""))
                .collect();
            assert!(
                errors.is_empty(),
                "{app} logged errors:\n{}",
                errors.join("\n")
            );
        }
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        if self.created {
            if std::thread::panicking() {
                eprintln!("{}: state\n{}", self.name, self.diagnostics());
            }
            let _ = Command::new("kind")
                .args(["delete", "cluster", "--name", &self.name])
                .arg("--kubeconfig")
                .arg(&self.kubeconfig)
                .output();
        }
        for image in &self.images {
            let _ = Command::new("docker")
                .args(["image", "rm", "--force", image])
                .output();
        }
        // The secrets are throwaway but secrets still: never left behind.
        let _ = std::fs::remove_dir_all(&self.scratch.0);
    }
}

/// `kind-config.yaml` with every `hostPort` moved to a free port in
/// 5200–5999 (the first, the ingress controller's, returned), and one
/// more mapping from `node_port_host` to the test's `NodePort`.
fn kind_config(node_port_host: u16) -> (String, u16) {
    let path = repo().join("examples/shop/deploy/kubernetes/kind-config.yaml");
    let text = std::fs::read_to_string(path).expect("kind-config.yaml");
    let mut doc = YamlLoader::load_from_str(&text).expect("YAML").remove(0);
    let key = |k: &str| Yaml::String(k.to_owned());
    let mut ingress_port = None;
    let Yaml::Hash(cluster) = &mut doc else {
        panic!("kind-config.yaml is a mapping")
    };
    let Some(Yaml::Array(nodes)) = cluster.get_mut(&key("nodes")) else {
        panic!("kind-config.yaml has nodes")
    };
    let Some(Yaml::Hash(node)) = nodes.first_mut() else {
        panic!("kind-config.yaml has a node")
    };
    let mappings = node
        .entry(key("extraPortMappings"))
        .or_insert_with(|| Yaml::Array(Vec::new()));
    let Yaml::Array(mappings) = mappings else {
        panic!("extraPortMappings is a list")
    };
    for m in mappings.iter_mut() {
        let Yaml::Hash(m) = m else { continue };
        if m.contains_key(&key("hostPort")) {
            let port = free_port();
            ingress_port.get_or_insert(port);
            m.insert(key("hostPort"), Yaml::Integer(i64::from(port)));
            m.insert(key("listenAddress"), key("127.0.0.1"));
        }
    }
    let mut extra = yaml_rust2::yaml::Hash::new();
    extra.insert(key("containerPort"), Yaml::Integer(i64::from(NODE_PORT)));
    extra.insert(key("hostPort"), Yaml::Integer(i64::from(node_port_host)));
    extra.insert(key("listenAddress"), key("127.0.0.1"));
    extra.insert(key("protocol"), key("TCP"));
    mappings.push(Yaml::Hash(extra));
    let mut out = String::new();
    YamlEmitter::new(&mut out).dump(&doc).expect("YAML out");
    (
        out,
        ingress_port.expect("kind-config.yaml maps a host port"),
    )
}

/// The test's overlay over `deploy/kubernetes`: the images renamed to the
/// test's tags, `K_PUBLIC_ORIGIN` at the test port, and a `NodePort`
/// Service in front of the shop pods. Kustomize takes a base only by a
/// relative path, so the overlay links to it.
fn write_overlay(dir: &Path, origin: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let base = dir.join("base");
    std::os::unix::fs::symlink(repo().join("examples/shop/deploy/kubernetes"), &base)
        .expect("a link to the manifests");
    let images: String = IMAGES
        .iter()
        .map(|(_, name, test)| format!("  - name: {name}\n    newName: {test}\n"))
        .collect();
    std::fs::write(
        dir.join("kustomization.yaml"),
        format!(
            "apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - base
  - nodeport.yaml
images:
{images}patches:
  - target:
      kind: ConfigMap
      name: shop-config
    patch: |
      - op: replace
        path: /data/K_PUBLIC_ORIGIN
        value: {origin}
"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("nodeport.yaml"),
        format!(
            "apiVersion: v1
kind: Service
metadata:
  name: shop-kind-test
  namespace: {NAMESPACE}
spec:
  type: NodePort
  selector:
    app: shop
  ports:
    - name: http
      port: 8080
      targetPort: http
      nodePort: {NODE_PORT}
"
        ),
    )
    .unwrap();
}

/// Polls `GET path` until it answers 200.
fn wait_ok(port: u16, path: &str) -> Response {
    let deadline = Instant::now() + Duration::from_secs(90);
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

/// Calls an rpc as the shop's pages do.
fn rpc(port: u16, origin: &str, path: &str, body: &str) -> Response {
    let headers = [
        ("content-type", "application/json"),
        ("connect-protocol-version", "1"),
        ("origin", origin),
    ];
    http(port, "POST", path, &headers, body).unwrap_or_else(|| panic!("{path} answers"))
}

/// One `GET /_k/health` on a new connection: its status, or what went
/// wrong.
fn health_once(port: u16) -> Result<u16, String> {
    let addr = ("127.0.0.1", port);
    let mut s = TcpStream::connect(addr).map_err(|e| format!("connect: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    s.write_all(
        format!("GET /_k/health HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\n\r\n")
            .as_bytes(),
    )
    .map_err(|e| format!("write: {e}"))?;
    let mut answer = String::new();
    s.read_to_string(&mut answer)
        .map_err(|e| format!("read: {e}"))?;
    answer
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("no status line in {answer:?}"))
}

#[test]
fn shop_manifests_run_on_kind_and_survive_a_rolling_restart() {
    if !enabled() {
        return;
    }
    let mut cluster = Cluster::new();
    let web = free_port();
    let origin = format!("http://localhost:{web}");
    let (config, _ingress_port) = kind_config(web);
    cluster.create(&config);
    cluster.build_and_load();

    // Throwaway secrets, as the README makes them, into the scratch
    // directory; never into secrets/.
    let shop_dir = repo().join("examples/shop");
    let secrets = cluster.scratch().join("secrets");
    check(
        "init-secrets.sh",
        Command::new(shop_dir.join("scripts/init-secrets.sh"))
            .arg(&secrets)
            .env("K", env!("CARGO_BIN_EXE_k"))
            .output()
            .expect("init-secrets.sh runs"),
    );
    check(
        "create-secrets.sh",
        Command::new(shop_dir.join("deploy/kubernetes/create-secrets.sh"))
            .arg(&secrets)
            .env("KUBECTL", kubectl_bin())
            .env("KUBECONFIG", &cluster.kubeconfig)
            .output()
            .expect("create-secrets.sh runs"),
    );

    let overlay = cluster.scratch().join("overlay");
    write_overlay(&overlay, &origin);
    let start = Instant::now();
    cluster.run(&["apply", "--kustomize", overlay.to_str().unwrap()]);
    // Postgres pulls and initialises first; the Jobs retry until it
    // answers.
    cluster.run(&[
        "wait",
        "--for=condition=complete",
        "job/paybox-migrate",
        "job/shop-migrate",
        "--timeout=420s",
    ]);
    cluster.run(&[
        "wait",
        "--for=condition=available",
        "deployment/paybox",
        "deployment/shop",
        "deployment/mail",
        "--timeout=300s",
    ]);
    cluster.run(&["rollout", "status", "statefulset/db", "--timeout=120s"]);
    eprintln!(
        "{}: migrated and available after {:.0?}",
        cluster.name,
        start.elapsed()
    );
    // The Ingress is accepted, though no controller serves it here.
    cluster.run(&["get", "ingress", "shop"]);

    eprintln!("health, home page and an order through port {web}");
    let health = wait_ok(web, "/_k/health");
    let v: Value = serde_json::from_str(&health.body).expect("JSON health");
    assert_eq!(v["status"], "ready", "{}", health.body);

    // The demo catalog, as the README loads it, and the home page.
    let catalog = std::fs::read(shop_dir.join("shop/scripts/demo-catalog.sql")).unwrap();
    let mut psql = cluster
        .kubectl()
        .args(["exec", "--stdin", "statefulset/db", "--"])
        .args(["psql", "--username=shop", "--dbname=shop", "--quiet"])
        .args(["--set", "ON_ERROR_STOP=1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("kubectl exec psql");
    psql.stdin.take().unwrap().write_all(&catalog).unwrap();
    check("psql demo-catalog.sql", psql.wait_with_output().unwrap());
    let home = wait_ok(web, "/");
    assert!(home.body.contains("K Mug"), "{}", home.body);

    // An order: the shop creates the payment at paybox (a service call
    // through paybox's Service), and paybox's event, a background job
    // calling the shop's Service, marks it paid.
    let order = |email: &str| {
        let placed = rpc(
            web,
            &origin,
            "/shop.store.StoreProtocol/PlaceOrder",
            &json!({"request": {"productSlug": "k-mug", "quantity": 1, "email": email}})
                .to_string(),
        );
        assert_eq!(placed.status, 200, "placeOrder: {}", placed.body);
        let placed: Value = serde_json::from_str(&placed.body).unwrap();
        let id = placed["orderId"].as_str().unwrap().to_owned();
        assert_eq!(placed["paymentUrl"], format!("{origin}/payment/{id}"));
        id
    };
    let id = order("kind@example.com");
    let paid = rpc(
        web,
        &origin,
        "/shop.paybox.CardProtocol/Pay",
        &json!({"orderId": id, "card": {"number": "4242 4242 4242 4242"}}).to_string(),
    );
    assert_eq!(paid.status, 200, "pay: {}", paid.body);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !wait_ok(web, &format!("/receipt/{id}"))
        .body
        .contains("Payment received")
    {
        assert!(
            Instant::now() < deadline,
            "the receipt never showed the payment"
        );
        std::thread::sleep(Duration::from_millis(500));
    }

    eprintln!("paid; rolling restart of the shop");
    // A rolling restart: every new connection to the Service is answered
    // throughout, as old pods sleep (preStop), get SIGTERM and drain.
    let before = cluster.restarts();
    let old_pods = cluster.run(&["get", "pods", "--selector", "app=shop", "--output", "name"]);
    let stop = Arc::new(AtomicBool::new(false));
    let poller = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let (mut ok, mut failed) = (0u32, Vec::new());
            let started = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                match health_once(web) {
                    Ok(200) => ok += 1,
                    other => failed.push(format!("{:.1?}: {other:?}", started.elapsed())),
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            (ok, failed)
        })
    };
    let restart = Instant::now();
    cluster.run(&["rollout", "restart", "deployment/shop"]);
    cluster.run(&["rollout", "status", "deployment/shop", "--timeout=300s"]);
    // The last old pod is still draining; keep asking until it is gone.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let pods = cluster.run(&["get", "pods", "--selector", "app=shop", "--output", "name"]);
        if !old_pods.lines().any(|p| pods.lines().any(|q| q == p)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the old shop pods never went away"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    stop.store(true, Ordering::Relaxed);
    let (ok, failed) = poller.join().unwrap();
    eprintln!(
        "rollout restart: {ok} health checks answered in {:.0?}",
        restart.elapsed()
    );
    assert!(
        failed.is_empty(),
        "{} of {} health checks failed during the rolling restart:\n{}",
        failed.len(),
        ok as usize + failed.len(),
        failed.join("\n")
    );
    assert!(ok > 100, "the health checks ran throughout: {ok}");

    // The new pods serve, and take orders.
    let id = order("kind-after-restart@example.com");
    assert!(!id.is_empty());

    // Nothing crashed after start-up (paybox and the shop may have
    // restarted while Postgres was still starting), and no app logged an
    // error.
    let after = cluster.restarts();
    for (pod, n) in &after {
        let was = before.iter().find(|(p, _)| p == pod).map_or(0, |(_, n)| *n);
        assert_eq!(*n, was, "{pod} restarted during the test");
    }
    cluster.assert_no_errors();
}
