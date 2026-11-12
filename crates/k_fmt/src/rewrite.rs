//! The canonical rewrites: the redundant spellings the formatter replaces
//! (`docs/design/56-formatter.md`, "Canonical rewrites").
//!
//! Each rewrite is a list of text [`Edit`]s computed from the syntax tree.
//! [`tokens`] changes tokens in place; [`moves`] moves whole declarations
//! and runs on the tree that [`tokens`]'s edits produced, so the edits of
//! one pass never overlap.

use k_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextRange, TextSize};

/// Replaces `range` of the text with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    /// The byte range replaced; empty for an insertion.
    pub(crate) range: TextRange,
    /// The replacement.
    pub(crate) text: String,
}

impl Edit {
    /// An edit deleting `range`.
    fn delete(range: TextRange) -> Edit {
        Edit {
            range,
            text: String::new(),
        }
    }
}

/// Applies `edits` to `text`. Edits are applied in order of their ranges;
/// one that overlaps an earlier one is dropped (the rewrites never produce
/// such a pair, and the next `format` run would apply it).
pub(crate) fn apply(text: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| (e.range.start(), e.range.end()));
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for edit in edits {
        let (start, end) = (
            usize::from(edit.range.start()),
            usize::from(edit.range.end()),
        );
        if start < at {
            continue;
        }
        out.push_str(&text[at..start]);
        out.push_str(&edit.text);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// The member kinds whose `public` is redundant (K1302) and whose
/// `private` is checked by sema: `docs/design/16-modules.md`, "Visibility
/// of members".
const MEMBERS: [K; 5] = [
    K::FieldDecl,
    K::FuncDecl,
    K::InitDecl,
    K::RpcDecl,
    K::EnumCase,
];

/// The nodes that hold a comma-separated list closed by `)`, `]` or `>`.
const LISTS: [K; 13] = [
    K::ArgList,
    K::ParamList,
    K::GenericParamList,
    K::GenericArgList,
    K::ListExpr,
    K::MapExpr,
    K::TupleExpr,
    K::TupleType,
    K::FuncType,
    K::EnumCase,
    K::EnumCasePattern,
    K::TuplePattern,
    K::LambdaParamList,
];

/// The pattern node kinds.
const PATTERNS: [K; 7] = [
    K::WildcardPattern,
    K::BindingPattern,
    K::IdentPattern,
    K::LiteralPattern,
    K::RangePattern,
    K::EnumCasePattern,
    K::TuplePattern,
];

/// The rewrites that change tokens in place: redundant member modifiers,
/// `case _` arms, trailing commas and the last style declaration's `;`.
pub(crate) fn tokens(root: &SyntaxNode) -> Vec<Edit> {
    let mut edits = Vec::new();
    for node in root.descendants() {
        let kind = node.kind();
        if MEMBERS.contains(&kind) && node.parent().is_some_and(|p| p.kind() == K::MemberList) {
            member_modifier(&node, &mut edits);
        }
        if kind == K::MatchArm {
            wildcard_arm(&node, &mut edits);
        }
        if LISTS.contains(&kind) {
            trailing_comma(&node, &mut edits);
        }
        if kind == K::Declaration
            && !node
                .children_with_tokens()
                .any(|c| c.kind() == K::Semicolon)
        {
            edits.push(Edit {
                range: TextRange::empty(node.text_range().end()),
                text: ";".to_owned(),
            });
        }
    }
    edits
}

/// The direct child tokens of `node`.
fn child_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + '_ {
    node.children_with_tokens()
        .filter_map(NodeOrToken::into_token)
}

/// Removes `token` and the spaces after it on its line.
fn remove_word(token: &SyntaxToken, edits: &mut Vec<Edit>) {
    let mut end = token.text_range().end();
    if let Some(next) = token.next_token()
        && next.kind() == K::Whitespace
    {
        end = next.text_range().end();
    }
    edits.push(Edit::delete(TextRange::new(
        token.text_range().start(),
        end,
    )));
}

/// `public` on a member is redundant (K1302; on an `@Inject` field it is
/// K1303 with the same fix), and so is `private` on an `@Inject` field.
fn member_modifier(member: &SyntaxNode, edits: &mut Vec<Edit>) {
    let inject = member.children().any(|a| {
        a.kind() == K::Annotation
            && child_tokens(&a).any(|t| t.kind() == K::Ident && t.text() == "Inject")
    });
    for token in child_tokens(member) {
        match token.kind() {
            K::PublicKw => remove_word(&token, edits),
            K::PrivateKw if inject && member.kind() == K::FieldDecl => remove_word(&token, edits),
            _ => {}
        }
    }
}

/// A top-level `case _` arm without a guard is written `default` (K2303).
fn wildcard_arm(arm: &SyntaxNode, edits: &mut Vec<Edit>) {
    let Some(case) = child_tokens(arm).find(|t| t.kind() == K::CaseKw) else {
        return;
    };
    let patterns: Vec<SyntaxNode> = arm
        .children()
        .filter(|c| PATTERNS.contains(&c.kind()))
        .collect();
    let guarded = arm.children().any(|c| c.kind() == K::MatchGuard);
    if let [pattern] = patterns.as_slice()
        && pattern.kind() == K::WildcardPattern
        && pattern.text() == "_"
        && !guarded
        && !has_comment(
            arm,
            TextRange::new(case.text_range().start(), pattern.text_range().end()),
        )
    {
        edits.push(Edit {
            range: TextRange::new(case.text_range().start(), pattern.text_range().end()),
            text: "default".to_owned(),
        });
    }
}

/// A comma directly before the closing bracket of a list is removed, unless
/// the list is a one-element tuple, where it would change the meaning.
fn trailing_comma(list: &SyntaxNode, edits: &mut Vec<Edit>) {
    if matches!(list.kind(), K::TupleExpr | K::TupleType | K::TuplePattern)
        && list.children().count() < 2
    {
        return;
    }
    let significant: Vec<NodeOrToken<SyntaxNode, SyntaxToken>> = list
        .children_with_tokens()
        .filter(|c| !c.kind().is_trivia())
        .collect();
    for pair in significant.windows(2) {
        if pair[0].kind() == K::Comma
            && matches!(pair[1].kind(), K::RParen | K::RBrack | K::Gt)
            && let NodeOrToken::Token(comma) = &pair[0]
        {
            edits.push(Edit::delete(comma.text_range()));
        }
    }
}

/// The rewrites that move declarations: the imports are sorted, and a
/// service's `init()` goes after its stored fields and before its functions
/// (`docs/design/10-services.md`).
pub(crate) fn moves(root: &SyntaxNode) -> Vec<Edit> {
    let text = root.text().to_string();
    let mut edits = Vec::new();
    sort_imports(root, &text, &mut edits);
    for service in root.descendants().filter(|n| n.kind() == K::ServiceDecl) {
        if let Some(list) = service.children().find(|c| c.kind() == K::MemberList) {
            place_init(&list, &text, &mut edits);
        }
    }
    edits
}

/// Returns `true` if a comment token lies inside `range`, a range of
/// `root`.
fn has_comment(root: &SyntaxNode, range: TextRange) -> bool {
    root.covering_element(range)
        .as_node()
        .unwrap_or(root)
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .any(|t| {
            t.kind().is_trivia()
                && !matches!(t.kind(), K::Whitespace | K::Newline)
                && range.contains_range(t.text_range())
        })
}

/// Sorts the imports at the top of the file by their text, one per line,
/// unless a comment sits among them.
fn sort_imports(root: &SyntaxNode, text: &str, edits: &mut Vec<Edit>) {
    let imports: Vec<SyntaxNode> = root
        .children()
        .take_while(|c| c.kind() == K::Import)
        .collect();
    let (Some(first), Some(last)) = (imports.first(), imports.last()) else {
        return;
    };
    let range = TextRange::new(first.text_range().start(), last.text_range().end());
    if imports.len() < 2 || has_comment(root, range) {
        return;
    }
    // Each import as its words joined by one space, so the order doesn't
    // depend on the spacing (an import path has no whitespace inside).
    let words = |import: &SyntaxNode| {
        import
            .children_with_tokens()
            .filter(|c| !c.kind().is_trivia())
            .map(|c| match c {
                NodeOrToken::Node(n) => n.text().to_string(),
                NodeOrToken::Token(t) => t.text().to_owned(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut lines: Vec<String> = imports.iter().map(words).collect();
    lines.sort();
    let sorted = lines.join("\n");
    if sorted != text[range] {
        edits.push(Edit {
            range,
            text: sorted,
        });
    }
}

/// The start of the line holding `at`, including the comment-only lines
/// directly above it, if only spaces precede `at` on its line.
fn chunk_start(text: &str, at: usize) -> Option<usize> {
    let mut start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    if !text[start..at].trim().is_empty() {
        return None;
    }
    while start > 0 {
        let above = text[..start - 1].rfind('\n').map_or(0, |i| i + 1);
        if text[above..start - 1].trim_start().starts_with("//") {
            start = above;
        } else {
            break;
        }
    }
    Some(start)
}

/// The end of the line holding `at`, past its newline, if only spaces or a
/// line comment follow `at` on its line.
fn chunk_end(text: &str, at: usize) -> Option<usize> {
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
    let rest = text[at..end].trim();
    (rest.is_empty() || rest.starts_with("//")).then_some(end)
}

/// Moves a service's `init()` after its last stored field, or, with no
/// stored fields, before its first function or rpc.
fn place_init(list: &SyntaxNode, text: &str, edits: &mut Vec<Edit>) {
    let members: Vec<SyntaxNode> = list.children().collect();
    let inits: Vec<usize> = (0..members.len())
        .filter(|&i| members[i].kind() == K::InitDecl)
        .collect();
    let [init] = inits.as_slice() else {
        return;
    };
    let stored =
        |n: &SyntaxNode| n.kind() == K::FieldDecl && !n.children().any(|c| c.kind() == K::Block);
    let function = |n: &SyntaxNode| matches!(n.kind(), K::FuncDecl | K::RpcDecl);
    let others: Vec<&SyntaxNode> = members
        .iter()
        .enumerate()
        .filter(|(i, _)| i != init)
        .map(|(_, n)| n)
        .collect();
    // The index among `others` the init goes before.
    let target = match others.iter().rposition(|n| stored(n)) {
        Some(last) => last + 1,
        None => match others.iter().position(|n| function(n)) {
            Some(first) => first,
            None => return,
        },
    };
    if target == *init {
        return;
    }
    let node = &members[*init];
    let span = |n: &SyntaxNode| -> Option<(usize, usize)> {
        let start = chunk_start(text, usize::from(n.text_range().start()))?;
        let end = chunk_end(text, usize::from(n.text_range().end()))?;
        Some((start, end))
    };
    let Some((start, end)) = span(node) else {
        return;
    };
    let mut chunk = text[start..end].to_owned();
    if !chunk.ends_with('\n') {
        chunk.push('\n');
    }
    let (at, inserted) = if target > 0 && stored(others[target - 1]) {
        let Some((_, after)) = span(others[target - 1]) else {
            return;
        };
        (after, format!("\n{chunk}"))
    } else {
        let Some((before, _)) = span(others[target]) else {
            return;
        };
        (before, format!("{chunk}\n"))
    };
    if (start..end).contains(&at) {
        return;
    }
    let to_size = |i: usize| TextSize::try_from(i).unwrap_or_default();
    edits.push(Edit::delete(TextRange::new(to_size(start), to_size(end))));
    edits.push(Edit {
        range: TextRange::empty(to_size(at)),
        text: inserted,
    });
}
