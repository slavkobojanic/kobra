//! `k fix`: applies the machine-applicable fixes of the current
//! diagnostics (`docs/design/48-releases-and-versioning.md`, "Upgrading a
//! project").
//!
//! It loads the projects named by its paths as `k check` does
//! ([`sources`]), checks each group without running tests, applies the
//! first machine-applicable fix of every diagnostic (or of the codes named
//! by `--code`) and checks again, until nothing is left to apply or
//! [`fix::Rounds::DEFAULT`] rounds have run ([`fix::fix_until_stable`]).
//! Fixes whose edits overlap wait for the next round, against the updated
//! text. A fix in a workspace library is applied to the library's own
//! files, once, however many members mount it; nothing outside the
//! group's files (the standard library) is ever written.
//!
//! `--dry-run` prints the changes as a unified diff on standard output and
//! writes nothing. Either way the problems that remain after the fixes are
//! printed as `k check` prints them, and `k fix` exits with 1 when any
//! remain, so `k fix && git commit` commits only a clean tree.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use k_diagnostics::fix;
use k_diagnostics::{Code, Diagnostic, SourceMap, render};
use k_sema::ProjectPath;

use crate::check;
use crate::cli::{ColorChoice, FixArgs, Format};
use crate::sources::{self, Group};

/// What fixing one group did.
pub(crate) struct Fixed {
    /// The group's root.
    pub(crate) root: std::path::PathBuf,
    /// The files before fixing, by path relative to the root.
    pub(crate) before: BTreeMap<String, String>,
    /// The files after fixing.
    pub(crate) after: BTreeMap<String, String>,
    /// What the loop did; its `last` check is what remains.
    pub(crate) outcome: fix::Outcome,
}

/// Runs `k fix`.
pub fn run(args: &FixArgs) -> ExitCode {
    let mut codes = Vec::new();
    for text in &args.code {
        match Code::parse(text).filter(|c| c.info().is_some()) {
            Some(code) => codes.push(code),
            None => {
                eprintln!("error: `{text}` is not a diagnostic code (`--code K2012`)");
                return ExitCode::from(2);
            }
        }
    }
    let loaded = sources::load(&args.paths);
    for path in &loaded.missing {
        eprintln!("error: `{path}` does not exist");
    }
    let wanted = |d: &Diagnostic| codes.is_empty() || codes.contains(&d.code);
    let mut results: Vec<Fixed> = Vec::new();
    for group in loaded.groups {
        results.push(fix_group(group, &wanted));
    }

    // Write, or show, the changes.
    let mut out = std::io::stdout().lock();
    for r in &results {
        for path in &r.outcome.changed {
            let full = r.root.join(path);
            if args.dry_run {
                let shown = sources::display(&full);
                let _ = write!(
                    out,
                    "{}",
                    unified_diff(&shown, &r.before[path], &r.after[path])
                );
            } else if let Err(e) = std::fs::write(&full, &r.after[path]) {
                eprintln!("error: cannot write `{}`: {e}", sources::display(&full));
                return ExitCode::from(2);
            }
        }
    }

    // What remains, as `k check` prints it.
    let color = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stderr().is_terminal(),
    };
    let options = if color {
        render::HumanOptions::colored()
    } else {
        render::HumanOptions::plain()
    };
    let mut err = std::io::stderr().lock();
    let mut errors = 0;
    let mut warnings = 0;
    for r in &results {
        let (map, diagnostics) = &r.outcome.last;
        let named = renamed(map, &r.root);
        for d in diagnostics {
            if d.is_error() {
                errors += 1;
            } else {
                warnings += 1;
            }
            match args.format {
                Format::Json => {
                    let _ = writeln!(out, "{}", render::json(d, &named));
                }
                Format::Human => {
                    let _ = writeln!(err, "{}", render::human(d, &named, &options));
                }
            }
        }
        if !r.outcome.converged {
            let _ = writeln!(
                err,
                "warning: stopped after {} rounds in `{}`: some fixes keep undoing each other",
                fix::Rounds::DEFAULT.0,
                sources::display(&r.root)
            );
        }
    }
    let applied: usize = results.iter().map(|r| r.outcome.applied).sum();
    let files: usize = results.iter().map(|r| r.outcome.changed.len()).sum();
    let verb = if args.dry_run { "would fix" } else { "fixed" };
    let fixes = if applied == 1 { "problem" } else { "problems" };
    let noun = if files == 1 { "file" } else { "files" };
    let remaining = if errors + warnings == 0 {
        "no problems remain".to_owned()
    } else {
        format!("{errors} error(s), {warnings} warning(s) remain")
    };
    let _ = writeln!(
        err,
        "{verb} {applied} {fixes} in {files} {noun}: {remaining}"
    );
    let converged = results.iter().all(|r| r.outcome.converged);
    if errors + warnings == 0 && converged && loaded.missing.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Fixes one group in memory: its files keyed by their path relative to
/// the root, checked again after every round.
pub(crate) fn fix_group(group: Group, wanted: &impl Fn(&Diagnostic) -> bool) -> Fixed {
    let root = group.root.clone();
    let scope = group.scope.clone();
    let problems = group.problems;
    let before: BTreeMap<String, String> = group
        .files
        .into_iter()
        .map(|(p, t)| (p.as_str().to_owned(), t))
        .collect();
    let mut after = before.clone();
    let outcome = fix::fix_until_stable(&mut after, fix::Rounds::DEFAULT, wanted, |files| {
        let group = Group {
            root: root.clone(),
            scope: scope.clone(),
            files: files
                .iter()
                .filter_map(|(p, t)| Some((ProjectPath::new(p).ok()?, t.clone())))
                .collect(),
            // Unreadable files are never fixed, only reported.
            problems: problems.clone(),
        };
        let report = check::check_group(group, false, str::to_owned);
        (report.map, report.diagnostics)
    });
    Fixed {
        root,
        before,
        after,
        outcome,
    }
}

/// `map` with its group-relative names shown as `k check` shows them
/// (`app/src/…` from the working directory).
pub(crate) fn renamed(map: &SourceMap, root: &std::path::Path) -> SourceMap {
    let mut out = SourceMap::new();
    for i in 0..map.len() {
        let id = k_diagnostics::FileId::new(u32::try_from(i).unwrap_or(u32::MAX));
        if let Some(file) = map.get(id) {
            let name = if file.path().starts_with('<') || file.path().contains(':') {
                file.path().to_owned()
            } else {
                sources::display(&root.join(file.path()))
            };
            out.add(name, file.text());
        }
    }
    out
}

/// A unified diff of one file, three lines of context around each change.
fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let a: Vec<&str> = before.split_inclusive('\n').collect();
    let b: Vec<&str> = after.split_inclusive('\n').collect();
    let ops = diff_lines(&a, &b);
    let mut out = format!("--- {path}\n+++ {path}\n");
    // Group the operations into hunks.
    let context = 3;
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Same(..)))
        .map(|(i, _)| i)
        .collect();
    let mut i = 0;
    while i < changed.len() {
        let start = changed[i].saturating_sub(context);
        let mut end = changed[i];
        while i < changed.len() && changed[i] <= end + 2 * context {
            end = changed[i];
            i += 1;
        }
        let end = (end + context + 1).min(ops.len());
        let hunk = &ops[start..end];
        let (a_start, b_start) = position(&ops[..start]);
        let a_len = hunk.iter().filter(|o| !matches!(o, Op::Add(_))).count();
        let b_len = hunk.iter().filter(|o| !matches!(o, Op::Remove(_))).count();
        let _ = writeln!(
            out,
            "@@ -{},{a_len} +{},{b_len} @@",
            a_start + usize::from(a_len > 0),
            b_start + usize::from(b_len > 0)
        );
        for op in hunk {
            let (mark, line) = match op {
                Op::Same(l) => (' ', *l),
                Op::Remove(l) => ('-', *l),
                Op::Add(l) => ('+', *l),
            };
            out.push(mark);
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

/// One line of a diff.
enum Op<'a> {
    /// In both texts.
    Same(&'a str),
    /// Only in the old text.
    Remove(&'a str),
    /// Only in the new text.
    Add(&'a str),
}

/// How many old and new lines `ops` cover.
fn position(ops: &[Op<'_>]) -> (usize, usize) {
    let a = ops.iter().filter(|o| !matches!(o, Op::Add(_))).count();
    let b = ops.iter().filter(|o| !matches!(o, Op::Remove(_))).count();
    (a, b)
}

/// The line operations turning `a` into `b`: the common prefix and suffix,
/// and a longest common subsequence of the lines between (all removed,
/// then all added, when that middle is too large to align).
fn diff_lines<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<Op<'a>> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut ops: Vec<Op<'a>> = a[..prefix].iter().map(|l| Op::Same(l)).collect();
    if am.len().saturating_mul(bm.len()) > 4_000_000 {
        ops.extend(am.iter().map(|l| Op::Remove(l)));
        ops.extend(bm.iter().map(|l| Op::Add(l)));
    } else {
        // lcs[i][j]: the common length of am[i..] and bm[j..].
        let mut lcs = vec![vec![0u32; bm.len() + 1]; am.len() + 1];
        for i in (0..am.len()).rev() {
            for j in (0..bm.len()).rev() {
                lcs[i][j] = if am[i] == bm[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < am.len() || j < bm.len() {
            if i < am.len() && j < bm.len() && am[i] == bm[j] {
                ops.push(Op::Same(am[i]));
                i += 1;
                j += 1;
            } else if i < am.len() && (j == bm.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
                // Removals first, as `diff -u` writes them.
                ops.push(Op::Remove(am[i]));
                i += 1;
            } else {
                ops.push(Op::Add(bm[j]));
                j += 1;
            }
        }
    }
    ops.extend(a[a.len() - suffix..].iter().map(|l| Op::Same(l)));
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_shows_each_change_with_context() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";
        let after = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nK\nl\n";
        let diff = unified_diff("x.k", before, after);
        assert_eq!(
            diff,
            "--- x.k\n+++ x.k\n@@ -1,5 +1,5 @@\n a\n-b\n+B\n c\n d\n e\n@@ -8,5 +8,5 @@\n h\n i\n j\n-k\n+K\n l\n"
        );
    }
}
