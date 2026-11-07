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
