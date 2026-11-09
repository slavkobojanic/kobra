//! The tooling queries of an [`Analysis`] that the language server builds
//! on (`docs/compiler/sema.md` §17): the project-wide reverse index
//! ([`Analysis::references`], [`Analysis::file_references`]), definitions
//! ([`Analysis::definition`]), implementors
//! ([`Analysis::implementations`]), recorded doc-link resolutions and page
//! URLs ([`Analysis::page_url`]).
//!
//! The property at the end checks, over the example apps and well-typed
//! generated projects, that every resolved reference with a definition
//! points at a declaration spelled exactly like the reference: what makes
//! rename and go-to-definition safe.

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::tgen;

use std::fs;
use std::path::{Path, PathBuf};

use arbitrary::Unstructured;
use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::view::Resolved;
use k_sema::{Analysis, AnalysisHost, Change, FileKind, Options, ProjectPath, Resolution};
use proptest::prelude::*;

const MANIFEST: &str = "[package]\nname = \"t\"\nk = \"0.1\"\n";

fn path(text: &str) -> ProjectPath {
    ProjectPath::new(text).unwrap()
}

fn host(files: &[(&str, &str)]) -> AnalysisHost {
    let mut all = vec![(path("k.toml"), MANIFEST.to_owned())];
    all.extend(files.iter().map(|(p, t)| (path(p), (*t).to_owned())));
    AnalysisHost::with_files(Options::new(), all)
}

/// The span of the `nth` occurrence (0-based) of `needle` in `file`.
fn find(analysis: &Analysis, file: &str, needle: &str, nth: usize) -> Span {
    let id = analysis.file_id(&path(file)).unwrap();
    let text = analysis.text(id).unwrap();
    let start = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no occurrence {nth} of {needle:?} in {file}"))
        .0;
    let start = u32::try_from(start).unwrap();
    let end = start + u32::try_from(needle.len()).unwrap();
    Span::new(id, TextRange::new(start.into(), end.into()))
}

const SHAPE: &str = "\
/// Anything with an area. See [`Square`] and [`Shape.area`].
public interface Shape {
    /// The area.
    func area() -> Double
}
";

const COMPOSITE: &str = "\
/// A shape made of shapes.
public interface Composite implements Shape {
}
";

const SQUARE: &str = "\
/// A square.
public struct Square implements Shape {
    /// The side.
    let side: Double

    /// The area.
    func area() -> Double {
        return side * side
    }
}
";

const GROUP: &str = "\
/// A group.
public struct Group implements Composite {
    /// The area.
    func area() -> Double {
        return 0.0
    }
}
";

/// The `geo` module, plus `extra` files.
fn geo<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut files = vec![
        ("src/geo/Shape.k", SHAPE),
        ("src/geo/Composite.k", COMPOSITE),
        ("src/geo/Square.k", SQUARE),
        ("src/geo/Group.k", GROUP),
    ];
    files.extend_from_slice(extra);
    files
}

const USES: &str = "\
import geo.Square
import geo.Shape

func total(shapes: List<Shape>) -> Double {
    var sum = 0.0
    for shape in shapes {
        sum += shape.area()
    }
    return sum
}

func unit() -> Square {
    let square = Square(side: 1.0)
    return square
}
";

#[test]
fn references_span_files_imports_and_doc_links() {
    let host = host(&geo(&[("src/use/Uses.k", USES)]));
    let analysis = host.analysis();
    assert!(
        analysis
            .diagnostics()
            .unwrap()
            .iter()
            .all(|d| !d.is_error()),
        "{:#?}",
        analysis.diagnostics().unwrap()
    );
    let square = Resolution::Declaration {
        name: "geo.Square".to_owned(),
    };
    let refs = analysis.references(&square).unwrap();
    let expected = [
        // The doc link of `Shape`.
        find(&analysis, "src/geo/Shape.k", "Square", 0),
        find(&analysis, "src/use/Uses.k", "Square", 0),
        find(&analysis, "src/use/Uses.k", "Square", 1),
        find(&analysis, "src/use/Uses.k", "Square", 2),
    ];
    for span in expected {
        assert!(refs.contains(&span), "{span:?} missing from {refs:?}");
    }
    let definition = analysis.definition(&square).unwrap().unwrap();
    assert_eq!(definition, find(&analysis, "src/geo/Square.k", "Square", 0));
}

#[test]
fn doc_links_resolve_each_segment() {
    let host = host(&geo(&[]));
    let analysis = host.analysis();
    let shape = find(&analysis, "src/geo/Shape.k", "Shape.area", 0);
    let owner = Span::new(shape.file, TextRange::at(shape.range.start(), 5.into()));
    let member = Span::new(
        shape.file,
        TextRange::new(shape.range.start() + TextSize::from(6), shape.range.end()),
    );
    assert_eq!(
        analysis.resolve_at(owner.file, owner.range).unwrap(),
        Some(Resolution::Declaration {
            name: "geo.Shape".to_owned()
        })
    );
    assert!(matches!(
        analysis.resolve_at(member.file, member.range).unwrap(),
        Some(Resolution::Member { owner, name }) if owner == "geo.Shape" && name == "area()"
    ));
}

#[test]
fn file_references_are_sorted_and_find_locals() {
    let host = host(&geo(&[("src/use/Uses.k", USES)]));
    let analysis = host.analysis();
    let file = analysis.file_id(&path("src/use/Uses.k")).unwrap();
    let refs = analysis.file_references(file).unwrap();
    assert!(refs.windows(2).all(|w| w[0].0.start() <= w[1].0.start()));
    // `square` in `return square` is a local bound by `let square`.
    let use_site = find(&analysis, "src/use/Uses.k", "square", 1);
    let (_, resolution) = refs
        .iter()
        .find(|(r, _)| *r == use_site.range)
        .expect("the local is resolved");
    let definition = analysis.definition(resolution).unwrap().unwrap();
    assert_eq!(definition, find(&analysis, "src/use/Uses.k", "square", 0));
    // Parameters too.
    let shapes = find(&analysis, "src/use/Uses.k", "shapes", 1);
    let (_, param) = refs.iter().find(|(r, _)| *r == shapes.range).unwrap();
    assert!(matches!(&**param, Resolution::Parameter { name, .. } if name == "shapes"));
    assert_eq!(
        analysis.definition(param).unwrap(),
        Some(find(&analysis, "src/use/Uses.k", "shapes", 0))
    );
}

#[test]
fn implementations_are_transitive() {
    let host = host(&geo(&[]));
    let analysis = host.analysis();
    let program = analysis.program().unwrap();
    let shape = program.lookup("geo.Shape").unwrap();
    let mut names: Vec<String> = analysis
        .implementations(shape)
        .unwrap()
        .into_iter()
        .map(|d| program.qualified_name(d))
        .collect();
    names.sort();
    assert_eq!(names, ["geo.Composite", "geo.Group", "geo.Square"]);
    let square = program.lookup("geo.Square").unwrap();
    assert!(analysis.implementations(square).unwrap().is_empty());
}

#[test]
fn page_urls_follow_the_routing_rules() {
    let host = host(&[
        (
            "src/web/Pages.k",
            "\
public page Home at \"/\" {
    view { text { \"home\" } }
}

public page SignIn {
    view { text { \"sign in\" } }
}

public page Product {
    @Param let id: Int
    @Param let tab: String? = nil

    view { text { \"product\" } }
}

public page Order at \"/orders/{id}\" {
    @Param let id: Int

    view { text { \"order\" } }
}
",
        ),
        ("src/web/Other.k", "public struct NotAPage {\n}\n"),
    ]);
    let analysis = host.analysis();
    let program = analysis.program().unwrap();
    let url = |name: &str| analysis.page_url(program.lookup(name).unwrap()).unwrap();
    assert_eq!(url("web.Home").as_deref(), Some("/"));
    assert_eq!(url("web.SignIn").as_deref(), Some("/sign-in"));
    assert_eq!(url("web.Product").as_deref(), Some("/product/{id}"));
    assert_eq!(url("web.Order").as_deref(), Some("/orders/{id}"));
    assert_eq!(url("web.NotAPage"), None);
}

#[test]
fn the_index_follows_edits() {
    let mut host = host(&geo(&[("src/use/Uses.k", USES)]));
    let square = Resolution::Declaration {
        name: "geo.Square".to_owned(),
    };
    let before = host.analysis().references(&square).unwrap().len();
    let mut change = Change::new();
    change.set_file(
        path("src/use/Uses.k"),
        USES.replace(
            "    return sum\n",
            "    let extra = Square(side: 2.0)\n    return sum + extra.side\n",
        ),
    );
    host.apply_change(change);
    let after = host.analysis().references(&square).unwrap().len();
    assert_eq!(after, before + 1);
}

#[test]
fn stale_snapshots_are_cancelled() {
    let mut host = host(&geo(&[]));
    let old = host.analysis();
    let mut change = Change::new();
    change.set_file(path("src/geo/Shape.k"), format!("{SHAPE}\n"));
    host.apply_change(change);
    let square = Resolution::Declaration {
        name: "geo.Square".to_owned(),
    };
    assert!(old.references(&square).is_err());
    assert!(old.file_references(FileId::new(0)).is_err());
    assert!(old.definition(&square).is_err());
    assert!(old.page_url(k_sema::view::DeclId(0)).is_err());
    assert!(old.implementations(k_sema::view::DeclId(0)).is_err());
}

// ---------------------------------------------------------------- property

fn examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("k.toml").is_file())
        .collect();
    found.sort();
    found
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !name.starts_with('.') && name != "dist" && name != "target" {
                collect(&p, root, out);
            }
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(pp) = ProjectPath::new(&rel) else {
            continue;
        };
        // A workspace member's manifest and lockfiles are classified
        // relative to the member, by `Workspace::from_files`.
        let member_file = name == "k.toml" || name.ends_with(".lock");
        if pp.kind() != FileKind::Other || member_file {
            out.push((pp, fs::read_to_string(&p).unwrap()));
        }
    }
}

/// Checks the property on one analysis: every reference whose definition
/// is known names a declaration spelled like the reference, and the
/// definition is in a file of the analysis. Returns how many references
/// were checked.
fn check_definitions(analysis: &Analysis) -> usize {
    let mut checked = 0;
    for (file, project_path) in analysis.files() {
        let Some(text) = analysis.text(file) else {
            continue;
        };
        let refs = analysis.file_references(file).unwrap();
        for (range, resolution) in refs {
            let Some(definition) = analysis.definition(&resolution).unwrap() else {
                continue;
            };
            let reference = &text[usize::from(range.start())..usize::from(range.end())];
            let Some(def_text) = analysis.text(definition.file) else {
                panic!("{resolution:?} is defined in an unknown file");
            };
            let Some(name) = def_text
                .get(usize::from(definition.range.start())..usize::from(definition.range.end()))
            else {
                panic!("{resolution:?} has a definition outside its file");
            };
            // A page case (`.signIn` of `Page` or `Endpoint`) is defined by
            // its page (`SignIn`): the one kind of reference spelled unlike
            // its definition.
            if matches!(&*resolution, Resolution::Member { owner, .. } if owner == "Page" || owner == "Endpoint")
                && reference != name
            {
                let mut chars = name.chars();
                let lowered: String = chars
                    .next()
                    .map(|c| c.to_ascii_lowercase())
                    .into_iter()
                    .chain(chars)
                    .collect();
                assert_eq!(reference, lowered, "a page case is its page's name");
                continue;
            }
            assert_eq!(
                reference, name,
                "{project_path} {range:?}: `{reference}` resolves to {resolution:?}, defined as `{name}`"
            );
            checked += 1;
        }
    }
    checked
}

/// The analyses of the example at `dir`: one, or for a workspace
/// (`examples/shop`, `examples/stack`) one per member with the libraries it
/// depends on.
fn example_analyses(dir: &Path) -> Vec<Analysis> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    let manifest = fs::read_to_string(dir.join("k.toml")).unwrap_or_default();
    if !k_sema::Workspace::is_workspace_manifest(&manifest) {
        return vec![AnalysisHost::with_files(Options::new(), files).analysis()];
    }
    let ws =
        k_sema::Workspace::from_files(files.into_iter().map(|(p, t)| (p.as_str().to_owned(), t)));
    assert!(
        ws.problems.is_empty(),
        "{}: {:?}",
        dir.display(),
        ws.problems
    );
    (0..ws.members.len())
        .map(|m| AnalysisHost::with_files(Options::new(), ws.analysis_files(m)).analysis())
        .collect()
}

#[test]
fn every_definition_of_the_examples_is_spelled_like_its_references() {
    for dir in examples() {
        let mut checked = 0;
        for analysis in example_analyses(&dir) {
            checked += check_example_definitions(&analysis);
        }
        assert!(
            checked > 100,
            "{}: only {checked} references",
            dir.display()
        );
    }
}

/// [`check_definitions`] on one analysis of an example, and every
/// declaration resolving back to itself. Returns how many references were
/// checked.
fn check_example_definitions(analysis: &Analysis) -> usize {
    let checked = check_definitions(analysis);
    // Every declaration resolves back to itself, except a test suite named
    // like the declaration it tests.
    let program = analysis.program().unwrap();
    for (i, decl) in program.decls.iter().enumerate() {
        if decl.stdlib
            || decl.name.is_empty()
            || decl.named_kind() == Some(k_sema::stdlib::DeclarationKind::TestSuite)
        {
            continue;
        }
        let id = k_sema::view::DeclId(u32::try_from(i).unwrap());
        let resolution = Resolution::Declaration {
            name: decl.display_name.clone(),
        };
        assert_eq!(program.resolve(&resolution), Some(Resolved::Decl(id)));
    }
    checked
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn every_definition_of_generated_projects_is_spelled_like_its_references(
        raw in prop::collection::vec(any::<u8>(), 0..4096)
    ) {
        let program = tgen::generate(&mut Unstructured::new(&raw));
        let files = program
            .files
            .iter()
            .map(|(p, t)| (path(p), t.clone()));
        let host = AnalysisHost::with_files(Options::new(), files);
        check_definitions(&host.analysis());
    }
}
