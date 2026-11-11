//! The shop's deployment files (`examples/shop`): `docker-compose.yml` and
//! the Kubernetes manifests in `deploy/kubernetes/` must give each container
//! everything its `dist/app` reads (`docs/compiler/testing.md`,
//! "Deployment files").
//!
//! The workspace is built with `k build` (in a copy, so the committed
//! `rpc.lock` files are compared with what a build writes rather than
//! rewritten), and `dist/<app>/app config` lists each app's variables. For
//! every container that runs an app, in both deployments, the test checks
//! that
//!
//! - every required variable is set, a secret only as `NAME_FILE` naming a
//!   mounted secret file, and no variable the app doesn't read is set (a
//!   typo would otherwise be ignored);
//! - every secret file comes from a file in `secrets/` with a committed
//!   `.example`, and the two services' tokens pair up: the shop signs its
//!   calls with paybox's `K_SERVICE_TOKEN` and paybox its callbacks with the
//!   shop's, which differ;
//! - the URLs between the services name services that exist, health checks
//!   probe `/health`, and the images are the ones the Dockerfile builds.
//!
//! It needs neither Docker nor a cluster: `scripts/e2e.sh` runs the
//! Compose stack itself.
//!
//! ```text
//! cargo test -p k_cli --test it -- deploy::
//! ```

// The helpers below fail the calling test with a message.
#![allow(clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `examples/shop` workspace.
fn shop() -> PathBuf {
    repo().join("examples/shop")
}

/// The apps of the workspace, by member directory, with their images.
const APPS: [(&str, &str); 2] = [("paybox", "k-paybox:local"), ("shop", "k-shop:local")];

/// A temporary directory, removed afterwards.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("k_deploy_{tag}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copies the sources of the tree at `from` to `to`, without build output,
/// local state or secrets.
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

/// One variable of `dist/app config`.
#[derive(Clone, Debug)]
struct Var {
    required: bool,
    secret: bool,
}

/// Parses `dist/app config`: `NAME  Type  required|optional  [default "…"]  [secret]`.
fn parse_config(text: &str) -> BTreeMap<String, Var> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            assert!(words.len() >= 3, "a config line: {l:?}");
            (
                words[0].to_owned(),
                Var {
                    required: words[2] == "required",
                    secret: words.last() == Some(&"secret"),
                },
            )
        })
        .collect()
}

/// Builds the workspace in a copy and returns each app's variables, after
/// checking that the committed `rpc.lock` files are what the build writes.
fn configs() -> BTreeMap<String, BTreeMap<String, Var>> {
    let dir = TempDir::new("build");
    let copy = dir.0.join("shop");
    copy_tree(&shop(), &copy);
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
    let mut all = BTreeMap::new();
    for (app, _) in APPS {
        let committed =
            std::fs::read_to_string(shop().join(app).join("rpc.lock")).unwrap_or_default();
        let written = std::fs::read_to_string(copy.join(app).join("rpc.lock")).expect("written");
        assert_eq!(committed, written, "examples/shop/{app}/rpc.lock is stale");
        let config = Command::new(dir.0.join("dist").join(app).join("app"))
            .arg("config")
            .output()
            .expect("dist/app config runs");
        assert!(config.status.success(), "{app}: dist/app config");
        let vars = parse_config(&String::from_utf8_lossy(&config.stdout));
        assert!(vars.contains_key("DATABASE_URL"), "{app}: {vars:?}");
        all.insert(app.to_owned(), vars);
    }
    all
}

fn yaml(path: &Path) -> Vec<Yaml> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    YamlLoader::load_from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn text(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(n) => Some(n.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `y` with YAML merge keys (`<<: *anchor`) applied, recursively; keys of
/// the mapping itself win over merged ones.
fn merged(y: &Yaml) -> Yaml {
    match y {
        Yaml::Hash(h) => {
            let mut out = yaml_rust2::yaml::Hash::new();
            if let Some(Yaml::Hash(base)) = h.get(&Yaml::String("<<".into())) {
                for (k, v) in base {
                    out.insert(k.clone(), merged(v));
                }
            }
            for (k, v) in h {
                if k.as_str() != Some("<<") {
                    out.insert(k.clone(), merged(v));
                }
            }
            Yaml::Hash(out)
        }
        Yaml::Array(a) => Yaml::Array(a.iter().map(merged).collect()),
        other => other.clone(),
    }
}

/// A container that runs one of the apps, as a deployment describes it.
#[derive(Debug)]
struct Container {
    /// Where it is described, for messages.
    what: String,
    /// The app: `paybox` or `shop`.
    app: String,
    /// Its environment.
    env: BTreeMap<String, String>,
    /// The secret files it can read, by path, with the file in `secrets/`
    /// each one comes from.
    files: BTreeMap<String, String>,
    /// The URL path its health probes request.
    probes: Vec<String>,
}

/// The problems of `c` against its app's variables.
fn problems(c: &Container, vars: &BTreeMap<String, Var>) -> Vec<String> {
    let mut out = Vec::new();
    for (name, value) in &c.env {
        if let Some(var) = vars.get(name) {
            if var.secret {
                out.push(format!("{name} is a secret; give it as {name}_FILE"));
            }
            continue;
        }
        let file_of = name.strip_suffix("_FILE").and_then(|n| vars.get(n));
        match file_of {
            Some(var) if var.secret => {
                if !c.files.contains_key(value) {
                    out.push(format!(
                        "{name}={value}, which is not a mounted secret file"
                    ));
                }
            }
            Some(_) => out.push(format!("{name}: only secrets can come from a file")),
            None => out.push(format!("{name} is not a variable the app reads")),
        }
    }
    for (name, var) in vars {
        let given = c.env.contains_key(name) || c.env.contains_key(&format!("{name}_FILE"));
        if var.required && !given {
            out.push(format!("{name} is required but not set"));
        }
    }
    for source in c.files.values() {
        let example = shop().join(format!("{source}.example"));
        if !example.is_file() {
            out.push(format!("{source} has no committed {source}.example"));
        }
    }
    if c.probes.is_empty() || c.probes.iter().any(|p| p != "/health") {
        out.push(format!("health probes {:?}, expected /health", c.probes));
    }
    out.into_iter()
        .map(|p| format!("{}: {p}", c.what))
        .collect()
}

/// The `secrets/` file behind the secret file variable `name` of `c`.
fn secret_source(c: &Container, name: &str) -> String {
    let path = c
        .env
        .get(&format!("{name}_FILE"))
        .unwrap_or_else(|| panic!("{}: {name}_FILE", c.what));
    c.files
        .get(path)
        .unwrap_or_else(|| panic!("{}: {path} is not mounted", c.what))
        .clone()
}

/// The host of the URL in `c`'s variable `name`.
fn host_of(c: &Container, name: &str) -> String {
    let url = c
        .env
        .get(name)
        .unwrap_or_else(|| panic!("{}: {name}", c.what));
    let rest = url.split_once("://").map_or(url.as_str(), |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    host.split(':').next().unwrap_or_default().to_owned()
}

/// Checks every container of a deployment, the pairing of the service
/// tokens, and that the services' addresses name `services`.
fn check_deployment(
    what: &str,
    containers: &[Container],
    services: &BTreeSet<String>,
    configs: &BTreeMap<String, BTreeMap<String, Var>>,
) {
    let mut all = Vec::new();
    for c in containers {
        all.extend(problems(c, &configs[&c.app]));
    }
    assert!(all.is_empty(), "{what}:\n{}", all.join("\n"));
    for (app, _) in APPS {
        assert!(
            containers.iter().any(|c| c.app == app),
            "{what}: no container runs {app}"
        );
    }
    for paybox in containers.iter().filter(|c| c.app == "paybox") {
        for shop in containers.iter().filter(|c| c.app == "shop") {
            let paybox_token = secret_source(paybox, "K_SERVICE_TOKEN");
            let shop_token = secret_source(shop, "K_SERVICE_TOKEN");
            assert_eq!(
                secret_source(shop, "PAYBOX_TOKEN"),
                paybox_token,
                "{what}: the shop ({}) signs its calls with paybox's secret ({})",
                shop.what,
                paybox.what
            );
            assert_eq!(
                secret_source(paybox, "PAYBOX_CALLBACK_TOKEN"),
                shop_token,
                "{what}: paybox ({}) signs its callbacks with the shop's secret ({})",
                paybox.what,
                shop.what
            );
            assert_ne!(
                paybox_token, shop_token,
                "{what}: each service has its own secret"
            );
        }
    }
    for c in containers {
        let mut urls = vec!["DATABASE_URL"];
        if c.app == "shop" {
            urls.extend(["PAYBOX_URL", "PAYBOX_CALLBACK_URL", "SMTP_HOST"]);
        }
        for name in urls {
            let host = if name == "SMTP_HOST" {
                c.env[name].clone()
            } else {
                host_of(c, name)
            };
            assert!(
                services.contains(&host),
                "{what}: {}: {name} names {host}, which is not one of {services:?}",
                c.what
            );
        }
    }
    let shops: Vec<&Container> = containers.iter().filter(|c| c.app == "shop").collect();
    for c in &shops {
        assert_eq!(host_of(c, "PAYBOX_URL"), "paybox", "{what}: {}", c.what);
        assert_eq!(
            host_of(c, "PAYBOX_CALLBACK_URL"),
            "shop",
            "{what}: {}",
            c.what
        );
    }
}

/// The SQL of the script that creates paybox's role and database.
fn paybox_database_sql(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("CREATE "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn compose_gives_every_container_what_its_dist_app_reads() {
    let configs = configs();
    let doc = merged(&yaml(&shop().join("docker-compose.yml"))[0]);
    let services = doc["services"].as_hash().expect("services");
    let top_secrets = &doc["secrets"];
    let names: BTreeSet<String> = services.keys().filter_map(text).collect();
    let mut containers = Vec::new();
    for (name, service) in services {
        let name = text(name).expect("a service name");
        let Some((app, image)) = APPS.iter().find(|(app, _)| *app == name) else {
            continue;
        };
        assert_eq!(service["image"].as_str(), Some(*image), "{name}: image");
        assert_eq!(
            service["build"]["args"]["APP"].as_str(),
            Some(*app),
            "{name}: the Dockerfile builds this app"
        );
        assert_eq!(
            service["build"]["dockerfile"].as_str(),
            Some("examples/shop/Dockerfile"),
            "{name}"
        );
        let mut env = BTreeMap::new();
        match &service["environment"] {
            Yaml::Hash(h) => {
                for (k, v) in h {
                    env.insert(text(k).expect("a name"), text(v).unwrap_or_default());
                }
            }
            Yaml::Array(a) => {
                for item in a {
                    let line = item.as_str().expect("NAME=value");
                    let (k, v) = line.split_once('=').unwrap_or((line, ""));
                    env.insert(k.to_owned(), v.to_owned());
                }
            }
            other => panic!("{name}: environment {other:?}"),
        }
        let mut files = BTreeMap::new();
        for s in service["secrets"].as_vec().into_iter().flatten() {
            let (source, target) = match s {
                Yaml::String(n) => (n.clone(), format!("/run/secrets/{n}")),
                h => {
                    let source = h["source"].as_str().expect("source").to_owned();
                    let target = h["target"]
                        .as_str()
                        .map_or_else(|| format!("/run/secrets/{source}"), str::to_owned);
                    (source, target)
                }
            };
            let file = top_secrets[source.as_str()]["file"]
                .as_str()
                .unwrap_or_else(|| panic!("{name}: secret {source} has no file"));
            files.insert(target, file.trim_start_matches("./").to_owned());
        }
        let test: Vec<String> = service["healthcheck"]["test"]
            .as_vec()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect();
        assert_eq!(
            test,
            ["CMD", "/app", "probe", "http://127.0.0.1:8080/health"],
            "{name}: the image has no shell, so the app probes itself"
        );
        containers.push(Container {
            what: format!("docker-compose.yml: {name}"),
            app: (*app).to_owned(),
            env,
            files,
            probes: vec!["/health".to_owned()],
        });
    }
    check_deployment("docker-compose.yml", &containers, &names, &configs);
    // Two shop replicas behind the proxy.
    assert_eq!(
        services[&Yaml::String("shop".into())]["deploy"]["replicas"].as_i64(),
        Some(2)
    );
    // Postgres creates paybox's database with paybox's secret.
    let db = &services[&Yaml::String("db".into())];
    let db_secrets: Vec<&str> = db["secrets"]
        .as_vec()
        .into_iter()
        .flatten()
        .filter_map(Yaml::as_str)
        .collect();
    assert!(db_secrets.contains(&"paybox_db_password"), "{db_secrets:?}");
    let script = std::fs::read_to_string(shop().join("deploy/postgres/create-paybox-database.sh"))
        .expect("the init script");
    assert!(script.contains("/run/secrets/paybox_db_password"));
    assert_eq!(
        paybox_database_sql(&script),
        [
            "CREATE ROLE paybox LOGIN PASSWORD :'password';",
            "CREATE DATABASE paybox OWNER paybox;"
        ]
    );
}

/// The Secrets `create-secrets.sh` makes: name → key → file in `secrets/`.
fn script_secrets(script: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in script.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("secret ") {
            let name = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            out.entry(name.clone()).or_default();
            current = Some(name);
        }
        if let (Some(name), Some(spec)) = (&current, line.strip_prefix("--from-file=")) {
            let spec = spec.trim_end_matches('\\').trim();
            let (key, path) = spec.split_once('=').expect("key=path");
            // The files are read from the script's directory argument,
            // `secrets/` unless the kind smoke test passes its own.
            let path = path
                .strip_prefix("\"$dir\"/")
                .map_or_else(|| path.to_owned(), |file| format!("secrets/{file}"));
            out.get_mut(name)
                .expect("the current secret")
                .insert(key.to_owned(), path);
        }
        if !line.ends_with('\\') {
            current = None;
        }
    }
    out
}

#[test]
fn kubernetes_gives_every_container_what_its_dist_app_reads() {
    let configs = configs();
    let dir = shop().join("deploy/kubernetes");
    let script = std::fs::read_to_string(dir.join("create-secrets.sh")).expect("create-secrets.sh");
    let secrets = script_secrets(&script);
    assert!(secrets.len() >= 3, "{secrets:?}");
    // Every manifest is in the kustomization.
    let kustomization = &yaml(&dir.join("kustomization.yaml"))[0];
    let listed: BTreeSet<String> = kustomization["resources"]
        .as_vec()
        .expect("resources")
        .iter()
        .filter_map(text)
        .collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .filter(|p| {
            let n = p.file_name().unwrap_or_default().to_string_lossy();
            n != "kustomization.yaml" && n != "kind-config.yaml"
        })
        .collect();
    files.sort();
    let names: BTreeSet<String> = files
        .iter()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(listed, names, "kustomization.yaml lists every manifest");
    let docs: Vec<Yaml> = files.iter().flat_map(|p| yaml(p)).collect();
    let kind = |d: &Yaml| d["kind"].as_str().unwrap_or_default().to_owned();
    let config_maps: BTreeMap<String, &Yaml> = docs
        .iter()
        .filter(|d| kind(d) == "ConfigMap")
        .map(|d| {
            (
                d["metadata"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                &d["data"],
            )
        })
        .collect();
    let services: BTreeSet<String> = docs
        .iter()
        .filter(|d| kind(d) == "Service")
        .filter_map(|d| d["metadata"]["name"].as_str().map(str::to_owned))
        .collect();
    for d in &docs {
        assert_eq!(
            d["metadata"]["namespace"].as_str().or(d["metadata"]["name"]
                .as_str()
                .filter(|_| kind(d) == "Namespace")),
            Some("k-shop"),
            "{} {:?} is in the k-shop namespace",
            kind(d),
            d["metadata"]["name"]
        );
    }
    let mut containers = Vec::new();
    let mut migrations = BTreeSet::new();
    let mut replicas = BTreeMap::new();
    for d in docs
        .iter()
        .filter(|d| matches!(kind(d).as_str(), "Deployment" | "Job" | "StatefulSet"))
    {
        let name = d["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let pod = &d["spec"]["template"]["spec"];
        // Found on a kind cluster: with the API token mounted, the kubelet
        // must create /var/run/secrets/kubernetes.io inside the read-only
        // Secret volume at /run/secrets, and the container never starts.
        // Nothing here calls the Kubernetes API, so no pod gets a token.
        assert_eq!(
            pod["automountServiceAccountToken"].as_bool(),
            Some(false),
            "{name}: automountServiceAccountToken: false"
        );
        let volumes: BTreeMap<String, &Yaml> = pod["volumes"]
            .as_vec()
            .into_iter()
            .flatten()
            .map(|v| (v["name"].as_str().unwrap_or_default().to_owned(), v))
            .collect();
        for c in pod["containers"].as_vec().into_iter().flatten() {
            let image = c["image"].as_str().unwrap_or_default();
            let Some((app, _)) = APPS.iter().find(|(_, i)| *i == image) else {
                continue;
            };
            let mut env = BTreeMap::new();
            for from in c["envFrom"].as_vec().into_iter().flatten() {
                let map = from["configMapRef"]["name"].as_str().unwrap_or_else(|| {
                    panic!("{name}: envFrom only from ConfigMaps; secrets are files")
                });
                let data = config_maps
                    .get(map)
                    .unwrap_or_else(|| panic!("{name}: no ConfigMap {map}"));
                for (k, v) in data.as_hash().into_iter().flatten() {
                    env.insert(text(k).expect("a name"), text(v).unwrap_or_default());
                }
            }
            for e in c["env"].as_vec().into_iter().flatten() {
                assert!(
                    e["valueFrom"]["secretKeyRef"].is_badvalue(),
                    "{name}: a secret as a plain variable; mount it and use NAME_FILE"
                );
                env.insert(
                    e["name"].as_str().expect("a name").to_owned(),
                    text(&e["value"]).unwrap_or_default(),
                );
            }
            let mut files = BTreeMap::new();
            for m in c["volumeMounts"].as_vec().into_iter().flatten() {
                let volume = volumes[m["name"].as_str().expect("a volume name")];
                let Some(secret) = volume["secret"]["secretName"].as_str() else {
                    continue;
                };
                let keys = secrets.get(secret).unwrap_or_else(|| {
                    panic!("{name}: create-secrets.sh makes no Secret {secret}")
                });
                let mount = m["mountPath"].as_str().expect("a mount path");
                for (key, path) in keys {
                    files.insert(format!("{mount}/{key}"), path.clone());
                }
            }
            let probes: Vec<String> = if kind(d) == "Job" {
                // A migration runs once and exits: no probes.
                assert_eq!(
                    c["args"]
                        .as_vec()
                        .map(|a| a.iter().filter_map(text).collect::<Vec<_>>()),
                    Some(vec!["migrate".to_owned()]),
                    "{name}: a migration Job runs `dist/app migrate`"
                );
                migrations.insert((*app).to_owned());
                vec!["/health".to_owned()]
            } else {
                ["startupProbe", "readinessProbe", "livenessProbe"]
                    .iter()
                    .map(|p| {
                        c[*p]["httpGet"]["path"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned()
                    })
                    .collect()
            };
            if kind(d) == "Deployment" {
                // Found on a kind cluster: `serve` stops accepting on
                // SIGTERM, before kube-proxy has taken the pod out of the
                // Service, so a rollout refused connections. A preStop
                // sleep covers that, and the grace period covers the sleep
                // plus the drain (K_SHUTDOWN_GRACE, 25 s unless set).
                let sleep = c["lifecycle"]["preStop"]["sleep"]["seconds"]
                    .as_i64()
                    .unwrap_or(0);
                assert!(
                    (3..=15).contains(&sleep),
                    "{name}: a preStop sleep of a few seconds, before SIGTERM"
                );
                let drain = env
                    .get("K_SHUTDOWN_GRACE")
                    .map_or(Some(25), |g| g.trim_end_matches('s').parse::<i64>().ok())
                    .unwrap_or_else(|| panic!("{name}: K_SHUTDOWN_GRACE in seconds"));
                let grace = pod["terminationGracePeriodSeconds"].as_i64().unwrap_or(30);
                assert!(
                    grace > sleep + drain,
                    "{name}: terminationGracePeriodSeconds {grace} must exceed the \
                     preStop sleep ({sleep} s) plus the drain ({drain} s)"
                );
                replicas.insert(
                    (*app).to_owned(),
                    d["spec"]["replicas"].as_i64().unwrap_or(1),
                );
            }
            containers.push(Container {
                what: format!("{} {name}", kind(d)),
                app: (*app).to_owned(),
                env,
                files,
                probes,
            });
        }
    }
    check_deployment("deploy/kubernetes", &containers, &services, &configs);
    assert_eq!(
        migrations,
        APPS.iter().map(|(a, _)| (*a).to_owned()).collect(),
        "a migration Job per app"
    );
    assert_eq!(replicas.get("shop"), Some(&2), "{replicas:?}");
    // Postgres creates paybox's database as in Docker Compose, with the
    // secret paybox's database password comes from.
    let init = config_maps["postgres-init"]["10-paybox.sh"]
        .as_str()
        .expect("the init script");
    let compose_script =
        std::fs::read_to_string(shop().join("deploy/postgres/create-paybox-database.sh"))
            .expect("the init script");
    assert_eq!(
        paybox_database_sql(init),
        paybox_database_sql(&compose_script)
    );
    assert_eq!(
        secrets["postgres-secrets"].get("paybox_db_password"),
        secrets["paybox-secrets"].get("paybox_db_password")
    );
}

#[test]
fn the_secrets_script_and_the_deployments_read_the_same_files() {
    // Every file the deployments read is one `scripts/init-secrets.sh`
    // generates, and no committed file is a real secret.
    let init = std::fs::read_to_string(shop().join("scripts/init-secrets.sh")).expect("script");
    let generated: BTreeSet<String> = init
        .lines()
        .filter_map(|l| l.trim().strip_prefix("secret "))
        .filter_map(|r| r.split_whitespace().next())
        .map(|n| format!("secrets/{n}.txt"))
        .chain(std::iter::once(
            "secrets/admin_password_hash.txt".to_owned(),
        ))
        .collect();
    let script = std::fs::read_to_string(shop().join("deploy/kubernetes/create-secrets.sh"))
        .expect("create-secrets.sh");
    let read: BTreeSet<String> = script_secrets(&script)
        .into_values()
        .flat_map(BTreeMap::into_values)
        .collect();
    let doc = &yaml(&shop().join("docker-compose.yml"))[0];
    let composed: BTreeSet<String> = doc["secrets"]
        .as_hash()
        .expect("secrets")
        .values()
        .filter_map(|s| s["file"].as_str())
        .map(|f| f.trim_start_matches("./").to_owned())
        .collect();
    assert_eq!(read, composed, "Kubernetes and Compose read the same files");
    for file in &composed {
        assert!(generated.contains(file), "init-secrets.sh generates {file}");
    }
    let ignore = std::fs::read_to_string(shop().join(".gitignore")).expect(".gitignore");
    assert!(ignore.lines().any(|l| l == "secrets/*"), "{ignore}");
    assert!(
        ignore.lines().any(|l| l == "!secrets/*.example"),
        "{ignore}"
    );
    let docker_ignore =
        std::fs::read_to_string(shop().join("Dockerfile.dockerignore")).expect("dockerignore");
    assert!(
        docker_ignore.lines().any(|l| l == "examples/shop/secrets/"),
        "secrets never enter an image: {docker_ignore}"
    );
}
