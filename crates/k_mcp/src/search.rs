//! `search_docs`: term-based search over the corpus.
//!
//! The query is split into terms (case-insensitive). Each document is cut
//! into **sections**: a Markdown document at its headings, any other file
//! into blocks of [`BLOCK_LINES`] lines. A section matches when it contains
//! every term; when no section contains every term, sections containing
//! the most terms match instead, so a query with a misspelled word still
//! finds something. A match scores, per term, `1 + ln(occurrences)`,
//! tripled for a term in the section's heading and doubled for a term in
//! the document's path or title; the book's pages score a little above the
//! design and compiler docs, which say the same things for implementers.
//! Each result names the best line of its section: the one holding the
//! most terms.

use crate::corpus::Doc;

/// The lines of a non-Markdown section.
const BLOCK_LINES: usize = 40;
/// The longest snippet, in characters.
const SNIPPET: usize = 240;

/// One search result.
#[derive(Clone, Debug)]
pub struct Hit {
    /// The document's corpus path.
    pub path: String,
    /// The 1-based line of the snippet.
    pub line: usize,
    /// The heading of the section, if any.
    pub heading: Option<String>,
    /// The best line of the section, trimmed.
    pub snippet: String,
    /// The score; higher is better.
    pub score: f64,
}

/// A run of lines of one document.
struct Section<'a> {
    /// The heading (without `#`s), if the section starts at one.
    heading: Option<&'a str>,
    /// The 1-based line of its first line.
    first: usize,
    /// Its lines.
    lines: Vec<&'a str>,
}

/// The sections of a document.
fn sections(doc: &Doc) -> Vec<Section<'_>> {
    let mut out = Vec::new();
    let markdown = doc.path.ends_with(".md");
    let mut current = Section {
        heading: None,
        first: 1,
        lines: Vec::new(),
    };
    let mut fence = false;
    for (i, line) in doc.text.lines().enumerate() {
        if markdown && line.trim_start().starts_with("```") {
            fence = !fence;
        }
        let starts = if markdown {
            !fence && line.starts_with('#') && line.trim_start_matches('#').starts_with(' ')
        } else {
            i > 0 && i % BLOCK_LINES == 0
        };
        if starts {
            let next = Section {
                heading: markdown.then(|| line.trim_start_matches('#').trim()),
                first: i + 1,
                lines: Vec::new(),
            };
            out.push(std::mem::replace(&mut current, next));
        }
        current.lines.push(line);
    }
    out.push(current);
    out.retain(|s| !s.lines.is_empty());
    out
}

/// The terms of a query: its words, lower-cased, without surrounding
/// punctuation (so `` `@Load`, `` is `@load`).
#[must_use]
pub fn terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = query
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !(c.is_alphanumeric() || "@_.-#".contains(c)))
                .trim_end_matches(['.', '-'])
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();
    terms.dedup();
    terms
}

/// Searches `docs` for `query`, best first, at most `limit` results.
pub fn search<'a>(docs: impl Iterator<Item = &'a Doc>, query: &str, limit: usize) -> Vec<Hit> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    // (terms matched, hit); only the best `matched` count is kept.
    let mut hits: Vec<(usize, Hit)> = Vec::new();
    for doc in docs {
        let path = doc.path.to_lowercase();
        let title = doc.title().to_lowercase();
        let weight = match doc.section() {
            "book" => 1.2,
            "project" => 1.3,
            _ => 1.0,
        };
        for section in sections(doc) {
            let heading = section.heading.map(str::to_lowercase);
            let lower: Vec<String> = section.lines.iter().map(|l| l.to_lowercase()).collect();
            let mut matched = 0;
            let mut score = 0.0;
            for term in &terms {
                let count: usize = lower.iter().map(|l| l.matches(term.as_str()).count()).sum();
                if count == 0 {
                    continue;
                }
                matched += 1;
                #[allow(clippy::cast_precision_loss)]
                let mut s = 1.0 + (count as f64).ln();
                if heading
                    .as_deref()
                    .is_some_and(|h| h.contains(term.as_str()))
                {
                    s *= 3.0;
                }
                if path.contains(term.as_str()) || title.contains(term.as_str()) {
                    s *= 2.0;
                }
                score += s;
            }
            if matched == 0 {
                continue;
            }
            let (best, _) = lower
                .iter()
                .enumerate()
                .map(|(i, l)| (i, terms.iter().filter(|t| l.contains(t.as_str())).count()))
                .fold((0, 0), |acc, (i, n)| if n > acc.1 { (i, n) } else { acc });
            let snippet = snippet(section.lines[best]);
            hits.push((
                matched,
                Hit {
                    path: doc.path.clone(),
                    line: section.first + best,
                    heading: section.heading.map(str::to_owned),
                    snippet,
                    score: score * weight,
                },
            ));
        }
    }
    let most = hits.iter().map(|(m, _)| *m).max().unwrap_or(0);
    let mut hits: Vec<Hit> = hits
        .into_iter()
        .filter(|(m, _)| *m == most)
        .map(|(_, h)| h)
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });
    hits.truncate(limit);
    hits
}

/// A line, trimmed and cut to [`SNIPPET`] characters.
fn snippet(line: &str) -> String {
    let line = line.trim();
    if line.chars().count() <= SNIPPET {
        return line.to_owned();
    }
    let cut: String = line.chars().take(SNIPPET).collect();
    format!("{cut}…")
}
