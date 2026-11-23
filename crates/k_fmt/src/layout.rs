//! Layout: the whitespace of a file, recomputed from its syntax tree
//! (`docs/design/56-formatter.md`, "Layout").
//!
//! The file is a sequence of *items*: every token except whitespace and
//! newlines, comments included. Between two items the layout decides one
//! of three things: nothing, one space, or a line break with zero or one
//! blank lines. Line breaks are the author's except where a rule adds one
//! (chained modifiers of a view node) or removes one (`}` `else`, a body's
//! `{` on its own line); those are exactly the places where
//! `docs/compiler/grammar.md` §5 makes a newline insignificant. Each line
//! is then indented by four spaces per level, computed from the brackets
//! open at its start and from whether its first token starts an element
//! (a statement, member, argument, …) or continues one.

use std::collections::{HashMap, HashSet};

use k_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextSize};

use crate::tree::comment_text;

/// One indentation level.
const INDENT: &str = "    ";

/// Lays out the file whose (error-free) tree is `root`.
pub(crate) fn layout(root: &SyntaxNode) -> String {
    Layout::new(root).run()
}

/// Returns `true` for comment tokens.
fn is_comment(kind: K) -> bool {
    matches!(kind, K::LineComment | K::BlockComment | K::DocComment)
}

/// Returns `true` for a `<` or `>` of a generic parameter or argument list.
fn is_generic_bracket(token: &SyntaxToken) -> bool {
    matches!(token.kind(), K::Lt | K::Gt)
        && matches!(
            parent_kind(token),
            Some(K::GenericArgList | K::GenericParamList)
        )
}

/// Returns `true` for brackets that open an indentation level.
fn is_opener(token: &SyntaxToken) -> bool {
    matches!(
        token.kind(),
        K::LBrace | K::LParen | K::LBrack | K::InterpStart
    ) || token.kind() == K::Lt && is_generic_bracket(token)
}

/// Returns `true` for brackets that close an indentation level.
fn is_closer(token: &SyntaxToken) -> bool {
    matches!(
        token.kind(),
        K::RBrace | K::RParen | K::RBrack | K::InterpEnd
    ) || token.kind() == K::Gt && is_generic_bracket(token)
}

/// Returns the kind of `token`'s parent node.
fn parent_kind(token: &SyntaxToken) -> Option<K> {
    token.parent().map(|p| p.kind())
}

/// Returns the first token of `node` that isn't trivia.
fn first_significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
}

/// Returns the last token of `node` that isn't trivia.
fn last_significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut token = node.last_token()?;
    while token.kind().is_trivia() {
        token = token.prev_token()?;
        if !node.text_range().contains_range(token.text_range()) {
            return None;
        }
    }
    Some(token)
}

/// The nodes whose children after their opening bracket are elements: each
/// starts a line at the bracket's inner indentation.
fn is_container(kind: K) -> bool {
    matches!(
        kind,
        K::Block
            | K::MemberList
            | K::LambdaExpr
            | K::MatchExpr
            | K::ThemeVariant
            | K::ArgList
            | K::ParamList
            | K::GenericArgList
            | K::GenericParamList
            | K::ListExpr
            | K::MapExpr
            | K::TupleExpr
            | K::ParenExpr
            | K::TupleType
            | K::ParenType
            | K::FuncType
            | K::LambdaParamList
            | K::EnumCase
            | K::EnumCasePattern
            | K::TuplePattern
            | K::StyleBlock
            | K::AtRule
            | K::KeyframesRule
            | K::CssFunction
            | K::MediaFeature
            | K::PseudoClass
    )
}

/// Returns `true` if `node` is an element: a child of the file, or of a
/// container after the container's opening bracket. A style rule's
/// selectors and its items are all elements.
fn is_element(node: &SyntaxNode) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        K::SourceFile | K::StyleRule | K::KeyframeBlock => true,
        kind if is_container(kind) => parent
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .find(|t| matches!(t.kind(), K::LBrace | K::LParen | K::LBrack | K::Lt))
            .is_some_and(|open| node.text_range().start() >= open.text_range().end()),
        _ => false,
    }
}

/// The first token of the innermost node that contains `token` and starts
/// before it, if `token` continues its element: a continuation line is
/// indented one level from the line that node starts on (`implements A,`
/// then `B` is indented from `implements`). `None` if `token` starts its
/// element.
fn anchor(token: &SyntaxToken) -> Option<SyntaxToken> {
    for node in token.parent_ancestors() {
        let first = first_significant(&node)?;
        if first != *token {
            return Some(first);
        }
        if is_element(&node) {
            return None;
        }
    }
    None
}

/// Returns `true` if `token` is inside a style block and not inside a K
/// interpolation within it.
fn in_style(token: &SyntaxToken) -> bool {
    for node in token.parent_ancestors() {
        match node.kind() {
            K::Interpolation => return false,
            K::StyleBlock => return true,
            _ => {}
        }
    }
    false
}

/// The binary operators written with a space on each side.
fn is_spaced_operator(kind: K) -> bool {
    matches!(
        kind,
        K::Plus
            | K::Minus
            | K::Star
            | K::Slash
            | K::Percent
            | K::EqEq
            | K::BangEq
            | K::Lt
            | K::LtEq
            | K::Gt
            | K::GtEq
            | K::QuestionQuestion
            | K::Eq
            | K::PlusEq
            | K::MinusEq
            | K::StarEq
            | K::SlashEq
            | K::PercentEq
            | K::AndKw
            | K::OrKw
    )
}

/// Whether one space separates `prev` and `next` on a line, in code.
/// `had` says whether the input had whitespace there; it decides the
/// pairs no rule covers, so the layout never joins or splits tokens it
/// doesn't understand.
fn code_space(prev: &SyntaxToken, next: &SyntaxToken, had: bool) -> bool {
    let (p, n) = (prev.kind(), next.kind());
    let (pp, np) = (parent_kind(prev), parent_kind(next));
    let generic = |k: Option<K>| matches!(k, Some(K::GenericArgList | K::GenericParamList));
    if pp == Some(K::ImportPath) && np == Some(K::ImportPath) {
        return false;
    }
    if n == K::Comma {
        return false;
    }
    if p == K::Comma {
        return true;
    }
    if matches!(n, K::RParen | K::RBrack) || matches!(p, K::LParen | K::LBrack) {
        return false;
    }
    if p == K::LBrace {
        return n != K::RBrace;
    }
    if matches!(n, K::LBrace | K::RBrace) {
        return true;
    }
    if matches!(p, K::Dot | K::QuestionDot) {
        return false;
    }
    if matches!(n, K::Dot | K::QuestionDot) {
        let leading = next.parent().and_then(|n| first_significant(&n)).as_ref() == Some(next);
        match np {
            Some(K::MemberExpr | K::PathType | K::Guard) => return false,
            Some(K::EnumCasePattern) if !leading => return false,
            _ => {}
        }
    }
    if (n == K::Lt || n == K::Gt) && generic(np) || p == K::Lt && generic(pp) {
        return false;
    }
    if n == K::Question || p == K::At {
        return false;
    }
    // `preview (value: Int = 1)` and `preview "name" (…)`: the parameters
    // are a clause of the header, not a call.
    if n == K::LParen
        && np == Some(K::ParamList)
        && next.parent().and_then(|l| l.parent()).map(|d| d.kind()) == Some(K::PreviewDecl)
    {
        return true;
    }
    if n == K::LParen
        && matches!(
            np,
            Some(K::ArgList | K::ParamList | K::EnumCase | K::EnumCasePattern | K::FailedBlock)
        )
    {
        return false;
    }
    if n == K::LBrack && np == Some(K::IndexExpr) {
        return false;
    }
    if n == K::Colon {
        return false;
    }
    if p == K::Colon {
        return n != K::RBrack;
    }
    if matches!(p, K::Arrow | K::FatArrow) || matches!(n, K::Arrow | K::FatArrow) {
        return true;
    }
    if p == K::Minus && matches!(pp, Some(K::PrefixExpr | K::LiteralPattern)) {
        return false;
    }
    let range = |k: K, parent: Option<K>| {
        matches!(k, K::DotDotLt | K::DotDotDot)
            && matches!(parent, Some(K::BinaryExpr | K::RangePattern))
    };
    if range(p, pp) || range(n, np) {
        return false;
    }
    if is_spaced_operator(p) && !generic(pp) || is_spaced_operator(n) && !generic(np) {
        return true;
    }
    if p.is_keyword() || p.is_contextual_keyword() || n.is_keyword() || n.is_contextual_keyword() {
        return true;
    }
    had
}

/// Whether one space separates `prev` and `next` on a line, in a style
/// block. Outside the rules below the input's spacing is kept, because
/// whitespace is significant in selectors (`.a .b`, `.a:hover`) and in
/// values.
fn style_space(prev: &SyntaxToken, next: &SyntaxToken, had: bool) -> bool {
    let (p, n) = (prev.kind(), next.kind());
    let property =
        |t: &SyntaxToken| matches!(parent_kind(t), Some(K::Declaration | K::MediaFeature));
    match (p, n) {
        (_, K::Semicolon | K::Comma) => false,
        (K::Comma | K::Semicolon, _) => true,
        (K::LBrace, K::RBrace) => false,
        (K::LBrace, _) | (_, K::LBrace | K::RBrace) => true,
        (K::Colon, _) if property(prev) => true,
        (_, K::Colon) if property(next) => false,
        _ => had,
    }
}

/// Whether one space separates two items on a line.
fn space(prev: &SyntaxToken, next: &SyntaxToken, had: bool) -> bool {
    let (p, n) = (prev.kind(), next.kind());
    if matches!(n, K::LineComment | K::DocComment) {
        return true;
    }
    if p == K::BlockComment || n == K::BlockComment {
        return had;
    }
    if matches!(p, K::StringStart | K::StringText | K::StringEscape)
        || matches!(n, K::StringText | K::StringEscape | K::StringEnd)
    {
        return had;
    }
    if p == K::InterpStart || n == K::InterpEnd {
        return false;
    }
    if in_style(prev) && in_style(next) {
        style_space(prev, next, had)
    } else {
        code_space(prev, next, had)
    }
}

/// An open bracket.
struct Open {
    /// The indentation of the line the bracket is on.
    indent: usize,
}

/// The layout of one file.
struct Layout {
    /// The items: every token except whitespace and newlines.
    items: Vec<SyntaxToken>,
    /// `newlines[i]`: the line terminators between items `i - 1` and `i`
    /// in the input.
    newlines: Vec<usize>,
    /// `spaced[i]`: whether the input had any whitespace before item `i`.
    spaced: Vec<bool>,
    /// The item index of each item token, by its start offset.
    index: HashMap<TextSize, usize>,
    /// Items that start a line although the input had them on the line of
    /// the item before (chained view modifiers).
    break_before: HashSet<usize>,
    /// Items joined to the line before by one space (`else`, `catch`, a
    /// body's `{`).
    join_before: HashSet<usize>,
    /// Items that get exactly one blank line before them (sibling view
    /// nodes).
    blank_before: HashSet<usize>,
    /// The output.
    out: String,
    /// The indentation of each output line.
    line_indent: Vec<usize>,
    /// For each output line with code: whether its first token is a
    /// closing bracket.
    line_first: Vec<Option<bool>>,
    /// The output line of each token placed so far, by item index.
    line_of: HashMap<usize, usize>,
    /// The brackets open at the current point.
    open: Vec<Open>,
    /// The offset of a member chain's continuation lines (0 or 1 level), by
    /// the item index of the element's first token.
    chain_offset: HashMap<usize, usize>,
}

impl Layout {
    /// Collects the items of `root` and the rules that apply to them.
    fn new(root: &SyntaxNode) -> Layout {
        let mut items = Vec::new();
        let mut newlines = Vec::new();
        let mut spaced = Vec::new();
        let (mut pending_newlines, mut pending_space) = (0, false);
        for token in root
            .descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
        {
            match token.kind() {
                K::Newline => {
                    pending_newlines += 1;
                    pending_space = true;
                }
                K::Whitespace => pending_space = true,
                _ => {
                    items.push(token);
                    newlines.push(pending_newlines);
                    spaced.push(pending_space);
                    pending_newlines = 0;
                    pending_space = false;
                }
            }
        }
        let index = items
            .iter()
            .enumerate()
            .map(|(i, t)| (t.text_range().start(), i))
            .collect();
        let mut layout = Layout {
            items,
            newlines,
            spaced,
            index,
            break_before: HashSet::new(),
            join_before: HashSet::new(),
            blank_before: HashSet::new(),
            out: String::new(),
            line_indent: Vec::new(),
            line_first: Vec::new(),
            line_of: HashMap::new(),
            open: Vec::new(),
            chain_offset: HashMap::new(),
        };
        layout.find_joins();
        let items: Vec<SyntaxNode> = root.children().collect();
        for pair in items.windows(2) {
            let both = |k: K| pair[0].kind() == k && pair[1].kind() == k;
            if !both(K::Import) && !both(K::FieldDecl) {
                layout.separate(&pair[0], &pair[1]);
            }
        }
        for body in view_bodies(root) {
            layout.view_body(&body);
        }
        layout
    }

    /// The item index of `token`.
    fn item(&self, token: &SyntaxToken) -> Option<usize> {
        self.index.get(&token.text_range().start()).copied()
    }

    /// Marks `else` and `catch` after `}`, and a body's `{` on its own
    /// line, to join the line before.
    fn find_joins(&mut self) {
        for i in 1..self.items.len() {
            if self.newlines[i] == 0 || is_comment(self.items[i - 1].kind()) {
                continue;
            }
            let token = &self.items[i];
            let join = match token.kind() {
                K::ElseKw | K::CatchKw => self.items[i - 1].kind() == K::RBrace,
                K::LBrace => body_owner(token).is_some(),
                _ => false,
            };
            if join {
                self.join_before.insert(i);
            }
        }
    }

    /// Puts one blank line between the siblings `a` and `b`, at the first
    /// line break after `a`: before `b`, or before the comments above it.
    fn separate(&mut self, a: &SyntaxNode, b: &SyntaxNode) {
        let (Some(end), Some(start)) = (
            last_significant(a).and_then(|t| self.item(&t)),
            first_significant(b).and_then(|t| self.item(&t)),
        ) else {
            return;
        };
        if let Some(first_break) = (end + 1..=start).find(|&i| self.newlines[i] > 0) {
            self.blank_before.insert(first_break);
        }
    }

    /// Applies the view rules to the statements of a view body: one blank
    /// line between sibling view nodes, and each of two or more chained
    /// modifier calls on its own line.
    fn view_body(&mut self, body: &SyntaxNode) {
        let statements: Vec<SyntaxNode> = body
            .children()
            .filter(|c| c.kind() != K::LambdaParamList)
            .collect();
        for pair in statements.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if a.kind() != K::LetStmt && b.kind() != K::LetStmt {
                self.separate(a, b);
            }
        }
        for statement in &statements {
            let dots = chain(statement);
            if dots.len() >= 2 {
                for dot in dots {
                    if let Some(i) = self.item(&dot) {
                        self.break_before.insert(i);
                    }
                }
            }
        }
    }

    /// Writes the output.
    fn run(mut self) -> String {
        for i in 0..self.items.len() {
            let token = self.items[i].clone();
            let new_line = i == 0
                || !self.join_before.contains(&i)
                    && (self.newlines[i] > 0
                        || self.break_before.contains(&i)
                        || matches!(self.items[i - 1].kind(), K::LineComment | K::DocComment));
            if new_line {
                self.start_line(i);
            } else if space(
                &self.items[i - 1],
                &token,
                self.spaced[i] || self.join_before.contains(&i),
            ) {
                self.out.push(' ');
            }
            if is_comment(token.kind()) {
                self.out.push_str(&comment_text(token.text()));
            } else {
                self.out.push_str(token.text());
                let line = self.line_indent.len() - 1;
                self.line_of.insert(i, line);
                if self.line_first[line].is_none() {
                    self.line_first[line] = Some(is_closer(&token));
                }
                if is_closer(&token) {
                    self.open.pop();
                }
                if is_opener(&token) {
                    // A body is indented from the line its declaration or
                    // statement starts on, even when a clause before the
                    // `{` was wrapped onto a continuation line.
                    let indent = body_owner(&token)
                        .and_then(|owner| first_significant(&owner))
                        .and_then(|first| self.item(&first))
                        .and_then(|first| self.line_of.get(&first))
                        .map_or(self.line_indent[line], |&l| self.line_indent[l]);
                    self.open.push(Open { indent });
                }
            }
        }
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        align_trailing_comments(&self.out)
    }

    /// Starts the output line whose first item is item `i`.
    fn start_line(&mut self, i: usize) {
        if i > 0 {
            let blanks = self.blank_lines(i);
            for _ in 0..=blanks {
                self.out.push('\n');
            }
        }
        let indent = self.line_indent_for(i);
        for _ in 0..indent {
            self.out.push_str(INDENT);
        }
        self.line_indent.push(indent);
        self.line_first.push(None);
    }

    /// The number of blank lines (0 or 1) before item `i`, which starts a
    /// line.
    fn blank_lines(&self, i: usize) -> usize {
        if self.blank_before.contains(&i) {
            return 1;
        }
        let (prev, token) = (&self.items[i - 1], &self.items[i]);
        if self.newlines[i] < 2
            || is_opener(prev)
            || prev.kind() == K::DocComment
            || is_closer(token)
        {
            return 0;
        }
        if !is_comment(token.kind()) && anchor(token).is_some() {
            return 0;
        }
        1
    }

    /// The indentation of the line starting with item `i`.
    fn line_indent_for(&mut self, i: usize) -> usize {
        if !is_comment(self.items[i].kind()) {
            return self.indent_of(i);
        }
        // A comment line is indented like the code that follows it; before
        // a closing bracket, like the lines inside the bracket.
        let mut same_line = true;
        for j in i + 1..self.items.len() {
            if self.newlines[j] > 0 || self.break_before.contains(&j) {
                same_line = false;
            }
            if is_comment(self.items[j].kind()) {
                continue;
            }
            if !same_line && is_closer(&self.items[j]) {
                return self.inner_indent();
            }
            return self.indent_of(j);
        }
        self.inner_indent()
    }

    /// The indentation inside the innermost open bracket.
    fn inner_indent(&self) -> usize {
        self.open.last().map_or(0, |o| o.indent + 1)
    }

    /// The indentation of a line whose first code token is item `i`.
    fn indent_of(&mut self, i: usize) -> usize {
        let token = self.items[i].clone();
        if is_closer(&token) {
            return self.open.last().map_or(0, |o| o.indent);
        }
        let inner = self.inner_indent();
        let Some(first) = anchor(&token) else {
            return inner;
        };
        let Some(first) = self.item(&first) else {
            return inner;
        };
        let Some(&line) = self.line_of.get(&first) else {
            return inner;
        };
        let base = self.line_indent[line];
        let previous_code = self.items[..i].iter().rev().find(|t| !is_comment(t.kind()));
        let after_brace = previous_code.is_some_and(|t| t.kind() == K::RBrace);
        let after_annotation = previous_code
            .and_then(|prev| {
                let annotation = prev
                    .parent_ancestors()
                    .find(|n| n.kind() == K::Annotation)?;
                (last_significant(&annotation).as_ref() == Some(prev)).then_some(annotation)
            })
            .and_then(|annotation| annotation.parent())
            .is_some_and(|owner| token.parent_ancestors().any(|n| n == owner));
        let offset = match token.kind() {
            _ if after_annotation => 0,
            // `else` or `catch` after a `}` it couldn't join (a comment is
            // between them) lines up with the `}`.
            K::ElseKw | K::CatchKw if after_brace => 0,
            K::Dot | K::QuestionDot => {
                if let Some(&offset) = self.chain_offset.get(&first) {
                    offset
                } else {
                    // A chain after a multi-line node whose `}` is back at
                    // the node's indentation stays at that indentation.
                    let previous = (0..self.line_first.len())
                        .rev()
                        .find_map(|l| self.line_first[l].map(|k| (k, self.line_indent[l])));
                    let offset = match previous {
                        Some((true, indent)) if indent == base => 0,
                        _ => 1,
                    };
                    self.chain_offset.insert(first, offset);
                    offset
                }
            }
            _ => 1,
        };
        (base + offset).max(inner)
    }
}

/// Aligns the trailing line comments of adjacent lines with the same
/// indentation to one column, one space after the longest code among them.
/// A trailing comment on its own gets one space.
fn align_trailing_comments(text: &str) -> String {
    let lexed = k_syntax::lex(k_diagnostics::FileId::new(0), text);
    // The byte offset of each trailing comment.
    let mut trailing = HashSet::new();
    for index in 0..lexed.len() {
        let (Some(kind), Some(range)) = (lexed.kind(index), lexed.range(index)) else {
            continue;
        };
        if matches!(kind, K::LineComment | K::DocComment) {
            let start = usize::from(range.start());
            let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
            if !text[line_start..start].trim().is_empty() {
                trailing.insert(start);
            }
        }
    }
    if trailing.is_empty() {
        return text.to_owned();
    }
    // Each line: its text and, with a trailing comment, where it starts.
    let mut lines: Vec<(&str, Option<usize>)> = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let comment = (0..line.len()).find(|i| trailing.contains(&(offset + i)));
        lines.push((line, comment));
        offset += line.len();
    }
    let indent = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let mut out = String::with_capacity(text.len() + 64);
    let mut i = 0;
    while i < lines.len() {
        let Some(_) = lines[i].1 else {
            out.push_str(lines[i].0);
            i += 1;
            continue;
        };
        let mut end = i + 1;
        while end < lines.len()
            && lines[end].1.is_some()
            && indent(lines[end].0) == indent(lines[i].0)
        {
            end += 1;
        }
        let code = |(line, comment): &(&str, Option<usize>)| {
            line[..comment.unwrap_or(0)].trim_end().to_owned()
        };
        let width = lines[i..end]
            .iter()
            .map(|l| code(l).chars().count())
            .max()
            .unwrap_or(0);
        for line in &lines[i..end] {
            let code = code(line);
            out.push_str(&code);
            for _ in code.chars().count()..=width {
                out.push(' ');
            }
            out.push_str(&line.0[line.1.unwrap_or(0)..]);
        }
        i = end;
    }
    out
}

/// The node a body's `{` belongs to, if `brace` opens a body: the
/// declaration or statement of a `Block` or `MemberList`, or the `match`,
/// style rule or theme variant itself. A body's `{` may move up to the line
/// before (grammar.md §5, N3), unlike a lambda's.
fn body_owner(brace: &SyntaxToken) -> Option<SyntaxNode> {
    if brace.kind() != K::LBrace {
        return None;
    }
    let parent = brace.parent()?;
    match parent.kind() {
        K::Block | K::MemberList => parent.parent(),
        K::MatchExpr
        | K::StyleBlock
        | K::ThemeVariant
        | K::StyleRule
        | K::AtRule
        | K::KeyframesRule
        | K::KeyframeBlock => Some(parent),
        _ => None,
    }
}

/// The view bodies of the file: the blocks of `view`, `loading` and
/// `failed`, and inside them the blocks of `if`, `for` and `match` view
/// nodes and the trailing lambdas of elements and their modifiers.
fn view_bodies(root: &SyntaxNode) -> Vec<SyntaxNode> {
    let mut bodies = Vec::new();
    let mut pending: Vec<SyntaxNode> = root
        .descendants()
        .filter(|n| matches!(n.kind(), K::ViewBlock | K::LoadingBlock | K::FailedBlock))
        .filter_map(|n| n.children().find(|c| c.kind() == K::Block))
        .collect();
    while let Some(body) = pending.pop() {
        for statement in body.children().filter(|c| c.kind() != K::LambdaParamList) {
            view_node_bodies(&statement, &mut pending);
        }
        bodies.push(body);
    }
    bodies
}

/// Adds the bodies nested in the view node `node` to `out`.
fn view_node_bodies(node: &SyntaxNode, out: &mut Vec<SyntaxNode>) {
    match node.kind() {
        K::IfExpr => {
            for child in node.children() {
                match child.kind() {
                    K::Block => out.push(child),
                    K::IfExpr => view_node_bodies(&child, out),
                    _ => {}
                }
            }
        }
        K::ForStmt => out.extend(node.children().filter(|c| c.kind() == K::Block)),
        K::MatchExpr => {
            for arm in node.children().filter(|c| c.kind() == K::MatchArm) {
                out.extend(arm.children().filter(|c| c.kind() == K::Block));
            }
        }
        K::CallExpr => {
            // A `metadata { … }` block inside a view holds calls, not view
            // nodes (docs/design/22-metadata.md).
            let metadata = node
                .first_child()
                .is_some_and(|c| c.kind() == K::NameExpr && c.text() == "metadata");
            if !metadata {
                out.extend(node.children().filter(|c| c.kind() == K::LambdaExpr));
            }
            if let Some(callee) = node.first_child() {
                view_node_bodies(&callee, out);
            }
        }
        K::MemberExpr => {
            if let Some(receiver) = node.first_child() {
                view_node_bodies(&receiver, out);
            }
        }
        _ => {}
    }
}

/// Returns `true` for a name or a path of names (`Icons.check`, `.sample`).
fn is_path(node: &SyntaxNode) -> bool {
    match node.kind() {
        K::NameExpr | K::ImplicitMemberExpr => true,
        K::MemberExpr => node.first_child().is_some_and(|r| is_path(&r)),
        _ => false,
    }
}

/// The `.` tokens of the modifier calls chained on the view node
/// `statement`, last first: `box { … }.role(.status).class(.notice)` has
/// two. A call on a path (`Icons.check()`) is the node itself, not a
/// modifier.
fn chain(statement: &SyntaxNode) -> Vec<SyntaxToken> {
    let mut dots = Vec::new();
    let mut node = statement.clone();
    while node.kind() == K::CallExpr {
        let Some(callee) = node.first_child() else {
            break;
        };
        if callee.kind() != K::MemberExpr {
            break;
        }
        let Some(dot) = callee
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .find(|t| t.kind() == K::Dot)
        else {
            break;
        };
        let Some(receiver) = callee.first_child() else {
            break;
        };
        if is_path(&receiver) {
            break;
        }
        dots.push(dot);
        node = receiver;
    }
    dots
}
