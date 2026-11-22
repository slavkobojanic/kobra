//! A static file host for the tests of static-only apps
//! (`docs/design/38-deployment.md`, "Static-only apps"): it serves a
//! `dist/public` directory the way the examples' nginx images do, so a
//! test sees what a deployment serves.
//!
//! * `/assets/…` answers the hashed file with
//!   `cache-control: public, max-age=31536000, immutable`, or 404;
//! * any other path answers the file when one exists, else `index.html`
//!   (the single-page-app fallback, so a deep link reaches the client
//!   router), with `cache-control: no-cache`.
//!
//! It speaks just enough HTTP/1.1 for the tests: one request per
//! connection, `GET` and `HEAD`, and closes the connection after the
//! response.
//!
//! For the fake GitHub API of the toolchain tests
//! (`docs/design/48-releases-and-versioning.md`, "Private distribution"):
//!
//! * a directory is answered with its `_index.json` when it has one, so a
//!   collection (`repos/o/r/releases`) and its members
//!   (`repos/o/r/releases/latest`) can both be files;
//! * a missing `<path>` with a `<path>.redirect` file next to it answers
//!   `302 Found` with the file's contents as the `location`;
//! * every request is recorded ([`StaticHost::requests`]) with its headers,
//!   so a test can see which host was sent a token.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// One request a [`StaticHost`] answered.
#[derive(Clone, Debug)]
pub struct Request {
    /// The request target (`/repos/o/r/releases?per_page=30`).
    pub target: String,
    /// Its headers, names in lower case.
    pub headers: Vec<(String, String)>,
}

impl Request {
    /// The value of header `name` (lower case), if sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A running static host, stopped when dropped.
pub struct StaticHost {
    /// The address it listens on.
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<Request>>>,
    thread: Option<JoinHandle<()>>,
}

impl StaticHost {
    /// Serves `public` on a free loopback port.
    #[allow(dead_code)]
    pub fn start(public: &Path) -> StaticHost {
        StaticHost::serve(
            TcpListener::bind("127.0.0.1:0").expect("a free port"),
            public,
        )
    }

    /// Serves `public` on `listener`, already bound.
    pub fn serve(listener: TcpListener, public: &Path) -> StaticHost {
        let addr = listener.local_addr().expect("an address");
        let stop = Arc::new(AtomicBool::new(false));
        let root = public.to_path_buf();
        let flag = Arc::clone(&stop);
        let log = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&log);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(s) = stream {
                    let _ = answer(s, &root, &record);
                }
            }
        });
        StaticHost {
            addr,
            stop,
            log,
            thread: Some(thread),
        }
    }

    /// Every request answered so far, oldest first.
    pub fn requests(&self) -> Vec<Request> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Drop for StaticHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accept loop so it sees the flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The media type of a served file.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "map" | "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// The file below `root` that `target` names, if it is a plain relative
/// path to an existing file.
fn file_for(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let relative = Path::new(path.trim_start_matches('/'));
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let full = root.join(relative);
    if full.is_dir() {
        let listing = full.join("_index.json");
        return listing.is_file().then_some(listing);
    }
    full.is_file().then_some(full)
}

/// Where a missing `target` redirects to, from its `<path>.redirect` file.
fn redirect_for(root: &Path, target: &str) -> Option<String> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let file = file_for(root, &format!("{path}.redirect"))?;
    std::fs::read_to_string(file)
        .ok()
        .map(|t| t.trim().to_owned())
}

/// Reads one request from `s` and answers it.
fn answer(s: TcpStream, root: &Path, log: &Mutex<Vec<Request>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    if let Ok(mut l) = log.lock() {
        l.push(Request {
            target: target.clone(),
            headers,
        });
    }
    let mut out = s;
    if method != "GET" && method != "HEAD" {
        return respond(&mut out, 405, "text/plain", "no-cache", b"", &method);
    }
    if file_for(root, &target).is_none()
        && let Some(location) = redirect_for(root, &target)
    {
        let head = format!(
            "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        out.write_all(head.as_bytes())?;
        return out.flush();
    }
    let asset = target.starts_with("/assets/");
    let file = match file_for(root, &target) {
        Some(f) => Some(f),
        None if asset => None,
        None => file_for(root, "index.html"),
    };
    match file {
        Some(f) => {
            let body = std::fs::read(&f)?;
            let cache = if asset {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            respond(&mut out, 200, content_type(&f), cache, &body, &method)
        }
        None => respond(
            &mut out,
            404,
            "text/plain",
            "no-cache",
            b"not found",
            &method,
        ),
    }
}

/// Writes a complete response and closes the connection.
fn respond(
    out: &mut TcpStream,
    status: u16,
    ty: &str,
    cache: &str,
    body: &[u8],
    method: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Method Not Allowed",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {ty}\r\ncache-control: {cache}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    out.write_all(head.as_bytes())?;
    if method != "HEAD" {
        out.write_all(body)?;
    }
    out.flush()
}
