//! Where the full client runtime's gzip bytes go (docs/compiler/performance.md
//! §11.8, "Measuring the runtime"). Not a budget check: an `#[ignore]`d report
//! for whoever needs room in [`FULL_SIZE_BUDGET`]. Run it with
//!
//! ```text
//! cargo test -p k_runtime --test it -- size_report:: --ignored --nocapture
//! K_SIZE_PARTS=Stdlib cargo test -p k_runtime --test it -- size_report:: --ignored --nocapture
//! ```
//!
//! For each file and each top-level declaration it prints the *marginal*
//! cost: the gzip size of the whole measured text minus the gzip size
//! without that piece. Marginal costs don't add up to the total (gzip shares
//! repeated text between pieces), but they are what removing or moving a
//! piece would save, which is the question being asked.

use std::io::Write;

use k_runtime::{FULL_SIZE_BUDGET, Part, compact, modules, strip_links};

/// The size of `s` gzipped at the best compression level, as the budget
/// tests measure it.
fn gzip_len(s: &str) -> usize {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(s.as_bytes()).expect("gzip");
    encoder.finish().expect("gzip").len()
}

/// One top-level statement group of a runtime file: a declaration with the
/// comments above it, compacted.
struct Piece {
    file: &'static str,
    name: String,
    text: String,
}

/// The name a line at column 0 declares, if it starts a top-level
/// declaration (`export async function x(`, `const x =`, `class X`).
fn declared(line: &str) -> Option<String> {
    let rest = line.strip_prefix("export ").unwrap_or(line);
    let rest = rest.strip_prefix("async ").unwrap_or(rest);
    let rest = [
        "function ",
        "function* ",
        "class ",
        "const ",
        "let ",
        "var ",
    ]
    .iter()
    .find_map(|k| rest.strip_prefix(k))?;
    let name: String = rest
        .trim_start_matches('*')
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Splits a file (links already stripped) at its top-level declarations;
/// each piece runs from the end of the previous declaration to the end of
/// its own, so the comment above a declaration belongs to it.
fn pieces(file: &'static str, src: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut name = String::from("(file head)");
    let mut buf = String::new();
    for line in src.lines() {
        if let Some(next) = declared(line) {
            let head = std::mem::take(&mut buf);
            // The comment block above `next` moves with it.
            let (before, comment) = match head.rfind("\n/**") {
                Some(i) => head.split_at(i + 1),
                None if head.starts_with("/**") => ("", head.as_str()),
                None => (head.as_str(), ""),
            };
            if !before.trim().is_empty() {
                out.push(Piece {
                    file,
                    name: std::mem::replace(&mut name, next.clone()),
                    text: compact(before).expect("compacts"),
                });
            } else {
                name = next.clone();
            }
            buf.push_str(comment);
        }
        buf.push_str(line);
        buf.push('\n');
    }
    if !buf.trim().is_empty() {
        out.push(Piece {
            file,
            name,
            text: compact(&buf).expect("compacts"),
        });
    }
    out
}

/// The parts to measure: `K_SIZE_PARTS` (comma-separated `Part` names as
/// `Debug` prints them), or the full client runtime.
fn measured_parts() -> Vec<Part> {
    let all = [
        Part::Core,
        Part::Extended,
        Part::ClientEntry,
        Part::Stdlib,
        Part::Services,
        Part::Forms,
        Part::Load,
        Part::Money,
        Part::Refs,
        Part::Channels,
    ];
    match std::env::var("K_SIZE_PARTS") {
        Ok(v) => v
            .split(',')
            .map(|name| {
                *all.iter()
                    .find(|p| format!("{p:?}") == name.trim())
                    .expect("K_SIZE_PARTS names parts as `Part`'s Debug prints them")
            })
            .collect(),
        Err(_) => vec![Part::Core, Part::Extended, Part::ClientEntry],
    }
}

#[test]
#[ignore = "a report, not a check: run with --ignored --nocapture"]
fn where_the_runtime_bytes_go() {
    let parts = measured_parts();
    let files: Vec<_> = modules()
        .iter()
        .filter(|m| parts.contains(&m.part))
        .collect();
    let all: Vec<Piece> = files
        .iter()
        .flat_map(|m| pieces(m.path, &strip_links(m.source)))
        .collect();
    let joined = |skip: &dyn Fn(&Piece) -> bool| -> String {
        all.iter()
            .filter(|p| !skip(p))
            .map(|p| p.text.as_str())
            .collect()
    };
    let total = gzip_len(&joined(&|_| false));
    println!("parts {parts:?}: {total} B gzip (full budget {FULL_SIZE_BUDGET} B)\n");
    println!("{:>7} {:>7} {:>8}  file", "margin", "alone", "compact");
    for m in &files {
        let without = gzip_len(&joined(&|p| p.file == m.path));
        let own: String = all
            .iter()
            .filter(|p| p.file == m.path)
            .map(|p| p.text.as_str())
            .collect();
        println!(
            "{:>7} {:>7} {:>8}  {}",
            total - without,
            gzip_len(&own),
            own.len(),
            m.path
        );
    }
    let mut rows: Vec<(usize, usize, &Piece)> = all
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let without: String = all
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, q)| q.text.as_str())
                .collect();
            (total.saturating_sub(gzip_len(&without)), p.text.len(), p)
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let limit = std::env::var("K_SIZE_TOP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    println!("\n{:>7} {:>8}  declaration", "margin", "compact");
    for (margin, len, p) in rows.iter().take(limit) {
        println!("{margin:>7} {len:>8}  {} {}", p.file, p.name);
    }
}
