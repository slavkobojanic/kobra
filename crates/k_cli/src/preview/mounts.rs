//! The mounts of `k preview` (docs/compiler/preview.md, "Mounts"): every
//! preview the panel shows runs in a server of its own, a
//! [`k_server::Server`] with one worker over the preview bundle, a fresh
//! process and an in-memory SQLite database with every migration applied,
//! as `k test` gives a test. The browser loads the frame from that server,
//! so the preview's requests reach its server container over the real
//! wire format, and the frame's own origin keeps its document, styles and
//! theme apart from the panel's.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use k_server::http::StaticAsset;
use k_server::{JsModule, Server, ServerBundle, ServerOptions};

/// What a build gives every mount.
pub(crate) struct Bundle {
    /// The server modules of the preview bundle.
    server: Vec<JsModule>,
    /// The server entry (`k:/app/$server.js`), when the project has
    /// server code.
    server_entry: Option<String>,
    /// The rpc paths the front checks.
    rpc_paths: BTreeSet<String>,
    /// The database half of the build.
    database: Option<k_db::manifest::DbManifest>,
    /// The endpoint templates without a session (no forgery check).
    forgery_exempt: Vec<String>,
    /// Every file the frame loads: the client chunks under `/_k/c/`, the
    /// runtime under `/_k/rt/`, the server modules test code may import
    /// under `/_k/app/`.
    assets: Vec<StaticAsset>,
    /// The client entry's path (`app.1a2b3c4d.js`).
    entry: String,
    /// The client's stylesheets.
    styles: Vec<String>,
}

/// The longest a stopping mount waits for its requests.
const GRACE: std::time::Duration = std::time::Duration::from_millis(200);

impl Bundle {
    /// The mount bundle of a preview build.
    pub(crate) fn new(output: &k_codegen::Output) -> Bundle {
        let content_type = |path: &str| match path.rsplit('.').next() {
            Some("js") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("map") => "application/json",
            _ => "application/octet-stream",
        };
        let mut assets: Vec<StaticAsset> = output
            .client
            .iter()
            .map(|f| StaticAsset {
                path: format!("/_k/c/{}", f.path),
                content_type: content_type(&f.path).to_owned(),
                body: f.contents.clone().into_bytes().into(),
                immutable: false,
            })
            .collect();
        assets.extend(k_runtime::modules().iter().map(|m| StaticAsset {
            path: format!("/_k/rt/{}", m.path),
            content_type: content_type(m.path).to_owned(),
            body: m.source.as_bytes().to_vec().into(),
            immutable: false,
        }));
        assets.extend(output.server.iter().filter_map(|f| {
            let rest = f.path.strip_prefix("k:/app/")?;
            Some(StaticAsset {
                path: format!("/_k/app/{rest}"),
                content_type: content_type(&f.path).to_owned(),
                body: f.contents.clone().into_bytes().into(),
                immutable: false,
            })
        }));
        let entry = output
            .client
            .iter()
            .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
            .map(|f| f.path.clone())
            .unwrap_or_default();
        Bundle {
            server: output
                .server
                .iter()
                .map(|f| JsModule {
                    specifier: f.path.clone(),
                    source: f.contents.clone(),
                })
                .collect(),
            server_entry: output.server_entry.clone(),
            rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
            database: output.database.clone(),
            forgery_exempt: output
                .manifest
                .endpoints
                .iter()
                .filter(|e| !e.uses_session)
                .map(|e| e.template.clone())
                .collect(),
            assets,
            entry,
            styles: output
                .client
                .iter()
                .filter(|f| f.path.ends_with(".css"))
                .map(|f| f.path.clone())
                .collect(),
        }
    }

    /// The frame document of group `group`: the import map that resolves
    /// the bundle's `k:` imports to the served files, the stylesheets, and
    /// the module that mounts the preview (`framePreview`).
    fn frame(&self, group: usize) -> String {
        // Every specifier is listed: Chrome doesn't prefix-match keys of a
        // scheme it doesn't know (`k:/runtime/`).
        let mut map = serde_json::Map::new();
        for (short, file) in [
            ("k:runtime", "runtime/index.js"),
            ("k:runtime/server", "runtime/server.js"),
            ("k:testing", "testing/harness.js"),
            ("k:testing/kit", "testing/kit.js"),
        ] {
            map.insert(
                short.to_owned(),
                serde_json::json!(format!("/_k/rt/{file}")),
            );
        }
        for m in k_runtime::modules() {
            map.insert(
                m.specifier.to_owned(),
                serde_json::json!(format!("/_k/rt/{}", m.path)),
            );
        }
        for m in &self.server {
            if let Some(rest) = m.specifier.strip_prefix("k:/app/") {
                map.insert(
                    m.specifier.clone(),
                    serde_json::json!(format!("/_k/app/{rest}")),
                );
            }
        }
        let imports = serde_json::json!({ "imports": map });
        let styles: String = self
            .styles
            .iter()
            .map(|s| format!("<link rel=\"stylesheet\" href=\"/_k/c/{s}\">\n"))
            .collect();
        let entry = serde_json::json!(format!("/_k/c/{}", self.entry));
        format!(
            "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
             <title></title>\n\
             <script type=\"importmap\">{imports}</script>\n{styles}</head>\n<body>\n\
             <script type=\"module\">\n\
             import {{ framePreview }} from \"k:testing/kit\";\n\
             const {{ $groups, $previews, $pages }} = await import({entry});\n\
             await framePreview($groups, $previews, $pages ?? [], {group});\n\
             </script>\n</body>\n</html>\n"
        )
    }
}

/// The running mounts, one per panel slot (a single view's frame, or a
/// grid cell's).
#[derive(Default)]
pub(crate) struct Mounts {
    /// The mount of each slot, with its number.
    slots: HashMap<String, (u64, Server)>,
    /// The number of the next mount.
    next: u64,
}

/// The most mounts that run at once: the oldest stops beyond it.
const MAX_MOUNTS: usize = 48;

impl Mounts {
    /// Starts a fresh mount of group `group` of `bundle` for `slot`,
    /// stopping the slot's previous one, and returns its URL.
    ///
    /// # Errors
    ///
    /// Why the server couldn't start.
    pub(crate) fn mount(
        &mut self,
        bundle: &Arc<Bundle>,
        group: usize,
        slot: &str,
    ) -> Result<String, String> {
        if let Some((_, old)) = self.slots.remove(slot) {
            stop(old);
        }
        while self.slots.len() >= MAX_MOUNTS {
            let Some(oldest) = self
                .slots
                .iter()
                .min_by_key(|(_, (n, _))| *n)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some((_, s)) = self.slots.remove(&oldest) {
                stop(s);
            }
        }
        let server = start(bundle, group)?;
        let url = format!("http://127.0.0.1:{}/", server.local_addr().port());
        self.next += 1;
        self.slots.insert(slot.to_owned(), (self.next, server));
        Ok(url)
    }

    /// Stops every mount.
    pub(crate) fn stop_all(&mut self) {
        for (_, (_, s)) in self.slots.drain() {
            stop(s);
        }
    }
}

/// Stops `server` without waiting for it.
fn stop(server: Server) {
    std::thread::spawn(move || {
        let _ = server.shutdown();
    });
}

/// Starts the server of one mount on a free loopback port.
fn start(bundle: &Bundle, group: usize) -> Result<Server, String> {
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("no free port: {e}"))?;
    let server = bundle.server_entry.as_deref().map_or_else(
        || "null".to_owned(),
        |spec| format!("(await import({})).$test", serde_json::json!(spec)),
    );
    let main = format!(
        "import * as kit from \"k:testing/kit\";\n\
         const server = {server};\n\
         await kit.servePreview(server, {group}, {frame});\n",
        frame = serde_json::json!(bundle.frame(group)),
    );
    let entry = "k:/preview/main.js".to_owned();
    let mut modules = bundle.server.clone();
    modules.push(JsModule {
        specifier: entry.clone(),
        source: main,
    });
    let mut options = ServerOptions::new(
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        ServerBundle {
            modules,
            entry,
            rpc_paths: bundle.rpc_paths.clone(),
        },
    );
    // One isolate holds the mount's server container and its handles.
    options.workers = 1;
    options.public_origin = format!("http://127.0.0.1:{port}");
    options.log = k_server::Output::Stderr("preview");
    options.shutdown_grace = GRACE;
    options.dev_errors = true;
    options.forgery_exempt = bundle.forgery_exempt.clone();
    options.assets = bundle.assets.clone();
    options.database = bundle
        .database
        .clone()
        .map(|manifest| k_server::db::DatabaseSetup {
            url: "sqlite::memory:".to_owned(),
            password: None,
            manifest,
        });
    Server::start(options).map_err(|e| e.to_string())
}
