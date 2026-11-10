//! Tests of the generated benchmark projects (`benches/project/mod.rs`).
//!
//! The semantic benchmarks and budget checks are only meaningful if their
//! input is valid K: a project full of errors measures error paths, not
//! checking. These tests guarantee that the realistic project at several
//! sizes and seeds, every pathological family, and every edit the warm
//! benchmarks apply, analyse with zero diagnostics; that sizes are as
//! requested; and that generation is deterministic.

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::project;

use k_sema::{AnalysisHost, Change, Options, ProjectPath};
use project::{File, Pathological};

fn host(files: &[File]) -> AnalysisHost {
    AnalysisHost::with_files(
        Options::new(),
        files
            .iter()
            .map(|(p, t)| (ProjectPath::new(p).unwrap(), t.clone())),
    )
}

fn assert_clean(host: &AnalysisHost, what: &str) {
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().unwrap();
    if diagnostics.is_empty() {
        return;
    }
    let map = analysis.source_map(ToString::to_string);
    let rendered: Vec<String> = diagnostics
        .iter()
        .take(20)
        .map(|d| {
            k_diagnostics::render::human(d, &map, &k_diagnostics::render::HumanOptions::plain())
        })
        .collect();
    panic!(
        "{what}: {} diagnostics, the first ones:\n{}",
        diagnostics.len(),
        rendered.join("\n")
    );
}

#[test]
fn realistic_projects_have_no_diagnostics() {
    for lines in [200, 1_000, 10_000] {
        let files = project::realistic(lines, project::SEED);
        assert_clean(&host(&files), &format!("realistic {lines}"));
    }
}

#[test]
fn realistic_projects_have_no_diagnostics_for_many_seeds() {
    for seed in 0..16 {
        let files = project::realistic(3_000, seed);
        assert_clean(&host(&files), &format!("realistic 3k, seed {seed}"));
    }
}

#[test]
fn realistic_projects_have_the_requested_line_count() {
    for lines in [1_000, 4_321, 10_000, 100_000] {
        let files = project::realistic(lines, project::SEED);
        assert_eq!(project::project_lines(&files), lines, "{lines}");
    }
}

#[test]
fn realistic_projects_have_realistic_shape() {
    let files = project::realistic(100_000, project::SEED);
    let k_files = files.iter().filter(|(p, _)| p.ends_with(".k")).count();
    let modules: std::collections::HashSet<&str> = files
        .iter()
        .filter_map(|(p, _)| p.strip_prefix("src/")?.split('/').next())
        .collect();
    let bytes = project::project_bytes(&files);
    // architecture.md §12: about 1,000 files and 3.5 MB for 100k lines.
    assert!((700..=2_000).contains(&k_files), "{k_files} files");
    assert!(modules.len() >= 80, "{} modules", modules.len());
    assert!((2_500_000..=4_500_000).contains(&bytes), "{bytes} bytes");
    // Every kind of feature file is present.
    for kind in [
        "Record", "Status", "Failure", "Shape", "Protocol", "Api", "Service", "Card", "Screen",
    ] {
        assert!(
            files.iter().any(|(p, _)| p.contains(&format!("/{kind}"))),
            "{kind}"
        );
    }
    assert!(files.iter().any(|(p, _)| p.ends_with(".test.k")));
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(project::realistic(5_000, 7), project::realistic(5_000, 7));
    assert_ne!(project::realistic(5_000, 7), project::realistic(5_000, 8));
    for family in Pathological::ALL {
        assert_eq!(family.generate(20), family.generate(20));
    }
}

#[test]
fn pathological_families_have_no_diagnostics() {
    for family in Pathological::ALL {
        let files = family.generate(family.default_size());
        assert_clean(&host(&files), family.name());
    }
}

#[test]
fn pathological_families_have_no_diagnostics_at_small_sizes() {
    for family in Pathological::ALL {
        for size in [1, 2, 5] {
            let files = family.generate(size);
            assert_clean(&host(&files), &format!("{} at {size}", family.name()));
        }
    }
}

#[test]
fn pathological_families_grow_with_size() {
    for family in Pathological::ALL {
        let small = project::project_bytes(&family.generate(10));
        let large = project::project_bytes(&family.generate(40));
        assert!(large > small * 2, "{}: {small} -> {large}", family.name());
    }
}

/// Applies `edit` to the edited service, repeatedly, and checks the project
/// stays clean and the text really changes.
fn check_edits(edit: fn(&str, usize) -> String, what: &str) {
    let files = project::realistic(3_000, project::SEED);
    let mut host = host(&files);
    let path = project::edited_service_path();
    let mut text = files.iter().find(|(p, _)| *p == path).unwrap().1.clone();
    for generation in 0..3 {
        let next = edit(&text, generation);
        assert_ne!(next, text, "{what} {generation} changed nothing");
        text = next;
        let mut change = Change::new();
        change.set_file(ProjectPath::new(&path).unwrap(), text.clone());
        host.apply_change(change);
        assert_clean(&host, &format!("{what} {generation}"));
    }
}

#[test]
fn body_edits_keep_the_project_clean() {
    check_edits(project::body_edit, "body edit");
}

#[test]
fn body_edits_change_only_a_body() {
    let files = project::realistic(1_000, project::SEED);
    let path = project::edited_service_path();
    let text = &files.iter().find(|(p, _)| *p == path).unwrap().1;
    let edited = project::body_edit(text, 0);
    // Same line count, one line differs, and it is the helper's body.
    let before: Vec<&str> = text.lines().collect();
    let after: Vec<&str> = edited.lines().collect();
    assert_eq!(before.len(), after.len());
    let differing: Vec<usize> = (0..before.len())
        .filter(|&i| before[i] != after[i])
        .collect();
    assert_eq!(differing.len(), 1);
    assert!(
        after[differing[0]]
            .trim_start()
            .starts_with("return value * 3 + ")
    );
}

#[test]
fn signature_edits_keep_the_project_clean() {
    check_edits(project::signature_edit, "signature edit");
}

#[test]
fn line_count_counts_a_final_unterminated_line() {
    assert_eq!(project::line_count(""), 0);
    assert_eq!(project::line_count("a"), 1);
    assert_eq!(project::line_count("a\n"), 1);
    assert_eq!(project::line_count("a\nb"), 2);
    assert_eq!(project::line_count("\n\n"), 2);
}

/// A project with one enum of `cases` cases (every third with a payload)
/// and one function whose `match` has `arms`.
fn match_project(cases: usize, arms: &str) -> Vec<File> {
    let mut e = String::from("/// Many cases.\npublic enum Wide {\n");
    for c in 0..cases {
        if c % 3 == 0 {
            e.push_str(&format!("    /// Case {c}.\n    case c{c}(value: Int)\n"));
        } else {
            e.push_str(&format!("    /// Case {c}.\n    case c{c}\n"));
        }
    }
    e.push_str("}\n");
    let f = format!(
        "/// Classifies.\nfunc classify(wide: Wide) -> Int {{\n    return match wide {{\n{arms}    }}\n}}\n"
    );
    vec![
        ("src/wide/Wide.k".to_owned(), e),
        ("src/wide/Classify.k".to_owned(), f),
    ]
}

fn arm(c: usize) -> String {
    if c.is_multiple_of(3) {
        format!("        case .c{c}(let value) {{ value }}\n")
    } else {
        format!("        case .c{c} {{ {c} }}\n")
    }
}

fn codes(files: &[File]) -> Vec<String> {
    host(files)
        .analysis()
        .diagnostics()
        .unwrap()
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

// The usefulness algorithm indexes rows by head constructor for wide enums
// (`pattern::Rows`); these check that the index changes no answer.

#[test]
fn a_wide_match_over_every_case_is_exhaustive() {
    let arms: String = (0..600).map(arm).collect();
    assert_eq!(codes(&match_project(600, &arms)), Vec::<String>::new());
}

#[test]
fn a_wide_match_missing_one_case_is_not_exhaustive() {
    for missing in [0, 1, 299, 599] {
        let arms: String = (0..600).filter(|&c| c != missing).map(arm).collect();
        assert_eq!(
            codes(&match_project(600, &arms)),
            ["K2301"],
            "missing {missing}"
        );
    }
}

#[test]
fn a_repeated_arm_in_a_wide_match_is_unreachable() {
    for repeated in [0, 2, 300] {
        let mut arms: String = (0..600).map(arm).collect();
        arms.push_str(&arm(repeated));
        assert_eq!(
            codes(&match_project(600, &arms)),
            ["K2302"],
            "repeated {repeated}"
        );
    }
}

#[test]
fn guarded_arms_and_default_in_a_wide_match() {
    // A guarded arm covers nothing; the unguarded one after it is needed.
    let mut arms = String::from("        case .c0(let value) where value > 1 { value }\n");
    arms.extend((0..600).map(arm));
    assert_eq!(codes(&match_project(600, &arms)), Vec::<String>::new());
    // `default` after every case can never run.
    let mut arms: String = (0..600).map(arm).collect();
    arms.push_str("        default { 0 }\n");
    assert_eq!(codes(&match_project(600, &arms)), ["K2302"]);
    // `default` after all but one case is needed.
    let mut arms: String = (1..600).map(arm).collect();
    arms.push_str("        default { 0 }\n");
    assert_eq!(codes(&match_project(600, &arms)), Vec::<String>::new());
    // A case after `default` can never match.
    let mut arms = String::from("        default { 0 }\n");
    arms.push_str(&arm(5));
    assert_eq!(codes(&match_project(600, &arms)), ["K2302"]);
}
