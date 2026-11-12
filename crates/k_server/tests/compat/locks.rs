//! Lockfile formats (`docs/design/48-releases-and-versioning.md`, "Lockfile
//! format versions"; `docs/compiler/release.md` §4, "A lockfile format
//! change").
//!
//! `tests/compat/locks/project/` is a small app with something for every
//! lockfile to record; `tests/compat/locks/<file>/<format>/<file>` is the
//! lock it has in that format, as the toolchain that introduced the format
//! wrote it. Every format of every lockfile stays: the current toolchain
//! must read each one, write it back unchanged, and refuse a format newer
//! than its own without rewriting the lock.
//!
//! When a lockfile's format changes, add its golden file for the new
//! format here; the tests below then check the older goldens are still
//! read (and, once `k upgrade` exists, that it rewrites them).

use std::collections::BTreeMap;

use k_diagnostics::{Diagnostic, codes};
use k_sema::lockformat::{self, LockFormat};
use k_sema::workspace::{lock_toolchain, with_lock_toolchain};
use k_sema::{AnalysisHost, Options, ProjectPath};

use super::{collect, compat_dir};

/// Every lockfile with a format.
const LOCKS: &[&str] = &[
    "k.lock",
    "routes.lock",
    "rpc.lock",
    "schema.lock",
    "storage.lock",
];

/// The golden files of `lock`: (format, text), oldest first.
fn goldens(lock: &str) -> Vec<(u32, String)> {
    let dir = compat_dir().join("locks").join(lock);
    let mut out: Vec<(u32, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter_map(|e| {
            let n: u32 = e.file_name().to_str()?.parse().ok()?;
            let text = std::fs::read_to_string(e.path().join(lock))
                .unwrap_or_else(|err| panic!("{}/{lock}: {err}", e.path().display()));
            Some((n, text))
        })
        .collect();
    out.sort_by_key(|(n, _)| *n);
    assert!(!out.is_empty(), "{} has no golden file", dir.display());
    out
}

/// The newest golden of `lock`.
fn newest(lock: &str) -> String {
    goldens(lock).pop().map(|(_, t)| t).unwrap_or_default()
}

/// What the toolchain says about the project with these locks.
struct Checked {
    /// `k check`'s errors and warnings, with the file each is in.
    diagnostics: Vec<(String, Diagnostic)>,
    /// The new text `k build` would write, by lock; absent when it leaves
    /// the lock as it is.
    written: BTreeMap<String, String>,
}

/// Checks the fixture project with `locks` (file name to text) next to it,
/// as `k check` and `k build` would.
fn check(locks: &BTreeMap<String, String>) -> Checked {
    let dir = compat_dir().join("locks/project");
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    files.extend(locks.iter().map(|(n, t)| (n.clone(), t.clone())));
    let host = AnalysisHost::with_files(
        Options::new(),
        files
            .iter()
            .map(|(p, t)| (ProjectPath::new(p).expect("a project path"), t.clone())),
    );
    let analysis = host.analysis();
    let paths: BTreeMap<_, _> = analysis
        .files()
        .into_iter()
        .map(|(id, p)| (id, p.as_str().to_owned()))
        .collect();
    let mut diagnostics: Vec<Diagnostic> = analysis.diagnostics().expect("not cancelled");
    diagnostics.extend(k_codegen::project_diagnostics(&analysis));
    let diagnostics = diagnostics
        .into_iter()
        .map(|d| (paths.get(&d.span.file).cloned().unwrap_or_default(), d))
        .collect();
    let mut written = BTreeMap::new();
    let mut wrote = |name: &str, text: Option<String>| {
        if let Some(text) = text
            && locks.get(name) != Some(&text)
        {
            written.insert(name.to_owned(), text);
        }
    };
    wrote("rpc.lock", analysis.rpc_lock().expect("not cancelled"));
    wrote(
        "storage.lock",
        analysis.storage_lock().expect("not cancelled"),
    );
    wrote(
        "schema.lock",
        k_codegen::schema_lock(&analysis).expect("not cancelled"),
    );
    wrote(
        "routes.lock",
        k_codegen::routes_lock(&analysis).expect("not cancelled"),
    );
    // `k build` records the toolchain that built it; here, the one the lock
    // names, so a current lock stays as it is.
    if let Some(old) = locks.get("k.lock")
        && let Ok(Some((version, _))) = lock_toolchain(old)
    {
        wrote("k.lock", Some(with_lock_toolchain(Some(old), &version)));
    }
    Checked {
        diagnostics,
        written,
    }
}

/// The newest golden of every lock, with `lock` replaced by `text`.
fn with(lock: &str, text: &str) -> BTreeMap<String, String> {
    let mut locks: BTreeMap<String, String> =
        LOCKS.iter().map(|l| ((*l).to_owned(), newest(l))).collect();
    locks.insert(lock.to_owned(), text.to_owned());
    locks
}

/// Describes diagnostics for an assertion message.
fn shown(diagnostics: &[(String, Diagnostic)]) -> String {
    diagnostics
        .iter()
        .map(|(f, d)| format!("{f}: {}: {}", d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` without its last table (from its last `[` line to the end), so
/// the lock no longer records everything the program has; for `k.lock`,
/// which has no tables, with another toolchain recorded.
fn stale(lock: &str, text: &str) -> String {
    if lock == "k.lock" {
        return text.replace("toolchain = \"0.1.0\"", "toolchain = \"0.0.1\"");
    }
    let start = text
        .match_indices("\n[")
        .last()
        .map_or(text.len(), |(i, _)| i + 1);
    text[..start].trim_end_matches('\n').to_owned() + "\n"
}

/// The text of `k.lock` the toolchain writes for a stale one: the
/// golden's toolchain again.
fn rewritten(lock: &str, stale_text: &str, golden: &str) -> Option<String> {
    if lock != "k.lock" {
        return None;
    }
    let (version, _) = lock_toolchain(golden).ok()??;
    Some(with_lock_toolchain(Some(stale_text), &version))
}

/// `text` with `format = N` after its comment header.
fn declaring(text: &str, number: u32) -> String {
    LockFormat {
        number,
        declared: true,
    }
    .stamp(text)
}

#[test]
fn every_lockfile_has_a_golden_file_of_its_current_format() {
    assert_eq!(
        LOCKS,
        lockformat::FORMATS
            .iter()
            .map(|(f, _)| *f)
            .collect::<Vec<_>>()
            .as_slice(),
        "every lockfile with a format is tested here"
    );
    assert_eq!(
        lockformat::current("schema.lock"),
        Some(k_db::lock::FORMAT),
        "k_db's schema.lock format is the one k_sema lists"
    );
    for lock in LOCKS {
        let current = lockformat::current(lock).expect("a format");
        let formats: Vec<u32> = goldens(lock).iter().map(|(n, _)| *n).collect();
        assert_eq!(
            formats,
            (1..=current).collect::<Vec<_>>(),
            "{lock}: one golden file per format, up to the current one"
        );
    }
}

#[test]
fn a_lock_without_format_is_format_1() {
    for lock in LOCKS {
        let (n, text) = goldens(lock).remove(0);
        assert_eq!(n, 1);
        assert!(!text.contains("format ="), "{lock}: format 1 has no key");
        assert_eq!(
            lockformat::read(lock, &text),
            Ok(LockFormat::IMPLIED),
            "{lock}"
        );
    }
}

#[test]
fn every_golden_lock_is_read_and_written_back_unchanged() {
    for lock in LOCKS {
        for (n, text) in goldens(lock) {
            let checked = check(&with(lock, &text));
            assert!(
                checked.diagnostics.is_empty(),
                "{lock} format {n}:\n{}",
                shown(&checked.diagnostics)
            );
            assert!(
                checked.written.is_empty(),
                "{lock} format {n} is rewritten: {:#?}",
                checked.written
            );
            if *lock == "k.lock" {
                assert!(
                    matches!(lock_toolchain(&text), Ok(Some(_))),
                    "{lock} format {n}"
                );
            }
        }
    }
}

#[test]
fn writing_keeps_the_format_the_lock_was_read_in() {
    for lock in LOCKS {
        let golden = newest(lock);
        for text in [golden.clone(), declaring(&golden, 1)] {
            // Current: nothing to write, the `format` line included.
            let checked = check(&with(lock, &text));
            assert!(
                checked.diagnostics.iter().all(|(_, d)| !d.is_error()),
                "{lock}:\n{}",
                shown(&checked.diagnostics)
            );
            assert!(checked.written.is_empty(), "{lock}: {:#?}", checked.written);
            // Stale: rewritten exactly as it was, with or without the key.
            let old = stale(lock, &text);
            assert_ne!(old, text, "{lock}: the stale lock differs");
            let checked = check(&with(lock, &old));
            let written = checked
                .written
                .get(*lock)
                .cloned()
                .or_else(|| rewritten(lock, &old, &golden));
            assert_eq!(written.as_deref(), Some(text.as_str()), "{lock}");
        }
    }
}

#[test]
fn a_newer_format_is_k8202_and_the_lock_is_never_rewritten() {
    for lock in LOCKS {
        let next = lockformat::current(lock).expect("a format") + 1;
        let text = declaring(&newest(lock), next);
        if *lock == "k.lock" {
            let e = lock_toolchain(&text).expect_err("refused");
            assert_eq!(e.code, codes::LOCKFILE_TOO_NEW);
            assert!(
                e.message.contains(&format!("format {next}")),
                "{}",
                e.message
            );
            continue;
        }
        let checked = check(&with(lock, &text));
        let in_lock: Vec<&Diagnostic> = checked
            .diagnostics
            .iter()
            .filter(|(f, _)| f == lock)
            .map(|(_, d)| d)
            .collect();
        assert_eq!(in_lock.len(), 1, "{lock}:\n{}", shown(&checked.diagnostics));
        let d = in_lock[0];
        assert_eq!(d.code, codes::LOCKFILE_TOO_NEW, "{lock}: {}", d.message);
        let start = usize::from(d.span.range.start());
        assert!(
            text[start..].starts_with(&format!("format = {next}")),
            "{lock}: at the format line"
        );
        assert!(
            d.message.contains(&format!("{lock} is in format {next}")),
            "{}",
            d.message
        );
        assert!(
            d.helps.iter().any(|h| h.contains("k toolchain install")),
            "{:?}",
            d.helps
        );
        assert!(
            !checked.written.contains_key(*lock),
            "{lock} in a newer format is rewritten"
        );
    }
}

#[test]
fn a_format_that_isnt_a_number_is_k8002() {
    for lock in LOCKS {
        let text = newest(lock).replacen('\n', "\nformat = \"two\"\n", 1);
        if *lock == "k.lock" {
            let e = lock_toolchain(&text).expect_err("refused");
            assert_eq!(e.code, codes::INVALID_LOCKFILE);
            continue;
        }
        let checked = check(&with(lock, &text));
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|(f, d)| f == lock && d.code == codes::INVALID_LOCKFILE),
            "{lock}:\n{}",
            shown(&checked.diagnostics)
        );
    }
}
