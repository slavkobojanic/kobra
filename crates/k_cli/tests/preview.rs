//! `k preview` (docs/design/57-previews.md, "`k preview`";
//! docs/compiler/preview.md): the server on the `previewing` fixture, its
//! index and pages, the editor protocol's live rebuilds and error badge,
//! and, in real headless Chrome when there is one, a mounted preview whose
//! toggle changes what it renders.
//!
//! ```text
//! cargo test -p k_cli --test it -- preview::
//! ```

#![allow(clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::apps::{Scratch, copy_project};
use crate::page;

/// A running `k preview`, killed when dropped.
struct Preview {
    child: Child,
    stdin: Option<ChildStdin>,
    /// Lines of standard output (the editor protocol with `--editor`).
    lines: mpsc::Receiver<String>,
    /// `http://127.0.0.1:<port>`.
    origin: String,
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Preview {
    fn start(dir: &Path, editor: bool) -> Preview {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_k"));
        cmd.arg("preview").arg("--no-open").arg(dir);
        if editor {
            cmd.arg("--editor");
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("k preview starts");
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut p = Preview {
            child,
            stdin,
            lines,
            origin: String::new(),
        };
        let first = p.line(Duration::from_secs(60));
        let url = if editor {
            let v: Value = serde_json::from_str(&first).unwrap();
            assert_eq!(v["type"], "ready", "{first}");
            v["url"].as_str().unwrap().to_owned()
        } else {
            first
                .strip_prefix("previews on ")
                .unwrap_or_else(|| panic!("{first}"))
                .to_owned()
        };
        p.origin = url.trim_end_matches('/').to_owned();
        p
    }

    fn line(&self, wait: Duration) -> String {
        self.lines
            .recv_timeout(wait)
            .unwrap_or_else(|_| panic!("k preview said nothing for {wait:?}"))
    }

    /// The next editor message of type `ty`, skipping others.
    fn message(&self, ty: &str) -> Value {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "no `{ty}` message");
            let v: Value = serde_json::from_str(&self.line(left)).unwrap();
            if v["type"] == ty {
                return v;
            }
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn get(&self, path: &str) -> (u16, String) {
        http(&self.origin, "GET", path, "")
    }

    fn post(&self, path: &str, body: &Value) -> Value {
        let (status, text) = http(&self.origin, "POST", path, &body.to_string());
        assert_eq!(status, 200, "{path}: {text}");
        serde_json::from_str(&text).unwrap()
    }

    fn state(&self) -> Value {
        serde_json::from_str(&self.get("/api/state").1).unwrap()
    }
}

/// One HTTP/1.1 exchange with `connection: close`.
fn http(origin: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let host = origin.trim_start_matches("http://");
    let mut s = TcpStream::connect(host).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nhost: {host}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, body.to_owned())
}

/// The page's event stream: one JSON event per message.
fn events(origin: &str) -> mpsc::Receiver<Value> {
    let host = origin.trim_start_matches("http://").to_owned();
    let mut s = TcpStream::connect(&host).unwrap();
    write!(s, "GET /api/events HTTP/1.1\r\nhost: {host}\r\n\r\n").unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(s).lines().map_while(Result::ok) {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(v) = serde_json::from_str::<Value>(data)
                && tx.send(v).is_err()
            {
                break;
            }
        }
    });
    rx
}

/// The next event of type `ty`, skipping others.
fn event(rx: &mpsc::Receiver<Value>, ty: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        let v = rx
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("no `{ty}` event"));
        if v["type"] == ty {
            return v;
        }
    }
}

/// A copy of the `previewing` fixture.
fn fixture(scratch: &Scratch) -> PathBuf {
    let dir = scratch.0.join("previewing");
    copy_project(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/previewing"),
        &dir,
    );
    dir.canonicalize().unwrap()
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// The preview of `file` labelled `label` in `/api/state`.
fn preview<'a>(state: &'a Value, file: &str, label: &str) -> &'a Value {
    state["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == file)
        .and_then(|f| {
            f["previews"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["label"] == label)
        })
        .unwrap_or_else(|| panic!("no preview {label:?} in {file}: {state}"))
}

/// The index lists every preview by file; a preview's page is the panel;
/// an edit sent over the editor protocol rebuilds (control values kept by
/// name and type) and every page is told to remount; a broken edit keeps
/// the last good build under an error badge; moving the cursor into a
/// preview selects it.
#[test]
fn edits_over_the_editor_protocol_rebuild_and_errors_keep_the_last_build() {
    let scratch = Scratch::new("preview-editor");
    let dir = fixture(&scratch);
    let mut k = Preview::start(&dir, true);
    let built = k.message("built");
    assert_eq!(built["build"], 1);
    let (status, index) = k.get("/");
    assert_eq!(status, 200);
    for text in [
        "src/shop/Counter.k",
        "src/shop/Badge.k",
        "setup throws",
        "signed out",
    ] {
        assert!(index.contains(text), "{text} in {index}");
    }
    let (status, page) = k.get("/p/src/shop/Counter.k/0");
    assert_eq!(status, 200);
    assert!(
        page.contains("/_panel/panel.js") && page.contains("id=\"switcher\""),
        "{page}"
    );
    let (status, script) = k.get("/_panel/panel.js");
    assert_eq!(status, 200);
    assert!(script.contains("EventSource"));

    let state = k.state();
    let counter = preview(&state, "src/shop/Counter.k", "Counter");
    let group = counter["group"].as_u64().unwrap();
    assert_eq!(
        counter["params"][0]["control"],
        json!({ "kind": "integer" })
    );
    assert_eq!(counter["values"], json!({ "value": 1, "label": "Items" }));
    // A value of the wrong type is refused, by parameter.
    let (status, text) = http(
        &k.origin,
        "POST",
        "/api/values",
        &json!({ "group": group, "values": { "value": "seven" } }).to_string(),
    );
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("whole number"), "{text}");
    let set = k.post(
        "/api/values",
        &json!({ "group": group, "values": { "value": 7, "label": "Cart" } }),
    );
    assert_eq!(set["values"], json!({ "value": 7, "label": "Cart" }));
    // A mount is a server of its own, which serves the frame.
    let mounted = k.post("/api/mount", &json!({ "group": group, "slot": "main" }));
    let frame_url = mounted["url"]
        .as_str()
        .unwrap()
        .trim_end_matches('/')
        .to_owned();
    let (status, frame) = http(&frame_url, "GET", "/", "");
    assert_eq!(status, 200);
    assert!(
        frame.contains("framePreview") && frame.contains("importmap"),
        "{frame}"
    );

    let stream = events(&k.origin);
    std::thread::sleep(Duration::from_millis(200));
    // Rename `label` to `caption`: `value` is kept, `caption` starts at
    // its default.
    let file = dir.join("src/shop/Counter.k");
    let text = std::fs::read_to_string(&file).unwrap();
    let renamed = text
        .replace(
            "preview (value: Int = 1, label: String = \"Items\")",
            "preview (value: Int = 1, caption: String = \"Items\")",
        )
        .replace(
            "Counter(value: bind value, label: label)",
            "Counter(value: bind value, label: caption)",
        );
    assert_ne!(renamed, text);
    k.send(&json!({ "type": "didChange", "uri": uri(&file), "text": renamed }));
    assert_eq!(k.message("built")["build"], 2);
    assert_eq!(event(&stream, "build")["build"], 2);
    let previews = k.message("previews");
    assert_eq!(previews["uri"], uri(&file));
    assert_eq!(previews["list"][1]["label"], "setup throws");
    assert_eq!(previews["list"][0]["path"], "/p/src/shop/Counter.k/0");
    let state = k.state();
    let counter = preview(&state, "src/shop/Counter.k", "Counter");
    assert_eq!(counter["values"], json!({ "value": 7, "caption": "Items" }));

    // A broken edit: the badge names the first error, the build stays.
    let broken = renamed.replace(
        "Counter(value: bind value, label: caption)",
        "Counter(value: bind value, label: nope)",
    );
    k.send(&json!({ "type": "didChange", "uri": uri(&file), "text": broken }));
    let error = k.message("error");
    assert_eq!(error["uri"], uri(&file));
    assert!(
        error["message"].as_str().unwrap().contains("nope"),
        "{error}"
    );
    assert_eq!(error["line"], 24, "{error}");
    let badge = event(&stream, "error");
    assert_eq!(badge["error"]["file"], "src/shop/Counter.k");
    let state = k.state();
    assert_eq!(state["build"], 2, "the last good build stays");
    assert!(state["error"]["message"].as_str().unwrap().contains("nope"));
    let again = k.post("/api/mount", &json!({ "group": group, "slot": "main" }));
    assert_eq!(again["build"], 2);

    // The cursor in the second preview selects it.
    let offset = broken.find("preview \"setup throws\"").unwrap() + 3;
    k.send(&json!({ "type": "select", "uri": uri(&file), "offset": offset }));
    let select = event(&stream, "select");
    assert_eq!(
        select,
        json!({ "type": "select", "file": "src/shop/Counter.k", "index": 1 })
    );

    // Closing the buffer goes back to the file on disk, which builds.
    k.send(&json!({ "type": "didClose", "uri": uri(&file) }));
    assert_eq!(k.message("built")["build"], 3);
    k.send(&json!({ "type": "shutdown" }));
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = k.child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(Instant::now() < until, "k preview stops on `shutdown`");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// In real Chrome: a mounted preview renders in the frame's own document,
/// and the panel's toggle changes what it renders.
#[test]
fn a_toggle_in_the_panel_changes_the_rendered_preview() {
    let scratch = Scratch::new("preview-chrome");
    let dir = fixture(&scratch);
    let Some(mut tab) = page::chrome(&scratch.0) else {
        return;
    };
    let k = Preview::start(&dir, false);
    let mut wait = 0;
    while k.state()["build"].is_null() {
        wait += 1;
        assert!(wait < 600, "the first build");
        std::thread::sleep(Duration::from_millis(100));
    }
    tab.open(&format!("{}/p/src/shop/Badge.k/0", k.origin));
    let frame = |tab: &mut page::Tab| {
        tab.wait("the preview's frame", |t| {
            let src = t.eval("document.querySelector('iframe')?.src ?? ''");
            src.as_str().filter(|s| s.contains('#')).map(str::to_owned)
        })
    };
    let first = frame(&mut tab);
    // The frame on its own: the preview with its defaults.
    let mut view = tab.new_tab();
    view.open(&first);
    view.wait_text("Plain: New");
    view.wait_text("Quiet");
    let strong = tab.css(".controls input[type=checkbox]");
    tab.click(&strong);
    tab.wait("a remount", |t| {
        let src = t.eval("document.querySelector('iframe')?.src ?? ''");
        src.as_str()
            .filter(|s| s.contains('#') && *s != first)
            .map(|_| ())
    });
    let state = k.state();
    assert_eq!(
        preview(&state, "src/shop/Badge.k", "Badge")["values"]["strong"],
        true
    );
    let second = frame(&mut tab);
    view.open(&second);
    view.wait_text("Strong: New");
}
