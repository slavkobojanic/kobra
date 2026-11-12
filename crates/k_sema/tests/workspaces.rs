//! Workspace planning (`k_sema::Workspace`,
//! `docs/design/40-workspaces-and-services.md`) on generated input: any
//! set of member manifests, dependencies (cycles, apps, unknown names and
//! garbage included) and files is planned without panicking, every
//! problem's range lies inside its manifest, the planned graph is a DAG
//! whose library lists are closed and ordered, and every mounted path maps
//! back to a real file of a library.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use k_sema::{ProjectPath, Workspace};
use proptest::prelude::*;

/// One generated member: its directory index, whether it is an app, its
/// dependencies (by index, possibly itself or out of range) and garbage.
#[derive(Clone, Debug)]
struct GenMember {
    app: bool,
    deps: Vec<usize>,
    garbage: Option<String>,
    module: usize,
}

fn member() -> impl Strategy<Value = GenMember> {
    (
        any::<bool>(),
        prop::collection::vec(0usize..7, 0..4),
        prop::option::weighted(0.15, "[ -~\\n]{0,40}"),
        0usize..4,
    )
        .prop_map(|(app, deps, garbage, module)| GenMember {
            app,
            deps,
            garbage,
            module,
        })
}

fn files(members: &[GenMember], listed: &[usize]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let list: Vec<String> = listed.iter().map(|i| format!("\"m{i}\"")).collect();
    out.push((
        "k.toml".to_owned(),
        format!("[workspace]\nmembers = [{}]\n", list.join(", ")),
    ));
    for (i, m) in members.iter().enumerate() {
        let mut manifest = format!("[package]\nname = \"m{i}\"\nk = \"0.1\"\n\n[dependencies]\n");
        for d in &m.deps {
            manifest.push_str(&format!("m{d} = {{ workspace = true }}\n"));
        }
        if let Some(g) = &m.garbage {
            manifest.push_str(g);
        }
        out.push((format!("m{i}/k.toml"), manifest));
        if m.app {
            out.push((format!("m{i}/App.k"), format!("public app M{i} {{}}\n")));
        }
        out.push((
            format!("m{i}/src/mod{}/T{i}.k", m.module),
            format!("public struct T{i} {{}}\n"),
        ));
        out.push((
            format!("m{i}/src/mod{}/T{i}.test.k", m.module),
            String::new(),
        ));
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn planning_never_panics_and_keeps_a_dag(
        members in prop::collection::vec(member(), 1..7),
        extra in prop::collection::vec(0usize..9, 0..3),
    ) {
        let mut listed: Vec<usize> = (0..members.len()).collect();
        listed.extend(extra);
        let input = files(&members, &listed);
        let ws = Workspace::from_files(input.clone());
        for p in &ws.problems {
            let text = input.iter().find(|(f, _)| *f == p.path).map(|(_, t)| t.as_str()).unwrap_or("");
            prop_assert!(u32::from(p.range.end()) as usize <= text.len(), "{p:?}");
            prop_assert!(p.code.info().is_some(), "{p:?}");
        }
        // Edges go only to libraries, and the library lists are closed.
        for (i, m) in ws.members.iter().enumerate() {
            for &d in &m.dependencies {
                prop_assert!(!ws.members[d].is_app);
                prop_assert!(d != i);
                prop_assert!(m.libraries.contains(&d));
            }
            for &l in &m.libraries {
                prop_assert!(l != i, "a member is never its own library: a cycle got through");
                for &ll in &ws.members[l].libraries {
                    prop_assert!(m.libraries.contains(&ll));
                }
            }
        }
        // The order puts libraries first.
        let order = ws.order();
        prop_assert_eq!(order.len(), ws.members.len());
        let position = |m: usize| order.iter().position(|&x| x == m).unwrap();
        for (i, m) in ws.members.iter().enumerate() {
            for &l in &m.libraries {
                prop_assert!(position(l) < position(i));
            }
        }
        // Every mounted file maps back to a library's own file.
        for i in 0..ws.members.len() {
            let paths: BTreeSet<ProjectPath> = ws.analysis_files(i).into_iter().map(|(p, _)| p).collect();
            for p in &paths {
                let real = ws.real_path(i, p).unwrap();
                prop_assert!(input.iter().any(|(f, _)| *f == real), "{p} → {real}");
                if p.dependency().is_some() {
                    prop_assert!(!real.ends_with(".test.k"));
                    let back = ws.view_path(i, &real);
                    prop_assert_eq!(back.as_ref(), Some(p));
                }
            }
        }
    }
}

// ------------------------------------------------- toolchain requirements
//
// `docs/compiler/release.md` §12 step 2: `[package] k` and `[workspace] k`
// are requirements with Cargo's caret meaning
// (`docs/design/48-releases-and-versioning.md`, "The requirement and the
// lock").

mod requirements {
    use k_diagnostics::codes;
    use k_sema::Workspace;
    use k_sema::workspace::{Requirement, Version};

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} is a version"))
    }

    fn r(text: &str) -> Requirement {
        Requirement::parse(text).unwrap_or_else(|| panic!("{text} is a requirement"))
    }

    #[test]
    fn a_minor_before_1_0_accepts_its_patches_only() {
        let req = r("0.5");
        for ok in ["0.5.0", "0.5.1", "0.5.99"] {
            assert!(req.matches(&v(ok)), "0.5 accepts {ok}");
        }
        for no in ["0.4.9", "0.6.0", "1.5.0", "0.6.0-rc.1"] {
            assert!(!req.matches(&v(no)), "0.5 rejects {no}");
        }
        // With a patch: that patch or a later one of the minor.
        let req = r("0.5.2");
        assert!(req.matches(&v("0.5.3")));
        assert!(!req.matches(&v("0.5.1")));
        assert!(!req.matches(&v("0.6.0")));
    }

    #[test]
    fn from_1_0_a_requirement_accepts_later_minors() {
        let req = r("1.4");
        assert!(req.matches(&v("1.4.0")));
        assert!(req.matches(&v("1.9.3")));
        assert!(!req.matches(&v("1.3.9")));
        assert!(!req.matches(&v("2.0.0")));
    }

    #[test]
    fn an_equals_sign_pins_exactly() {
        let req = r("=0.5.2");
        assert!(req.matches(&v("0.5.2")));
        assert!(!req.matches(&v("0.5.3")));
        assert!(!req.matches(&v("0.5.1")));
        let rc = r("=0.6.0-rc.1");
        assert!(rc.matches(&v("0.6.0-rc.1")));
        assert!(!rc.matches(&v("0.6.0")));
        assert!(!rc.matches(&v("0.6.0-rc.2")));
    }

    #[test]
    fn a_nightly_satisfies_its_own_minor_only() {
        let nightly = v("0.7.0-nightly.20261109");
        assert!(r("0.7").matches(&nightly));
        for no in ["0.6", "0.8", "=0.7.0", "0.7.1"] {
            assert!(!r(no).matches(&nightly), "{no} rejects the nightly");
        }
        // A release candidate likewise.
        assert!(r("0.6").matches(&v("0.6.0-rc.1")));
    }

    #[test]
    fn versions_order_by_semantic_precedence() {
        let mut list = [
            v("0.6.0"),
            v("0.6.0-rc.2"),
            v("0.5.10"),
            v("0.6.0-rc.10"),
            v("0.5.9"),
            v("0.7.0-nightly.20261109"),
        ];
        list.sort();
        let shown: Vec<String> = list.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "0.5.9",
                "0.5.10",
                "0.6.0-rc.2",
                "0.6.0-rc.10",
                "0.6.0",
                "0.7.0-nightly.20261109"
            ]
        );
    }

    #[test]
    fn malformed_requirements_are_rejected() {
        for bad in [
            "",
            " 0.5",
            "0.5 ",
            "v0.5",
            ">=0.5",
            "^0.5",
            "0.5.*",
            "0.5.2.1",
            "05",
            "0.05",
            "0.5-rc.1",
            "=0.5-rc.1",
            "0..5",
            "latest",
            "0.5,0.6",
        ] {
            assert!(
                Requirement::parse(bad).is_none(),
                "{bad:?} is not a requirement"
            );
        }
        for bad in ["0.5", "0.5.2.1", "0.5.2+build", "0.5.2-", "0.5.2-rc..1"] {
            assert!(Version::parse(bad).is_none(), "{bad:?} is not a version");
        }
    }

    #[test]
    fn requirements_intersect_when_one_version_satisfies_both() {
        assert!(r("0.5").intersects(&r("0.5.2")));
        assert!(r("0.5").intersects(&r("=0.5.3")));
        assert!(!r("0.5").intersects(&r("0.6")));
        assert!(!r("0.5").intersects(&r("0.4")));
        assert!(!r("=0.5.1").intersects(&r("0.5.2")));
        assert!(r("1.4").intersects(&r("1.9")));
    }

    fn workspace(root: &str, member: &str) -> Workspace {
        Workspace::from_files([
            ("k.toml".to_owned(), root.to_owned()),
            ("shop/k.toml".to_owned(), member.to_owned()),
            ("shop/App.k".to_owned(), "public app Shop {}\n".to_owned()),
        ])
    }

    const ROOT: &str = "[workspace]\nmembers = [\"shop\"]\nk = \"0.5\"\n";

    #[test]
    fn a_member_incompatible_with_the_workspace_requirement_is_an_error() {
        let ws = workspace(ROOT, "[package]\nname = \"shop\"\nk = \"0.4\"\n");
        let codes: Vec<_> = ws
            .problems
            .iter()
            .map(|p| (p.code, p.path.as_str()))
            .collect();
        assert_eq!(codes, [(codes::CONFLICTING_TOOLCHAIN, "shop/k.toml")]);
        assert!(
            ws.problems[0].message.contains("0.4") && ws.problems[0].message.contains("0.5"),
            "{:?}",
            ws.problems[0]
        );
        // A compatible one, or none at all, is fine: the root's applies.
        for member in [
            "[package]\nname = \"shop\"\nk = \"0.5.2\"\n",
            "[package]\nname = \"shop\"\n",
        ] {
            let ws = workspace(ROOT, member);
            assert!(ws.problems.is_empty(), "{member}: {:?}", ws.problems);
        }
        // Every requirement constrains the toolchain: the root's first.
        let shown = |ws: &Workspace| -> Vec<(String, String)> {
            ws.requirements()
                .iter()
                .map(|q| (q.path.clone(), q.requirement.as_str().to_owned()))
                .collect()
        };
        let ws = workspace(ROOT, "[package]\nname = \"shop\"\nk = \"0.5.2\"\n");
        assert_eq!(
            shown(&ws),
            [
                ("k.toml".to_owned(), "0.5".to_owned()),
                ("shop/k.toml".to_owned(), "0.5.2".to_owned())
            ]
        );
        let ws = workspace(ROOT, "[package]\nname = \"shop\"\n");
        assert_eq!(shown(&ws), [("k.toml".to_owned(), "0.5".to_owned())]);
    }

    #[test]
    fn a_member_without_a_requirement_needs_the_workspace_one() {
        let ws = workspace(
            "[workspace]\nmembers = [\"shop\"]\n",
            "[package]\nname = \"shop\"\n",
        );
        let codes: Vec<_> = ws.problems.iter().map(|p| p.code).collect();
        assert_eq!(codes, [codes::INVALID_MANIFEST]);
    }

    #[test]
    fn a_malformed_requirement_is_k8101() {
        for (root, member) in [
            (ROOT, "[package]\nname = \"shop\"\nk = \">=0.5\"\n"),
            (ROOT, "[package]\nname = \"shop\"\nk = 5\n"),
            (
                "[workspace]\nmembers = [\"shop\"]\nk = \"0.x\"\n",
                "[package]\nname = \"shop\"\nk = \"0.5\"\n",
            ),
        ] {
            let ws = workspace(root, member);
            let codes: Vec<_> = ws.problems.iter().map(|p| p.code).collect();
            assert_eq!(codes, [codes::INVALID_MANIFEST], "{root}{member}");
            assert!(
                ws.problems[0].message.contains("requirement"),
                "{:?}",
                ws.problems[0]
            );
        }
        let (_, problems) = k_sema::workspace::parse_manifest(
            "k.toml",
            "[package]\nname = \"shop\"\nk = \"0.5.x\"\n",
            true,
            false,
        );
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code, codes::INVALID_MANIFEST);
        // At the `k` line.
        assert_eq!(u32::from(problems[0].range.start()), 24);
    }

    #[test]
    fn a_project_outside_a_workspace_has_its_own_requirement() {
        let ws = Workspace::from_files([
            (
                "k.toml".to_owned(),
                "[package]\nname = \"shop\"\nk = \"0.5\"\n".to_owned(),
            ),
            ("App.k".to_owned(), "public app Shop {}\n".to_owned()),
        ]);
        assert!(ws.problems.is_empty(), "{:?}", ws.problems);
        let reqs = ws.requirements();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].path, "k.toml");
        assert_eq!(reqs[0].requirement.as_str(), "0.5");
    }
}
