//! Unit tests of the features on small fixture projects. A fixture is a
//! list of files; `$0` in one of them marks the cursor and is removed.

#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::{FileId, Span, TextRange, TextSize};
use k_sema::{Analysis, AnalysisHost, Options, ProjectPath};

use super::completion::{ItemKind, completions};
use super::navigation::{definition, highlights, implementation, references, type_definition};
use super::reactive::Dependencies;
use super::{hover, inlay, rename, signature_help, symbol_at, symbols};

const MANIFEST: &str = "[package]\nname = \"t\"\nk = \"0.1\"\n";

const SHAPE: &str = "\
/// Anything with an area. See [`Square`].
public interface Shape {
    /// The area.
    func area() -> Double
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

const CIRCLE: &str = "\
/// A circle.
public struct Circle implements Shape {
    /// The radius.
    let radius: Double

    /// The area.
    func area() -> Double {
        return radius * radius * 3.14
    }
}
";

const COUNTER: &str = "\
import k.observe.Log

/// Counts clicks.
public component Counter {
    /// The current count.
    @State var count: Int = 0

    /// Twice the count.
    @Derived let doubled: Int = count * 2

    /// Logs every change.
    @Effect
    func logChanges() {
        Log.info(\"count is {count}\")
    }

    func increment() {
        count += 1
    }

    view {
        button { \"{doubled}\" }
            .on(click: { increment() })
    }
}
";

const THEME: &str = "\
/// The tokens.
public theme AppTheme {
    color primary: Color = #2563EB

    space sm: Spacing = 8px
    space md: Spacing = 16px
}
";

const SIGN_IN: &str = "\
/// The sign-in page.
public page SignIn {
    view {
        text { \"sign in\" }
    }
}
";

/// A fixture: the analysis, the file with the cursor and the cursor.
struct Fixture {
    host: AnalysisHost,
    file: FileId,
    offset: TextSize,
}

impl Fixture {
    fn analysis(&self) -> Analysis {
        self.host.analysis()
    }

    fn id(&self, path: &str) -> FileId {
        self.analysis()
            .file_id(&ProjectPath::new(path).unwrap())
            .unwrap()
    }

    /// The span of the `nth` occurrence of `needle` in `path`.
    fn span(&self, path: &str, needle: &str, nth: usize) -> Span {
        let analysis = self.analysis();
        let id = self.id(path);
        let text = analysis.text(id).unwrap();
        let start = text
            .match_indices(needle)
            .nth(nth)
            .unwrap_or_else(|| panic!("{needle} {nth}"))
            .0;
        Span::new(
            id,
            TextRange::at(
                TextSize::from(u32::try_from(start).unwrap()),
                TextSize::from(u32::try_from(needle.len()).unwrap()),
            ),
        )
    }
}

/// Builds a fixture from `files` (paths and texts), the geometry module
/// included; exactly one file holds `$0`.
fn fixture(files: &[(&str, &str)]) -> Fixture {
    let mut all: Vec<(ProjectPath, String)> =
        vec![(ProjectPath::new("k.toml").unwrap(), MANIFEST.to_owned())];
    let mut cursor = None;
    let base = [
        ("src/geo/Shape.k", SHAPE),
        ("src/geo/Square.k", SQUARE),
        ("src/geo/Circle.k", CIRCLE),
        ("src/ui/Counter.k", COUNTER),
        ("src/web/SignIn.k", SIGN_IN),
        ("src/ui/AppTheme.k", THEME),
    ];
    for (path, text) in base
        .iter()
        .filter(|(p, _)| !files.iter().any(|(q, _)| q == p))
        .chain(files)
    {
        let text = match text.find("$0") {
            Some(i) => {
                cursor = Some(((*path).to_owned(), i));
                text.replacen("$0", "", 1)
            }
            None => (*text).to_owned(),
        };
        all.push((ProjectPath::new(path).unwrap(), text));
    }
    let host = AnalysisHost::with_files(Options::new(), all);
    let (path, offset) = cursor.unwrap_or_else(|| ("src/geo/Shape.k".to_owned(), 0));
    let file = host
        .analysis()
        .file_id(&ProjectPath::new(&path).unwrap())
        .unwrap();
    Fixture {
        host,
        file,
        offset: TextSize::from(u32::try_from(offset).unwrap()),
    }
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

fn with_cursor(text: &str, needle: &str, nth: usize, shift: usize) -> String {
    let at = text.match_indices(needle).nth(nth).unwrap().0 + shift;
    format!("{}$0{}", &text[..at], &text[at..])
}

#[test]
fn the_fixture_has_no_errors() {
    let f = fixture(&[("src/use/Uses.k", USES)]);
    let errors: Vec<_> = f
        .analysis()
        .diagnostics()
        .unwrap()
        .into_iter()
        .filter(|d| d.is_error())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn symbols_under_the_cursor() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "square", 1, 2))]);
    let symbol = symbol_at(&f.analysis(), f.file, f.offset).unwrap().unwrap();
    assert!(!symbol.is_definition);
    assert!(
        matches!(symbol.resolution, k_sema::Resolution::Local { ref name, .. } if name == "square")
    );
    // At the end of a name.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "square", 1, 6))]);
    assert!(
        symbol_at(&f.analysis(), f.file, f.offset)
            .unwrap()
            .is_some()
    );
    // On a declaration's name.
    let f = fixture(&[("src/geo/Square.k", &with_cursor(SQUARE, "Square", 0, 1))]);
    let symbol = symbol_at(&f.analysis(), f.file, f.offset).unwrap().unwrap();
    assert!(symbol.is_definition);
    // On whitespace.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "\n\nfunc", 0, 1))]);
    assert_eq!(symbol_at(&f.analysis(), f.file, f.offset).unwrap(), None);
    // Out of range.
    assert_eq!(
        symbol_at(&f.analysis(), f.file, TextSize::from(1 << 20)).unwrap(),
        None
    );
}

#[test]
fn go_to_definition_across_files() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Square(side", 0, 2))]);
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Square.k", "Square", 0)]);
    // A member through an interface-typed receiver.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "area()", 0, 1))]);
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Shape.k", "area", 2)]);
    // A doc link.
    let f = fixture(&[("src/geo/Shape.k", &with_cursor(SHAPE, "Square", 0, 1))]);
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Square.k", "Square", 0)]);
    // An import.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Square", 0, 0))]);
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Square.k", "Square", 0)]);
}

#[test]
fn go_to_type_definition() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "square", 1, 1))]);
    let defs = type_definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Square.k", "Square", 0)]);
    // `shapes: List<Shape>` goes to `Shape`.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "shapes", 1, 1))]);
    let defs = type_definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/geo/Shape.k", "Shape", 0)]);
}

#[test]
fn go_to_implementation_of_interfaces_and_their_methods() {
    let f = fixture(&[("src/geo/Shape.k", &with_cursor(SHAPE, "Shape {", 0, 1))]);
    let mut impls = implementation(&f.analysis(), f.file, f.offset).unwrap();
    impls.sort();
    let mut expected = vec![
        f.span("src/geo/Square.k", "Square", 0),
        f.span("src/geo/Circle.k", "Circle", 0),
    ];
    expected.sort();
    assert_eq!(impls, expected);
    let f = fixture(&[("src/geo/Shape.k", &with_cursor(SHAPE, "area()", 0, 0))]);
    let mut impls = implementation(&f.analysis(), f.file, f.offset).unwrap();
    impls.sort();
    let mut expected = vec![
        f.span("src/geo/Square.k", "area", 1),
        f.span("src/geo/Circle.k", "area", 1),
    ];
    expected.sort();
    assert_eq!(impls, expected);
}

#[test]
fn references_and_highlights() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Square(side", 0, 0))]);
    let refs = references(&f.analysis(), f.file, f.offset, true).unwrap();
    assert_eq!(
        refs[0],
        f.span("src/geo/Square.k", "Square", 0),
        "the declaration first"
    );
    for span in [
        f.span("src/geo/Shape.k", "Square", 0),
        f.span("src/use/Uses.k", "Square", 0),
        f.span("src/use/Uses.k", "Square", 1),
        f.span("src/use/Uses.k", "Square", 2),
    ] {
        assert!(refs.contains(&span), "{span:?}");
    }
    let without = references(&f.analysis(), f.file, f.offset, false).unwrap();
    assert_eq!(without.len(), refs.len() - 1);
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "sum", 0, 0))]);
    let hl = highlights(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(hl.len(), 3, "{hl:?}");
}

/// Calls `area` through the concrete type, not the interface.
const CONCRETE: &str = "\
import geo.Square

func squareArea(square: Square) -> Double {
    return square.area()
}
";

#[test]
fn references_of_an_interface_method_include_implementations_and_concrete_calls() {
    // From the interface method, like rename: the implementing methods,
    // the call through the interface and the call through `Square`.
    let f = fixture(&[
        ("src/geo/Shape.k", &with_cursor(SHAPE, "area()", 0, 0)),
        ("src/use/Uses.k", USES),
        ("src/use/Concrete.k", CONCRETE),
    ]);
    let refs = references(&f.analysis(), f.file, f.offset, false).unwrap();
    let expected = [
        f.span("src/geo/Square.k", "area", 1),
        f.span("src/geo/Circle.k", "area", 1),
        f.span("src/use/Uses.k", "area", 0),
        f.span("src/use/Concrete.k", "area", 0),
    ];
    for span in expected {
        assert!(refs.contains(&span), "{span:?} in {refs:?}");
    }
    assert!(
        !refs.contains(&f.span("src/geo/Shape.k", "area", 2)),
        "no declaration"
    );
    let with = references(&f.analysis(), f.file, f.offset, true).unwrap();
    assert_eq!(
        with[0],
        f.span("src/geo/Shape.k", "area", 2),
        "the declaration first"
    );
    assert_eq!(with.len(), refs.len() + 1);
    // From an implementing method: the same family.
    let f = fixture(&[
        ("src/geo/Square.k", &with_cursor(SQUARE, "area()", 0, 0)),
        ("src/use/Uses.k", USES),
        ("src/use/Concrete.k", CONCRETE),
    ]);
    let refs = references(&f.analysis(), f.file, f.offset, true).unwrap();
    assert_eq!(
        refs[0],
        f.span("src/geo/Square.k", "area", 1),
        "its declaration first"
    );
    for span in [
        f.span("src/geo/Shape.k", "area", 2),
        f.span("src/geo/Circle.k", "area", 1),
        f.span("src/use/Uses.k", "area", 0),
        f.span("src/use/Concrete.k", "area", 0),
    ] {
        assert!(refs.contains(&span), "{span:?} in {refs:?}");
    }
    // No duplicates.
    let mut sorted = refs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), refs.len());
}

#[test]
fn rename_covers_imports_doc_links_and_files() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Square(side", 0, 0))]);
    let analysis = f.analysis();
    let (range, name) = rename::prepare(&analysis, f.file, f.offset)
        .unwrap()
        .unwrap();
    assert_eq!(name, "Square");
    assert_eq!(range, f.span("src/use/Uses.k", "Square", 2).range);
    let edits = rename::rename(&analysis, f.file, f.offset, "Box")
        .unwrap()
        .unwrap();
    assert_eq!(edits.edits.len(), 5, "{edits:?}");
    assert!(edits.edits.iter().all(|(_, t)| t == "Box"));
    assert_eq!(
        edits.file_renames,
        [(f.id("src/geo/Square.k"), "Box.k".to_owned())]
    );
}

#[test]
fn rename_of_an_interface_method_renames_its_implementations() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "area()", 0, 0))]);
    let edits = rename::rename(&f.analysis(), f.file, f.offset, "surface")
        .unwrap()
        .unwrap();
    let spans: Vec<Span> = edits.edits.iter().map(|(s, _)| *s).collect();
    for span in [
        f.span("src/geo/Shape.k", "area", 2),
        f.span("src/geo/Square.k", "area", 1),
        f.span("src/geo/Circle.k", "area", 1),
        f.span("src/use/Uses.k", "area", 0),
    ] {
        assert!(spans.contains(&span), "{span:?} in {spans:?}");
    }
}

#[test]
fn rename_rejects_bad_names_and_stdlib_symbols() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "square", 1, 0))]);
    let analysis = f.analysis();
    for bad in ["", "1x", "a-b", "match", "_", "é"] {
        assert!(
            rename::rename(&analysis, f.file, f.offset, bad)
                .unwrap()
                .is_err(),
            "{bad}"
        );
    }
    // `List` is a stdlib declaration.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "List", 0, 1))]);
    assert!(
        rename::prepare(&f.analysis(), f.file, f.offset)
            .unwrap()
            .is_err()
    );
    // A built-in element.
    let f = fixture(&[("src/ui/Counter.k", &with_cursor(COUNTER, "button", 0, 1))]);
    assert!(
        rename::prepare(&f.analysis(), f.file, f.offset)
            .unwrap()
            .is_err()
    );
    // A local renames its binding and its uses.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "square", 1, 0))]);
    let edits = rename::rename(&f.analysis(), f.file, f.offset, "box")
        .unwrap()
        .unwrap();
    assert_eq!(edits.edits.len(), 2);
}

#[test]
fn rename_keeps_implicit_labels() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "shapes", 1, 0))]);
    let edits = rename::rename(&f.analysis(), f.file, f.offset, "all")
        .unwrap()
        .unwrap();
    let texts: Vec<&str> = edits.edits.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(texts, ["shapes all", "all"]);
}

#[test]
fn rename_of_a_page_renames_its_cases() {
    let link = "\
public component Nav {
    view {
        link(to: .signIn) { \"Sign in\" }
    }
}
";
    let f = fixture(&[
        ("src/web/SignIn.k", &with_cursor(SIGN_IN, "SignIn", 0, 0)),
        ("src/web/Nav.k", link),
    ]);
    assert!(
        f.analysis()
            .diagnostics()
            .unwrap()
            .iter()
            .all(|d| !d.is_error()),
        "{:#?}",
        f.analysis().diagnostics().unwrap()
    );
    let edits = rename::rename(&f.analysis(), f.file, f.offset, "LogIn")
        .unwrap()
        .unwrap();
    let texts: Vec<&str> = edits.edits.iter().map(|(_, t)| t.as_str()).collect();
    assert!(texts.contains(&"logIn"), "{texts:?}");
    assert!(texts.contains(&"LogIn"), "{texts:?}");
}

#[test]
fn hover_shows_signatures_docs_and_links() {
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Square(side", 0, 1))]);
    let link = |s: Span| {
        Some(format!(
            "file:///{}#L{}",
            s.file.raw(),
            u32::from(s.range.start())
        ))
    };
    let h = hover::hover(&f.analysis(), f.file, f.offset, &link)
        .unwrap()
        .unwrap();
    assert!(
        h.markdown
            .contains("public struct geo.Square implements geo.Shape"),
        "{}",
        h.markdown
    );
    assert!(h.markdown.contains("A square."), "{}", h.markdown);
    // The doc link of `Shape` is rendered as a link.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "Shape", 1, 1))]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &link)
        .unwrap()
        .unwrap();
    assert!(h.markdown.contains("[`Square`](file:///"), "{}", h.markdown);
    // A local's type.
    let f = fixture(&[("src/use/Uses.k", &with_cursor(USES, "sum", 1, 1))]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &link)
        .unwrap()
        .unwrap();
    assert!(h.markdown.contains("let sum: Double"), "{}", h.markdown);
}

#[test]
fn hover_shows_reactive_dependencies() {
    let f = fixture(&[("src/ui/Counter.k", &with_cursor(COUNTER, "doubled", 0, 1))]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(
        h.markdown.contains("Reactive dependencies: `count`"),
        "{}",
        h.markdown
    );
    let f = fixture(&[(
        "src/ui/Counter.k",
        &with_cursor(COUNTER, "logChanges", 0, 1),
    )]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(
        h.markdown.contains("Reactive dependencies: `count`"),
        "{}",
        h.markdown
    );
    let explicit = COUNTER.replace("@Effect\n", "@Effect(on: [doubled])\n");
    let f = fixture(&[(
        "src/ui/Counter.k",
        &with_cursor(&explicit, "logChanges", 0, 1),
    )]);
    let view_ok = f
        .analysis()
        .diagnostics()
        .unwrap()
        .iter()
        .all(|d| !d.is_error());
    assert!(view_ok, "{:#?}", f.analysis().diagnostics().unwrap());
    let analysis = f.analysis();
    let program = analysis.program().unwrap();
    let m = program
        .members
        .iter()
        .position(|m| m.name == "logChanges")
        .unwrap();
    let deps = super::reactive::dependencies(
        &analysis,
        program,
        k_sema::view::MemberId(u32::try_from(m).unwrap()),
    )
    .unwrap();
    assert_eq!(
        deps,
        Some(Dependencies::Explicit(vec!["doubled".to_owned()]))
    );
}

#[test]
fn hover_shows_page_urls() {
    let f = fixture(&[("src/web/SignIn.k", &with_cursor(SIGN_IN, "SignIn", 0, 1))]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(h.markdown.contains("URL: `/sign-in`"), "{}", h.markdown);
    let hints = inlay::inlay_hints(
        &f.analysis(),
        f.file,
        TextRange::up_to(TextSize::from(1000)),
    )
    .unwrap();
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].label, "at \"/sign-in\"");
}

const DRAFT: &str = "\
/// The draft, from local storage.
@Rendering(.client)
public component Draft {
    view {
        text { \"draft\" }
    }

    loading {
        text { \"…\" }
    }
}
";

// 47-client-rendered-components.md, "Interactions": hover over a
// client-rendered component's name says where it renders.
#[test]
fn hover_says_a_client_rendered_component_renders_in_the_browser() {
    let f = fixture(&[("src/web/Draft.k", &with_cursor(DRAFT, "Draft {", 0, 1))]);
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(
        h.markdown
            .contains("Rendered in the browser only; the server renders `loading`"),
        "{}",
        h.markdown
    );
    assert!(
        h.markdown.contains("The draft, from local storage."),
        "{}",
        h.markdown
    );
}

#[test]
fn outline_and_workspace_symbols() {
    let f = fixture(&[]);
    let analysis = f.analysis();
    let outline = symbols::document_symbols(&analysis, f.id("src/ui/Counter.k")).unwrap();
    assert_eq!(outline.len(), 1);
    let names: Vec<&str> = outline[0]
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["count", "doubled", "logChanges", "increment"]);
    for symbol in &outline {
        assert!(symbol.range.contains_range(symbol.selection));
        for child in &symbol.children {
            assert!(child.range.contains_range(child.selection));
        }
    }
    let found = symbols::workspace_symbols(&analysis, "sq", 10).unwrap();
    assert_eq!(found[0].name, "Square");
    let found = symbols::workspace_symbols(&analysis, "incr", 10).unwrap();
    assert_eq!(found[0].name, "increment");
    assert_eq!(found[0].container.as_deref(), Some("ui.Counter"));
}

fn labels(items: &[super::completion::CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn completion_of_members() {
    let text = USES.replace("    return square\n", "    return square.$0\n");
    let f = fixture(&[("src/use/Uses.k", &text)]);
    let items = completions(&f.analysis(), f.file, f.offset).unwrap();
    let names = labels(&items);
    assert!(
        names.contains(&"side") && names.contains(&"area"),
        "{names:?}"
    );
    // With a prefix.
    let text = USES.replace("    return square\n", "    return square.si$0\n");
    let f = fixture(&[("src/use/Uses.k", &text)]);
    let names = labels(&completions(&f.analysis(), f.file, f.offset).unwrap()).join(",");
    assert_eq!(names, "side");
}

#[test]
fn completion_of_implicit_members() {
    let filter = "\
public enum Filter {
    case all
    case active
    case done
}
";
    let text = "\
import todo.Filter

func pick() -> Filter {
    var filter: Filter = .all
    filter = .$0
    return filter
}
";
    let f = fixture(&[("src/todo/Filter.k", filter), ("src/use/Pick.k", text)]);
    let items = completions(&f.analysis(), f.file, f.offset).unwrap();
    let names = labels(&items);
    assert!(
        names.contains(&"active") && names.contains(&"done"),
        "{names:?}"
    );
    assert!(
        items.iter().all(|i| i.kind == ItemKind::EnumMember),
        "{items:?}"
    );
    // In a style block, the property's token category.
    let style = COUNTER.replace(
        "    view {",
        "    style {\n        .box {\n            padding: .$0\n        }\n    }\n\n    view {",
    );
    let f = fixture(&[("src/ui/Counter.k", &style)]);
    let items = completions(&f.analysis(), f.file, f.offset).unwrap();
    let names = labels(&items);
    assert!(names.contains(&"md"), "{names:?}");
}

#[test]
fn completion_of_names_in_scope() {
    let text = USES.replace("    return sum\n", "    return s$0\n");
    let f = fixture(&[("src/use/Uses.k", &text)]);
    let items = completions(&f.analysis(), f.file, f.offset).unwrap();
    let names = labels(&items);
    assert!(
        names.contains(&"sum") && names.contains(&"shapes"),
        "{names:?}"
    );
    assert!(names.contains(&"Square"), "an import");
    assert!(
        !names.contains(&"square"),
        "a local of another function: {names:?}"
    );
    // Other modules' public declarations come with an import.
    let counter = items.iter().find(|i| i.label == "SignIn");
    assert!(
        counter.is_some_and(|i| i
            .import
            .as_ref()
            .is_some_and(|(_, t)| t == "import web.SignIn\n")),
        "{counter:?}"
    );
}

#[test]
fn completion_of_annotations() {
    let text = COUNTER.replace("    func increment", "    @Sta$0\n    func increment");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let names = labels(&completions(&f.analysis(), f.file, f.offset).unwrap()).join(",");
    assert!(names.contains("State"), "{names}");
}

#[test]
fn signature_help_of_calls() {
    let text = USES.replace("Square(side: 1.0)", "Square(side: $0");
    let f = fixture(&[("src/use/Uses.k", &text)]);
    let help = signature_help::signature_help(&f.analysis(), f.file, f.offset).unwrap();
    let help = help.expect("signature help in an unclosed call");
    assert_eq!(help.signatures[0].label, "Square(side: Double)");
    assert_eq!(help.active_parameter, 0);
}

#[test]
fn half_typed_code_never_panics() {
    let analysis_texts = [
        "func f() {\n    let x = \n}\n",
        "func f() -> Int {\n    return foo.\n}\n",
        "import \nstruct",
        "@",
        ".",
        "func f(a: Int, b",
        "page P at \"/{",
        "component C {\n    view {\n        button(",
    ];
    for text in analysis_texts {
        let f = fixture(&[("src/z/Z.k", text)]);
        let analysis = f.analysis();
        let len = u32::try_from(text.len()).unwrap();
        for offset in 0..=len + 2 {
            let offset = TextSize::from(offset);
            let _ = completions(&analysis, f.file, offset).unwrap();
            let _ = hover::hover(&analysis, f.file, offset, &|_| None).unwrap();
            let _ = definition(&analysis, f.file, offset).unwrap();
            let _ = signature_help::signature_help(&analysis, f.file, offset).unwrap();
            let _ = rename::prepare(&analysis, f.file, offset).unwrap();
        }
    }
}

// ----------------------------------------------------------- the kanban app
//
// Completion, hover and navigation in style blocks and `.on(…)` calls,
// against `examples/kanban` with `src/pages/ColumnView.k` edited.

const COLUMN_VIEW: &str = "src/pages/ColumnView.k";

/// The kanban example, with `ColumnView.k` replaced by `edit` of it (which
/// holds `$0`).
fn kanban(edit: impl Fn(&str) -> String) -> Fixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kanban");
    let mut files: Vec<(String, String)> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                if rel == "src" || rel.starts_with("src/") {
                    stack.push(path);
                }
            } else if rel.ends_with(".k") || rel == "k.toml" || rel.ends_with(".lock") {
                let text = std::fs::read_to_string(&path).unwrap();
                let text = if rel == COLUMN_VIEW {
                    edit(&text)
                } else {
                    text
                };
                files.push((rel, text));
            }
        }
    }
    let mut cursor = None;
    let mut all = Vec::new();
    for (path, text) in files {
        let text = match text.find("$0") {
            Some(i) => {
                cursor = Some((path.clone(), i));
                text.replacen("$0", "", 1)
            }
            None => text,
        };
        all.push((ProjectPath::new(&path).unwrap(), text));
    }
    let host = AnalysisHost::with_files(Options::new(), all);
    let (path, offset) = cursor.unwrap_or_else(|| (COLUMN_VIEW.to_owned(), 0));
    let file = host
        .analysis()
        .file_id(&ProjectPath::new(&path).unwrap())
        .unwrap();
    Fixture {
        host,
        file,
        offset: TextSize::from(u32::try_from(offset).unwrap()),
    }
}

/// `ColumnView.k` with `old` replaced by `new` (which holds `$0`).
fn column_view(old: &'static str, new: &'static str) -> Fixture {
    kanban(move |text| {
        assert!(text.contains(old), "{old}");
        text.replacen(old, new, 1)
    })
}

fn complete(f: &Fixture) -> Vec<super::completion::CompletionItem> {
    completions(&f.analysis(), f.file, f.offset).unwrap()
}

#[test]
fn the_kanban_column_has_no_errors() {
    let f = kanban(str::to_owned);
    let errors: Vec<_> = f
        .analysis()
        .file_diagnostics(f.id(COLUMN_VIEW))
        .unwrap()
        .into_iter()
        .filter(|d| d.is_error())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn completion_of_component_events() {
    let edit = ".on(edit: { edit.emit(card.id) })";
    // In a closed call, as editors leave it.
    let f = column_view(edit, ".on($0)");
    let items = complete(&f);
    let item = items.iter().find(|i| i.label == "edit:").expect("edit:");
    assert_eq!(item.detail.as_deref(), Some("Void"), "{item:?}");
    assert_eq!(
        item.documentation.as_deref(),
        Some("The user wants to edit the card.")
    );
    assert_eq!(item.insert_text.as_deref(), Some("edit: "));
    assert_eq!(labels(&items), ["edit:"], "only the events");
    // In an unclosed call with a prefix: the rest of the file is still
    // there.
    let f = column_view(edit, ".on(e$0");
    assert_eq!(labels(&complete(&f)), ["edit:"]);
    // A stdlib component's events.
    let f = column_view(
        ".on(moved: { move => boards.moveCard(board: board, move: move) })",
        ".on($0)",
    );
    let items = complete(&f);
    let moved = items.iter().find(|i| i.label == "moved:").expect("moved:");
    assert!(
        moved
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("ListMove<")),
        "{moved:?}"
    );
    assert!(moved.documentation.is_some());
    // Another `.on(` after the first offers the events not yet handled.
    let f = column_view(
        "{ closed.emit() })\n                    } else {",
        "{ closed.emit() })\n                            .on($0)\n                    } else {",
    );
    let items = complete(&f);
    let names = labels(&items).join(",");
    assert!(!names.contains("closed:"), "{names}");
    // After the first event of a call: none, since `.on(…)` takes one
    // (K3108).
    let f = column_view(edit, ".on(edit: { edit.emit(card.id) }, $0)");
    let items = complete(&f);
    let names = labels(&items).join(",");
    assert!(!names.contains(':'), "{names}");
    // In the handler: names in scope, not events.
    let f = column_view(edit, ".on(edit: $0)");
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"card"), "{names:?}");
}

/// Completion as the user types, with the call or rule left unfinished and
/// the rest of `ColumnView.k` after it.
#[test]
fn completion_while_typing_in_the_kanban_column() {
    let has = |f: &Fixture, want: &[&str], not: &[&str]| {
        let items = complete(f);
        let names = labels(&items);
        for w in want {
            assert!(names.contains(w), "{w}: {names:?}");
        }
        for n in not {
            assert!(!names.contains(n), "{n}: {names:?}");
        }
    };
    // `.on(` with nothing after it, on each kind of callee.
    let f = column_view(".on(edit: { edit.emit(card.id) })", ".on($0");
    has(&f, &["edit:"], &["click:", "card"]);
    let f = column_view(
        "CardEditor(board: board, card: card)\n                            .on(closed: { closed.emit() })",
        "CardEditor(board: board, card: card)\n                            .on($0",
    );
    has(&f, &["closed:"], &["edit:", "card"]);
    let f = column_view(
        ".on(moved: { move => boards.moveCard(board: board, move: move) })",
        ".on($0",
    );
    has(&f, &["moved:"], &["edit:", "card"]);
    let f = column_view(".on(click: { add.emit() })", ".on($0");
    has(&f, &["click:", "focus:"], &["edit:", "moved:"]);
    // A new declaration with no value and no `;` yet.
    let f = column_view(
        "            justifyContent: spaceBetween;\n",
        "            justifyContent: spaceBetween;\n            textDecoration: $0\n",
    );
    has(
        &f,
        &["lineThrough", "underline", "none", "inherit"],
        &["line-through", "spaceBetween", "let"],
    );
    let f = column_view(
        ".count { color: .muted;",
        ".count { textDecoration: li$0 color: .muted;",
    );
    has(&f, &["lineThrough"], &["none", "inherit"]);
    // `.class(.` unclosed, with the line ending there.
    let f = column_view(".class(.count)", ".class(.$0");
    has(&f, &["count", "header", "add"], &["let"]);
    let f = column_view(
        ".class([.column, .done when isDone])",
        ".class([.column, .$0",
    );
    has(&f, &["done", "count"], &["let"]);
}

#[test]
fn completion_of_element_events() {
    let f = column_view(".on(click: { add.emit() })", ".on($0)");
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"click:") && names.contains(&"focus:"),
        "{names:?}"
    );
    assert!(!names.contains(&"edit:"), "{names:?}");
    let click = items.iter().find(|i| i.label == "click:").unwrap();
    assert_eq!(click.detail.as_deref(), Some("Void"));
}

#[test]
fn completion_of_css_property_names() {
    let f = column_view(
        ".count { color: .muted; fontSize: .sm; }",
        ".count { color: .muted; fontSize: .sm; just$0 }",
    );
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"justifyContent") && names.contains(&"justifySelf"),
        "{names:?}"
    );
    assert!(!names.contains(&"let"), "no keywords: {names:?}");
    assert!(items.iter().all(|i| i.kind == ItemKind::Property));
    // At the start of a line, with no prefix.
    let f = column_view(
        "            display: flex;\n            alignItems",
        "            $0\n            display: flex;\n            alignItems",
    );
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"display") && names.contains(&"backgroundColor"),
        "{names:?}"
    );
    assert!(!names.contains(&"count"), "{names:?}");
}

#[test]
fn completion_of_css_values() {
    // Keywords of the property.
    let f = column_view("justifyContent: spaceBetween;", "justifyContent: $0;");
    let items = complete(&f);
    let names = labels(&items);
    for k in ["spaceBetween", "center", "flexStart", "inherit"] {
        assert!(names.contains(&k), "{k}: {names:?}");
    }
    assert!(!names.contains(&"let"), "{names:?}");
    // With a prefix and no `;`.
    let f = column_view("justifyContent: spaceBetween;", "justifyContent: spa$0");
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"spaceBetween") && names.contains(&"spaceAround"),
        "{names:?}"
    );
    assert!(!names.contains(&"center"), "{names:?}");
    // The theme's tokens of the property's kind, with their values.
    let f = column_view("backgroundColor: .column;", "backgroundColor: $0;");
    let items = complete(&f);
    let names = labels(&items);
    for k in [
        ".column",
        ".chartDone",
        ".muted",
        "transparent",
        "currentColor",
    ] {
        assert!(names.contains(&k), "{k}: {names:?}");
    }
    assert!(!names.contains(&".lg"), "a spacing token: {names:?}");
    let column = items.iter().find(|i| i.label == ".column").unwrap();
    assert!(
        column
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("#E2E8F0")),
        "{column:?}"
    );
    // After the dot.
    let f = column_view("backgroundColor: .column;", "backgroundColor: .$0;");
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"column") && names.contains(&"chartDone"),
        "{names:?}"
    );
    // Spacing for gaps, radii for corners, sizes for fonts.
    let f = column_view("gap: .sm;", "gap: $0;");
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&".sm") && names.contains(&".lg"),
        "{names:?}"
    );
    assert!(!names.contains(&".column"), "{names:?}");
    let f = column_view("borderRadius: .md;", "borderRadius: .$0;");
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"sm") && names.contains(&"md"), "{names:?}");
    assert!(!names.contains(&"lg"), "no radius lg: {names:?}");
    let f = column_view("fontSize: .sm;", "fontSize: .$0;");
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"xl"), "{names:?}");
    // A later part of a value.
    let f = column_view(
        ".done { border: 2px solid .chartDone; }",
        ".done { border: 2px $0 }",
    );
    let items = complete(&f);
    let names = labels(&items);
    assert!(
        names.contains(&"solid") && names.contains(&"dashed"),
        "{names:?}"
    );
}

#[test]
fn completion_of_class_names() {
    let classes = ["add", "column", "count", "done", "header"];
    // In a selector.
    let f = column_view(
        "        .done { border",
        "        .$0\n\n        .done { border",
    );
    let items = complete(&f);
    let names = labels(&items);
    for c in classes {
        assert!(names.contains(&c), "{c}: {names:?}");
    }
    // In `.class(…)`.
    let f = column_view(".class(.count)", ".class(.$0)");
    let items = complete(&f);
    let names = labels(&items);
    assert_eq!(names, classes, "{items:?}");
    let f = column_view(".class(.count)", ".class([.column, .$0])");
    assert_eq!(labels(&complete(&f)), classes);
    let f = column_view(".class(.count)", ".class(.co$0");
    assert_eq!(labels(&complete(&f)), ["column", "count"]);
    let f = column_view(".done when isDone", ".d$0 when isDone");
    assert_eq!(labels(&complete(&f)), ["add", "done", "header"]);
}

#[test]
fn hover_and_definition_of_tokens_and_classes() {
    // A token in a style block: its declaration, with the value.
    let f = column_view("solid .chartDone;", "solid .chart$0Done;");
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(h.markdown.contains("#16A34A"), "{}", h.markdown);
    assert!(h.markdown.contains("`dark`: `#4ADE80`"), "{}", h.markdown);
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    assert_eq!(defs, [f.span("src/ui/KanbanTheme.k", "chartDone", 0)]);
    // A class reference: its rule.
    let f = column_view(".class(.count)", ".class(.co$0unt)");
    let h = hover::hover(&f.analysis(), f.file, f.offset, &|_| None)
        .unwrap()
        .unwrap();
    assert!(
        h.markdown
            .contains(".count { color: .muted; fontSize: .sm; }"),
        "{}",
        h.markdown
    );
    let defs = definition(&f.analysis(), f.file, f.offset).unwrap();
    let rule = f.span(COLUMN_VIEW, ".count {", 0);
    let name = TextRange::at(rule.range.start() + TextSize::from(1), TextSize::from(5));
    assert_eq!(defs, [Span::new(rule.file, name)]);
}

#[test]
fn half_typed_styles_and_events_never_panic() {
    let texts = [
        "component C {\n    view {\n        button { \"x\" }\n            .on(\n    }\n    style {\n        .a {\n            gap: ;\n            just\n        }\n        .\n        .b { color: .; border: 2px solid  }\n    }\n}\n",
        "component C {\n    style {\n        @media (prefersReducedMotion: ",
        "component C {\n    view {\n        text { \"a\" }.class([.a, .",
        "component C {\n    style {\n        .a:hover { col",
    ];
    for text in texts {
        let f = fixture(&[("src/z/Z.k", text)]);
        let analysis = f.analysis();
        let len = u32::try_from(text.len()).unwrap();
        for offset in 0..=len + 2 {
            let offset = TextSize::from(offset);
            let _ = completions(&analysis, f.file, offset).unwrap();
            let _ = hover::hover(&analysis, f.file, offset, &|_| None).unwrap();
            let _ = definition(&analysis, f.file, offset).unwrap();
        }
    }
}

#[test]
fn completion_in_media_queries_and_after_a_missing_semicolon() {
    let style = |body: &str| {
        COUNTER.replace(
            "    view {",
            &format!("    style {{\n{body}\n    }}\n\n    view {{"),
        )
    };
    let text = style("        @media (prefersReducedMotion: $0) {\n        }");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = complete(&f);
    assert_eq!(labels(&items), ["noPreference", "reduce"]);
    let text = style("        @media (pre$0");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"prefersColorScheme"), "{names:?}");
    // A declaration on the next line of one missing its `;`.
    let text = style("        .box {\n            display: flex\n            ali$0\n        }");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"alignItems"), "{names:?}");
    // Inside an interpolation: names in scope.
    let text = style("        .box {\n            width: {dou$0}\n        }");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = complete(&f);
    let names = labels(&items);
    assert!(names.contains(&"doubled"), "{names:?}");
}

#[test]
fn parenthesis_and_colon_trigger_only_specific_completions() {
    use super::completion::completions_triggered;
    // `(` of an ordinary call: nothing.
    let text = USES.replace("Square(side: 1.0)", "Square($0)");
    let f = fixture(&[("src/use/Uses.k", &text)]);
    assert!(
        completions_triggered(&f.analysis(), f.file, f.offset, Some('('))
            .unwrap()
            .is_empty()
    );
    // Invoked there, names in scope.
    assert!(!complete(&f).is_empty());
    // `(` of `.on`: the events.
    let text = COUNTER.replace(".on(click: { increment() })", ".on($0)");
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = completions_triggered(&f.analysis(), f.file, f.offset, Some('(')).unwrap();
    assert!(labels(&items).contains(&"click:"), "{items:?}");
    // `:` of a CSS property: its values.
    let text = COUNTER.replace(
        "    view {",
        "    style {\n        .box {\n            display:$0\n        }\n    }\n\n    view {",
    );
    let f = fixture(&[("src/ui/Counter.k", &text)]);
    let items = completions_triggered(&f.analysis(), f.file, f.offset, Some(':')).unwrap();
    assert!(labels(&items).contains(&"grid"), "{items:?}");
}
