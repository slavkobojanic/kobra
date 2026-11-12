//! Applying machine-applicable fixes: what `k fix` does, and how the
//! corpus's `.fixed` expectations are made
//! (`docs/design/48-releases-and-versioning.md`, "Upgrading a project").
//!
//! [`select`] picks, from a list of diagnostics, the first
//! [`Applicability::MachineApplicable`] fix of each one, leaving out a fix
//! whose edits overlap a fix already picked (it is picked in a later round,
//! once the text it was computed for has settled) and counting a fix that
//! another diagnostic already asked for once. [`apply`] applies edits to a
//! text. [`fix_until_stable`] runs the whole loop over a set of named
//! files: check, pick, apply, check again, until no fix is left or a round
//! limit is reached, so a pair of fixes that undo each other can't loop.
//!
//! ```
//! use k_diagnostics::fix::{self, Rounds};
//! use k_diagnostics::{codes, Diagnostic, Fix, SourceMap, Span, TextRange};
//! use std::collections::BTreeMap;
//!
//! let mut files = BTreeMap::from([("App.k".to_owned(), "if a && b {}".to_owned())]);
//! let outcome = fix::fix_until_stable(&mut files, Rounds::DEFAULT, |_| true, |files| {
//!     let mut map = SourceMap::new();
//!     let text = &files["App.k"];
//!     let file = map.add("App.k", text.clone());
//!     let diagnostics = match text.find("&&") {
//!         Some(at) => {
//!             let at = u32::try_from(at).unwrap();
//!             let span = Span::new(file, TextRange::new(at.into(), (at + 2).into()));
//!             vec![Diagnostic::new(codes::SYMBOLIC_AND, span, "K uses `and`, not `&&`")
//!                 .with_fix(Fix::replace("Replace `&&` with `and`", span, "and"))]
//!         }
//!         None => Vec::new(),
//!     };
//!     (map, diagnostics)
//! });
//! assert_eq!(files["App.k"], "if a and b {}");
//! assert_eq!(outcome.applied, 1);
//! assert!(outcome.converged);
//! ```

use std::collections::{BTreeMap, BTreeSet};

use text_size::{TextRange, TextSize};

use crate::{Applicability, Diagnostic, FileId, Fix, SourceMap, TextEdit};

/// How many check-and-fix rounds [`fix_until_stable`] runs at most.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rounds(pub usize);

impl Rounds {
    /// The default limit: enough for fixes that enable other fixes (an
    /// added import whose name another fix then uses), few enough that two
    /// fixes undoing each other stop quickly.
    pub const DEFAULT: Rounds = Rounds(8);
}

/// What [`fix_until_stable`] did.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// How many rounds applied at least one fix.
    pub rounds: usize,
    /// How many fixes were applied in all.
    pub applied: usize,
    /// The files whose text changed, by name.
    pub changed: BTreeSet<String>,
    /// `false` when the round limit stopped the loop while fixes were still
    /// left to apply (two fixes that keep undoing each other).
    pub converged: bool,
    /// The source map and diagnostics of the last check, after every
    /// applied fix.
    pub last: (SourceMap, Vec<Diagnostic>),
}

/// The fix of `diagnostic` that `k fix` applies: its first
/// machine-applicable one.
#[must_use]
pub fn applicable(diagnostic: &Diagnostic) -> Option<&Fix> {
    diagnostic
        .fixes
        .iter()
        .find(|f| f.applicability == Applicability::MachineApplicable && !f.edits.is_empty())
}

/// `true` when two edits of one file can't both be applied: their ranges
/// overlap, or both insert at the same offset (their order would be a
/// guess), or one inserts strictly inside the other's range.
fn conflicts(a: TextRange, b: TextRange) -> bool {
    if a.is_empty() && b.is_empty() {
        return a.start() == b.start();
    }
    if a.is_empty() {
        return b.start() < a.start() && a.start() < b.end();
    }
    if b.is_empty() {
        return a.start() < b.start() && b.start() < a.end();
    }
    a.start() < b.end() && b.start() < a.end()
}

/// Picks the fixes one round applies: for each diagnostic `wanted` accepts,
/// in order, its [`applicable`] fix, unless one of its edits is in a file
/// `writable` rejects, or conflicts with an edit already picked. A fix
/// whose edits were all picked already (two diagnostics asking for the same
/// change) is counted once. Returns the edits by file, sorted, and how many
/// fixes they make up.
#[must_use]
pub fn select(
    diagnostics: &[Diagnostic],
    wanted: impl Fn(&Diagnostic) -> bool,
    writable: impl Fn(FileId) -> bool,
) -> (BTreeMap<FileId, Vec<TextEdit>>, usize) {
    let mut picked: BTreeMap<FileId, Vec<TextEdit>> = BTreeMap::new();
    let mut count = 0;
    for diagnostic in diagnostics.iter().filter(|d| wanted(d)) {
        let Some(fix) = applicable(diagnostic) else {
            continue;
        };
        if !fix.edits.iter().all(|e| writable(e.span.file)) {
            continue;
        }
        let already = |e: &TextEdit| picked.get(&e.span.file).is_some_and(|l| l.contains(e));
        if fix.edits.iter().all(already) {
            continue;
        }
        let clash = fix.edits.iter().enumerate().any(|(i, e)| {
            let earlier = &fix.edits[..i];
            earlier
                .iter()
                .any(|o| o.span.file == e.span.file && conflicts(o.span.range, e.span.range))
                || picked.get(&e.span.file).is_some_and(|list| {
                    list.iter()
                        .any(|o| o != e && conflicts(o.span.range, e.span.range))
                })
        });
        if clash {
            continue;
        }
        for edit in &fix.edits {
            let list = picked.entry(edit.span.file).or_default();
            if !list.contains(edit) {
                list.push(edit.clone());
            }
        }
        count += 1;
    }
    for list in picked.values_mut() {
        list.sort_by_key(|e| (e.span.range.start(), e.span.range.end()));
    }
    (picked, count)
}

/// Applies `edits` (non-conflicting, as [`select`] returns them) to `text`.
/// An edit whose range is outside the text or not on character boundaries
/// is skipped.
#[must_use]
pub fn apply(text: &str, edits: &[TextEdit]) -> String {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.span.range.start(), e.span.range.end()));
    let mut out = String::with_capacity(text.len());
    let mut at = TextSize::from(0);
    for edit in sorted {
        let (start, end) = (edit.span.range.start(), edit.span.range.end());
        let (s, e) = (usize::from(start), usize::from(end));
        if start < at || e > text.len() || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
            continue;
        }
        out.push_str(&text[usize::from(at)..s]);
        out.push_str(&edit.replacement);
        at = end;
    }
    out.push_str(&text[usize::from(at).min(text.len())..]);
    out
}

/// Checks `files` with `check`, applies the [`select`]ed fixes of the
/// diagnostics `wanted` accepts to the files they point into, and checks
/// again, until a round has nothing to apply or `rounds` rounds have
/// applied fixes. `check` returns a source map whose file names are the
/// keys of `files` (a diagnostic in any other file, such as the standard
/// library, is never fixed) and the diagnostics in it.
pub fn fix_until_stable(
    files: &mut BTreeMap<String, String>,
    rounds: Rounds,
    wanted: impl Fn(&Diagnostic) -> bool,
    mut check: impl FnMut(&BTreeMap<String, String>) -> (SourceMap, Vec<Diagnostic>),
) -> Outcome {
    let mut outcome = Outcome {
        converged: true,
        ..Outcome::default()
    };
    loop {
        let (map, diagnostics) = check(files);
        let known: BTreeSet<String> = files.keys().cloned().collect();
        let name = |file: FileId| {
            map.get(file)
                .map(|f| f.path().to_owned())
                .filter(|p| known.contains(p))
        };
        let (edits, count) = select(&diagnostics, &wanted, |f| name(f).is_some());
        if count == 0 {
            outcome.last = (map, diagnostics);
            return outcome;
        }
        if outcome.rounds == rounds.0 {
            outcome.converged = false;
            outcome.last = (map, diagnostics);
            return outcome;
        }
        for (file, list) in &edits {
            let Some(path) = name(*file) else { continue };
            let Some(text) = files.get_mut(&path) else {
                continue;
            };
            let fixed = apply(text, list);
            if fixed != *text {
                *text = fixed;
                outcome.changed.insert(path);
            }
        }
        outcome.rounds += 1;
        outcome.applied += count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Span, codes};

    fn span(file: FileId, start: u32, end: u32) -> Span {
        Span::new(file, TextRange::new(start.into(), end.into()))
    }

    fn diagnostic(fix: Fix) -> Diagnostic {
        let at = fix.edits[0].span;
        Diagnostic::new(codes::SYMBOLIC_AND, at, "test").with_fix(fix)
    }

    fn edit(span: Span, text: &str) -> TextEdit {
        TextEdit {
            span,
            replacement: text.to_owned(),
        }
    }

    /// Overlapping fixes wait for the next round; duplicates count once;
    /// maybe-incorrect fixes are never picked.
    #[test]
    fn select_skips_conflicts_duplicates_and_maybe_incorrect() {
        let f = FileId::new(0);
        let a = diagnostic(Fix::replace("a", span(f, 0, 4), "x"));
        let overlapping = diagnostic(Fix::replace("b", span(f, 2, 6), "y"));
        let duplicate = diagnostic(Fix::replace("a", span(f, 0, 4), "x"));
        let maybe = diagnostic(Fix::replace("c", span(f, 8, 9), "z").maybe_incorrect());
        let later = diagnostic(Fix::replace("d", span(f, 10, 11), "w"));
        let (edits, count) = select(
            &[a, overlapping, duplicate, maybe, later],
            |_| true,
            |_| true,
        );
        assert_eq!(count, 2);
        assert_eq!(
            edits[&f],
            vec![edit(span(f, 0, 4), "x"), edit(span(f, 10, 11), "w")]
        );
    }

    /// Two insertions at one offset conflict; an insertion at the edge of a
    /// replacement doesn't.
    #[test]
    fn insertions_conflict_only_at_the_same_point() {
        let r = |a: u32, b: u32| TextRange::new(a.into(), b.into());
        assert!(conflicts(r(3, 3), r(3, 3)));
        assert!(!conflicts(r(3, 3), r(3, 5)));
        assert!(!conflicts(r(5, 5), r(3, 5)));
        assert!(conflicts(r(4, 4), r(3, 5)));
        assert!(!conflicts(r(1, 3), r(3, 5)));
    }

    #[test]
    fn apply_replaces_inserts_and_deletes() {
        let f = FileId::new(0);
        let text = "let a = 1\n";
        let out = apply(
            text,
            &[
                edit(span(f, 0, 3), "var"),
                edit(span(f, 9, 9), ";"),
                edit(span(f, 5, 6), ""),
            ],
        );
        assert_eq!(out, "var a= 1;\n");
    }

    /// Two fixes that undo each other stop at the round limit, unconverged.
    #[test]
    fn a_fix_loop_terminates() {
        let mut files = BTreeMap::from([("a.k".to_owned(), "a".to_owned())]);
        let mut checks = 0;
        let outcome = fix_until_stable(
            &mut files,
            Rounds(5),
            |_| true,
            |files| {
                checks += 1;
                let mut map = SourceMap::new();
                let text = files["a.k"].clone();
                let file = map.add("a.k", text.clone());
                let other = if text == "a" { "b" } else { "a" };
                let d = diagnostic(Fix::replace("flip", span(file, 0, 1), other));
                (map, vec![d])
            },
        );
        assert!(!outcome.converged);
        assert_eq!(outcome.rounds, 5);
        assert_eq!(outcome.applied, 5);
        assert_eq!(checks, 6);
        assert_eq!(files["a.k"], "b");
    }

    /// A fix with an edit in a file outside the set (the standard library)
    /// is not applied.
    #[test]
    fn fixes_outside_the_files_are_left_alone() {
        let mut files = BTreeMap::from([("a.k".to_owned(), "abc".to_owned())]);
        let outcome = fix_until_stable(
            &mut files,
            Rounds::DEFAULT,
            |_| true,
            |_| {
                let mut map = SourceMap::new();
                let _ = map.add("a.k", "abc");
                let other = map.add("<stdlib>/k/core/Int.k", "xyz");
                (
                    map,
                    vec![diagnostic(Fix::replace("x", span(other, 0, 1), "q"))],
                )
            },
        );
        assert!(outcome.converged);
        assert_eq!(outcome.applied, 0);
        assert_eq!(files["a.k"], "abc");
    }
}
