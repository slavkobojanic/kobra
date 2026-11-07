//! Completion (`docs/compiler/lsp.md`, "Completion").
//!
//! The context is read from the text and the syntax before the cursor:
//!
//! * after `@`: **annotations**, from the stdlib's annotation table;
//! * after `receiver.` or `receiver?.`: the **members** of the receiver's
//!   type (recorded by the checker even when the member name is still
//!   missing), its extensions' and its interfaces' members; static members
//!   and cases when the receiver names a type;
//! * after a `.` with no receiver: the **implicit members** of the expected
//!   type (`padding: .` lists `Spacing` tokens, `filter = .` a `Filter`'s
//!   cases), taken from the checker's type of the implicit member
//!   expression, or else from its context: a CSS property's value types,
//!   an assignment's target, the other side of a comparison, a `let` or
//!   field's declared type, an argument's parameter, a `return`'s function,
//!   a `case` pattern's scrutinee;
//! * anywhere else: **names in scope**: locals and parameters bound before
//!   the cursor, the enclosing declaration's members, the module's
//!   declarations, imports, the prelude, keywords and, inside a `view`,
//!   built-in elements; and the public declarations of other modules, with
//!   an edit adding their import.
//!
//! Items are filtered by the identifier prefix before the cursor (its
//! characters in order, ignoring case), so the list is complete for that
//! prefix and the editor filters further as the user types.

use std::collections::HashSet;

use k_diagnostics::{FileId, TextRange, TextSize};
use k_sema::stdlib::{DeclarationKind, annotations, css, elements};
use k_sema::view::{DeclId, DeclKind, MemberKind, ProgramView, Resolved, Type};
use k_sema::{Analysis, Cancelled, FileOrigin, Resolution};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::symbols::fuzzy;
use super::{decl_containing, render, syntax};

/// What a completion item is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    /// A local binding or parameter.
    Variable,
    /// A field or computed property.
    Field,
    /// A method.
    Method,
    /// A top-level function.
    Function,
    /// An initializer.
    Constructor,
    /// An enum case.
    EnumMember,
    /// A theme token or other static value.
    Constant,
    /// A struct, entity or config.
    Struct,
    /// A class-like declaration: component, page, service, class and the
    /// rest.
    Class,
    /// An enum.
    Enum,
    /// An interface or protocol.
    Interface,
    /// A keyword.
    Keyword,
    /// An annotation.
    Annotation,
    /// A built-in element.
    Element,
}

/// One completion item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    /// The label (and the inserted text).
    pub label: String,
    /// What it is.
    pub kind: ItemKind,
    /// The type or signature.
    pub detail: Option<String>,
    /// An edit adding the import the item needs, if any.
    pub import: Option<(TextSize, String)>,
    /// The sort group: lower sorts first (locals, members, the module,
    /// imports, the prelude, other modules, keywords).
    pub group: u8,
}

/// The hard and contextual keywords offered in code.
const KEYWORDS: &[&str] = &[
    "let",
    "var",
    "if",
    "else",
    "guard",
    "for",
    "in",
    "while",
    "match",
    "case",
    "default",
    "return",
    "throw",
    "try",
    "await",
    "do",
    "catch",
    "break",
    "continue",
    "func",
    "not",
    "and",
    "or",
    "nil",
    "true",
    "false",
    "self",
    "when",
    "bind",
    "expect",
    "untracked",
];

/// The completions at `offset` in `file`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
pub fn completions(
    analysis: &Analysis,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<CompletionItem>, Cancelled> {
    let Some(text) = analysis.text(file) else {
        return Ok(Vec::new());
    };
    let at = usize::from(offset).min(text.len());
    if !text.is_char_boundary(at) {
        return Ok(Vec::new());
    }
    // After the last character that can't be part of a name, which may be
    // more than one byte long.
    let prefix_start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let prefix = &text[prefix_start..at];
    let before = text[..prefix_start].chars().next_back();
    let Some(root) = syntax(analysis, file) else {
        return Ok(Vec::new());
    };
    let view = analysis.program()?;
    let start = TextSize::from(u32::try_from(prefix_start).unwrap_or(u32::MAX));
    let mut items = match before {
        Some('@') => annotation_items(),
        Some('.') => dot_items(analysis, view, &root, file, start)?,
        _ => scope_items(analysis, view, &root, file, offset)?,
    };
    items.retain(|i| fuzzy(prefix, &i.label).is_some());
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert((i.label.clone(), i.group)));
    items.sort_by(|a, b| a.group.cmp(&b.group).then_with(|| a.label.cmp(&b.label)));
    Ok(items)
}

/// The annotations user code may write.
fn annotation_items() -> Vec<CompletionItem> {
    annotations::ANNOTATIONS
        .iter()
        .map(|a| {
            let name = a.name.rsplit('.').next().unwrap_or(a.name);
            CompletionItem {
                label: name.to_owned(),
                kind: ItemKind::Annotation,
                detail: Some(format!("{} ({})", a.name, a.design)),
                import: None,
                group: 0,
            }
        })
        .collect()
}

/// The completions after a `.` ending at `dot_end` (the start of the
/// prefix).
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn dot_items(
    analysis: &Analysis,
    view: &ProgramView,
    root: &SyntaxNode,
    file: FileId,
    dot_end: TextSize,
) -> Result<Vec<CompletionItem>, Cancelled> {
    let Some(dot) = root.token_at_offset(dot_end).find(|t| {
        matches!(t.kind(), SyntaxKind::Dot | SyntaxKind::QuestionDot)
            && t.text_range().end() == dot_end
    }) else {
        return Ok(Vec::new());
    };
    let inside = decl_containing(view, file, dot_end);
    // `receiver.`: the receiver is the expression the dot follows.
    if let Some(receiver) = receiver_of(&dot) {
        let static_access = matches!(
            analysis.resolve_at(file, receiver.text_range())?,
            Some(Resolution::Declaration { .. })
        ) && receiver.kind() == SyntaxKind::NameExpr;
        let name_range = name_range_of(&receiver);
        let ty = analysis
            .type_info_at(file, receiver.text_range())?
            .or(analysis.type_info_at(file, name_range)?);
        let ty = match ty {
            Some(t) => Some(t),
            None if static_access => match analysis.resolve_at(file, name_range)? {
                Some(r) => match view.resolve(&r) {
                    Some(Resolved::Decl(d)) => {
                        Some(Type::Meta(Box::new(Type::Named(d, Vec::new()))))
                    }
                    _ => None,
                },
                None => None,
            },
            None => None,
        };
        let Some(ty) = ty else {
            return Ok(Vec::new());
        };
        return Ok(type_members(view, &ty, inside));
    }
    // `.name` with no receiver: the expected type's static members.
    let implicit = dot
        .parent()
        .filter(|p| p.kind() == SyntaxKind::ImplicitMemberExpr);
    if let Some(node) = &implicit {
        if let Some(ty) = analysis.type_info_at(file, node.text_range())? {
            let items = static_members(view, ty.unwrap_optional());
            if !items.is_empty() {
                return Ok(items);
            }
        }
    }
    let context = implicit.unwrap_or_else(|| dot.parent().unwrap_or_else(|| root.clone()));
    if let Some(types) = css_value_types(&context) {
        let mut items = Vec::new();
        for name in types {
            if let Some(d) = view.lookup_display(name).or_else(|| view.lookup(name)) {
                items.extend(static_members(view, &Type::Named(d, Vec::new())));
            }
        }
        return Ok(items);
    }
    match expected_type(analysis, view, file, &context)? {
        Some(ty) => Ok(static_members(view, ty.unwrap_optional())),
        None => Ok(Vec::new()),
    }
}

/// The expression a `.` or `?.` token follows, if it follows one: the
/// receiver of a member expression, or the expression just before the
/// dot.
fn receiver_of(dot: &SyntaxToken) -> Option<SyntaxNode> {
    let parent = dot.parent()?;
    if parent.kind() == SyntaxKind::MemberExpr {
        return parent
            .first_child()
            .filter(|c| c.text_range().end() <= dot.text_range().start());
    }
    if parent.kind() == SyntaxKind::ImplicitMemberExpr {
        return None;
    }
    // A dot the parser left dangling: the node ending right before it.
    let prev = dot.prev_token()?;
    if prev.kind().is_trivia() {
        return None;
    }
    prev.parent_ancestors()
        .take_while(|n| n.text_range().end() == prev.text_range().end())
        .filter(|n| is_expression(n.kind()))
        .last()
}

/// Whether nodes of `kind` are expressions.
fn is_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::NameExpr
            | SyntaxKind::MemberExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::IndexExpr
            | SyntaxKind::ParenExpr
            | SyntaxKind::Literal
            | SyntaxKind::StringLit
            | SyntaxKind::ListExpr
            | SyntaxKind::MapExpr
            | SyntaxKind::TupleExpr
            | SyntaxKind::ImplicitMemberExpr
            | SyntaxKind::AwaitExpr
            | SyntaxKind::TryExpr
    )
}

/// The range of the name of a name or member expression (the expression's
/// own range otherwise).
fn name_range_of(expr: &SyntaxNode) -> TextRange {
    expr.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| super::is_name(t.kind()))
        .last()
        .map_or_else(|| expr.text_range(), |t| t.text_range())
}

/// The members of a value of type `ty` (instance members), or of the type
/// itself for a metatype (static members and cases). Private members are
/// offered only inside their declaration (`inside`).
fn type_members(view: &ProgramView, ty: &Type, inside: Option<DeclId>) -> Vec<CompletionItem> {
    match ty {
        Type::Meta(inner) => static_members(view, inner),
        Type::Optional(inner) => type_members(view, inner, inside),
        Type::Param(g) => view
            .generics
            .get(g.0 as usize)
            .map(|info| {
                info.constraints
                    .iter()
                    .flat_map(|c| type_members(view, c, inside))
                    .collect()
            })
            .unwrap_or_default(),
        Type::Named(d, _) => {
            let mut items = Vec::new();
            for owner in member_owners(view, *d) {
                for &m in &view.decl(owner).members {
                    let member = view.member(m);
                    if member.is_static
                        || member.kind == MemberKind::Case
                        || member.kind == MemberKind::Init
                    {
                        continue;
                    }
                    if member.is_private && inside != Some(owner) && inside != Some(*d) {
                        continue;
                    }
                    items.push(member_item(view, m, 1));
                }
            }
            items
        }
        _ => Vec::new(),
    }
}

/// The declarations whose members a value of type `d` has: `d`, its
/// extensions, and the interfaces it implements (transitively), with their
/// extensions.
fn member_owners(view: &ProgramView, d: DeclId) -> Vec<DeclId> {
    let mut out = Vec::new();
    let mut stack = vec![d];
    while let Some(next) = stack.pop() {
        if out.contains(&next) || view.decls.get(next.0 as usize).is_none() {
            continue;
        }
        out.push(next);
        if let Some(exts) = view.extensions_of.get(&next) {
            out.extend(exts.iter().copied());
        }
        for t in &view.decl(next).implements {
            if let Some(i) = t.decl() {
                stack.push(i);
            }
        }
    }
    out
}

/// The static members of type `ty`: cases, static fields and functions,
/// and the theme tokens of a token category (`Spacing`).
fn static_members(view: &ProgramView, ty: &Type) -> Vec<CompletionItem> {
    let Some(d) = ty.unwrap_optional().decl() else {
        return Vec::new();
    };
    let Some(decl) = view.decls.get(d.0 as usize) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for owner in member_owners(view, d) {
        for &m in &view.decl(owner).members {
            let member = view.member(m);
            if (member.is_static
                || member.kind == MemberKind::Case
                || member.kind == MemberKind::Generated)
                && !member.is_private
            {
                items.push(member_item(view, m, 1));
            }
        }
    }
    // Theme tokens are members of themes, typed by their category.
    for member in &view.members {
        if member.kind == MemberKind::Token && member.ty.as_ref().and_then(Type::decl) == Some(d) {
            let owner = view.decl(member.owner);
            items.push(CompletionItem {
                label: member.name.clone(),
                kind: ItemKind::Constant,
                detail: Some(format!("{} token of {}", decl.name, owner.display_name)),
                import: None,
                group: if owner.stdlib { 2 } else { 1 },
            });
        }
    }
    items
}

/// The item of member `m`.
fn member_item(view: &ProgramView, m: k_sema::view::MemberId, group: u8) -> CompletionItem {
    let member = view.member(m);
    let kind = match member.kind {
        MemberKind::Field | MemberKind::Computed => ItemKind::Field,
        MemberKind::Func | MemberKind::Rpc => ItemKind::Method,
        MemberKind::Init => ItemKind::Constructor,
        MemberKind::Case | MemberKind::Generated => ItemKind::EnumMember,
        MemberKind::Token => ItemKind::Constant,
    };
    let detail = match (&member.ty, &member.signature) {
        (Some(t), _) => Some(render::ty(view, t)),
        (None, Some(sig)) => Some(render::signature_tail(view, sig)),
        _ => None,
    };
    CompletionItem {
        label: member.name.clone(),
        kind,
        detail,
        import: None,
        group,
    }
}

/// The value types of the CSS property whose value `node` is in, as
/// qualified type names (`k.ui.Spacing`).
fn css_value_types(node: &SyntaxNode) -> Option<&'static [&'static str]> {
    let declaration = node
        .ancestors()
        .find(|n| n.kind() == SyntaxKind::Declaration)?;
    let name = declaration
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| super::is_name(t.kind()))?;
    css::property_named(name.text()).map(|p| p.types)
}

/// The type an implicit member at `node` is expected to have, from its
/// context.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn expected_type(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    node: &SyntaxNode,
) -> Result<Option<Type>, Cancelled> {
    for ancestor in node.ancestors().take(6) {
        match ancestor.kind() {
            SyntaxKind::AssignStmt | SyntaxKind::BinaryExpr => {
                // The other operand: the first child that doesn't contain the
                // implicit member.
                let other = ancestor
                    .children()
                    .find(|c| !c.text_range().contains_range(node.text_range()));
                if let Some(other) = other {
                    if let Some(t) = analysis.type_info_at(file, other.text_range())? {
                        return Ok(Some(t));
                    }
                }
            }
            SyntaxKind::LetStmt => {
                let name = ancestor
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident);
                if let Some(name) = name {
                    if let Some(t) = analysis.type_info_at(file, name.text_range())? {
                        return Ok(Some(t));
                    }
                }
            }
            SyntaxKind::FieldDecl => {
                let offset = node.text_range().start();
                for member in &view.members {
                    if member.range.is_some_and(|r| r == ancestor.text_range())
                        && view.decl(member.owner).file == file
                        && member.range.is_some_and(|r| r.contains(offset))
                    {
                        return Ok(member.ty.clone());
                    }
                }
            }
            SyntaxKind::Arg => {
                if let Some(t) = argument_type(analysis, view, file, &ancestor)? {
                    return Ok(Some(t));
                }
            }
            SyntaxKind::ReturnStmt => {
                let offset = node.text_range().start();
                let func = view
                    .members
                    .iter()
                    .filter(|m| {
                        view.decl(m.owner).file == file
                            && m.range.is_some_and(|r| r.contains(offset))
                    })
                    .find_map(|m| m.signature.as_ref());
                if let Some(sig) = func {
                    return Ok(Some(sig.ret.clone()));
                }
            }
            SyntaxKind::MatchArm => {
                if let Some(scrutinee) = ancestor.parent().and_then(|m| m.first_child()) {
                    return analysis.type_info_at(file, scrutinee.text_range());
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

/// The parameter type of argument `arg` of the call it belongs to, by label
/// or position.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn argument_type(
    analysis: &Analysis,
    view: &ProgramView,
    file: FileId,
    arg: &SyntaxNode,
) -> Result<Option<Type>, Cancelled> {
    let Some(list) = arg.parent() else {
        return Ok(None);
    };
    let Some(call) = list.parent().filter(|c| c.kind() == SyntaxKind::CallExpr) else {
        return Ok(None);
    };
    let Some(callee) = call.first_child() else {
        return Ok(None);
    };
    let name = name_range_of(&callee);
    let Some(resolution) = analysis.resolve_at(file, name)? else {
        return Ok(None);
    };
    let label = arg
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
        .filter(|t| super::is_name(t.kind()))
        .filter(|t| {
            t.next_token()
                .is_some_and(|n| n.kind() == SyntaxKind::Colon)
        })
        .map(|t| t.text().to_owned());
    let position = list
        .children()
        .take_while(|c| c != arg)
        .filter(|c| c.kind() == SyntaxKind::Arg)
        .count();
    let sig = match view.resolve(&resolution) {
        Some(Resolved::Member(m)) => view.member(m).signature.clone(),
        Some(Resolved::Decl(d)) => view.decl(d).signature.clone().or_else(|| {
            view.decl(d)
                .members
                .iter()
                .map(|&m| view.member(m))
                .find(|m| m.kind == MemberKind::Init)
                .and_then(|m| m.signature.clone())
        }),
        None => None,
    };
    if let Some(sig) = sig {
        let param = match &label {
            Some(l) => sig
                .params
                .iter()
                .find(|p| p.label.as_deref() == Some(l.as_str())),
            None => sig.params.get(position),
        };
        return Ok(param.map(|p| p.ty.clone()));
    }
    // A memberwise initializer: the field named by the label.
    if let (Some(Resolved::Decl(d)), Some(l)) = (view.resolve(&resolution), &label) {
        let field = view
            .decl(d)
            .members
            .iter()
            .map(|&m| view.member(m))
            .find(|m| m.name == *l && m.kind == MemberKind::Field);
        return Ok(field.and_then(|f| f.ty.clone()));
    }
    Ok(None)
}

/// The names in scope at `offset`.
///
/// # Errors
///
/// [`Cancelled`] if the snapshot is stale.
fn scope_items(
    analysis: &Analysis,
    view: &ProgramView,
    root: &SyntaxNode,
    file: FileId,
    offset: TextSize,
) -> Result<Vec<CompletionItem>, Cancelled> {
    let mut items = Vec::new();
    let token = root.token_at_offset(offset).left_biased();
    let in_view = token.as_ref().is_some_and(|t| {
        t.parent_ancestors()
            .any(|n| n.kind() == SyntaxKind::ViewBlock)
    });
    // Locals and parameters.
    if let Some(token) = &token {
        for (name, range) in bindings_before(token, offset) {
            let detail = analysis.type_at(file, range)?;
            items.push(CompletionItem {
                label: name,
                kind: ItemKind::Variable,
                detail,
                import: None,
                group: 0,
            });
        }
    }
    // The enclosing declaration's members.
    let inside = decl_containing(view, file, offset);
    if let Some(d) = inside {
        let target = match (&view.decl(d).kind, &view.decl(d).extends) {
            (DeclKind::Extension, Some(t)) => t.decl().unwrap_or(d),
            _ => d,
        };
        for owner in member_owners(view, target) {
            for &m in &view.decl(owner).members {
                let member = view.member(m);
                if member.kind != MemberKind::Init && member.kind != MemberKind::Generated {
                    items.push(member_item(view, m, 1));
                }
            }
        }
    }
    // Top-level declarations: the module's, imported, the prelude's, and
    // other modules' with an import edit.
    let module = module_of(analysis, file);
    let imports = imports_of(root);
    let import_at = import_offset(root);
    for (i, decl) in view.decls.iter().enumerate() {
        if decl.name.is_empty() || decl.kind == DeclKind::Extension {
            continue;
        }
        if matches!(
            decl.named_kind(),
            Some(DeclarationKind::TestSuite | DeclarationKind::App | DeclarationKind::Migration)
        ) {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let qualified = view.qualified_name(id);
        let (group, import) = if decl.stdlib {
            if decl.display_name == decl.name {
                (4, None)
            } else if imports.contains(&qualified) {
                (3, None)
            } else {
                continue;
            }
        } else if decl.module == module {
            (2, None)
        } else if imports.contains(&qualified) {
            (3, None)
        } else if decl.is_public && !decl.in_test_module && decl.module.is_some() {
            (5, Some((import_at, format!("import {qualified}\n"))))
        } else {
            continue;
        };
        items.push(CompletionItem {
            label: decl.name.clone(),
            kind: decl_item_kind(decl.kind),
            detail: Some(match &decl.signature {
                Some(sig) => render::signature_tail(view, sig),
                None => qualified,
            }),
            import,
            group,
        });
    }
    if in_view {
        for e in elements::ELEMENTS {
            items.push(CompletionItem {
                label: e.name.to_owned(),
                kind: ItemKind::Element,
                detail: Some(format!("<{}>", e.html)),
                import: None,
                group: 2,
            });
        }
    }
    for k in KEYWORDS {
        items.push(CompletionItem {
            label: (*k).to_owned(),
            kind: ItemKind::Keyword,
            detail: None,
            import: None,
            group: 6,
        });
    }
    Ok(items)
}

/// The kind of a declaration's item.
fn decl_item_kind(kind: DeclKind) -> ItemKind {
    match kind {
        DeclKind::Named(
            DeclarationKind::Struct | DeclarationKind::Entity | DeclarationKind::Config,
        ) => ItemKind::Struct,
        DeclKind::Named(DeclarationKind::Enum) => ItemKind::Enum,
        DeclKind::Named(DeclarationKind::Interface | DeclarationKind::Protocol) => {
            ItemKind::Interface
        }
        DeclKind::Named(DeclarationKind::Function) => ItemKind::Function,
        DeclKind::Named(DeclarationKind::Constant) => ItemKind::Constant,
        _ => ItemKind::Class,
    }
}

/// The module of project file `file` (`shop.cart` for
/// `src/shop/cart/Cart.k`); `None` for `App.k` and stray files.
fn module_of(analysis: &Analysis, file: FileId) -> Option<String> {
    let Some(FileOrigin::Project(path)) = analysis.origin(file) else {
        return None;
    };
    let rest = path.as_str().strip_prefix("src/")?;
    let dir = rest.rsplit_once('/')?.0;
    Some(dir.replace('/', "."))
}

/// The qualified names a file imports.
fn imports_of(root: &SyntaxNode) -> Vec<String> {
    root.children()
        .filter(|c| c.kind() == SyntaxKind::Import)
        .filter_map(|i| i.children().find(|c| c.kind() == SyntaxKind::ImportPath))
        .map(|p| p.text().to_string().split_whitespace().collect::<String>())
        .collect()
}

/// Where a new import goes: after the last import, else at the start.
fn import_offset(root: &SyntaxNode) -> TextSize {
    root.children()
        .filter(|c| c.kind() == SyntaxKind::Import)
        .last()
        .and_then(|i| {
            let mut t = i.last_token()?;
            // After the import's line break.
            while let Some(next) = t.next_token() {
                if next.kind() == SyntaxKind::Newline {
                    return Some(next.text_range().end());
                }
                if !next.kind().is_trivia() {
                    break;
                }
                t = next;
            }
            Some(i.text_range().end())
        })
        .unwrap_or_else(|| TextSize::from(0))
}

/// The local bindings and parameters visible at `offset`: walking up from
/// `token`, the bindings of each enclosing scope introduced before the
/// cursor. An over-approximation (an `if let` binding is offered in the
/// `else` branch too); the checker reports a misuse.
fn bindings_before(token: &SyntaxToken, offset: TextSize) -> Vec<(String, TextRange)> {
    let mut out: Vec<(String, TextRange)> = Vec::new();
    for ancestor in token.parent_ancestors() {
        match ancestor.kind() {
            SyntaxKind::Block | SyntaxKind::ViewBlock => {
                for child in ancestor.children() {
                    if child.text_range().start() >= offset {
                        break;
                    }
                    if matches!(child.kind(), SyntaxKind::LetStmt | SyntaxKind::GuardStmt)
                        && !child.text_range().contains(offset)
                    {
                        collect_bindings(&child, &mut out);
                    }
                }
            }
            SyntaxKind::ForStmt
            | SyntaxKind::MatchArm
            | SyntaxKind::LambdaExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::CatchClause
            | SyntaxKind::WhileStmt => collect_bindings(&ancestor, &mut out),
            SyntaxKind::FuncDecl | SyntaxKind::InitDecl | SyntaxKind::RpcDecl => {
                if let Some(list) = ancestor
                    .children()
                    .find(|c| c.kind() == SyntaxKind::ParamList)
                {
                    for param in list.children().filter(|c| c.kind() == SyntaxKind::Param) {
                        if let Some(name) = param_name(&param) {
                            out.push((name.text().to_owned(), name.text_range()));
                        }
                    }
                }
                break;
            }
            SyntaxKind::FieldDecl => break,
            _ => {}
        }
    }
    out
}

/// The internal name of a parameter: the last name before its `:`.
fn param_name(param: &SyntaxNode) -> Option<SyntaxToken> {
    let mut last = None;
    for el in param.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Colon => break,
            rowan::NodeOrToken::Token(t) if super::is_name(t.kind()) => last = Some(t),
            rowan::NodeOrToken::Node(_) => break,
            _ => {}
        }
    }
    last
}

/// The names `node` binds, not looking into nested blocks: a `let`'s name
/// or pattern, a `for`'s pattern, lambda parameters, `case` and `catch`
/// pattern bindings, `if let`/`guard let` conditions.
fn collect_bindings(node: &SyntaxNode, out: &mut Vec<(String, TextRange)>) {
    let mut push = |t: &SyntaxToken| {
        if t.kind() == SyntaxKind::Ident
            && !out
                .iter()
                .any(|(n, r)| n == t.text() && *r == t.text_range())
        {
            out.push((t.text().to_owned(), t.text_range()));
        }
    };
    if node.kind() == SyntaxKind::LetStmt {
        if let Some(t) = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)
        {
            push(&t);
        }
    }
    let mut stack: Vec<SyntaxNode> = node.children().collect();
    while let Some(n) = stack.pop() {
        match n.kind() {
            SyntaxKind::Block | SyntaxKind::ViewBlock | SyntaxKind::LambdaExpr if n != *node => {
                continue;
            }
            SyntaxKind::IdentPattern
            | SyntaxKind::BindingPattern
            | SyntaxKind::LambdaParam
            | SyntaxKind::LetCondition => {
                if let Some(t) = n
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident)
                {
                    push(&t);
                }
            }
            _ => {}
        }
        stack.extend(n.children());
    }
}
