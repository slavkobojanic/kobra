//! The green-tree token helpers of `ast::support` (`token_kinds`,
//! `token_at`, `token`, `token_where`, `has_token`, `token_kind_where`)
//! answer exactly what a walk over the red children answers, on every node
//! of every golden-corpus file and of arbitrary inputs.
//!
//! They exist for speed (`docs/compiler/performance.md` §10.4: the item
//! tree is rebuilt on every revision, and rowan allocates a red node per
//! child visited), so the red walk is the oracle.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use k_diagnostics::FileId;
use k_syntax::ast::{self, AstNode, support};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, parse};
use proptest::prelude::*;

/// Every `.k` file below `tests/corpus`.
fn corpus_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "k") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out
}

/// The direct child tokens of `node`, from the red tree.
fn red_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .collect()
}

/// The name-like tokens before the `:` of a parameter, from the red tree
/// (what `Param::label` and `Param::name` were defined by).
fn red_param_names(node: &SyntaxNode) -> Vec<SyntaxToken> {
    red_tokens(node)
        .into_iter()
        .filter(|t| !t.kind().is_trivia())
        .take_while(|t| t.kind() != SyntaxKind::Colon)
        .filter(|t| t.kind().is_name_like() || t.kind() == SyntaxKind::Underscore)
        .collect()
}

/// Compares every helper with the red walk on every node of `text`;
/// returns the number of nodes checked.
fn check_text(text: &str) -> usize {
    let parse = parse(FileId::new(0), text);
    let mut nodes = 0;
    for node in parse.syntax().descendants() {
        nodes += 1;
        let red = red_tokens(&node);
        let green: Vec<(SyntaxKind, rowan::TextRange)> = support::token_kinds(&node).collect();
        let expected: Vec<(SyntaxKind, rowan::TextRange)> =
            red.iter().map(|t| (t.kind(), t.text_range())).collect();
        assert_eq!(green, expected, "token_kinds of {node:?}");
        for t in &red {
            assert_eq!(
                support::token_at(&node, t.text_range()).as_ref(),
                Some(t),
                "token_at {t:?} in {node:?}"
            );
            let first = red.iter().find(|x| x.kind() == t.kind());
            assert_eq!(support::token(&node, t.kind()).as_ref(), first);
            assert!(support::has_token(&node, t.kind()));
        }
        for kind in [
            SyntaxKind::StaticKw,
            SyntaxKind::AsyncKw,
            SyntaxKind::PublicKw,
            SyntaxKind::Ident,
        ] {
            let first = red.iter().find(|x| x.kind() == kind);
            assert_eq!(support::token(&node, kind).as_ref(), first);
            assert_eq!(support::has_token(&node, kind), first.is_some());
            assert_eq!(
                support::token_kind_where(&node, |k| k == kind),
                first.map(SyntaxToken::kind)
            );
        }
        if let Some(param) = ast::Param::cast(node.clone()) {
            let names = red_param_names(&node);
            assert_eq!(param.name(), names.last().cloned(), "{node:?}");
            let label = (names.len() == 2).then(|| names[0].clone());
            assert_eq!(param.label(), label, "{node:?}");
        }
        if let Some(field) = ast::FieldDecl::cast(node.clone()) {
            let keyword = red
                .iter()
                .find(|t| matches!(t.kind(), SyntaxKind::LetKw | SyntaxKind::VarKw));
            assert_eq!(
                field.is_var(),
                keyword.is_some_and(|t| t.kind() == SyntaxKind::VarKw)
            );
            assert_eq!(field.binding_keyword().as_ref(), keyword);
        }
    }
    nodes
}

#[test]
fn green_helpers_match_the_red_tree_on_the_corpus() {
    let files = corpus_files();
    assert!(
        files.len() > 1000,
        "found only {} corpus files",
        files.len()
    );
    let mut nodes = 0;
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        nodes += check_text(&text);
    }
    assert!(nodes > 50_000, "checked only {nodes} nodes");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Arbitrary text, recovery trees included.
    #[test]
    fn green_helpers_match_the_red_tree_on_arbitrary_text(text in "\\PC{0,200}") {
        check_text(&text);
    }

    /// Text built from K fragments, so declarations, parameters and
    /// modifiers appear.
    #[test]
    fn green_helpers_match_the_red_tree_on_k_fragments(
        parts in proptest::collection::vec(
            proptest::sample::select(&[
                "public ", "private ", "static ", "async ", "func ", "f", "(", ")",
                "for id: Int", "_ x: String", "x: Int = 1", ", ", "{", "}", "\n",
                "service S ", "let a: Int", "var b = 2", "@Inject ", "@State ",
                "-> Int ", "struct T ", ": ", "=", " ",
            ][..]),
            0..40,
        )
    ) {
        check_text(&parts.concat());
    }
}
