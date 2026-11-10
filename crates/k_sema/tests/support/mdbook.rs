//! Reading the repository's markdown the way mdBook renders it: fenced
//! blocks, prose paragraphs, inline code spans, links and heading ids.
//!
//! Shared by the book tests (`tests/book.rs`, `tests/book_links.rs`) and by
//! `k_cli`'s `tests/book_commands.rs`, which includes this file by path.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The repository root, from a crate two levels below it.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The book's source directory.
pub fn book_src() -> PathBuf {
    repo().join("docs/book/src")
}

/// Every `.md` file below `dir`, sorted; `skip` names directories (relative
/// to `dir`) that are left out, such as the build output.
pub fn markdown_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, skip: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let relative = path.strip_prefix(root).unwrap_or(&path);
                if !skip.iter().any(|s| Path::new(s) == relative) {
                    walk(root, &path, skip, out);
                }
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, skip, &mut out);
    out.sort();
    out
}

/// A fenced code block.
pub struct Fence {
    /// The 1-based line of the opening fence.
    pub line: usize,
    /// The info string: ` ```k,ignore ` gives `k,ignore`.
    pub info: String,
    /// The block's lines, with the opening fence's indentation removed.
    pub body: Vec<String>,
    /// Whether the block is inside an HTML comment (not rendered).
    pub hidden: bool,
}

impl Fence {
    /// The language: the info string's first comma-separated word.
    pub fn lang(&self) -> &str {
        self.info.split(',').next().unwrap_or("").trim()
    }
}

/// A run of rendered prose: a paragraph, a list item's lines, a heading or
/// one table row, joined with spaces.
pub struct Paragraph {
    /// The 1-based line it starts on.
    pub line: usize,
    /// Its text, lines joined with a space.
    pub text: String,
}

/// A markdown file split into its fenced blocks and its prose.
pub struct Document {
    /// Every fenced block, rendered or inside a comment.
    pub fences: Vec<Fence>,
    /// The rendered prose outside fences and HTML comments.
    pub paragraphs: Vec<Paragraph>,
    /// The ATX headings (`## Title`), as `(line, level, text)`.
    pub headings: Vec<(usize, usize, String)>,
}

/// Splits `text` into fenced blocks and prose paragraphs.
pub fn parse(text: &str) -> Document {
    let mut fences = Vec::new();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut headings = Vec::new();
    let mut in_comment = false;
    let mut open: Option<Paragraph> = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    let flush = |open: &mut Option<Paragraph>, paragraphs: &mut Vec<Paragraph>| {
        if let Some(p) = open.take() {
            paragraphs.push(p);
        }
    };
    while index < lines.len() {
        let line = lines[index];
        let trimmed = fence_start(line);
        if let Some(info) = trimmed.strip_prefix("```") {
            flush(&mut open, &mut paragraphs);
            let ticks = 3 + info.chars().take_while(|&c| c == '`').count();
            let info = info.trim_start_matches('`').trim().to_owned();
            let indent = line.len() - trimmed.len();
            let start = index;
            let mut body = Vec::new();
            index += 1;
            while index < lines.len() {
                let inner = lines[index];
                let t = inner.trim();
                if t.len() >= ticks && t.chars().all(|c| c == '`') {
                    break;
                }
                body.push(
                    inner
                        .get(indent..)
                        .filter(|_| inner[..indent.min(inner.len())].trim().is_empty())
                        .unwrap_or(inner.trim_start())
                        .to_owned(),
                );
                index += 1;
            }
            fences.push(Fence {
                line: start + 1,
                info,
                body,
                hidden: in_comment,
            });
            index += 1;
            continue;
        }
        // HTML comments: their prose is not rendered.
        // Markers inside inline code (`` `<!--` ``) don't count.
        let masked = without_code(line);
        let mut visible = String::new();
        let mut at = 0;
        loop {
            if in_comment {
                match masked[at..].find("-->") {
                    Some(end) => {
                        in_comment = false;
                        at += end + 3;
                    }
                    None => break,
                }
            } else {
                match masked[at..].find("<!--") {
                    Some(start) => {
                        visible.push_str(&line[at..at + start]);
                        in_comment = true;
                        at += start + 4;
                    }
                    None => {
                        visible.push_str(&line[at..]);
                        break;
                    }
                }
            }
        }
        let content = visible.trim();
        if content.is_empty() {
            flush(&mut open, &mut paragraphs);
        } else if content.starts_with('#') && !line.starts_with("    ") {
            flush(&mut open, &mut paragraphs);
            let level = content.chars().take_while(|&c| c == '#').count();
            let title = content[level..].trim();
            if (1..=6).contains(&level) && content[level..].starts_with(' ') {
                headings.push((index + 1, level, title.to_owned()));
            }
            paragraphs.push(Paragraph {
                line: index + 1,
                text: content.to_owned(),
            });
        } else if content.starts_with('|') {
            flush(&mut open, &mut paragraphs);
            paragraphs.push(Paragraph {
                line: index + 1,
                text: content.to_owned(),
            });
        } else {
            let new_item = content.starts_with("- ")
                || content.starts_with("* ")
                || content
                    .split_once(". ")
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if new_item {
                flush(&mut open, &mut paragraphs);
            }
            let content = content.strip_prefix('>').map_or(content, str::trim);
            match &mut open {
                Some(p) => {
                    p.text.push(' ');
                    p.text.push_str(content);
                }
                None => {
                    open = Some(Paragraph {
                        line: index + 1,
                        text: content.to_owned(),
                    });
                }
            }
        }
        index += 1;
    }
    flush(&mut open, &mut paragraphs);
    Document {
        fences,
        paragraphs,
        headings,
    }
}

/// `line` from where a fence could start: indentation, and a list marker
/// (`1. ```sh`), removed.
fn fence_start(line: &str) -> &str {
    let trimmed = line.trim_start();
    for marker in ["- ", "* "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    if let Some((number, rest)) = trimmed.split_once(". ")
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
    {
        return rest.trim_start();
    }
    trimmed
}

/// The inline code spans of `text`, in order: `` `k check` `` gives
/// `k check`. A span is closed by a backtick run of the same length.
pub fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let ticks = i - start;
        let mut j = i;
        let mut closed = None;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let run = j;
                while j < bytes.len() && bytes[j] == b'`' {
                    j += 1;
                }
                if j - run == ticks {
                    closed = Some(run);
                    break;
                }
            } else {
                j += 1;
            }
        }
        match closed {
            Some(end) => {
                let inner = &text[i..end];
                let inner = if inner.starts_with(' ') && inner.ends_with(' ') && inner.len() > 1 {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                spans.push(inner.to_owned());
                i = end + ticks;
            }
            None => i = start + ticks,
        }
    }
    spans
}

/// `text` with every inline code span replaced by spaces (byte for byte,
/// so offsets stay valid), so that links, comments and patterns inside code
/// aren't seen as prose.
pub fn without_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            let next = text[i..].find('`').map_or(text.len(), |n| i + n);
            out.push_str(&text[i..next]);
            i = next;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let ticks = i - start;
        let mut j = i;
        let mut closed = None;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let run = j;
                while j < bytes.len() && bytes[j] == b'`' {
                    j += 1;
                }
                if j - run == ticks {
                    closed = Some(j);
                    break;
                }
            } else {
                j += 1;
            }
        }
        match closed {
            Some(end) => {
                out.push_str(&" ".repeat(end - start));
                i = end;
            }
            None => out.push_str(&text[start..i]),
        }
    }
    out
}

/// The link targets of a prose paragraph: `[text](target)` and
/// `![alt](target)`, outside inline code.
pub fn links(text: &str) -> Vec<String> {
    let prose = without_code(text);
    let mut found = Vec::new();
    let mut rest = prose.as_str();
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        let mut depth = 1;
        let mut end = None;
        for (i, c) in after.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            break;
        };
        let target = after[..end].split_whitespace().next().unwrap_or("");
        let target = target.trim_start_matches('<').trim_end_matches('>');
        if !target.is_empty() {
            found.push(target.to_owned());
        }
        rest = &after[end..];
    }
    found
}

/// The id mdBook gives a heading: inline markup removed, then lowercase,
/// letters, digits, `-` and `_` kept, whitespace to `-`, everything else
/// dropped. GitHub computes the same ids for the design and compiler docs.
pub fn heading_id(title: &str) -> String {
    // `[text](url)` renders as its text.
    let mut plain = String::new();
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](") else {
            break;
        };
        let Some(end) = rest[open + close..].find(')') else {
            break;
        };
        plain.push_str(&rest[..open]);
        plain.push_str(&rest[open + 1..open + close]);
        rest = &rest[open + close + end + 1..];
    }
    plain.push_str(rest);
    plain
        .replace(['`', '*'], "")
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// The heading ids of a document, with `-1`, `-2` … appended to repeats,
/// as mdBook and GitHub do.
pub fn heading_ids(document: &Document) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut ids = Vec::new();
    for (_, _, title) in &document.headings {
        let id = heading_id(title);
        let count = seen.entry(id.clone()).or_insert(0);
        if *count == 0 {
            ids.push(id);
        } else {
            ids.push(format!("{id}-{count}"));
        }
        *count += 1;
    }
    ids
}

/// The book's version substitution: `{{k_version}}` becomes the toolchain's
/// minor version (`0.1`), as the site build substitutes it.
pub fn substitute_version(text: &str) -> String {
    text.replace("{{k_version}}", &k_minor_version())
}

/// The workspace version from the root `Cargo.toml` (`0.1.0`).
pub fn k_version() -> String {
    let manifest = fs::read_to_string(repo().join("Cargo.toml")).unwrap_or_default();
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(value) = line.strip_prefix("version") {
            let value = value.trim_start().trim_start_matches('=').trim();
            return value.trim_matches('"').to_owned();
        }
    }
    String::new()
}

/// The toolchain's minor version (`0.1` for `0.1.3`): what a project's
/// `k = "…"` requirement and the docs' version directory name.
pub fn k_minor_version() -> String {
    let version = k_version();
    let mut parts = version.split('.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        _ => version,
    }
}
