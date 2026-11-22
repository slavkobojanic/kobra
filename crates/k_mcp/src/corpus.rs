//! The documents the server answers from: the corpus embedded at build
//! time (`build.rs`) and, when the server runs in a project, that
//! project's own files, read from disk on every request.
//!
//! Every document has a **corpus path**, its name in tools and the tail of
//! its resource URI (`kobra://<path>`):
//!
//! | Path | Source |
//! |---|---|
//! | `book/…` | `docs/book/src/…`, the user-facing book |
//! | `design/…` | `docs/design/…`, the language design (the source of truth) |
//! | `compiler/…` | `docs/compiler/…`, the toolchain's internals |
//! | `README.md` | the repository README (section `book`) |
//! | `examples/…` | the example apps: Markdown, `k.toml` and `.k` sources |
//! | `project/…` | the project the server runs in (from disk) |

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/corpus_index.rs"));

/// The compressed corpus (`build.rs`).
static COMPRESSED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/corpus.deflate"));

/// The URI scheme of the server's resources.
pub const SCHEME: &str = "kobra://";

/// The sections a document belongs to, in listing order.
pub const SECTIONS: &[&str] = &["book", "design", "compiler", "examples", "project"];

/// The most project files read, and the largest one, so a server started
/// in a huge directory stays responsive.
const PROJECT_FILE_LIMIT: usize = 5000;
/// The largest project file read, in bytes.
const PROJECT_FILE_SIZE: u64 = 2 << 20;

/// Directories never searched in a project.
const SKIPPED_DIRS: &[&str] = &["dist", "target", "node_modules"];

/// One document.
#[derive(Clone, Debug)]
pub struct Doc {
    /// The corpus path, such as `book/language/types.md`.
    pub path: String,
    /// The text.
    pub text: Cow<'static, str>,
}

impl Doc {
    /// The section: the first segment of the path (`README.md` is in
    /// `book`).
    #[must_use]
    pub fn section(&self) -> &str {
        section_of(&self.path)
    }

    /// The title: the first Markdown heading, else the file name.
    #[must_use]
    pub fn title(&self) -> String {
        if self.path.ends_with(".md") {
            for line in self.text.lines() {
                if let Some(title) = line.strip_prefix("# ") {
                    return title.trim().to_owned();
                }
            }
        }
        self.path
            .rsplit('/')
            .next()
            .unwrap_or(&self.path)
            .to_owned()
    }

    /// The resource URI.
    #[must_use]
    pub fn uri(&self) -> String {
        format!("{SCHEME}{}", self.path)
    }

    /// The MIME type of the resource.
    #[must_use]
    pub fn mime_type(&self) -> &'static str {
        if self.path.ends_with(".md") {
            "text/markdown"
        } else if self.path.ends_with(".toml") {
            "application/toml"
        } else {
            // Kobra sources.
            "text/plain"
        }
    }
}

/// The section of a corpus path.
#[must_use]
pub fn section_of(path: &str) -> &str {
    match path.split_once('/') {
        Some((section, _)) => section,
        None => "book",
    }
}

/// The decompressed embedded corpus.
fn blob() -> &'static str {
    static BLOB: OnceLock<String> = OnceLock::new();
    BLOB.get_or_init(|| {
        let mut text = String::new();
        // The blob is written by build.rs from UTF-8 files; a failure here
        // would be a build bug, and leaves the corpus empty.
        if flate2::read::DeflateDecoder::new(COMPRESSED)
            .read_to_string(&mut text)
            .is_err()
        {
            eprintln!("k mcp: the embedded documentation is corrupt");
            text.clear();
        }
        text
    })
}

/// Every embedded document, sorted by path.
#[must_use]
pub fn embedded() -> &'static [Doc] {
    static DOCS: OnceLock<Vec<Doc>> = OnceLock::new();
    DOCS.get_or_init(|| {
        let blob = blob();
        INDEX
            .iter()
            .filter_map(|&(path, start, end)| {
                Some(Doc {
                    path: path.to_owned(),
                    text: Cow::Borrowed(blob.get(start..end)?),
                })
            })
            .collect()
    })
}

/// The documents the server answers from: the embedded ones, then the
/// project's.
pub struct Corpus {
    /// The project's documents (`project/…`), read when the corpus was made.
    project: Vec<Doc>,
}

impl Corpus {
    /// The embedded corpus plus the files of the project at `root`.
    #[must_use]
    pub fn load(root: Option<&Path>) -> Corpus {
        Corpus {
            project: root.map(project_docs).unwrap_or_default(),
        }
    }

    /// Every document, embedded first.
    pub fn docs(&self) -> impl Iterator<Item = &Doc> {
        embedded().iter().chain(&self.project)
    }

    /// The document at `path`, which may also be a `kobra://` URI.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Doc> {
        let path = normalize(path);
        self.docs().find(|d| d.path == path)
    }

    /// Close matches for a path that names no document, for an error
    /// message: documents whose path ends with the requested file name.
    #[must_use]
    pub fn suggestions(&self, path: &str) -> Vec<&str> {
        let path = normalize(path);
        let name = path.rsplit('/').next().unwrap_or(&path);
        if name.is_empty() {
            return Vec::new();
        }
        self.docs()
            .filter(|d| d.path.rsplit('/').next() == Some(name))
            .map(|d| d.path.as_str())
            .take(5)
            .collect()
    }
}

/// A path without the `kobra://` scheme or a leading `/`.
#[must_use]
pub fn normalize(path: &str) -> String {
    let path = path.trim();
    let path = path.strip_prefix(SCHEME).unwrap_or(path);
    path.trim_start_matches('/').to_owned()
}

/// The project's Markdown, `k.toml` and `.k` files, as `project/…`.
fn project_docs(root: &Path) -> Vec<Doc> {
    let mut out = Vec::new();
    project_walk(root, root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Adds the kept files below `dir` to `out`.
fn project_walk(root: &Path, dir: &Path, out: &mut Vec<Doc>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if out.len() >= PROJECT_FILE_LIMIT {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                project_walk(root, &path, out);
            }
            continue;
        }
        let kept = name.ends_with(".md") || name == "k.toml" || name.ends_with(".k");
        if !kind.is_file() || !kept {
            continue;
        }
        if entry.metadata().is_ok_and(|m| m.len() > PROJECT_FILE_SIZE) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if let Ok(text) = fs::read_to_string(&path) {
            out.push(Doc {
                path: format!("project/{}", relative.join("/")),
                text: Cow::Owned(text),
            });
        }
    }
}

/// The project the server serves when `--root` isn't given: the nearest
/// directory with a `k.toml` at or above `start`.
#[must_use]
pub fn find_project(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join("k.toml").is_file())
        .map(Path::to_path_buf)
}

/// One example app.
#[derive(Clone, Debug)]
pub struct Example {
    /// Its name: the directory below `examples/` (`todos`,
    /// `learn/shelf/01-create`).
    pub name: String,
    /// Its title: `k.toml`'s `title`, else its README's heading, else the
    /// name.
    pub title: String,
    /// One paragraph saying what it is: the README's first paragraph, else
    /// `k.toml`'s `description`.
    pub summary: String,
    /// Its files, relative to the example's directory.
    pub files: Vec<String>,
}

/// The example apps: each directory below `examples/` with a `k.toml`
/// that isn't inside another one (a workspace's members belong to it).
#[must_use]
pub fn examples() -> &'static [Example] {
    static EXAMPLES: OnceLock<Vec<Example>> = OnceLock::new();
    EXAMPLES.get_or_init(|| {
        let docs = embedded();
        let manifests: BTreeSet<&str> = docs
            .iter()
            .filter_map(|d| d.path.strip_suffix("/k.toml"))
            .filter(|dir| dir.starts_with("examples/"))
            .collect();
        let roots: Vec<&str> = manifests
            .iter()
            .copied()
            .filter(|dir| {
                !manifests
                    .iter()
                    .any(|other| other != dir && dir.starts_with(&format!("{other}/")))
            })
            .collect();
        roots
            .into_iter()
            .map(|dir| {
                let prefix = format!("{dir}/");
                let files: Vec<String> = docs
                    .iter()
                    .filter_map(|d| d.path.strip_prefix(&prefix))
                    .map(str::to_owned)
                    .collect();
                let text = |file: &str| {
                    docs.iter()
                        .find(|d| d.path == format!("{prefix}{file}"))
                        .map(|d| d.text.as_ref())
                };
                let manifest = text("k.toml").unwrap_or("");
                let readme = text("README.md");
                let name = dir.strip_prefix("examples/").unwrap_or(dir).to_owned();
                let title = toml_string(manifest, "title")
                    .or_else(|| readme.and_then(heading))
                    .unwrap_or_else(|| name.clone());
                let summary = readme
                    .and_then(first_paragraph)
                    .or_else(|| toml_string(manifest, "description"))
                    .unwrap_or_default();
                Example {
                    name,
                    title,
                    summary,
                    files,
                }
            })
            .collect()
    })
}

/// The value of `key = "…"` in a manifest's `[package]` (good enough for
/// the generated manifests of the examples).
fn toml_string(manifest: &str, key: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim();
        Some(v.strip_prefix('"')?.strip_suffix('"')?.to_owned())
    })
}

/// The first `# ` heading of a Markdown text.
fn heading(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_owned())
}

/// The first paragraph of prose of a Markdown text, on one line.
#[must_use]
pub fn first_paragraph(text: &str) -> Option<String> {
    let mut paragraph = Vec::new();
    let mut fence = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fence = !fence;
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if fence {
            continue;
        }
        if trimmed.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if paragraph.is_empty()
            && (trimmed.starts_with('#')
                || trimmed.starts_with('<')
                || trimmed.starts_with('|')
                || trimmed.starts_with("[!"))
        {
            continue;
        }
        paragraph.push(trimmed);
    }
    (!paragraph.is_empty()).then(|| paragraph.join(" "))
}

/// Lines `offset..offset + limit` (1-based) of `text`, with a header naming
/// the range, and how to read on when there is more.
#[must_use]
pub fn page(name: &str, text: &str, offset: usize, limit: usize, tool: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let start = offset.max(1);
    if total == 0 {
        return format!("{name} (empty)\n");
    }
    if start > total {
        return format!("{name} has {total} lines; offset {start} is past the end\n");
    }
    let end = (start - 1 + limit.max(1)).min(total);
    let mut out = if start == 1 && end == total {
        format!("{name} ({total} lines)\n\n")
    } else {
        format!("{name} (lines {start}–{end} of {total})\n\n")
    };
    for line in &lines[start - 1..end] {
        out.push_str(line);
        out.push('\n');
    }
    if end < total {
        out.push_str(&format!(
            "\n[… {} more lines: call {tool} with offset {}]\n",
            total - end,
            end + 1
        ));
    }
    out
}
