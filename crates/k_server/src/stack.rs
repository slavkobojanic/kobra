//! Server stack traces mapped back to the `.k` files
//! (`docs/compiler/server.md` §15; codegen.md §18).
//!
//! Every generated server module that holds K code ends with an inline
//! source map (`//# sourceMappingURL=data:application/json;base64,…`).
//! [`Symbolicator`] decodes a module's map the first time a stack trace
//! names it and rewrites each frame location `k:/app/home.js:12:5` to the
//! K file, line and column it came from, `src/home/Home.k:17:9`, keeping
//! the JavaScript location after it for the curious:
//!
//! ```text
//! at greeting (src/home/GreeterClient.k:7:13) [k:/app/home.js:40:22]
//! ```
//!
//! A frame without a map (the runtime, generated glue) is left as it is.
//! The logged traces and the development 500 page go through it.

use std::cell::RefCell;
use std::collections::HashMap;

/// The prefix of the K sources server maps name.
const SERVER_ROOT: &str = "k:/_k/src/";
/// The comment that carries an inline map.
const DATA_URL: &str = "//# sourceMappingURL=data:application/json;charset=utf-8;base64,";

/// Maps `k:/…js:line:col` locations in text to `.k` locations.
#[derive(Debug, Default)]
pub struct Symbolicator {
    /// Each module's inline map, still base64 (decoded on first use).
    encoded: HashMap<String, String>,
    /// Decoded maps; `None` when a map didn't decode.
    decoded: RefCell<HashMap<String, Option<sourcemap::SourceMap>>>,
}

impl Symbolicator {
    /// The symbolicator of a bundle: `(specifier, source)` pairs.
    pub fn new<'a>(modules: impl IntoIterator<Item = (&'a str, &'a str)>) -> Symbolicator {
        let encoded = modules
            .into_iter()
            .filter_map(|(spec, src)| {
                let (_, b64) = src.rsplit_once(DATA_URL)?;
                Some((spec.to_owned(), b64.trim().to_owned()))
            })
            .collect();
        Symbolicator {
            encoded,
            decoded: RefCell::new(HashMap::new()),
        }
    }

    /// `true` when no module has a map.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.encoded.is_empty()
    }

    /// The K location of generated `line`:`col` (1-based) of `module`:
    /// `(path, line, col)`, 1-based, the path project-relative.
    #[must_use]
    pub fn lookup(&self, module: &str, line: u32, col: u32) -> Option<(String, u32, u32)> {
        let mut decoded = self.decoded.borrow_mut();
        let map = decoded.entry(module.to_owned()).or_insert_with(|| {
            let b64 = self.encoded.get(module)?;
            let json = decode_base64(b64)?;
            sourcemap::SourceMap::from_slice(&json).ok()
        });
        let map = map.as_ref()?;
        let t = map.lookup_token(line.checked_sub(1)?, col.saturating_sub(1))?;
        if t.get_dst_line() != line - 1 {
            return None;
        }
        let src = t.get_source()?;
        let path = src.strip_prefix(SERVER_ROOT).unwrap_or(src);
        Some((path.to_owned(), t.get_src_line() + 1, t.get_src_col() + 1))
    }

    /// `text` with every `k:/….js:line:col` location that a map covers
    /// replaced by its K location (the JavaScript one kept in brackets).
    #[must_use]
    pub fn symbolicate(&self, text: &str) -> String {
        if self.encoded.is_empty() || !text.contains("k:/") {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find("k:/") {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            match self.location(tail) {
                Some((len, mapped)) => {
                    out.push_str(&mapped);
                    rest = &tail[len..];
                }
                None => {
                    out.push_str("k:/");
                    rest = &tail[3..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Parses `k:/path.js:line:col` at the start of `s`; returns its length
    /// and the replacement when it maps.
    fn location(&self, s: &str) -> Option<(usize, String)> {
        let end = s
            .find(|c: char| c.is_whitespace() || c == ')' || c == ']')
            .unwrap_or(s.len());
        let loc = &s[..end];
        let (rest, col) = loc.rsplit_once(':')?;
        let (module, line) = rest.rsplit_once(':')?;
        let line: u32 = line.parse().ok()?;
        let col: u32 = col.parse().ok()?;
        let (path, kl, kc) = self.lookup(module, line, col)?;
        Some((end, format!("{path}:{kl}:{kc} [{loc}]")))
    }
}

/// Decodes standard base64; `None` on a bad digit.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// A map of runtime module `text` onto itself, every source on the
/// `ignoreList`, as an inline comment: the inspector then steps over the
/// runtime and leaves it out of stack traces, as DevTools does for the
/// client runtime.
#[must_use]
pub fn ignored_runtime_comment(specifier: &str, text: &str) -> String {
    let lines = text.split('\n').count();
    let mut mappings = String::from("AAAA");
    for _ in 1..lines {
        mappings.push_str(";AACA");
    }
    let map = serde_json::json!({
        "version": 3,
        "sources": [specifier],
        "names": [],
        "mappings": mappings,
        "ignoreList": [0],
        "x_google_ignoreList": [0],
    });
    let json = map.to_string();
    format!("\n{DATA_URL}{}\n", encode_base64(json.as_bytes()))
}

/// Standard base64 with padding.
fn encode_base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module_with_map() -> (String, String) {
        // Line 2, column 2 of the module maps to Home.k 17:9.
        let map = serde_json::json!({
            "version": 3,
            "sources": ["k:/_k/src/src/home/Home.k"],
            "names": [],
            "mappings": ";EAgBQ",
        });
        let src = format!(
            "export function f() {{\n  boom();\n}}\n{DATA_URL}{}\n",
            encode_base64(map.to_string().as_bytes())
        );
        ("k:/app/home.js".to_owned(), src)
    }

    #[test]
    fn maps_frames_to_k_and_keeps_the_rest() {
        let (spec, src) = module_with_map();
        let s = Symbolicator::new([(spec.as_str(), src.as_str())]);
        let trace = "Error: boom\n    at f (k:/app/home.js:2:3)\n    at k:/runtime/server.js:10:4";
        let out = s.symbolicate(trace);
        assert!(
            out.contains("at f (src/home/Home.k:17:9 [k:/app/home.js:2:3])"),
            "{out}"
        );
        assert!(out.contains("at k:/runtime/server.js:10:4"), "{out}");
    }

    #[test]
    fn base64_round_trips() {
        let text = b"any carnal pleasure.";
        assert_eq!(
            decode_base64(&encode_base64(text)).as_deref(),
            Some(&text[..])
        );
    }
}
