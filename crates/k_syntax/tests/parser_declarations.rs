//! Integration tests for parsing declarations, written before the parser.
//!
//! The golden corpus (`tests/corpus/parser/declarations`) pins down the tree
//! *shape* of every declaration kind. These tests cover what an `.ast` dump
//! cannot show, plus robustness properties over whole families of inputs:
//!
//! * token kinds: contextual keywords are remapped (`page` becomes
//!   `PageKw`) only in keyword positions, and keyword labels keep their
//!   keyword kind;
//! * the typed AST accessors (`HasName`, `HasVisibility`, `HasAnnotations`,
//!   clause accessors) return the right parts of parsed declarations;
//! * doc comments attach to the declaration that follows them;
//! * diagnostics are sorted by position;
//! * every prefix of a declaration-rich file, every file with one line
//!   removed and deeply nested declarations parse without panicking and
//!   losslessly.
//!
//! Everything here goes through the public API of `k_syntax`.

#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::{Code, FileId};
use k_syntax::ast::{
    self, AstNode, HasAnnotations, HasMemberList, HasName, HasVisibility, Item, Member, Visibility,
};
use k_syntax::{Parse, SyntaxKind, SyntaxNode, parse};

// ------------------------------------------------------------------ helpers

/// Parses `src`, asserting losslessness.
fn parse_any(src: &str) -> Parse {
    let parse = parse(FileId::new(0), src);
    let root = parse.syntax();
    assert_eq!(root.kind(), SyntaxKind::SourceFile);
    assert_eq!(
        root.text().to_string(),
        src,
        "tree is not lossless for {src:?}"
    );
    for diagnostic in parse.diagnostics() {
        let start = usize::from(diagnostic.span.start());
        let end = usize::from(diagnostic.span.end());
        assert!(
            start <= end && end <= src.len(),
            "span out of bounds: {diagnostic:?}"
        );
        assert!(
            src.is_char_boundary(start) && src.is_char_boundary(end),
            "{diagnostic:?}"
        );
    }
    parse
}

/// Parses `src`, asserting losslessness and the absence of diagnostics.
fn parse_clean(src: &str) -> Parse {
    let parse = parse_any(src);
    assert!(
        parse.diagnostics().is_empty(),
        "unexpected diagnostics for {src:?}: {:#?}",
        parse.diagnostics()
    );
    parse
}

/// Returns the top-level items of a clean parse of `src`.
fn items(src: &str) -> Vec<Item> {
    parse_clean(src)
        .tree()
        .expect("a SourceFile root")
        .items()
        .collect()
}

/// Returns the only top-level item of a clean parse of `src`.
fn only_item(src: &str) -> Item {
    let mut all = items(src);
    assert_eq!(all.len(), 1, "expected one item in {src:?}, got {all:?}");
    all.remove(0)
}

/// Returns the first descendant node of `root` that casts to `N`.
fn find<N: AstNode<Language = k_syntax::KLanguage>>(root: &SyntaxNode) -> N {
    root.descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("no {} in {root:#?}", std::any::type_name::<N>()))
}

/// Returns every token kind in the tree, in order.
fn token_kinds(root: &SyntaxNode) -> Vec<SyntaxKind> {
    root.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .map(|t| t.kind())
        .collect()
}

/// Returns the declaration keyword token of an item: the first direct
/// non-trivia token that is not a modifier (annotations are child nodes, so
/// they are skipped).
fn decl_keyword(node: &SyntaxNode) -> SyntaxKind {
    ast::support::tokens(node)
        .map(|t| t.kind())
        .find(|k| {
            !matches!(
                k,
                SyntaxKind::PublicKw | SyntaxKind::PrivateKw | SyntaxKind::StaticKw
            )
        })
        .expect("a keyword")
}

// ------------------------------------------------------ contextual keywords

#[test]
fn declaration_keywords_have_their_keyword_kind() {
    let cases: &[(&str, SyntaxKind, SyntaxKind)] = &[
        (
            "component C {}",
            SyntaxKind::ComponentDecl,
            SyntaxKind::ComponentKw,
        ),
        ("page P {}", SyntaxKind::PageDecl, SyntaxKind::PageKw),
        ("layout L {}", SyntaxKind::LayoutDecl, SyntaxKind::LayoutKw),
        (
            "service S {}",
            SyntaxKind::ServiceDecl,
            SyntaxKind::ServiceKw,
        ),
        (
            "protocol P {}",
            SyntaxKind::ProtocolDecl,
            SyntaxKind::ProtocolKw,
        ),
        ("entity E {}", SyntaxKind::EntityDecl, SyntaxKind::EntityKw),
        (
            "repository R for E {}",
            SyntaxKind::RepositoryDecl,
            SyntaxKind::RepositoryKw,
        ),
        ("theme T {}", SyntaxKind::ThemeDecl, SyntaxKind::ThemeKw),
        ("app A {}", SyntaxKind::AppDecl, SyntaxKind::AppKw),
        ("config C {}", SyntaxKind::ConfigDecl, SyntaxKind::ConfigKw),
        (
            "migration M {}",
            SyntaxKind::MigrationDecl,
            SyntaxKind::MigrationKw,
        ),
        (
            "test Suite {}",
            SyntaxKind::TestSuiteDecl,
            SyntaxKind::TestKw,
        ),
        (
            "test \"case\" {}",
            SyntaxKind::TestCaseDecl,
            SyntaxKind::TestKw,
        ),
        ("struct S {}", SyntaxKind::StructDecl, SyntaxKind::StructKw),
        ("enum E {}", SyntaxKind::EnumDecl, SyntaxKind::EnumKw),
        ("class C {}", SyntaxKind::ClassDecl, SyntaxKind::ClassKw),
        (
            "interface I {}",
            SyntaxKind::InterfaceDecl,
            SyntaxKind::InterfaceKw,
        ),
        (
            "extend Int {}",
            SyntaxKind::ExtendDecl,
            SyntaxKind::ExtendKw,
        ),
        ("func f() {}", SyntaxKind::FuncDecl, SyntaxKind::FuncKw),
        ("let x = 1", SyntaxKind::FieldDecl, SyntaxKind::LetKw),
        (
            "@A public component C {}",
            SyntaxKind::ComponentDecl,
            SyntaxKind::ComponentKw,
        ),
        (
            "@A(x: 1)\nprivate page P at \"/p\" {}",
            SyntaxKind::PageDecl,
            SyntaxKind::PageKw,
        ),
        (
            "public static func f() {}",
            SyntaxKind::FuncDecl,
            SyntaxKind::FuncKw,
        ),
    ];
    for (src, node_kind, keyword) in cases {
        let item = only_item(src);
        assert_eq!(item.syntax().kind(), *node_kind, "{src}");
        assert_eq!(decl_keyword(item.syntax()), *keyword, "{src}");
    }
}

#[test]
fn contextual_words_used_as_names_stay_identifiers() {
    let cases: &[(&str, &str)] = &[
        ("let page = 1", "page"),
        ("let component = 1", "component"),
        ("let view = 1", "view"),
        ("let key = 1", "key"),
        ("let at = 1", "at"),
        ("let proto = 1", "proto"),
        ("struct component {}", "component"),
        ("struct page {}", "page"),
        ("component view {}", "view"),
        ("service test {}", "test"),
        ("func layout() {}", "layout"),
        ("func variant() {}", "variant"),
        ("enum app {}", "app"),
    ];
    for (src, name) in cases {
        let item = only_item(src);
        let token = match &item {
            Item::FieldDecl(n) => n.name(),
            Item::StructDecl(n) => n.name(),
            Item::ComponentDecl(n) => n.name(),
            Item::ServiceDecl(n) => n.name(),
            Item::FuncDecl(n) => n.name(),
            Item::EnumDecl(n) => n.name(),
            other => panic!("unexpected item {other:?} for {src}"),
        }
        .unwrap_or_else(|| panic!("no name in {src}"));
        assert_eq!(token.kind(), SyntaxKind::Ident, "{src}");
        assert_eq!(token.text(), *name, "{src}");
    }
}

#[test]
fn member_keywords_are_remapped_in_member_position() {
    let parse = parse_clean(
        "page P at \"/p\" {\n    view { \"v\" }\n    loading { \"l\" }\n    failed(let e) { \"f\" }\n    metadata { title(\"t\") }\n}\ntheme T {\n    color a: Color = #fff\n    b = #111\n    variant dark matches .prefersDark { a = #000 }\n}\nfunc g() async {\n    for await x in xs { }\n}\n",
    );
    let kinds = token_kinds(&parse.syntax());
    for keyword in [
        SyntaxKind::PageKw,
        SyntaxKind::AtKw,
        SyntaxKind::ViewKw,
        SyntaxKind::LoadingKw,
        SyntaxKind::FailedKw,
        SyntaxKind::MetadataKw,
        SyntaxKind::ThemeKw,
        SyntaxKind::VariantKw,
        SyntaxKind::MatchesKw,
        SyntaxKind::AwaitKw,
    ] {
        assert!(
            kinds.contains(&keyword),
            "{keyword:?} missing from {kinds:?}"
        );
    }
}

#[test]
fn contextual_words_in_other_member_positions_are_not_remapped() {
    let parse = parse_clean(
        "struct S {\n    let view: View\n    let loading: Bool\n    var failed: Int = 0\n    let metadata: Meta\n    let variant: Style\n    let at: Int\n    let key: String\n    let config: Config\n    let migration: Migration\n    let matches: Bool\n}\n",
    );
    let kinds = token_kinds(&parse.syntax());
    for keyword in [
        SyntaxKind::ViewKw,
        SyntaxKind::LoadingKw,
        SyntaxKind::FailedKw,
        SyntaxKind::MetadataKw,
        SyntaxKind::VariantKw,
        SyntaxKind::AtKw,
        SyntaxKind::KeyKw,
        SyntaxKind::ConfigKw,
        SyntaxKind::MigrationKw,
        SyntaxKind::MatchesKw,
    ] {
        assert!(
            !kinds.contains(&keyword),
            "{keyword:?} should be an Ident in {kinds:?}"
        );
    }
    let names: Vec<String> = find::<ast::StructDecl>(&parse.syntax())
        .members()
        .into_iter()
        .map(|m| match m {
            Member::FieldDecl(f) => f.name_text().unwrap(),
            other => panic!("unexpected member {other:?}"),
        })
        .collect();
    assert_eq!(
        names,
        [
            "view",
            "loading",
            "failed",
            "metadata",
            "variant",
            "at",
            "key",
            "config",
            "migration",
            "matches"
        ]
    );
}

#[test]
fn proto_is_a_keyword_only_before_a_string() {
    let parse = parse_clean("import proto \"a.proto\"\nimport proto.Messages\n");
    let file = parse.tree().unwrap();
    let imports: Vec<ast::Import> = file.imports().collect();
    assert_eq!(imports.len(), 2);

    assert!(imports[0].path().is_none());
    let proto = imports[0].proto_file().expect("a proto file string");
    assert_eq!(proto.plain_value().as_deref(), Some("a.proto"));
    assert!(token_kinds(imports[0].syntax()).contains(&SyntaxKind::ProtoKw));

    assert!(imports[1].proto_file().is_none());
    let path = imports[1].path().expect("a path");
    assert_eq!(path.text(), "proto.Messages");
    assert!(!token_kinds(imports[1].syntax()).contains(&SyntaxKind::ProtoKw));
}

#[test]
fn keyword_parameter_labels_keep_their_keyword_kind() {
    let item = only_item("func f(for id: Id, in range: R, _ x: Int, as format: F, plain: Int) {}");
    let Item::FuncDecl(func) = item else {
        panic!("not a function")
    };
    let params: Vec<ast::Param> = func.param_list().unwrap().params().collect();
    let labels: Vec<Option<SyntaxKind>> =
        params.iter().map(|p| p.label().map(|t| t.kind())).collect();
    assert_eq!(
        labels,
        [
            Some(SyntaxKind::ForKw),
            Some(SyntaxKind::InKw),
            Some(SyntaxKind::Underscore),
            Some(SyntaxKind::AsKw),
            None,
        ]
    );
    let names: Vec<String> = params
        .iter()
        .map(|p| p.name().unwrap().text().to_owned())
        .collect();
    assert_eq!(names, ["id", "range", "x", "format", "plain"]);
}

// ---------------------------------------------------------------- accessors

#[test]
fn visibility_of_items_and_members() {
    let all =
        items("public struct A {}\nprivate func b() {}\nenum C {}\n@X public static let d = 1\n");
    let visibilities: Vec<Visibility> = all
        .iter()
        .map(|item| match item {
            Item::StructDecl(n) => n.visibility(),
            Item::FuncDecl(n) => n.visibility(),
            Item::EnumDecl(n) => n.visibility(),
            Item::FieldDecl(n) => n.visibility(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        visibilities,
        [
            Visibility::Public,
            Visibility::Private,
            Visibility::Module,
            Visibility::Public
        ]
    );

    let Item::ServiceDecl(service) =
        only_item("service S {\n    private var a = 0\n    public func b() {}\n    init() {}\n}\n")
    else {
        panic!("not a service")
    };
    let members = service.members();
    assert_eq!(members.len(), 3);
    let Member::FieldDecl(a) = &members[0] else {
        panic!()
    };
    let Member::FuncDecl(b) = &members[1] else {
        panic!()
    };
    let Member::InitDecl(c) = &members[2] else {
        panic!()
    };
    assert_eq!(a.visibility(), Visibility::Private);
    assert_eq!(b.visibility(), Visibility::Public);
    assert_eq!(c.visibility(), Visibility::Module);
}

#[test]
fn annotations_in_source_order_with_arguments() {
    let Item::ServiceDecl(service) =
        only_item("@Scope(.request)\n@Breaking(\"why\") @Plain\nservice S {}\n")
    else {
        panic!("not a service")
    };
    let names: Vec<String> = service
        .annotations()
        .map(|a| a.name().unwrap().text().to_owned())
        .collect();
    assert_eq!(names, ["Scope", "Breaking", "Plain"]);
    let scope = service.annotation("Scope").unwrap();
    let args: Vec<ast::Arg> = scope.arg_list().unwrap().args().collect();
    assert_eq!(args.len(), 1);
    assert!(args[0].label().is_none());
    assert!(matches!(
        args[0].value(),
        Some(ast::Expr::ImplicitMemberExpr(_))
    ));
    assert!(service.annotation("Plain").unwrap().arg_list().is_none());
    assert!(service.annotation("Missing").is_none());
}

#[test]
fn labeled_annotation_arguments() {
    let Item::StructDecl(s) =
        only_item("struct S {\n    @Length(min: 8, max: 64) var password: String\n}\n")
    else {
        panic!()
    };
    let Member::FieldDecl(field) = &s.members()[0] else {
        panic!()
    };
    let length = field.annotation("Length").unwrap();
    let labels: Vec<String> = length
        .arg_list()
        .unwrap()
        .args()
        .map(|a| a.label().unwrap().text().to_owned())
        .collect();
    assert_eq!(labels, ["min", "max"]);
    assert!(field.is_var());
    assert_eq!(field.name_text().as_deref(), Some("password"));
}

#[test]
fn field_declaration_parts() {
    let Item::StructDecl(s) = only_item(
        "struct S {\n    static var a: Int = 1\n    let b: Int\n    var c = 2\n    var d: Int { 3 }\n}\n",
    ) else {
        panic!()
    };
    let fields: Vec<ast::FieldDecl> = s
        .members()
        .into_iter()
        .map(|m| match m {
            Member::FieldDecl(f) => f,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    let summary: Vec<(bool, bool, bool, bool, bool)> = fields
        .iter()
        .map(|f| {
            (
                f.is_static(),
                f.is_var(),
                f.ty().is_some(),
                f.initializer().is_some(),
                f.getter().is_some(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (true, true, true, true, false),
            (false, false, true, false, false),
            (false, true, false, true, false),
            (false, true, true, false, true),
        ]
    );
}

#[test]
fn function_signature_parts() {
    let Item::FuncDecl(f) =
        only_item("func f<T>(a: Int) async throws E -> T where T implements X {}")
    else {
        panic!()
    };
    assert_eq!(f.name_text().as_deref(), Some("f"));
    assert!(f.is_async());
    assert!(!f.is_static());
    assert_eq!(
        f.generic_param_list().unwrap().syntax().children().count(),
        1
    );
    assert_eq!(f.param_list().unwrap().params().count(), 1);
    assert!(f.throws_clause().unwrap().error_type().is_some());
    assert!(f.return_type().unwrap().ty().is_some());
    assert!(f.where_clause().is_some());
    assert!(f.body().is_some());

    let Item::InterfaceDecl(i) = only_item("interface I {\n    func g() throws E\n}\n") else {
        panic!()
    };
    let Member::FuncDecl(g) = &i.members()[0] else {
        panic!()
    };
    assert!(g.body().is_none(), "a requirement has no body");
    assert!(!g.is_async());
    assert!(g.return_type().is_none());
    assert!(g.throws_clause().is_some());
}

#[test]
fn rpc_parts_in_protocols_and_services() {
    let Item::ProtocolDecl(p) = only_item(
        "protocol P {\n    rpc add(request: R) async throws E -> T requires A, B(1)\n}\n",
    ) else {
        panic!()
    };
    let Member::RpcDecl(rpc) = &p.members()[0] else {
        panic!()
    };
    assert_eq!(rpc.name_text().as_deref(), Some("add"));
    assert_eq!(rpc.param_list().unwrap().params().count(), 1);
    assert!(rpc.throws_clause().is_some());
    assert!(rpc.return_type().is_some());
    let guards: Vec<ast::Guard> = rpc.requires_clause().unwrap().guards().collect();
    assert_eq!(guards.len(), 2);
    assert!(rpc.body().is_none());

    let Item::ServiceDecl(s) =
        only_item("service S implements P {\n    rpc add(request: R) -> T {}\n}\n")
    else {
        panic!()
    };
    let implemented: Vec<String> = s
        .implements_clause()
        .unwrap()
        .types()
        .map(|t| t.syntax().text().to_string())
        .collect();
    assert_eq!(implemented, ["P"]);
    let Member::RpcDecl(rpc) = &s.members()[0] else {
        panic!()
    };
    assert!(rpc.body().is_some());
    assert!(rpc.requires_clause().is_none());
}

#[test]
fn page_clauses() {
    let Item::PageDecl(page) =
        only_item("page Admin requires Authenticated, Role(.admin) at \"/admin\" {}")
    else {
        panic!()
    };
    assert_eq!(page.name_text().as_deref(), Some("Admin"));
    assert_eq!(page.requires_clause().unwrap().guards().count(), 2);
    let template = page.at_clause().unwrap().template().unwrap();
    assert!(template.is_plain());
    assert_eq!(template.plain_value().as_deref(), Some("/admin"));

    let Item::PageDecl(detail) = only_item("page D at \"/product/{id}/{tab}\" {}") else {
        panic!()
    };
    let template = detail.at_clause().unwrap().template().unwrap();
    assert!(!template.is_plain());
    assert_eq!(template.interpolations().count(), 2);
    assert!(detail.requires_clause().is_none());
}

#[test]
fn repository_enum_and_import_accessors() {
    let Item::RepositoryDecl(r) = only_item("repository Users for shop.User {}") else {
        panic!()
    };
    let ty: ast::Type = ast::support::child(r.for_clause().unwrap().syntax()).unwrap();
    assert_eq!(ty.syntax().text().to_string(), "shop.User");

    let Item::EnumDecl(e) = only_item(
        "enum E {\n    case a\n    case b(Int)\n    func f() {}\n    case c(x: Int, y: Int)\n}\n",
    ) else {
        panic!()
    };
    let cases: Vec<String> = e.cases().iter().map(|c| c.name_text().unwrap()).collect();
    assert_eq!(cases, ["a", "b", "c"]);
    assert_eq!(e.members().len(), 4);

    let file = parse_clean("import github.com/acme/my-charts.LineChart\n")
        .tree()
        .unwrap();
    let path = file.imports().next().unwrap().path().unwrap();
    assert_eq!(path.text(), "github.com/acme/my-charts.LineChart");
    assert_eq!(path.declaration_name().unwrap().text(), "LineChart");
}

#[test]
fn every_member_kind_is_a_member() {
    let src = "test Everything {\n    let a = 1\n    func b() {}\n    init() {}\n    rpc c()\n    case d\n    use E for F\n    color g: Color = #fff\n    variant h { g = #000 }\n    view { \"v\" }\n    style { .x { color: .g; } }\n    loading { \"l\" }\n    failed { \"f\" }\n    metadata { title(\"t\") }\n    test Inner {}\n    test \"case\" {}\n}\n";
    let Item::TestSuiteDecl(suite) = only_item(src) else {
        panic!()
    };
    let kinds: Vec<SyntaxKind> = suite.members().iter().map(|m| m.syntax().kind()).collect();
    assert_eq!(
        kinds,
        [
            SyntaxKind::FieldDecl,
            SyntaxKind::FuncDecl,
            SyntaxKind::InitDecl,
            SyntaxKind::RpcDecl,
            SyntaxKind::EnumCase,
            SyntaxKind::UseDecl,
            SyntaxKind::ThemeToken,
            SyntaxKind::ThemeVariant,
            SyntaxKind::ViewBlock,
            SyntaxKind::StyleBlock,
            SyntaxKind::LoadingBlock,
            SyntaxKind::FailedBlock,
            SyntaxKind::MetadataBlock,
            SyntaxKind::TestSuiteDecl,
            SyntaxKind::TestCaseDecl,
        ]
    );
}

// ------------------------------------------------------------ doc comments

#[test]
fn doc_comments_attach_to_the_following_declaration() {
    let src = "/// A point.\n@Inject\npublic struct Point {\n    /// The x.\n    let x: Int\n}\n";
    let item = only_item(src);
    assert_eq!(
        item.syntax().first_token().unwrap().kind(),
        SyntaxKind::DocComment,
        "the item starts at its doc comment"
    );
    assert_eq!(u32::from(item.syntax().text_range().start()), 0);

    let Item::StructDecl(s) = item else { panic!() };
    let Member::FieldDecl(x) = &s.members()[0] else {
        panic!()
    };
    let doc_tokens: Vec<String> = x
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::DocComment)
        .map(|t| t.text().to_owned())
        .collect();
    assert_eq!(doc_tokens, ["/// The x."]);
    // The struct's own header holds only its own doc comment.
    let header_docs =
        ast::support::token(s.syntax(), SyntaxKind::DocComment).map(|t| t.text().to_owned());
    assert_eq!(header_docs.as_deref(), Some("/// A point."));
}

#[test]
fn trailing_comments_stay_on_their_line() {
    // A comment on the same line as the end of a declaration stays with
    // that declaration's last token; it never becomes leading trivia of the
    // declaration on the next line.
    let parse = parse_clean("struct A {} // about A\nstruct B {}\n");
    let file = parse.tree().unwrap();
    let all: Vec<Item> = file.items().collect();
    assert_eq!(all.len(), 2);
    assert!(!all[1].syntax().text().to_string().contains("about A"));
}

// --------------------------------------------------------------- diagnostics

#[test]
fn diagnostics_are_sorted_and_each_error_is_reported() {
    let src = "struct {}\nenum E { case a, b }\nfunc f() throws {}\nvar top = 1\ncomponent Ok {\n    42\n}\n";
    let parse = parse_any(src);
    let codes: Vec<Code> = parse.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        [
            Code::new(100),
            Code::new(130),
            Code::new(129),
            Code::new(131),
            Code::new(126)
        ]
    );
    let starts: Vec<(u32, u32)> = parse
        .diagnostics()
        .iter()
        .map(|d| (d.span.start().into(), d.span.end().into()))
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted);
}

#[test]
fn broken_declarations_always_report_something() {
    for src in [
        "struct {}",
        "struct",
        "struct A",
        "@",
        "@State",
        "public",
        "public public struct A {}",
        "component",
        "component {}",
        "page P requires",
        "page P at",
        "func f(",
        "func f(x) {}",
        "enum E { case a, b }",
        "repository R {}",
        "use A for B",
        "theme T { color }",
        "import",
        "import a.",
        "test",
        "test \"x\"",
        "}",
        "{",
        "extend",
        "init() {}",
        "var x = 1",
        "struct A {} struct B {}",
        "struct A { let x: Int; }",
        "func f() throws {}",
        "struct as {}",
        "protocol P { rpc r( }",
        "struct A<> {}",
        "struct A implements {}",
        "func f<T>() where T {}",
        "service S { private @Inject let a: A }",
    ] {
        let parse = parse_any(src);
        assert!(!parse.diagnostics().is_empty(), "no diagnostic for {src:?}");
    }
}

// --------------------------------------------------------------- robustness

/// A file that uses most declaration syntax.
const SAMPLE: &str = r#"import shop.catalog.Product
import proto "billing/v1/invoice.proto"

/// The cart.
@Scope(.request)
public service CartService implements CartProtocol {
    @Inject private let api: CartApi
    @State var items: List<CartItem> = []
    @Derived let total: Money = items.sum { item => item.price }

    rpc add(request: AddItemRequest) async throws CartError -> CartItem requires Authenticated, RateLimit(100.perMinute) {
        return try await api.add(request: request)
    }

    func clear<T>(_ keep: (T) -> Bool = { t => false }) where T implements Codable {}
}

page Product requires Authenticated at "/product/{id}" {
    @Param let id: ProductId
    loading { Spinner() }
    failed(let error) { "Oops {error}" }
    view { heading(level: 1) { "P" } }
    style { .title { padding: .md; } }
}

enum LoadState<T> {
    case idle
    case loaded(T, at: Date)
}

theme AppTheme {
    color primary: Color = #3B82F6
    variant dark { primary = #111 }
}

@Layout(Main)
public app Shop {
    use Fake for Payments in .development, .test
    metadata { title("Shop") }
}

test CartService {
    use FakeCartApi for CartProtocol
    test "adds" { expect true }
}
"#;

#[test]
fn sample_parses_cleanly() {
    let parse = parse_clean(SAMPLE);
    let kinds: Vec<SyntaxKind> = parse
        .tree()
        .unwrap()
        .items()
        .map(|i| i.syntax().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            SyntaxKind::ServiceDecl,
            SyntaxKind::PageDecl,
            SyntaxKind::EnumDecl,
            SyntaxKind::ThemeDecl,
            SyntaxKind::AppDecl,
            SyntaxKind::TestSuiteDecl,
        ]
    );
}

#[test]
fn every_prefix_of_a_declaration_file_parses_losslessly() {
    for (index, _) in SAMPLE.char_indices() {
        let _ = parse_any(&SAMPLE[..index]);
    }
}

#[test]
fn removing_any_single_line_still_parses_losslessly() {
    let lines: Vec<&str> = SAMPLE.split_inclusive('\n').collect();
    for skip in 0..lines.len() {
        let text: String = lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, l)| *l)
            .collect();
        let _ = parse_any(&text);
    }
}

#[test]
fn removing_any_single_closing_brace_reports_an_unclosed_delimiter() {
    let positions: Vec<usize> = SAMPLE
        .char_indices()
        .filter(|(_, c)| *c == '}')
        .map(|(i, _)| i)
        .collect();
    for position in positions {
        let text = format!("{}{}", &SAMPLE[..position], &SAMPLE[position + 1..]);
        let parse = parse_any(&text);
        assert!(
            !parse.diagnostics().is_empty(),
            "removing the `}}` at byte {position} went unnoticed"
        );
    }
}

/// Runs `f` on a thread with a stack large enough for deep but reasonable
/// nesting, so a recursion bug shows up as a failure rather than an abort
/// of the whole test binary.
fn with_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deeply_nested_test_suites_parse_losslessly() {
    with_stack(|| {
        let depth = 1_000;
        let mut src = String::new();
        for i in 0..depth {
            src.push_str(&format!("test S{i} {{\n"));
        }
        src.push_str("test \"leaf\" { expect true }\n");
        for _ in 0..depth {
            src.push_str("}\n");
        }
        // The input is valid; whether the parser imposes a nesting limit with
        // a diagnostic is left to the implementation, so only losslessness
        // and the absence of a panic are asserted.
        let _ = parse_any(&src);
    });
}

#[test]
fn deeply_nested_generic_types_parse_losslessly() {
    with_stack(|| {
        let depth = 1_000;
        let src = format!(
            "let x: {}Int{} = []\n",
            "List<".repeat(depth),
            ">".repeat(depth)
        );
        let _ = parse_any(&src);
    });
}

#[test]
fn deeply_nested_unclosed_declarations_parse_losslessly() {
    with_stack(|| {
        let src = "test S {\n".repeat(1_000);
        let parse = parse_any(&src);
        assert!(!parse.diagnostics().is_empty());
    });
}

#[test]
fn long_runs_of_annotations_and_modifiers_parse_losslessly() {
    let src = format!(
        "{}{}struct A {{}}\n",
        "@A ".repeat(5_000),
        "public ".repeat(1_000)
    );
    let parse = parse_any(&src);
    // Only the duplicate modifiers are errors.
    assert!(
        parse.diagnostics().iter().all(|d| d.code == Code::new(127)),
        "{:?}",
        parse.diagnostics().first()
    );
}

// ------------------------------------------------------------- use operand

/// Returns the `UseDecl`s of the first member list in `src`.
fn use_decls(src: &str) -> Vec<ast::UseDecl> {
    let parse = parse_any(src);
    parse
        .syntax()
        .descendants()
        .filter_map(ast::UseDecl::cast)
        .collect()
}

#[test]
fn use_decl_accessors_split_operand_target_and_environments() {
    let decls = use_decls(
        "app A {\n    use Fake for Api in .development, .test\n}\n\
         test T {\n    use Config.sample for Config\n    use Config(a: 1) for Config\n    use Missing\n}\n",
    );
    assert_eq!(decls.len(), 4);

    let typed = &decls[0];
    assert_eq!(typed.implementation().unwrap().syntax().text(), "Fake");
    assert!(typed.value().is_none());
    assert_eq!(typed.target().unwrap().syntax().text(), "Api");
    let environments: Vec<String> = typed
        .environments()
        .iter()
        .map(|e| e.syntax().text().to_string())
        .collect();
    assert_eq!(environments, [".development", ".test"]);

    for value in &decls[1..3] {
        assert!(value.implementation().is_none());
        assert!(value.value().is_some());
        assert_eq!(value.target().unwrap().syntax().text(), "Config");
        assert!(value.environments().is_empty());
    }
    assert!(matches!(decls[1].value(), Some(ast::Expr::MemberExpr(_))));
    assert!(matches!(decls[2].value(), Some(ast::Expr::CallExpr(_))));

    let missing = &decls[3];
    assert_eq!(missing.implementation().unwrap().syntax().text(), "Missing");
    assert!(missing.target().is_none());
    assert!(missing.environments().is_empty());
}
