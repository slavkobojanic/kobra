//! Source maps (v3, ECMA-426) from generated JavaScript and CSS back to the
//! `.k` files (`docs/compiler/codegen.md` §18).
//!
//! The printer records [`Mark`]s while it prints ([`crate::js::Mark`]: a
//! byte offset into the output, a K [`Loc`] and maybe an original name);
//! [`SourceMapBuilder`] turns them into a map: generated lines and UTF-16
//! columns, K files by their project-relative path below the virtual root
//! [`CLIENT_ROOT`] (`/_k/src/`, which `k dev` serves) or [`SERVER_ROOT`]
//! (`k:/_k/src/`, the server isolate's), the K text embedded in
//! development builds (`sourcesContent`), and the `names` of renamed
//! locals.
//!
//! Release clients are minified after printing; [`compose`] chains oxc's
//! map (minified → readable chunk) with the chunk's K map (readable chunk →
//! `.k`), so minified code maps straight to `.k`. Chunks that hold no K
//! code (the runtime, feature chunks, the `app` entry) get a map whose one
//! source is on the `ignoreList` ([`identity`]), so stepping and stack
//! traces skip them.
//!
//! The encoder and decoder are small and dependency free; the tests check
//! them against the `sourcemap` crate, an independent implementation.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::js::{Loc, Mark};

/// The URL prefix of the K sources a client map names: `/_k/src/app/main.k`.
pub const CLIENT_ROOT: &str = "/_k/src/";
/// The prefix of the K sources a server module's map names:
/// `k:/_k/src/app/main.k` (the isolate's modules are `k:/app/…`).
pub const SERVER_ROOT: &str = "k:/_k/src/";
/// The prefix of the runtime and generated glue sources (ignore-listed):
/// `/_k/runtime/runtime.js`.
pub const RUNTIME_ROOT: &str = "/_k/runtime/";

/// One decoded mapping segment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    /// The generated line (0-based).
    pub line: u32,
    /// The generated column (0-based, UTF-16).
    pub col: u32,
    /// The source, line and column it maps to, if any.
    pub orig: Option<(u32, u32, u32)>,
    /// The index into `names`, if any.
    pub name: Option<u32>,
}

/// A decoded source map.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SourceMap {
    /// The source URLs.
    pub sources: Vec<String>,
    /// The embedded source texts, by source.
    pub sources_content: Vec<Option<String>>,
    /// The original names.
    pub names: Vec<String>,
    /// The sources on the ignore list.
    pub ignore_list: Vec<u32>,
    /// The segments, sorted by generated position.
    pub segments: Vec<Segment>,
}

impl SourceMap {
    /// Parses a v3 map.
    ///
    /// # Errors
    ///
    /// A message when the JSON or its `mappings` are malformed.
    pub fn parse(json: &str) -> Result<SourceMap, String> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let strings = |key: &str| -> Vec<String> {
            v[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|s| s.as_str().unwrap_or("").to_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let sources = strings("sources");
        let names = strings("names");
        let sources_content = v["sourcesContent"]
            .as_array()
            .map(|a| a.iter().map(|s| s.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let ignore_list = v
            .get("ignoreList")
            .or_else(|| v.get("x_google_ignoreList"))
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
                    .collect()
            })
            .unwrap_or_default();
        let segments = decode_mappings(v["mappings"].as_str().unwrap_or(""))?;
        Ok(SourceMap {
            sources,
            sources_content,
            names,
            ignore_list,
            segments,
        })
    }

    /// The segment that covers generated `line`:`col` (0-based): the last
    /// one at or before it on that line.
    #[must_use]
    pub fn lookup(&self, line: u32, col: u32) -> Option<&Segment> {
        let i = self
            .segments
            .partition_point(|s| (s.line, s.col) <= (line, col));
        let s = self.segments.get(i.checked_sub(1)?)?;
        (s.line == line).then_some(s)
    }

    /// Every generated position that maps to `source` at original `line`
    /// (0-based), in generated order.
    #[must_use]
    pub fn generated_for(&self, source: &str, line: u32) -> Vec<(u32, u32)> {
        let Some(src) = self.sources.iter().position(|s| s == source) else {
            return Vec::new();
        };
        let src = u32::try_from(src).unwrap_or(u32::MAX);
        self.segments
            .iter()
            .filter(|s| matches!(s.orig, Some((f, l, _)) if f == src && l == line))
            .map(|s| (s.line, s.col))
            .collect()
    }
}

// ---------------------------------------------------------------- VLQ

/// The base64 alphabet.
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Appends `n` as a base64 VLQ.
fn vlq(out: &mut String, n: i64) {
    let mut v = if n < 0 { ((-n) << 1) | 1 } else { n << 1 };
    loop {
        let mut digit = v & 31;
        v >>= 5;
        if v > 0 {
            digit |= 32;
        }
        out.push(B64[usize::try_from(digit).unwrap_or(0)] as char);
        if v == 0 {
            break;
        }
    }
}

/// Decodes a `mappings` string.
fn decode_mappings(text: &str) -> Result<Vec<Segment>, String> {
    let mut out = Vec::new();
    let (mut src, mut oline, mut ocol, mut name) = (0i64, 0i64, 0i64, 0i64);
    for (line, group) in text.split(';').enumerate() {
        let line = u32::try_from(line).map_err(|e| e.to_string())?;
        let mut col = 0i64;
        for seg in group.split(',').filter(|s| !s.is_empty()) {
            let mut fields = Vec::with_capacity(5);
            let (mut value, mut shift) = (0i64, 0u32);
            for b in seg.bytes() {
                let digit = B64
                    .iter()
                    .position(|&c| c == b)
                    .ok_or_else(|| format!("bad base64 digit {:?}", b as char))?;
                let digit = i64::try_from(digit).map_err(|e| e.to_string())?;
                value += (digit & 31) << shift;
                if digit & 32 != 0 {
                    shift += 5;
                } else {
                    let v = if value & 1 == 1 {
                        -(value >> 1)
                    } else {
                        value >> 1
                    };
                    fields.push(v);
                    value = 0;
                    shift = 0;
                }
            }
            col += fields.first().copied().unwrap_or(0);
            let to_u32 = |n: i64| u32::try_from(n).map_err(|_| format!("negative position {n}"));
            let orig = if fields.len() >= 4 {
                src += fields[1];
                oline += fields[2];
                ocol += fields[3];
                Some((to_u32(src)?, to_u32(oline)?, to_u32(ocol)?))
            } else {
                None
            };
            let n = if fields.len() >= 5 {
                name += fields[4];
                Some(to_u32(name)?)
            } else {
                None
            };
            out.push(Segment {
                line,
                col: to_u32(col)?,
                orig,
                name: n,
            });
        }
    }
    Ok(out)
}

/// Encodes segments (sorted by generated position) as `mappings`.
fn encode_mappings(segments: &[Segment]) -> String {
    let mut out = String::new();
    let (mut src, mut oline, mut ocol, mut name) = (0i64, 0i64, 0i64, 0i64);
    let mut line = 0u32;
    let mut col = 0i64;
    let mut first = true;
    for s in segments {
        while line < s.line {
            out.push(';');
            line += 1;
            col = 0;
            first = true;
        }
        if !first {
            out.push(',');
        }
        first = false;
        vlq(&mut out, i64::from(s.col) - col);
        col = i64::from(s.col);
        if let Some((f, l, c)) = s.orig {
            vlq(&mut out, i64::from(f) - src);
            vlq(&mut out, i64::from(l) - oline);
            vlq(&mut out, i64::from(c) - ocol);
            src = i64::from(f);
            oline = i64::from(l);
            ocol = i64::from(c);
            if let Some(n) = s.name {
                vlq(&mut out, i64::from(n) - name);
                name = i64::from(n);
            }
        }
    }
    out
}

/// Standard base64 (with padding) of `bytes`, for `data:` URLs.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The map as JSON.
#[must_use]
pub fn to_json(map: &SourceMap, file: Option<&str>) -> String {
    let mut v = serde_json::json!({
        "version": 3,
        "sources": map.sources,
        "names": map.names,
        "mappings": encode_mappings(&map.segments),
    });
    if let Some(file) = file {
        v["file"] = serde_json::Value::from(file);
    }
    if map.sources_content.iter().any(Option::is_some) {
        v["sourcesContent"] = serde_json::Value::from(map.sources_content.clone());
    }
    if !map.ignore_list.is_empty() {
        v["ignoreList"] = serde_json::Value::from(map.ignore_list.clone());
        // Chrome before 2024 reads the vendor key.
        v["x_google_ignoreList"] = serde_json::Value::from(map.ignore_list.clone());
    }
    v.to_string()
}

// ------------------------------------------------------------ building

/// Where a map's K sources come from: each file's URL, text and line starts.
pub(crate) struct Sources<'a> {
    /// The analysis whose files the [`Loc`]s name.
    analysis: &'a k_sema::Analysis,
    /// The URL prefix: [`CLIENT_ROOT`] or [`SERVER_ROOT`].
    root: &'static str,
    /// Embed the K text (`sourcesContent`).
    pub content: bool,
    /// Line-start offsets by file, computed once.
    lines: std::cell::RefCell<HashMap<u32, std::rc::Rc<Vec<u32>>>>,
}

impl<'a> Sources<'a> {
    /// The sources of `analysis`, named below `root`.
    pub(crate) fn new(analysis: &'a k_sema::Analysis, root: &'static str, content: bool) -> Self {
        Sources {
            analysis,
            root,
            content,
            lines: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// The URL of file `file`: its project path below the root; stdlib files
    /// are below `$std/` (and ignore-listed).
    fn url(&self, file: u32) -> Option<(String, bool)> {
        let id = k_diagnostics::FileId::new(file);
        match self.analysis.origin(id)? {
            k_sema::FileOrigin::Project(p) => Some((format!("{}{p}", self.root), false)),
            k_sema::FileOrigin::Stdlib(p) => Some((format!("{}$std/{p}", self.root), true)),
        }
    }

    /// The 0-based line and UTF-16 column of `loc`.
    fn line_col(&self, loc: Loc) -> Option<(u32, u32)> {
        let text = self.analysis.text(k_diagnostics::FileId::new(loc.file))?;
        let starts = {
            let mut cache = self.lines.borrow_mut();
            std::rc::Rc::clone(cache.entry(loc.file).or_insert_with(|| {
                let mut v = vec![0u32];
                for (i, b) in text.bytes().enumerate() {
                    if b == b'\n' {
                        v.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
                    }
                }
                std::rc::Rc::new(v)
            }))
        };
        let line = starts
            .partition_point(|&s| s <= loc.offset)
            .checked_sub(1)?;
        let start = starts[line] as usize;
        let end = (loc.offset as usize).min(text.len());
        let col = text.get(start..end)?.encode_utf16().count();
        Some((u32::try_from(line).ok()?, u32::try_from(col).ok()?))
    }
}

/// Builds the K map of a printed text from its marks.
pub(crate) fn from_marks(text: &str, marks: &[Mark], sources: &Sources<'_>) -> SourceMap {
    let mut map = SourceMap::default();
    let mut src_ix: HashMap<u32, Option<u32>> = HashMap::new();
    let mut name_ix: HashMap<String, u32> = HashMap::new();
    // Walk the text once, turning byte offsets into lines and UTF-16
    // columns (marks are in output order).
    let (mut line, mut col, mut at) = (0u32, 0u32, 0usize);
    for m in marks {
        if m.pos < at {
            continue;
        }
        let Some(chunk) = text.get(at..m.pos) else {
            continue;
        };
        for ch in chunk.chars() {
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += u32::try_from(ch.len_utf16()).unwrap_or(1);
            }
        }
        at = m.pos;
        let src = *src_ix.entry(m.loc.file).or_insert_with(|| {
            let (url, std) = sources.url(m.loc.file)?;
            let i = u32::try_from(map.sources.len()).ok()?;
            map.sources.push(url);
            map.sources_content.push(if sources.content {
                sources
                    .analysis
                    .text(k_diagnostics::FileId::new(m.loc.file))
                    .map(str::to_owned)
            } else {
                None
            });
            if std {
                map.ignore_list.push(i);
            }
            Some(i)
        });
        let (Some(src), Some((ol, oc))) = (src, sources.line_col(m.loc)) else {
            continue;
        };
        let name = m.name.as_ref().map(|n| {
            let next = u32::try_from(map.names.len()).unwrap_or(u32::MAX);
            *name_ix.entry(n.clone()).or_insert_with(|| {
                map.names.push(n.clone());
                next
            })
        });
        let seg = Segment {
            line,
            col,
            orig: Some((src, ol, oc)),
            name,
        };
        // Two marks at one position: the inner (later) one wins, but a
        // name is never dropped.
        match map.segments.last_mut() {
            Some(last) if last.line == line && last.col == col => {
                if seg.name.is_some() || last.name.is_none() {
                    *last = seg;
                }
            }
            _ => map.segments.push(seg),
        }
    }
    map
}

/// A map of `text` to itself as the single ignore-listed source `url`
/// (runtime and glue chunks): one segment per line, the text embedded
/// when `content`.
#[must_use]
pub fn identity(text: &str, url: &str, content: bool) -> SourceMap {
    let lines = text.split('\n').count();
    SourceMap {
        sources: vec![url.to_owned()],
        sources_content: vec![content.then(|| text.to_owned())],
        names: Vec::new(),
        ignore_list: vec![0],
        segments: (0..lines)
            .map(|l| Segment {
                line: u32::try_from(l).unwrap_or(u32::MAX),
                col: 0,
                orig: Some((0, u32::try_from(l).unwrap_or(u32::MAX), 0)),
                name: None,
            })
            .collect(),
    }
}

/// Chains `outer` (generated → intermediate, e.g. oxc's minified →
/// readable map) with `inner` (intermediate → original, the K map): each
/// segment of `outer` maps to what `inner` says about its intermediate
/// position. A name comes from `inner`, or else from `outer` when the
/// intermediate name is a renamed K local ([`crate::js::k_name`]).
/// Segments `inner` doesn't cover are dropped.
#[must_use]
pub fn compose(outer: &SourceMap, inner: &SourceMap) -> SourceMap {
    let mut map = SourceMap {
        sources: inner.sources.clone(),
        sources_content: inner.sources_content.clone(),
        names: inner.names.clone(),
        ignore_list: inner.ignore_list.clone(),
        segments: Vec::new(),
    };
    let mut name_ix: HashMap<String, u32> = map
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), u32::try_from(i).unwrap_or(u32::MAX)))
        .collect();
    for s in &outer.segments {
        let Some((_, l, c)) = s.orig else { continue };
        let Some(hit) = inner.lookup(l, c) else {
            continue;
        };
        let Some(orig) = hit.orig else { continue };
        let exact = hit.line == l && hit.col == c;
        let mut name = if exact { hit.name } else { None };
        if name.is_none()
            && let Some(js) = s.name.and_then(|n| outer.names.get(n as usize))
        {
            let k = crate::js::k_name(js).unwrap_or(js);
            if !k.starts_with('$') {
                let next = u32::try_from(map.names.len()).unwrap_or(u32::MAX);
                name = Some(*name_ix.entry(k.to_owned()).or_insert_with(|| {
                    map.names.push(k.to_owned());
                    next
                }));
            }
        }
        map.segments.push(Segment {
            line: s.line,
            col: s.col,
            orig: Some(orig),
            name,
        });
    }
    map
}

/// The `//# sourceMappingURL=` comment that ends a chunk.
#[must_use]
pub fn js_comment(url: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "//# sourceMappingURL={url}");
    s
}

/// A `data:` URL carrying `json` (server modules embed their maps).
#[must_use]
pub fn data_url(json: &str) -> String {
    format!(
        "data:application/json;charset=utf-8;base64,{}",
        base64(json.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_round_trips_through_the_sourcemap_crate() {
        let map = SourceMap {
            sources: vec!["/_k/src/a.k".into()],
            sources_content: vec![Some("x\ny\n".into())],
            names: vec!["item".into()],
            ignore_list: Vec::new(),
            segments: vec![
                Segment {
                    line: 0,
                    col: 4,
                    orig: Some((0, 1, 2)),
                    name: None,
                },
                Segment {
                    line: 2,
                    col: 100,
                    orig: Some((0, 0, 0)),
                    name: Some(0),
                },
                Segment {
                    line: 2,
                    col: 7000,
                    orig: Some((0, 40, 3)),
                    name: None,
                },
            ],
        };
        let json = to_json(&map, Some("x.js"));
        let theirs = sourcemap::SourceMap::from_slice(json.as_bytes()).expect("valid map");
        let t = theirs.lookup_token(2, 100).expect("token");
        assert_eq!((t.get_src_line(), t.get_src_col()), (0, 0));
        assert_eq!(t.get_name(), Some("item"));
        let t = theirs.lookup_token(2, 7001).expect("token");
        assert_eq!((t.get_src_line(), t.get_src_col()), (40, 3));
        assert_eq!(SourceMap::parse(&json).expect("ours"), map);
    }

    #[test]
    fn base64_matches_rfc_4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn compose_chains_two_maps_and_keeps_k_names() {
        let inner = SourceMap {
            sources: vec!["/_k/src/a.k".into()],
            sources_content: vec![None],
            names: Vec::new(),
            ignore_list: Vec::new(),
            segments: vec![Segment {
                line: 3,
                col: 2,
                orig: Some((0, 9, 4)),
                name: None,
            }],
        };
        let outer = SourceMap {
            sources: vec!["app.js".into()],
            sources_content: vec![None],
            names: vec!["item$1".into(), "$c".into()],
            ignore_list: Vec::new(),
            segments: vec![
                Segment {
                    line: 0,
                    col: 10,
                    orig: Some((0, 3, 8)),
                    name: Some(0),
                },
                Segment {
                    line: 0,
                    col: 12,
                    orig: Some((0, 3, 12)),
                    name: Some(1),
                },
                Segment {
                    line: 0,
                    col: 20,
                    orig: Some((0, 1, 0)),
                    name: None,
                },
            ],
        };
        let m = compose(&outer, &inner);
        assert_eq!(m.segments.len(), 2, "{m:?}");
        assert_eq!(m.segments[0].orig, Some((0, 9, 4)));
        assert_eq!(m.names, vec!["item".to_owned()]);
        assert_eq!(m.segments[0].name, Some(0));
        assert_eq!(m.segments[1].name, None, "generated names stay unnamed");
    }

    #[test]
    fn k_names_undo_the_generator_s_spellings() {
        use crate::js::k_name;
        assert_eq!(k_name("item$1"), Some("item"));
        assert_eq!(k_name("class$"), Some("class"));
        assert_eq!(k_name("class$2"), Some("class"));
        assert_eq!(k_name("item"), None);
        assert_eq!(k_name("$c"), None);
        assert_eq!(k_name("$root3"), None);
    }
}
