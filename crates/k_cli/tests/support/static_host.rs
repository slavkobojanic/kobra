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

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

/// A running static host, stopped when dropped.
pub struct StaticHost {
    /// The address it listens on.
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StaticHost {
    /// Serves `public` on a free loopback port.
    pub fn start(public: &Path) -> StaticHost {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = listener.local_addr().expect("an address");
        let stop = Arc::new(AtomicBool::new(false));
        let root = public.to_path_buf();
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(s) = stream {
                    let _ = answer(s, &root);
                }
            }
        });
        StaticHost {
            addr,
            stop,
            thread: Some(thread),
        }
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
    full.is_file().then_some(full)
}

/// Reads one request from `s` and answers it.
fn answer(s: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let mut out = s;
    if method != "GET" && method != "HEAD" {
        return respond(&mut out, 405, "text/plain", "no-cache", b"", &method);
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
