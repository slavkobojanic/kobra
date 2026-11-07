//! The semantic oracle: invariants of `k_sema` checked over whole projects.
//!
//! Every property test in `crates/k_sema/tests/properties.rs` and every
//! `check*` fuzz target in `fuzz/` goes through this module (the fuzz crate
//! includes it with `#[path]`), so a property is checked the same way on
//! stable and under libFuzzer.
//!
//! A project is a slice of `(project path, text)` pairs. Diagnostics are
//! compared in [`Norm`]al form, which names files by path instead of by
//! [`FileId`](k_diagnostics::FileId): file ids depend on the order files
//! were added, which is exactly what [`check_order_independent`] varies.
//!
//! | Check | Invariant |
//! |---|---|
//! | [`check_diagnostics`] | codes registered; messages one line and non-empty; primary spans, labels and fix edits in a known file, in bounds and on char boundaries; fixes never edit the standard library; the edits of one fix are sorted and disjoint; diagnostics sorted by span and code; `file_diagnostics` agrees with `diagnostics` |
//! | [`check_deterministic`] | a fresh analysis, and one on a single thread, report exactly the same diagnostics |
//! | [`check_order_independent`] | adding the files in another order changes nothing but file ids |
//! | [`check_incremental`] | after each edit, the re-check equals a fresh check of the edited project; the stale snapshot is cancelled |
//! | [`check_fixes_add_no_errors`] | applying every machine-applicable fix never adds an error |

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use arbitrary::Unstructured;
use k_diagnostics::{Applicability, Diagnostic, FileId, Span};
use k_sema::{Analysis, AnalysisHost, Change, FileOrigin, Options, ProjectPath};

/// The `k.toml` used when a target analyses loose files.
pub const MANIFEST: &str = "[package]\nname = \"fz\"\nk = \"0.1\"\n";

/// A project: `(project path, text)` pairs with unique, valid paths.
pub type Files = [(String, String)];

/// A diagnostic in normal form: files by path, ranges as byte offsets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Norm {
    /// The file of the primary span, by path.
    pub path: String,
    /// The primary range.
    pub range: (u32, u32),
    /// The code, such as `K2001`.
    pub code: String,
    /// `true` for an error.
    pub error: bool,
    /// The message.
    pub message: String,
    /// `(path, range, message)` of every label.
    pub labels: Vec<NormEdit>,
    /// Notes then helps.
    pub notes: Vec<String>,
    /// Every fix.
    pub fixes: Vec<NormFix>,
}

/// A fix edit in normal form: `(path, range, replacement)`.
pub type NormEdit = (String, (u32, u32), String);

/// A fix in normal form.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormFix {
    /// The fix's message.
    pub message: String,
    /// `true` if machine-applicable.
    pub machine: bool,
    /// `(path, range, replacement)` of every edit.
    pub edits: Vec<NormEdit>,
}

/// Builds a host holding `files`, added in slice order.
pub fn host(files: &Files, options: Options) -> AnalysisHost {
    let mut host = AnalysisHost::new(options);
    let mut change = Change::new();
    for (path, text) in files {
        let path = ProjectPath::new(path)
            .unwrap_or_else(|e| panic!("generated an invalid project path `{path}`: {e}"));
        change.set_file(path, text.as_str());
    }
    host.apply_change(change);
    host
}

/// Returns the path `analysis` gives `file` (`<stdlib>/…` for the
/// standard library), or `None` for an unknown id.
pub fn path_of(analysis: &Analysis, file: FileId) -> Option<String> {
    match analysis.origin(file)? {
        FileOrigin::Project(p) => Some(p.as_str().to_owned()),
        FileOrigin::Stdlib(p) => Some(format!("<stdlib>/{p}")),
    }
}

fn range_of(span: Span) -> (u32, u32) {
    (u32::from(span.start()), u32::from(span.end()))
}

/// Normalizes `d`.
pub fn norm(analysis: &Analysis, d: &Diagnostic) -> Norm {
    let path = |s: Span| path_of(analysis, s.file).unwrap_or_else(|| format!("<{:?}>", s.file));
    Norm {
        path: path(d.span),
        range: range_of(d.span),
        code: d.code.to_string(),
        error: d.is_error(),
        message: d.message.clone(),
        labels: d
            .labels
            .iter()
            .map(|l| (path(l.span), range_of(l.span), l.message.clone()))
            .collect(),
        notes: d.notes.iter().chain(&d.helps).cloned().collect(),
        fixes: d
            .fixes
            .iter()
            .map(|f| NormFix {
                message: f.message.clone(),
                machine: f.applicability == Applicability::MachineApplicable,
                edits: f
                    .edits
                    .iter()
                    .map(|e| (path(e.span), range_of(e.span), e.replacement.clone()))
                    .collect(),
            })
            .collect(),
    }
}

/// Analyses `files`, checks [`check_diagnostics`], and returns the
/// diagnostics in normal form, in the order the analysis reported them.
pub fn check(files: &Files) -> Vec<Norm> {
    let host = host(files, Options::new());
    check_analysis(&host.analysis(), files)
}

/// [`check_diagnostics`] on an existing analysis of `files`; returns the
/// normalized diagnostics.
pub fn check_analysis(analysis: &Analysis, files: &Files) -> Vec<Norm> {
    let diagnostics = analysis
        .diagnostics()
        .expect("a current snapshot is never cancelled");
    check_diagnostics(analysis, &diagnostics, files);
    diagnostics.iter().map(|d| norm(analysis, d)).collect()
}

/// Checks every structural invariant of `diagnostics` (see the module
/// table).
pub fn check_diagnostics(analysis: &Analysis, diagnostics: &[Diagnostic], files: &Files) {
    let context = || dump(files);
    let in_file = |span: Span, what: &str, d: &Diagnostic| {
        let Some(text) = analysis.text(span.file) else {
            panic!(
                "{} {what} is in unknown file {:?}: {d:#?}\n{}",
                d.code,
                span.file,
                context()
            );
        };
        let (start, end) = (usize::from(span.start()), usize::from(span.end()));
        assert!(
            start <= end && end <= text.len(),
            "{} {what} {start}..{end} is outside its file (len {}): {d:#?}\n{}",
            d.code,
            text.len(),
            context()
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "{} {what} {start}..{end} is not on char boundaries: {d:#?}\n{}",
            d.code,
            context()
        );
    };
    for d in diagnostics {
        assert!(
            d.code.info().is_some(),
            "unregistered code {}: {d:#?}\n{}",
            d.code,
            context()
        );
        assert!(
            !d.message.is_empty() && !d.message.contains('\n'),
            "{} message must be one non-empty line: {:?}\n{}",
            d.code,
            d.message,
            context()
        );
        in_file(d.span, "primary span", d);
        for label in &d.labels {
            in_file(label.span, "label", d);
        }
        for fix in &d.fixes {
            for edit in &fix.edits {
                in_file(edit.span, "fix edit", d);
                assert!(
                    matches!(
                        analysis.origin(edit.span.file),
                        Some(FileOrigin::Project(_))
                    ),
                    "{} fix `{}` edits the standard library\n{}",
                    d.code,
                    fix.message,
                    context()
                );
            }
            for pair in fix.edits.windows(2) {
                let (a, b) = (pair[0].span, pair[1].span);
                assert!(
                    a.file != b.file || a.end() <= b.start(),
                    "{} fix `{}` has unsorted or overlapping edits\n{}",
                    d.code,
                    fix.message,
                    context()
                );
            }
        }
    }
    for pair in diagnostics.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            (a.span, a.code) <= (b.span, b.code),
            "diagnostics are not sorted: {} at {:?} before {} at {:?}\n{}",
            a.code,
            a.span,
            b.code,
            b.span,
            context()
        );
    }
    // Per-file diagnostics are the slice of the whole list in that file.
    for (file, _) in analysis.files() {
        let per_file = analysis
            .file_diagnostics(file)
            .expect("a current snapshot is never cancelled");
        let expected: Vec<&Diagnostic> =
            diagnostics.iter().filter(|d| d.span.file == file).collect();
        assert_eq!(
            per_file.iter().collect::<Vec<_>>(),
            expected,
            "file_diagnostics disagrees with diagnostics for {:?}\n{}",
            path_of(analysis, file),
            context()
        );
    }
}

/// Checks that analysing `files` again, in a fresh host, and on a single
/// thread, reports exactly `expected` (normalized, in order).
pub fn check_deterministic(files: &Files, expected: &[Norm]) {
    let again = check(files);
    assert_eq!(again, expected, "a fresh analysis differs\n{}", dump(files));
    let one = NonZeroUsize::MIN;
    let host = host(files, Options::new().with_threads(one));
    let single = check_analysis(&host.analysis(), files);
    assert_eq!(
        single,
        expected,
        "a single-threaded analysis differs\n{}",
        dump(files)
    );
}

/// Returns `diagnostics` sorted, for comparisons that ignore file ids.
pub fn sorted(diagnostics: &[Norm]) -> Vec<Norm> {
    let mut v = diagnostics.to_vec();
    v.sort();
    v
}

/// Checks that adding the files of `files` in the order `perm` (a
/// permutation of its indices) reports the same diagnostics as `expected`,
/// up to file ids.
pub fn check_order_independent(files: &Files, perm: &[usize], expected: &[Norm]) {
    let shuffled: Vec<(String, String)> = perm.iter().map(|&i| files[i].clone()).collect();
    let got = check(&shuffled);
    let (got, want) = (sorted(&got), sorted(expected));
    if got != want {
        panic!(
            "diagnostics depend on the order files are added ({:?})\n{}\n{}",
            perm.iter()
                .map(|&i| files[i].0.as_str())
                .collect::<Vec<_>>(),
            diff(&want, &got),
            dump(files)
        );
    }
}

/// Returns a permutation of `0..n` read from `u` (identity once the bytes
/// run out, reversed if the first byte is missing so the check still
/// varies something).
pub fn permutation(u: &mut Unstructured<'_>, n: usize) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..n).collect();
    if u.is_empty() {
        perm.reverse();
        return perm;
    }
    for i in (1..n).rev() {
        let j = u.choose_index(i + 1).unwrap_or(0);
        perm.swap(i, j);
    }
    perm
}

/// One edit of a project.
#[derive(Clone, Debug)]
pub enum Edit {
    /// Sets the text of a file, adding it if it is new.
    Set(String, String),
    /// Removes a file.
    Remove(String),
}

/// Applies `edit` to `files`.
pub fn apply_edit(files: &mut Vec<(String, String)>, edit: &Edit) {
    match edit {
        Edit::Set(path, text) => match files.iter_mut().find(|(p, _)| p == path) {
            Some(slot) => slot.1.clone_from(text),
            None => files.push((path.clone(), text.clone())),
        },
        Edit::Remove(path) => files.retain(|(p, _)| p != path),
    }
}

/// Fragments spliced into files by [`random_edit`]: typing in progress,
/// broken syntax and common mistakes.
pub const EDIT_FRAGMENTS: &[&str] = &[
    "",
    "\n",
    " ",
    "x",
    "1",
    "(",
    ")",
    "{",
    "}",
    "\"",
    ".",
    "let q = 1\n",
    "func z() -> Int {\n    return 1\n}\n",
    "struct Q {\n    let a: Int\n}\n",
    "enum R {\n    case a\n}\n",
    "import model.T0\n",
    "public ",
    " and ",
    " && ",
    "nil",
    "?",
    "-> String",
    "return",
    "match",
    "T0",
    "f0(",
    "é",
];

/// Reads one edit of `files` from `u`: a splice into a file (delete a
/// range, insert a fragment), a file replaced by another's text, a file
/// removed, or a new file added under `src/logic/` (or at a stray place).
/// Paths stay valid project paths; texts stay valid UTF-8.
pub fn random_edit(files: &Files, u: &mut Unstructured<'_>) -> Edit {
    let pick = |u: &mut Unstructured<'_>, n: usize| {
        if n <= 1 {
            0
        } else {
            u.choose_index(n).unwrap_or(0)
        }
    };
    if files.is_empty() {
        return Edit::Set("src/logic/New.k".to_owned(), "let n = 1\n".to_owned());
    }
    let f = pick(u, files.len());
    let (path, text) = &files[f];
    match pick(u, 8) {
        0..=3 => {
            // Splice: delete `len` bytes at `at`, insert a fragment.
            let boundaries: Vec<usize> = (0..=text.len())
                .filter(|&i| text.is_char_boundary(i))
                .collect();
            let a = boundaries[pick(u, boundaries.len())];
            let rest: Vec<usize> = boundaries
                .iter()
                .copied()
                .filter(|&b| b >= a && b <= a + 24)
                .collect();
            let b = rest[pick(u, rest.len())];
            let insert = EDIT_FRAGMENTS[pick(u, EDIT_FRAGMENTS.len())];
            let mut new = String::with_capacity(text.len() + insert.len());
            new.push_str(&text[..a]);
            new.push_str(insert);
            new.push_str(&text[b..]);
            Edit::Set(path.clone(), new)
        }
        4 => {
            let other = &files[pick(u, files.len())].1;
            Edit::Set(path.clone(), other.clone())
        }
        5 => Edit::Remove(path.clone()),
        6 => {
            let n = pick(u, 4);
            let new_path = [
                "src/logic/N.k",
                "src/model/T9.k",
                "src/extra/E.k",
                "Stray.k",
            ][n];
            let body = EDIT_FRAGMENTS[pick(u, EDIT_FRAGMENTS.len())];
            Edit::Set(new_path.to_owned(), format!("{text}{body}"))
        }
        _ => {
            // Undo-like edit: restore the same text (no change at all).
            Edit::Set(path.clone(), text.clone())
        }
    }
}

/// Checks incremental re-checking: starting from a host that analysed
/// `files`, applies each of `edits` as a revision and compares its
/// diagnostics with a fresh analysis of the edited project. The snapshot
/// taken before each edit must report itself cancelled afterwards.
pub fn check_incremental(files: &Files, edits: &[Edit]) {
    let mut current: Vec<(String, String)> = files.to_vec();
    let mut host = host(files, Options::new());
    let _ = check_analysis(&host.analysis(), &current);
    for (step, edit) in edits.iter().enumerate() {
        let before = host.analysis();
        let mut change = Change::new();
        match edit {
            Edit::Set(p, t) => {
                change.set_file(ProjectPath::new(p).expect("valid path"), t.as_str())
            }
            Edit::Remove(p) => change.remove_file(ProjectPath::new(p).expect("valid path")),
        }
        let empty = change.is_empty();
        host.apply_change(change);
        apply_edit(&mut current, edit);
        if !empty {
            assert!(
                before.is_cancelled(),
                "the snapshot before edit {step} is not cancelled"
            );
        }
        let incremental = sorted(&check_analysis(&host.analysis(), &current));
        let fresh = sorted(&check(&current));
        if incremental != fresh {
            panic!(
                "incremental re-check after edit {step} ({edit:?}) differs from a fresh check\n{}\n--- before ---\n{}\n--- after ---\n{}",
                diff(&fresh, &incremental),
                dump(files),
                dump(&current)
            );
        }
    }
}

/// Applies every machine-applicable fix of `diagnostics` to `files`.
///
/// Identical fixes (the same edits) are applied once. Fixes are taken in
/// order of their first edit; a fix whose edits overlap an edit already
/// taken is skipped, as `rustfix` does. Two edits overlap when their ranges
/// share a byte, when one inserts strictly inside the other's range, or
/// when they replace the same non-empty range; distinct insertions at one
/// offset (several missing imports) don't, and land in the order taken.
/// Returns the new files and the number of fixes applied.
pub fn apply_machine_fixes(files: &Files, diagnostics: &[Norm]) -> (Vec<(String, String)>, usize) {
    let mut fixes: Vec<&NormFix> = diagnostics
        .iter()
        .flat_map(|d| &d.fixes)
        .filter(|f| f.machine && !f.edits.is_empty())
        .collect();
    fixes.sort_by(|a, b| a.edits[0].cmp(&b.edits[0]).then_with(|| a.cmp(b)));
    fixes.dedup_by(|a, b| a.edits == b.edits);
    let overlaps = |a: &NormEdit, b: &NormEdit| {
        a.0 == b.0
            && ((a.1 == b.1 && a.1.0 < a.1.1)
                || (a.1.0 < b.1.1 && b.1.0 < a.1.1)
                || (a.1.0 == a.1.1 && b.1.0 < a.1.0 && a.1.0 < b.1.1)
                || (b.1.0 == b.1.1 && a.1.0 < b.1.0 && b.1.0 < a.1.1))
    };
    let mut taken: Vec<NormEdit> = Vec::new();
    let mut applied = 0;
    for fix in fixes {
        if fix
            .edits
            .iter()
            .any(|e| taken.iter().any(|t| overlaps(e, t)))
        {
            continue;
        }
        taken.extend(fix.edits.iter().cloned());
        applied += 1;
    }
    let mut by_file: BTreeMap<&str, Vec<(usize, &NormEdit)>> = BTreeMap::new();
    for (i, edit) in taken.iter().enumerate() {
        by_file.entry(edit.0.as_str()).or_default().push((i, edit));
    }
    let mut out = files.to_vec();
    for (path, slot) in &mut out {
        let Some(edits) = by_file.get_mut(path.as_str()) else {
            continue;
        };
        // Back to front; at one offset the edit taken last goes in first,
        // so insertions end up in the order taken.
        edits.sort_by_key(|(i, e)| std::cmp::Reverse((e.1, *i)));
        for (_, (_, (start, end), replacement)) in edits.iter() {
            slot.replace_range(*start as usize..*end as usize, replacement);
        }
    }
    (out, applied)
}

/// Counts errors by `(path, code)`.
pub fn error_counts(diagnostics: &[Norm]) -> BTreeMap<(String, String), usize> {
    let mut counts = BTreeMap::new();
    for d in diagnostics.iter().filter(|d| d.error) {
        *counts.entry((d.path.clone(), d.code.clone())).or_insert(0) += 1;
    }
    counts
}

/// Checks that applying every machine-applicable fix of `before` (the
/// diagnostics of `files`) adds no error: for every file and code, the
/// fixed project has at most as many errors as before. Returns the fixed
/// project and its diagnostics.
pub fn check_fixes_add_no_errors(
    files: &Files,
    before: &[Norm],
) -> (Vec<(String, String)>, Vec<Norm>) {
    let (fixed, applied) = apply_machine_fixes(files, before);
    if applied == 0 {
        return (fixed, before.to_vec());
    }
    let after = check(&fixed);
    let (was, now) = (error_counts(before), error_counts(&after));
    let added: Vec<String> = now
        .iter()
        .filter(|(k, n)| was.get(*k).copied().unwrap_or(0) < **n)
        .map(|((p, c), n)| {
            format!(
                "{c} in {p}: {} -> {n}",
                was.get(&(p.clone(), c.clone())).copied().unwrap_or(0)
            )
        })
        .collect();
    if !added.is_empty() {
        let new_errors: Vec<&Norm> = after
            .iter()
            .filter(|d| {
                d.error
                    && added
                        .iter()
                        .any(|a| a.starts_with(&format!("{} in {}:", d.code, d.path)))
            })
            .collect();
        panic!(
            "applying {applied} machine-applicable fix(es) added errors: {added:?}\nnew errors: {new_errors:#?}\n--- before ---\n{}\n--- after ---\n{}",
            dump(files),
            dump(&fixed)
        );
    }
    (fixed, after)
}

/// Returns the errors among `diagnostics`.
pub fn errors(diagnostics: &[Norm]) -> Vec<&Norm> {
    diagnostics.iter().filter(|d| d.error).collect()
}

/// Exercises the position queries on up to `limit` token ranges of every
/// project file: each must answer or decline, never panic.
pub fn check_queries(analysis: &Analysis, limit: usize) {
    for (file, _) in analysis.files() {
        let Ok(Some(parse)) = analysis.parse(file) else {
            continue;
        };
        for token in parse
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .take(limit)
        {
            let range = token.text_range();
            let _ = analysis.type_at(file, range).expect("not cancelled");
            let _ = analysis.resolve_at(file, range).expect("not cancelled");
            let _ = analysis.side_of(file, range).expect("not cancelled");
        }
    }
}

fn diff(want: &[Norm], got: &[Norm]) -> String {
    // A multiset difference, so a diagnostic reported a different number of
    // times shows up too.
    let count = |list: &[Norm], d: &Norm| list.iter().filter(|x| *x == d).count();
    let mut out = String::new();
    let mut seen: Vec<&Norm> = Vec::new();
    for d in want.iter().chain(got) {
        if seen.contains(&d) {
            continue;
        }
        seen.push(d);
        let (w, g) = (count(want, d), count(got, d));
        if w != g {
            let sign = if w > g { '-' } else { '+' };
            out.push_str(&format!(
                "{sign} {} {} {:?} {} (expected {w}x, got {g}x)\n",
                d.code, d.path, d.range, d.message
            ));
        }
    }
    out
}

/// Formats `files` for failure messages.
pub fn dump(files: &Files) -> String {
    let mut out = String::new();
    for (path, text) in files {
        out.push_str("--- ");
        out.push_str(path);
        out.push_str(" ---\n");
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}
