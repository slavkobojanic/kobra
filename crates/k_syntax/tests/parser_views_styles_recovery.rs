//! Integration tests for the parser's handling of views, style blocks,
//! `metadata`/`loading`/`failed` blocks, and error recovery.
//!
//! The golden corpus (`tests/corpus/parser/views-styles-recovery`) pins exact
//! trees and diagnostics. These tests cover what golden files cannot express
//! well:
//!
//! * robustness on inputs too large or too numerous for golden files
//!   (random garbage, every prefix of a file, every single-character
//!   deletion, deep nesting), asserting the syntax invariants: no panic, a
//!   lossless tree rooted at `SourceFile`, and diagnostic spans in bounds;
//! * properties of the tree that the S-expression dump hides, such as
//!   contextual keywords being remapped only at member start;
//! * rules that apply to every diagnostic (registered code, message style,
//!   well-formed fixes), and that each independent error is reported once.

use std::collections::BTreeMap;

use k_diagnostics::{Code, Diagnostic, FileId};
use k_syntax::{Parse, SyntaxKind, SyntaxNode, parse};

/// A realistic page using every construct this area covers.
const FULL_PAGE: &str = r#"/// A product page.
page Product {
    @Param let id: ProductId
    @Inject let products: ProductService
    @Load let product: Product = try await products.find(id)
    @State var isExpanded: Bool = false
    @Slot let row: (CartItem) -> View

    metadata {
        title("{product.name} · Acme Shop")
        canonical(.product(id: id))
    }

    view {
        section {
            heading(level: 1) { product.name }
                .class(.title)

            for item in product.items key item.id {
                row(item)
            }

            if let badge = product.badge {
                Badge(text: badge)
            } else {
                "No badge"
            }

            match product.stock {
                case .inStock(let count) where count > 10 { "In stock" }
                case .soldOut { "Sold out" }
                default { "Few left" }
            }

            Card(title: "Details", items: product.items) {
                paragraph { "Hello, {product.owner}!" }
            }
            .row { item =>
                CartRow(item: item, quantity: bind item.quantity)
            }

            box {}
                .class([.panel, .expanded when isExpanded, global("legacy")])
                .on(click: { isExpanded = not isExpanded })
        }
    }

    loading {
        ProductSkeleton()
    }

    failed(let error) {
        ErrorBanner(message: error.message)
    }

    style {
        .title {
            fontSize: .xl;
            borderLeft: 4px solid {product.accent};
            fontFamily: "Inter", sans-serif;

            &:hover { color: .primary.opacity(0.8); }

            @media (maxWidth: 600px) { fontSize: .lg; }
        }

        .panel, .expanded > .child:not(.hidden)::after {
            width: calc(100% - 2px);
            margin: -4px auto;
        }
    }
}
"#;

fn parse_text(text: &str) -> Parse {
    parse(FileId::new(0), text)
}

/// Asserts the invariants every parse must satisfy and returns the parse.
fn check(text: &str) -> Parse {
    let parse = parse_text(text);
    let root = parse.syntax();
    assert_eq!(root.kind(), SyntaxKind::SourceFile, "root of {text:?}");
    assert_eq!(root.text().to_string(), text, "syntax tree is not lossless");
    assert!(parse.tree().is_some());
    for diagnostic in parse.diagnostics() {
        let start = usize::from(diagnostic.span.start());
        let end = usize::from(diagnostic.span.end());
        assert!(
            start <= end && end <= text.len(),
            "span out of bounds in {text:?}: {diagnostic:?}"
        );
        assert!(
            text.is_char_boundary(start) && text.is_char_boundary(end),
            "span not on char boundaries in {text:?}: {diagnostic:?}"
        );
        for fix in &diagnostic.fixes {
            for edit in &fix.edits {
                let s = usize::from(edit.span.start());
                let e = usize::from(edit.span.end());
                assert!(s <= e && e <= text.len(), "fix edit out of bounds: {fix:?}");
                assert!(text.is_char_boundary(s) && text.is_char_boundary(e));
            }
        }
    }
    parse
}

fn codes(parse: &Parse) -> BTreeMap<u16, usize> {
    let mut map = BTreeMap::new();
    for diagnostic in parse.diagnostics() {
        *map.entry(diagnostic.code.number()).or_insert(0) += 1;
    }
    map
}

fn has_node(root: &SyntaxNode, kind: SyntaxKind) -> bool {
    root.descendants().any(|node| node.kind() == kind)
}

fn token_kinds_with_text(root: &SyntaxNode, text: &str) -> Vec<SyntaxKind> {
    root.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.text() == text)
        .map(|token| token.kind())
        .collect()
}

/// A small deterministic pseudo-random generator (xorshift), so failures
/// reproduce without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

const FRAGMENTS: &[&str] = &[
    "view",
    "style",
    "loading",
    "failed",
    "metadata",
    "component",
    "page",
    "layout",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ",",
    ":",
    ";",
    ".",
    "&",
    ">",
    "+",
    "~",
    "*",
    "!",
    "@",
    "#",
    "#fff",
    "#zz",
    "&:hover",
    "::before",
    "@media",
    "(maxWidth: 600px)",
    "padding: .md;",
    "color: red",
    "!important",
    "{accent}",
    "{}",
    "\"text\"",
    "\"{name}\"",
    "\"",
    "box",
    "Card",
    ".class",
    "(.muted)",
    "[.a, .b when x]",
    ".on(click: f)",
    "bind",
    "when",
    "for",
    "in",
    "key",
    "if",
    "else",
    "let",
    "match",
    "case",
    "default",
    "=>",
    "->",
    "item",
    "16px",
    "4pz",
    "-4px",
    "calc(",
    "100%",
    " ",
    "  ",
    "\n",
    "\r\n",
    "\t",
    "// c",
    "/* c */",
    "/*",
    "///",
    "é",
    "$",
];

#[test]
fn full_page_parses_without_diagnostics() {
    let parse = check(FULL_PAGE);
    assert!(
        parse.diagnostics().is_empty(),
        "unexpected diagnostics: {:#?}",
        parse.diagnostics()
    );
    let root = parse.syntax();
    for kind in [
        SyntaxKind::PageDecl,
        SyntaxKind::ViewBlock,
        SyntaxKind::StyleBlock,
        SyntaxKind::LoadingBlock,
        SyntaxKind::FailedBlock,
        SyntaxKind::MetadataBlock,
        SyntaxKind::ForStmt,
        SyntaxKind::KeyClause,
        SyntaxKind::IfExpr,
        SyntaxKind::LetCondition,
        SyntaxKind::MatchExpr,
        SyntaxKind::WhenExpr,
        SyntaxKind::BindExpr,
        SyntaxKind::LambdaParamList,
        SyntaxKind::StyleRule,
        SyntaxKind::NestingSelector,
        SyntaxKind::PseudoClass,
        SyntaxKind::PseudoElement,
        SyntaxKind::AtRule,
        SyntaxKind::MediaFeature,
        SyntaxKind::Declaration,
        SyntaxKind::CssFunction,
        SyntaxKind::Interpolation,
    ] {
        assert!(has_node(&root, kind), "no {kind:?} node in the full page");
    }
    assert!(
        !has_node(&root, SyntaxKind::ErrorNode),
        "valid input produced an ErrorNode"
    );
}

#[test]
fn block_keywords_are_remapped_only_at_member_start() {
    let text = "component T {\n    @State var loading: Bool = false\n    view {\n        loading { view }\n        failed(metadata)\n    }\n    loading {}\n    failed(let e) {}\n    metadata {}\n}\n";
    let parse = check(text);
    assert!(parse.diagnostics().is_empty(), "{:#?}", parse.diagnostics());
    let root = parse.syntax();
    // `loading`: field name (Ident), element name in the view (Ident), block keyword.
    assert_eq!(
        token_kinds_with_text(&root, "loading"),
        [SyntaxKind::Ident, SyntaxKind::Ident, SyntaxKind::LoadingKw]
    );
    // `view`: block keyword, then a name inside the view.
    assert_eq!(
        token_kinds_with_text(&root, "view"),
        [SyntaxKind::ViewKw, SyntaxKind::Ident]
    );
    assert_eq!(
        token_kinds_with_text(&root, "failed"),
        [SyntaxKind::Ident, SyntaxKind::FailedKw]
    );
    assert_eq!(
        token_kinds_with_text(&root, "metadata"),
        [SyntaxKind::Ident, SyntaxKind::MetadataKw]
    );
}

#[test]
fn key_is_remapped_only_after_the_for_iterable() {
    let text = "component T {\n    view {\n        for key in keys key key {\n            text { key }\n        }\n    }\n}\n";
    let parse = check(text);
    assert!(parse.diagnostics().is_empty(), "{:#?}", parse.diagnostics());
    assert_eq!(
        token_kinds_with_text(&parse.syntax(), "key"),
        [
            SyntaxKind::Ident,
            SyntaxKind::KeyKw,
            SyntaxKind::Ident,
            SyntaxKind::Ident
        ]
    );
}

#[test]
fn style_after_dot_is_a_member_not_a_style_block() {
    let text = "component T {\n    view {\n        box {}.style { compact }\n        box {}\n            .style(.x)\n    }\n}\n";
    let parse = check(text);
    assert!(parse.diagnostics().is_empty(), "{:#?}", parse.diagnostics());
    assert!(!has_node(&parse.syntax(), SyntaxKind::StyleBlock));
}

#[test]
fn html_element_names_are_not_parse_errors() {
    let text = "component T {\n    view {\n        div {\n            p { \"x\" }\n            a(href: \"/\") { \"home\" }\n            img(src: \"a.png\")\n            span {}\n        }\n    }\n}\n";
    let parse = check(text);
    assert!(
        parse.diagnostics().is_empty(),
        "HTML names must be rejected by semantic analysis, not the parser: {:#?}",
        parse.diagnostics()
    );
}

#[test]
fn independent_errors_are_each_reported_exactly_once() {
    // Twenty components, each with the same five independent errors.
    let one = "component T {\n    + +\n    view {\n        text { \"a\" } text { \"b\" }\n        button {}\n            .disabled(!ready)\n    }\n    style {\n        .a { color: red !important; }\n        .b {\n            color: red\n            margin: 0;\n        }\n    }\n}\n";
    let text = one.repeat(20);
    let parse = check(&text);
    let expected: BTreeMap<u16, usize> = [(102, 20), (106, 20), (117, 20), (118, 20), (126, 20)]
        .into_iter()
        .collect();
    assert_eq!(codes(&parse), expected, "{:#?}", parse.diagnostics());
}

#[test]
fn unclosed_delimiters_point_at_their_openers() {
    let text = "component T {\n    view {\n        box {\n            Spinner(\n";
    let parse = check(text);
    let mut spans: Vec<(usize, usize)> = parse
        .diagnostics()
        .iter()
        .filter(|d| d.code == Code::new(101))
        .map(|d| (usize::from(d.span.start()), usize::from(d.span.end())))
        .collect();
    spans.sort_unstable();
    let openers: Vec<(usize, usize)> = text
        .char_indices()
        .filter(|(_, c)| matches!(c, '{' | '('))
        .map(|(i, _)| (i, i + 1))
        .collect();
    assert_eq!(spans, openers, "{:#?}", parse.diagnostics());
}

#[test]
fn parser_diagnostics_follow_the_message_rules() {
    let inputs = [
        "component T {\n    veiw {}\n}\n",
        "component T {\n    view {\n        text { \"a\" } text { \"b\" };\n    }\n}\n",
        "component T {\n    view {\n        if a && b || !c { x when y }\n    }\n}\n",
        "component T {\n    style {\n        color: red;\n        #main .a button { margin: 0 !important; padding: 4pz }\n    }\n}\n",
        "component T {\n    failed() {}\n    failed(let {}\n",
        ") ] } garbage $ 1 2 3\n",
    ];
    for text in inputs {
        let parse = check(text);
        assert!(
            !parse.diagnostics().is_empty(),
            "no diagnostics for {text:?}"
        );
        for diagnostic in parse.diagnostics() {
            check_diagnostic_style(diagnostic);
        }
    }
}

fn check_diagnostic_style(diagnostic: &Diagnostic) {
    assert!(
        diagnostic.code.info().is_some(),
        "unregistered code {}",
        diagnostic.code
    );
    let message = &diagnostic.message;
    assert!(!message.is_empty(), "empty message: {diagnostic:?}");
    assert!(
        !message.starts_with(char::is_uppercase),
        "message must start lowercase: {message:?}"
    );
    assert!(
        !message.ends_with('.'),
        "message must not end with a period: {message:?}"
    );
    assert!(
        !message.contains('\n'),
        "message must be one line: {message:?}"
    );
    for fix in &diagnostic.fixes {
        assert!(!fix.edits.is_empty(), "fix without edits: {fix:?}");
        assert!(!fix.message.is_empty(), "fix without message: {fix:?}");
    }
}

#[test]
fn garbage_inputs_never_crash() {
    let fixed = [
        "",
        "}}}}}}}}",
        "{{{{{{{{",
        "view {",
        "style {",
        "style { .a {",
        "style { .a { color: {",
        "style { .a { color: \"{",
        "component T { view { box { \"{ }",
        "component T { style { @media ( { } } }",
        "component T { style { ::: &&& >>> +++ ~~~ } }",
        "component T { failed(((((( {",
        "component T { metadata { title(\"\n\") } }",
        "@@@@ #### !!!! ;;;; ....",
        "\u{0}\u{1}\u{7f}\u{feff}\u{200b}",
        "view{style{loading{failed{metadata{",
        "}{}{}{)(][",
        "\"\"\"\"\"\"{{}}{{}}",
    ];
    for text in fixed {
        check(text);
    }

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..300 {
        let len = rng.below(400);
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
        check(&String::from_utf8_lossy(&bytes));
    }
    for _ in 0..1000 {
        let count = rng.below(80);
        let text: String = (0..count)
            .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
            .collect();
        check(&text);
    }
}

#[test]
fn nonempty_garbage_is_always_diagnosed() {
    for text in [
        ")))]]]}}} ,,, ::",
        "$$$ ^^^ ```",
        "+ 1 2 \"str\" ) => .x",
        "component T { veiw { } }",
        "component T { style { color: red; } }",
    ] {
        let parse = check(text);
        assert!(
            !parse.diagnostics().is_empty(),
            "no diagnostics for {text:?}"
        );
        assert!(
            has_node(&parse.syntax(), SyntaxKind::ErrorNode)
                || parse.diagnostics().iter().any(|d| d.code == Code::new(133)),
            "garbage {text:?} produced no ErrorNode"
        );
    }
}

#[test]
fn every_prefix_of_a_full_page_parses() {
    let mut boundaries: Vec<usize> = FULL_PAGE.char_indices().map(|(i, _)| i).collect();
    boundaries.push(FULL_PAGE.len());
    for end in boundaries {
        let prefix = &FULL_PAGE[..end];
        let parse = check(prefix);
        // A prefix that stops inside the page's braces leaves them unclosed.
        let opens = prefix.matches('{').count();
        let closes = prefix.matches('}').count();
        if end < FULL_PAGE.len() - 2 && opens > closes {
            assert!(
                !parse.diagnostics().is_empty(),
                "truncated input {prefix:?} produced no diagnostics"
            );
        }
    }
}

#[test]
fn deleting_any_single_character_never_crashes() {
    for (index, c) in FULL_PAGE.char_indices() {
        let mut text = String::with_capacity(FULL_PAGE.len());
        text.push_str(&FULL_PAGE[..index]);
        text.push_str(&FULL_PAGE[index + c.len_utf8()..]);
        check(&text);
    }
}

#[test]
fn duplicating_any_line_never_crashes() {
    let lines: Vec<&str> = FULL_PAGE.split_inclusive('\n').collect();
    for index in 0..lines.len() {
        let mut text = String::with_capacity(FULL_PAGE.len() + 80);
        for (i, line) in lines.iter().enumerate() {
            text.push_str(line);
            if i == index {
                text.push_str(line);
            }
        }
        check(&text);
    }
}

#[test]
fn deeply_nested_valid_views_parse_without_diagnostics() {
    let depth = 256;
    let mut text = String::from("component T {\n    view {\n");
    for _ in 0..depth {
        text.push_str("box {\n");
    }
    text.push_str("\"leaf\"\n");
    for _ in 0..depth {
        text.push_str("}\n");
    }
    text.push_str("    }\n}\n");
    let parse = check(&text);
    assert!(parse.diagnostics().is_empty(), "{:#?}", parse.diagnostics());
}

#[test]
fn deeply_nested_valid_styles_parse_without_diagnostics() {
    let depth = 256;
    let mut text = String::from("component T {\n    style {\n");
    for i in 0..depth {
        text.push_str(&format!(".l{i} {{\n"));
    }
    text.push_str("color: red;\n");
    for _ in 0..depth {
        text.push_str("}\n");
    }
    text.push_str("    }\n}\n");
    let parse = check(&text);
    assert!(parse.diagnostics().is_empty(), "{:#?}", parse.diagnostics());
}

#[test]
fn pathological_nesting_does_not_crash() {
    // Run on a thread with a generous, explicit stack so the test measures
    // the parser's own behavior rather than the test harness's default.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            for opener in ["{", "(", "[", "box {", "f(", "\"{", ".a {"] {
                let text = format!("component T {{\n    view {{\n{}", opener.repeat(5000));
                check(&text);
            }
            let style = format!("component T {{\n    style {{\n{}", ".a { ".repeat(5000));
            check(&style);
        })
        .expect("spawn test thread");
    assert!(handle.join().is_ok(), "parser panicked on deep nesting");
}

#[test]
fn many_errors_do_not_cascade() {
    // Each line has exactly one error; the count of diagnostics must equal
    // the number of lines, so no error produces follow-on noise.
    let mut text = String::from("component T {\n    view {\n");
    let n = 200;
    for i in 0..n {
        match i % 4 {
            0 => text.push_str(&format!("text {{ \"{i}\" }} text {{ \"x\" }}\n")),
            1 => text.push_str(&format!("box {{}}.disabled(!f{i})\n")),
            2 => text.push_str(&format!("if a{i} && b {{}}\n")),
            _ => text.push_str(&format!("text {{ \"{i}\" }};\n")),
        }
    }
    text.push_str("    }\n}\n");
    let parse = check(&text);
    let expected: BTreeMap<u16, usize> = [(102, 50), (103, 50), (104, 50), (106, 50)]
        .into_iter()
        .collect();
    assert_eq!(codes(&parse), expected);
}
