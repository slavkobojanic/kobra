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
