//! Shared state of the framework layer (sema.md §12): the classes and
//! keyframes of every component's `style` block, which of them are
//! referenced, which `@Stylable` hooks are applied, and the K types named by
//! the stdlib data tables.
//!
//! The state lives in one [`FwState`] owned by the checking context
//! ([`crate::cx::Cx::fw`]); bodies record references while they are checked
//! and the project-level pass reports what stayed unused at the end
//! ([`report_unused`]).
//!
//! # Invariants
//!
//! * A component's classes are the class selectors of its `style` block, in
//!   source order, each name once (the first selector naming it).
//! * References are recorded by `(declaration, name)`, so a class written in
//!   several selectors is one class.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use k_diagnostics::{Diagnostic, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::Cx;
use crate::program::{DeclId, MemberId};
use crate::stdlib::elements::TYPE_SCOPE;
use crate::ty::Ty;

/// The classes and keyframes a declaration's `style` block declares.
#[derive(Debug, Default)]
pub(crate) struct StyleInfo {
    /// `true` when the declaration has a `style` block.
    pub(crate) has_style: bool,
    /// Every class, by its first selector: name and the name token's range.
    pub(crate) classes: Vec<(String, TextRange)>,
    /// Every `@keyframes` name, first declaration first.
    pub(crate) keyframes: Vec<(String, TextRange)>,
}

impl StyleInfo {
    /// Returns `true` if the block declares class `name`.
    pub(crate) fn has_class(&self, name: &str) -> bool {
        self.classes.iter().any(|(n, _)| n == name)
    }

    /// Returns `true` if the block declares keyframes `name`.
    pub(crate) fn has_keyframes(&self, name: &str) -> bool {
        self.keyframes.iter().any(|(n, _)| n == name)
    }
}

/// The framework layer's mutable state for one analysis.
#[derive(Default)]
pub(crate) struct FwState {
    /// Style information by declaration.
    styles: RefCell<HashMap<DeclId, Rc<StyleInfo>>>,
    /// Classes referenced from views, by declaration and class name.
    pub(crate) used_classes: RefCell<HashSet<(DeclId, String)>>,
    /// Keyframes referenced from `animationName`, by declaration and name.
    pub(crate) used_keyframes: RefCell<HashSet<(DeclId, String)>>,
    /// `@Stylable` fields applied with `.class(...)` somewhere in their
    /// component's view.
    pub(crate) applied_stylables: RefCell<HashSet<MemberId>>,
    /// Table types by their text.
    table_tys: RefCell<HashMap<&'static str, Ty>>,
    /// `true` once style blocks are checked, so keyframes references are
    /// known.
    pub(crate) styles_checked: std::cell::Cell<bool>,
    /// The functions that may read `form.value`, by declaration.
    form_actions: RefCell<HashMap<DeclId, Rc<HashSet<MemberId>>>>,
    /// The functions that write their declaration's fields, by declaration.
    state_writers: RefCell<HashMap<DeclId, Rc<HashSet<MemberId>>>>,
    /// The error each `@Load` initializer throws (if any) and its range.
    pub(crate) loads: RefCell<HashMap<MemberId, (Option<Ty>, TextRange)>>,
}

impl FwState {
    /// Returns the style information of `decl`, computed once.
    pub(crate) fn style(&self, cx: &Cx<'_, '_>, decl: DeclId) -> Rc<StyleInfo> {
        if let Some(info) = self.styles.borrow().get(&decl) {
            return Rc::clone(info);
        }
        let info = Rc::new(collect_style(&cx.p.decl(decl).node));
        self.styles.borrow_mut().insert(decl, Rc::clone(&info));
        info
    }

    /// The functions of `decl` that may read `form.value` (sema.md §12.10):
    /// the actions passed by name to `form.handle(…)`, and functions every
    /// one of whose references comes from such a function.
    pub(crate) fn form_actions(&self, cx: &Cx<'_, '_>, decl: DeclId) -> Rc<HashSet<MemberId>> {
        if let Some(a) = self.form_actions.borrow().get(&decl) {
            return Rc::clone(a);
        }
        let a = Rc::new(compute_form_actions(cx, decl));
        self.form_actions.borrow_mut().insert(decl, Rc::clone(&a));
        a
    }

    /// The functions of `decl` that write one of its fields, directly or
    /// through calls of its other functions: calling one while a
    /// `@Derived` value is computed is K3011 (03-reactivity.md).
    ///
    /// A syntactic summary, computed once per declaration: an assignment
    /// or a mutating collection call (`cx::MUTATING_METHODS`) whose place
    /// starts with a field's name (or `self.` and the name) writes it,
    /// unless the function binds that name itself (a parameter or local
    /// shadowing the field).
    pub(crate) fn state_writers(&self, cx: &Cx<'_, '_>, decl: DeclId) -> Rc<HashSet<MemberId>> {
        if let Some(w) = self.state_writers.borrow().get(&decl) {
            return Rc::clone(w);
        }
        let w = Rc::new(compute_state_writers(cx, decl));
        self.state_writers.borrow_mut().insert(decl, Rc::clone(&w));
        w
    }

    /// Resolves a type written in a stdlib data table (`Binding<String>`,
    /// `ButtonType`, `Date?`) against the prelude and [`TYPE_SCOPE`]. `T`
    /// stands for `param`, the element's type parameter. Unknown names are
    /// `{error}`, which suppresses every diagnostic that depends on them.
    pub(crate) fn table_ty(&self, cx: &Cx<'_, '_>, text: &'static str, param: &Ty) -> Ty {
        if text.contains('T') && mentions_param(text) {
            return parse_table_ty(cx, text, param);
        }
        if let Some(t) = self.table_tys.borrow().get(text) {
            return t.clone();
        }
        let t = parse_table_ty(cx, text, &Ty::Error);
        self.table_tys.borrow_mut().insert(text, t.clone());
        t
    }
}

/// Returns `true` if a table type mentions the type parameter `T` as a whole
/// segment.
fn mentions_param(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|s| s == "T")
}

/// Parses a table type: `T?`, `Name<Args>`, `T` or a name.
fn parse_table_ty(cx: &Cx<'_, '_>, text: &str, param: &Ty) -> Ty {
    let text = text.trim();
    if let Some(inner) = text.strip_suffix('?') {
        return Ty::optional(parse_table_ty(cx, inner, param));
    }
    if text == "T" {
        return param.clone();
    }
    if let Some(open) = text.find('<') {
        let name = &text[..open];
        let inner = text[open + 1..].trim_end_matches('>');
        let Some(decl) = table_decl(cx, name) else {
            return Ty::Error;
        };
        let args = inner
            .split(',')
            .map(|a| parse_table_ty(cx, a, param))
            .collect();
        return Ty::generic(decl, args);
    }
    table_decl(cx, text).map_or(Ty::Error, Ty::named)
}

/// A declaration a table type names: the prelude, then [`TYPE_SCOPE`].
fn table_decl(cx: &Cx<'_, '_>, name: &str) -> Option<DeclId> {
    if let Some(&d) = cx.p.prelude.get(name) {
        return Some(d);
    }
    TYPE_SCOPE
        .iter()
        .find(|q| q.rsplit('.').next() == Some(name))
        .and_then(|q| cx.p.lookup_qualified(q))
}

/// Computes [`FwState::form_actions`] from the references to the declaration's functions.
fn compute_form_actions(cx: &Cx<'_, '_>, decl: DeclId) -> HashSet<MemberId> {
    let p = cx.p;
    let d = p.decl(decl);
    let funcs: HashMap<&str, MemberId> = d
        .members
        .iter()
        .copied()
        .filter(|m| p.member(*m).kind == MemberKind::Func)
        .map(|m| (p.member(m).name.as_str(), m))
        .collect();
    let mut actions = HashSet::new();
    // References: (referenced function, the function the reference is in).
    let mut refs: Vec<(MemberId, Option<MemberId>)> = Vec::new();
    for n in d.node.descendants() {
        if n.kind() != SyntaxKind::NameExpr {
            continue;
        }
        let Some(tok) = n.first_token() else { continue };
        let Some(&f) = funcs.get(tok.text()) else {
            continue;
        };
        // `form.handle(f)`: an action.
        let in_handle = n
            .parent()
            .filter(|a| a.kind() == SyntaxKind::Arg)
            .and_then(|a| a.parent())
            .and_then(|l| l.parent())
            .and_then(ast::CallExpr::cast)
            .and_then(|c| c.callee())
            .is_some_and(|callee| match callee {
                ast::Expr::MemberExpr(m) => m.name().is_some_and(|t| t.text() == "handle"),
                _ => false,
            });
        if in_handle {
            actions.insert(f);
            continue;
        }
        let caller = n
            .ancestors()
            .find(|a| a.kind() == SyntaxKind::FuncDecl)
            .and_then(|func| {
                funcs
                    .values()
                    .copied()
                    .find(|m| p.member(*m).node.as_ref() == Some(&func))
            });
        refs.push((f, caller));
    }
    // The callers of each function (`None`: a reference outside any
    // function, such as a view or a field initializer).
    let mut callers: HashMap<MemberId, Vec<Option<MemberId>>> = HashMap::new();
    let mut callees: HashMap<MemberId, Vec<MemberId>> = HashMap::new();
    for &(f, caller) in &refs {
        callers.entry(f).or_default().push(caller);
        if let Some(c) = caller {
            callees.entry(c).or_default().push(f);
        }
    }
    // The functions reachable from an action through calls…
    let mut allowed: HashSet<MemberId> = actions.clone();
    let mut stack: Vec<MemberId> = actions.iter().copied().collect();
    while let Some(f) = stack.pop() {
        for &g in callees.get(&f).map_or(&[][..], Vec::as_slice) {
            if allowed.insert(g) {
                stack.push(g);
            }
        }
    }
    // …minus, until nothing changes, every non-action whose references
    // don't all come from the remaining set: the greatest set closed under
    // "every reference comes from an action or a member of the set". A
    // recursive helper (`loop()` calling itself) stays; a function only
    // calling itself isn't reachable and was never in.
    let bad = |f: MemberId, allowed: &HashSet<MemberId>| {
        !actions.contains(&f)
            && callers
                .get(&f)
                .is_some_and(|cs| cs.iter().any(|c| c.is_none_or(|c| !allowed.contains(&c))))
    };
    let mut queue: Vec<MemberId> = allowed
        .iter()
        .copied()
        .filter(|f| bad(*f, &allowed))
        .collect();
    while let Some(f) = queue.pop() {
        if !allowed.remove(&f) {
            continue;
        }
        for &g in callees.get(&f).map_or(&[][..], Vec::as_slice) {
            if allowed.contains(&g) && bad(g, &allowed) {
                queue.push(g);
            }
        }
    }
    allowed
}

/// The first name of a place (`count` in `count.x[0]`, `x` in `self.x`),
/// if the place starts with a plain name.
fn place_root(place: &SyntaxNode) -> Option<String> {
    let mut tokens = place
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia());
    let first = tokens.next()?;
    if first.kind() == SyntaxKind::SelfKw {
        let _dot = tokens.next()?;
        return tokens.next().map(|t| t.text().to_owned());
    }
    (first.kind() == SyntaxKind::Ident).then(|| first.text().to_owned())
}

/// Computes [`FwState::state_writers`].
fn compute_state_writers(cx: &Cx<'_, '_>, decl: DeclId) -> HashSet<MemberId> {
    let p = cx.p;
    let d = p.decl(decl);
    let fields: HashSet<&str> = d
        .members
        .iter()
        .map(|m| p.member(*m))
        .filter(|m| m.kind == MemberKind::Field && !m.is_static)
        .map(|m| m.name.as_str())
        .collect();
    let funcs: HashMap<&str, MemberId> = d
        .members
        .iter()
        .copied()
        .filter(|m| p.member(*m).kind == MemberKind::Func && !p.member(*m).is_static)
        .map(|m| (p.member(m).name.as_str(), m))
        .collect();
    let mut writers: HashSet<MemberId> = HashSet::new();
    // Callers of each function, for propagating writes up the call graph.
    let mut callers: HashMap<MemberId, Vec<MemberId>> = HashMap::new();
    for &f in funcs.values() {
        let Some(node) = &p.member(f).node else {
            continue;
        };
        // Names the function binds itself shadow fields.
        let bound: HashSet<String> = node
            .descendants()
            .filter(|n| {
                matches!(
                    n.kind(),
                    SyntaxKind::Param
                        | SyntaxKind::LambdaParam
                        | SyntaxKind::LetStmt
                        | SyntaxKind::BindingPattern
                )
            })
            .filter_map(|n| {
                n.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind() == SyntaxKind::Ident)
                    .last()
                    .map(|t| t.text().to_owned())
            })
            .collect();
        let writes_field = |place: &SyntaxNode| {
            place_root(place).is_some_and(|r| fields.contains(r.as_str()) && !bound.contains(&r))
        };
        for n in node.descendants() {
            match n.kind() {
                SyntaxKind::AssignStmt => {
                    if n.first_child().is_some_and(|t| writes_field(&t)) {
                        writers.insert(f);
                    }
                }
                SyntaxKind::CallExpr => {
                    let Some(callee) = n.first_child() else {
                        continue;
                    };
                    match callee.kind() {
                        SyntaxKind::NameExpr => {
                            if let Some(&g) = callee.first_token().and_then(|t| funcs.get(t.text()))
                            {
                                callers.entry(g).or_default().push(f);
                            }
                        }
                        SyntaxKind::MemberExpr => {
                            let Some(m) = ast::MemberExpr::cast(callee.clone()) else {
                                continue;
                            };
                            let (Some(recv), Some(name)) = (m.receiver(), m.name()) else {
                                continue;
                            };
                            if matches!(recv, ast::Expr::NameExpr(ref r)
                                if r.syntax().first_token().is_some_and(|t| t.kind() == SyntaxKind::SelfKw))
                            {
                                if let Some(&g) = funcs.get(name.text()) {
                                    callers.entry(g).or_default().push(f);
                                }
                            } else if crate::cx::MUTATING_METHODS.contains(&name.text())
                                && writes_field(recv.syntax())
                            {
                                writers.insert(f);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    let mut stack: Vec<MemberId> = writers.iter().copied().collect();
    while let Some(f) = stack.pop() {
        for &c in callers.get(&f).map_or(&[][..], Vec::as_slice) {
            if writers.insert(c) {
                stack.push(c);
            }
        }
    }
    writers
}

/// Collects the classes and keyframes of a declaration's style blocks.
fn collect_style(node: &SyntaxNode) -> StyleInfo {
    let mut info = StyleInfo::default();
    let Some(list) = node.children().find(|c| c.kind() == SyntaxKind::MemberList) else {
        return info;
    };
    for block in list
        .children()
        .filter(|c| c.kind() == SyntaxKind::StyleBlock)
    {
        info.has_style = true;
        for n in block.descendants() {
            match n.kind() {
                SyntaxKind::ClassSelector => {
                    if let Some(name) = n
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind().is_name_like())
                    {
                        if !info.has_class(name.text()) {
                            info.classes
                                .push((name.text().to_owned(), name.text_range()));
                        }
                    }
                }
                SyntaxKind::KeyframesRule => {
                    if let Some(name) = ast::KeyframesRule::cast(n.clone()).and_then(|k| k.name()) {
                        info.keyframes
                            .push((name.text().to_owned(), name.text_range()));
                    }
                }
                _ => {}
            }
        }
    }
    info
}

/// Reports classes (K3204) and keyframes (K3206) nothing references, and
/// `@Stylable` hooks never applied (K3209), once every body is checked.
pub(crate) fn report_unused(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let styles: Vec<(DeclId, Rc<StyleInfo>)> = cx
        .fw
        .styles
        .borrow()
        .iter()
        .map(|(d, s)| (*d, Rc::clone(s)))
        .collect();
    let _ = styles;
    for (i, decl) in p.decls.iter().enumerate() {
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib {
            continue;
        }
        let info = cx.fw.style(cx, id);
        if info.has_style {
            let used = cx.fw.used_classes.borrow();
            for (name, range) in &info.classes {
                if !used.contains(&(id, name.clone())) {
                    cx.report(
                        decl.file,
                        Diagnostic::new(
                            codes::UNUSED_CLASS,
                            p.span(decl.file, *range),
                            format!("class `{name}` is never used"),
                        ),
                    );
                }
            }
            let used = cx.fw.used_keyframes.borrow();
            let mut seen = HashSet::new();
            // Keyframes are referenced from style blocks (sema.md §12.5).
            if !cx.fw.styles_checked.get() {
                seen.extend(info.keyframes.iter().map(|(n, _)| n.clone()));
            }
            for (name, range) in &info.keyframes {
                if !seen.insert(name.clone()) {
                    continue;
                }
                if !used.contains(&(id, name.clone())) {
                    cx.report(
                        decl.file,
                        Diagnostic::new(
                            codes::UNUSED_KEYFRAMES,
                            p.span(decl.file, *range),
                            format!("keyframes `{name}` are never used"),
                        ),
                    );
                }
            }
        }
        let applied = cx.fw.applied_stylables.borrow();
        for &m in &decl.members {
            let member = p.member(m);
            if member.has_annotation("Stylable")
                && decl.named_kind() == Some(crate::stdlib::DeclarationKind::Component)
                && !applied.contains(&m)
                && !cx.p.decl(m_owner(cx, m)).has_annotation("Native")
                && cx.field_ty(m).decl().is_some()
                && cx.field_ty(m).decl() == cx.p.lookup_qualified("k.core.Classes")
            {
                cx.report(
                    decl.file,
                    Diagnostic::new(
                        codes::UNAPPLIED_STYLABLE,
                        p.span(decl.file, member.name_range),
                        format!(
                            "`@Stylable` `{}` is never applied with `.class(...)`",
                            member.name
                        ),
                    ),
                );
            }
        }
    }
}

/// The declaration that declares member `m`.
fn m_owner(cx: &Cx<'_, '_>, m: MemberId) -> DeclId {
    cx.p.member(m).owner
}

// ------------------------------------------------------------ declarations

use k_diagnostics::Fix;

use crate::body::{BodyOwner, Checker};
use crate::lookup::Resolution;
use crate::program::{DeclKind, MemberKind};
use crate::stdlib::DeclarationKind as K;
use crate::stdlib::annotations::{self, AnnotationDef, Mutability, Target};

/// The HTTP method annotations of endpoint page handlers.
pub(crate) const HANDLER_METHODS: &[&str] = &["Get", "Post", "Put", "Patch", "Delete"];

/// Returns `true` for an endpoint page: a page with `@Get`/`@Post`/…
/// handlers (sema.md §12.7).
pub(crate) fn is_endpoint_page(cx: &Cx<'_, '_>, decl: DeclId) -> bool {
    let d = cx.p.decl(decl);
    d.named_kind() == Some(K::Page)
        && d.members.iter().any(|m| {
            let m = cx.p.member(*m);
            m.kind == MemberKind::Func && HANDLER_METHODS.iter().any(|h| m.has_annotation(h))
        })
}

/// An annotation of user code with its resolved stdlib declaration.
pub(crate) struct ResolvedAnnotation {
    /// The syntax node.
    pub(crate) node: ast::Annotation,
    /// The annotation's declaring type.
    pub(crate) decl: DeclId,
    /// Its table entry.
    pub(crate) def: &'static AnnotationDef,
    /// From `@` to the end of the name.
    pub(crate) range: TextRange,
}

/// Resolves the annotations directly on `node`. Annotations that resolve to
/// nothing the annotation table knows are skipped (packages may declare
/// their own; sema.md §16 "Round 1b notes").
pub(crate) fn annotations_of(
    cx: &Cx<'_, '_>,
    file: crate::program::FileIx,
    node: &SyntaxNode,
) -> Vec<ResolvedAnnotation> {
    let mut out = Vec::new();
    for a in node.children().filter_map(ast::Annotation::cast) {
        let Some(name) = a.name() else { continue };
        // Annotations are known by name, as everywhere in the checker
        // (`has_annotation`): one the file doesn't import resolves to the
        // annotation table's entry of that simple name.
        let d = match cx.lookup(file, name.text()) {
            crate::cx::Found::Decl(d) => d,
            crate::cx::Found::Missing => {
                let mut by_name = annotations::ANNOTATIONS
                    .iter()
                    .filter(|x| x.name.rsplit('.').next() == Some(name.text()));
                let (Some(only), None) = (by_name.next(), by_name.next()) else {
                    continue;
                };
                let Some(d) = cx.p.lookup_qualified(only.name) else {
                    continue;
                };
                d
            }
            _ => continue,
        };
        let Some(module) = cx.p.decl_module(d) else {
            continue;
        };
        let qualified = format!(
            "{}.{}",
            cx.p.modules[module.0 as usize].path,
            cx.p.decl(d).name
        );
        let Some(def) = annotations::annotation(&qualified) else {
            continue;
        };
        let start = a.syntax().text_range().start();
        let range = TextRange::new(start, name.text_range().end());
        out.push(ResolvedAnnotation {
            node: a,
            decl: d,
            def,
            range,
        });
    }
    out
}

/// Where annotations on a member of `decl` (a field when `field`, else a
/// function) may appear.
fn member_targets(cx: &Cx<'_, '_>, decl: DeclId, field: bool) -> Vec<Target> {
    let kind = cx.p.decl(decl).named_kind();
    let endpoint = is_endpoint_page(cx, decl);
    match (kind, field) {
        (Some(K::Component), true) => vec![Target::ViewField, Target::ComponentField],
        (Some(K::Page), true) => {
            let mut t = vec![Target::ViewField, Target::PageField];
            if endpoint {
                t.push(Target::EndpointField);
            }
            t
        }
        (Some(K::Layout), true) => vec![Target::ViewField, Target::LayoutField],
        (Some(K::Service), true) => vec![Target::ServiceField],
        (Some(K::TestSuite), true) => vec![Target::TestField],
        (Some(K::Repository), true) => vec![Target::RepositoryField],
        (Some(K::Config), true) => vec![Target::ConfigField],
        (Some(K::Entity), true) => vec![Target::EntityField],
        (Some(K::Struct), true) => vec![Target::StructField],
        (Some(K::Component | K::Layout | K::Service), false) => vec![Target::ViewFunction],
        (Some(K::Page), false) => vec![Target::ViewFunction, Target::EndpointHandler],
        _ => Vec::new(),
    }
}

/// Where annotations on the declaration itself may appear.
fn decl_targets(kind: Option<K>) -> Vec<Target> {
    match kind {
        Some(K::Page) => vec![Target::Page],
        Some(K::App) => vec![Target::App],
        Some(K::Service) => vec![Target::Service],
        Some(K::Struct) => vec![Target::Struct],
        Some(K::Protocol | K::Entity | K::Migration | K::Enum) => vec![Target::Locked],
        _ => Vec::new(),
    }
}

/// The offset after the spaces and tabs at `offset`.
fn next_non_space(text: &str, offset: k_diagnostics::TextSize) -> k_diagnostics::TextSize {
    let start = usize::from(offset);
    let rest = &text[start.min(text.len())..];
    let n = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    offset + k_diagnostics::TextSize::from(u32::try_from(n).unwrap_or(0))
}

/// K3002 for an annotation that doesn't apply where it is written.
fn not_allowed(cx: &Cx<'_, '_>, file: crate::program::FileIx, a: &ResolvedAnnotation) {
    let text = cx.p.file(file).text;
    let end = next_non_space(text, a.node.syntax().text_range().end());
    let removal = TextRange::new(a.range.start(), end);
    let name = &cx.p.decl(a.decl).name;
    cx.report(
        file,
        Diagnostic::new(
            codes::ANNOTATION_NOT_ALLOWED,
            cx.p.span(file, a.range),
            format!("`@{name}` doesn't apply here"),
        )
        .with_fix(
            Fix::replace("Remove the annotation", cx.p.span(file, removal), "").maybe_incorrect(),
        ),
    );
}

/// The framework checks of one declaration (sema.md §12.1): where each
/// annotation applies (K3002) and the mutability it needs (K3001), the
/// types of slots, class hooks and events (K3005), the view and block rules
/// (K3003, K3004) and assignment members outside themes (K3303). Returns
/// the members whose annotations were reported (their other rules don't
/// apply then).
pub(crate) fn check_declaration(cx: &Cx<'_, '_>, decl: DeclId, owner: &BodyOwner) {
    let p = cx.p;
    let d = p.decl(decl);
    let file = d.file;
    let kind = d.named_kind();
    // The declaration's own annotations.
    let targets = decl_targets(kind);
    for a in annotations_of(cx, file, &d.node) {
        if a.def.targets.iter().any(|t| targets.contains(t)) {
            check_annotation_args(cx, owner, &a);
        } else {
            not_allowed(cx, file, &a);
        }
    }
    for &m in &d.members {
        let member = p.member(m);
        let Some(node) = &member.node else { continue };
        let field = matches!(member.kind, MemberKind::Field | MemberKind::Computed);
        let is_func = member.kind == MemberKind::Func;
        let targets = if field || is_func {
            member_targets(cx, decl, field)
        } else {
            Vec::new()
        };
        for a in annotations_of(cx, file, node) {
            if !a.def.targets.iter().any(|t| targets.contains(t)) {
                not_allowed(cx, file, &a);
                continue;
            }
            if field {
                mutability(cx, file, m, &a);
                if !constraint(cx, file, m, &a) {
                    continue;
                }
            }
            check_annotation_args(cx, owner, &a);
        }
        if field {
            annotated_type(cx, file, m);
        }
        if is_func && member.has_annotation("Effect") {
            throwing_effect(cx, file, node);
        }
    }
    blocks(cx, decl);
    derived_cycles(cx, decl);
    check_entity(cx, decl);
    if kind == Some(K::Service) {
        service_inits(cx, decl);
    }
    if kind == Some(K::Theme) {
        check_theme(cx, decl, owner);
    }
}

/// K4016: a service has at most one `init()`, without parameters and
/// without `throws`: the framework creates it, so nothing could pass
/// arguments or catch an error.
fn service_inits(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    let mut seen = false;
    for &m in &d.members {
        let member = p.member(m);
        if member.kind != MemberKind::Init {
            continue;
        }
        let Some(node) = &member.node else { continue };
        if seen {
            cx.error(
                d.file,
                codes::INVALID_SERVICE_INIT,
                member.name_range,
                "a service declares at most one `init()`",
            );
            continue;
        }
        seen = true;
        if let Some(list) = node.children().find(|c| c.kind() == SyntaxKind::ParamList) {
            if list.children().any(|c| c.kind() == SyntaxKind::Param) {
                cx.error(
                    d.file,
                    codes::INVALID_SERVICE_INIT,
                    list.text_range(),
                    "a service's `init()` takes no parameters: its inputs are its dependencies",
                );
            }
        }
        if let Some(clause) = node
            .children()
            .find(|c| c.kind() == SyntaxKind::ThrowsClause)
        {
            cx.error(
                d.file,
                codes::INVALID_SERVICE_INIT,
                clause.text_range(),
                "a service's `init()` can't throw: nothing could catch the error",
            );
        }
    }
}

/// K3012: an `@Effect` can't be declared `throws`: nothing waits for it.
fn throwing_effect(cx: &Cx<'_, '_>, file: crate::program::FileIx, node: &SyntaxNode) {
    let Some(clause) = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ThrowsClause)
    else {
        return;
    };
    cx.error(
        file,
        codes::THROWING_EFFECT_OR_HANDLER,
        clause.text_range(),
        "an `@Effect` can't throw: nothing could catch its error",
    );
}

/// K3026: `@Derived` fields whose initializers read each other in a cycle
/// (03-reactivity.md). The graph's nodes are the declaration's `@Derived`
/// fields; a read of one (`b`, or `self.b`) in another's initializer is an
/// edge, `untracked { … }` included. Each field of a strongly connected
/// component with a cycle is reported once. Linear in the initializers.
fn derived_cycles(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    let derived: Vec<MemberId> = d
        .members
        .iter()
        .copied()
        .filter(|m| {
            let member = p.member(*m);
            member.kind == MemberKind::Field && member.has_annotation("Derived")
        })
        .collect();
    if derived.is_empty() {
        return;
    }
    let index: HashMap<&str, usize> = derived
        .iter()
        .enumerate()
        .map(|(i, m)| (p.member(*m).name.as_str(), i))
        .collect();
    let mut edges = vec![Vec::new(); derived.len()];
    let mut self_loop = vec![false; derived.len()];
    for (i, m) in derived.iter().enumerate() {
        let Some(init) = p
            .member(*m)
            .node
            .as_ref()
            .and_then(|n| ast::FieldDecl::cast(n.clone()))
            .and_then(|f| f.initializer())
        else {
            continue;
        };
        for n in init.syntax().descendants() {
            let name = match n.kind() {
                // `b`, unless it is a member name (`x.b`) or a call (`b()`).
                SyntaxKind::NameExpr => {
                    let callee = n.parent().is_some_and(|c| {
                        c.kind() == SyntaxKind::CallExpr && c.first_child().as_ref() == Some(&n)
                    });
                    if callee {
                        continue;
                    }
                    n.first_token().filter(|t| t.kind() == SyntaxKind::Ident)
                }
                // `self.b`.
                SyntaxKind::MemberExpr => {
                    let Some(m) = ast::MemberExpr::cast(n.clone()) else {
                        continue;
                    };
                    let on_self = matches!(m.receiver(), Some(ast::Expr::NameExpr(r))
                        if r.syntax().first_token().is_some_and(|t| t.kind() == SyntaxKind::SelfKw));
                    if !on_self {
                        continue;
                    }
                    m.name()
                }
                _ => continue,
            };
            if let Some(&j) = name.and_then(|t| index.get(t.text()).copied()).as_ref() {
                edges[i].push(j);
                if i == j {
                    self_loop[i] = true;
                }
            }
        }
    }
    let comp = crate::check::tarjan(&edges);
    let mut size: HashMap<usize, usize> = HashMap::new();
    for &c in &comp {
        *size.entry(c).or_default() += 1;
    }
    for (i, m) in derived.iter().enumerate() {
        if self_loop[i] || size.get(&comp[i]).is_some_and(|n| *n > 1) {
            let member = p.member(*m);
            cx.error(
                d.file,
                codes::DERIVED_CYCLE,
                member.name_range,
                format!(
                    "the `@Derived` field `{}` depends on itself through a cycle of `@Derived` fields",
                    member.name
                ),
            );
        }
    }
}

/// K3001: the `let`/`var` an annotation requires.
fn mutability(cx: &Cx<'_, '_>, file: crate::program::FileIx, m: MemberId, a: &ResolvedAnnotation) {
    let member = cx.p.member(m);
    let Some(kw) = member.keyword_range else {
        return;
    };
    let (wrong, want) = match a.def.mutability {
        Mutability::Let if member.is_var => ("var", "let"),
        Mutability::Var if !member.is_var => ("let", "var"),
        _ => return,
    };
    let name = &cx.p.decl(a.decl).name;
    let fix = Fix::replace(
        format!("Change `{wrong}` to `{want}`"),
        cx.p.span(file, kw),
        want,
    );
    // A `var` its declaration assigns can't simply become a `let`: the
    // assignment would then be K3014/K2011, so the edit needs review.
    let fix = if want == "let" && assigned_in_owner(cx, m) {
        fix.maybe_incorrect()
    } else {
        fix
    };
    cx.report(
        file,
        Diagnostic::new(
            codes::ANNOTATION_MUTABILITY,
            cx.p.span(file, kw),
            format!("`@{name}` fields are `{want}`"),
        )
        .with_fix(fix),
    );
}

/// Returns `true` if the declaration that owns field `m` holds an
/// assignment whose target starts with the field's name (`text = …`,
/// `self.text = …`, `text.count += 1`).
///
/// A syntactic scan of one declaration, used only to choose a fix's
/// applicability: a local shadowing the field makes it answer `true`
/// conservatively.
fn assigned_in_owner(cx: &Cx<'_, '_>, m: MemberId) -> bool {
    let member = cx.p.member(m);
    let owner = &cx.p.decl(member.owner).node;
    owner
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::AssignStmt)
        .filter_map(|n| n.first_child())
        .any(|target| {
            let mut tokens = target
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia());
            match tokens.next() {
                Some(t) if t.kind() == SyntaxKind::SelfKw => {
                    tokens.nth(1).is_some_and(|t| t.text() == member.name)
                }
                Some(t) => t.text() == member.name,
                None => false,
            }
        })
}

/// K3005: `@Slot` takes `View`, `View?` or a function returning `View`;
/// `@Stylable` takes `Classes`; `@Emits` takes `Event<T>`.
fn annotated_type(cx: &Cx<'_, '_>, file: crate::program::FileIx, m: MemberId) {
    let member = cx.p.member(m);
    let Some(ty_node) = member
        .node
        .as_ref()
        .and_then(|n| ast::FieldDecl::cast(n.clone()))
        .and_then(|f| f.ty())
    else {
        return;
    };
    let ty = cx.field_ty(m);
    if ty.contains_error() {
        return;
    }
    let is = |t: &Ty, q: &str| t.decl().is_some() && t.decl() == cx.p.lookup_qualified(q);
    let deopt = |t: &Ty| match t {
        Ty::Optional(i) => (**i).clone(),
        other => other.clone(),
    };
    let (ok, range, what) = if member.has_annotation("Slot") {
        match deopt(&ty) {
            Ty::Func(f) => {
                let ret_range = ty_node
                    .syntax()
                    .descendants()
                    .find(|n| n.kind() == SyntaxKind::ReturnType)
                    .and_then(|r| r.children().find_map(ast::Type::cast))
                    .map_or(ty_node.syntax().text_range(), |t| t.syntax().text_range());
                (
                    is(&f.ret, "k.core.View"),
                    ret_range,
                    "a slot holds `View`, `View?` or a function returning `View`",
                )
            }
            t => (
                is(&t, "k.core.View"),
                ty_node.syntax().text_range(),
                "a slot holds `View`, `View?` or a function returning `View`",
            ),
        }
    } else if member.has_annotation("Stylable") {
        (
            is(&ty, "k.core.Classes"),
            ty_node.syntax().text_range(),
            "a `@Stylable` field is `Classes`",
        )
    } else if member.has_annotation("Emits") {
        (
            is(&ty, "k.core.Event"),
            ty_node.syntax().text_range(),
            "an `@Emits` field is an `Event<T>`",
        )
    } else {
        return;
    };
    if !ok {
        cx.error(file, codes::INVALID_ANNOTATED_TYPE, range, what);
    }
}

/// K3003 (exactly one `view`), K3004 (blocks at most once) and K3303
/// (assignment members outside themes).
fn blocks(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    let file = d.file;
    let Some(list) = d
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
    else {
        return;
    };
    let mut first: HashMap<SyntaxKind, TextRange> = HashMap::new();
    for child in list.children() {
        let kind = child.kind();
        if kind == SyntaxKind::ThemeOverride && d.named_kind() != Some(K::Theme) {
            cx.error(
                file,
                codes::OVERRIDE_OUTSIDE_THEME,
                child.text_range(),
                "an assignment member overrides a theme token; it belongs in a `theme`",
            );
            continue;
        }
        if !matches!(
            kind,
            SyntaxKind::ViewBlock
                | SyntaxKind::StyleBlock
                | SyntaxKind::MetadataBlock
                | SyntaxKind::LoadingBlock
                | SyntaxKind::FailedBlock
        ) {
            continue;
        }
        let kw = child
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia())
            .map_or(child.text_range(), |t| t.text_range());
        if let Some(prev) = first.get(&kind) {
            cx.report(
                file,
                Diagnostic::new(
                    codes::DUPLICATE_BLOCK,
                    p.span(file, kw),
                    format!(
                        "`{}` is declared more than once",
                        p.file(file).text[usize::from(kw.start())..usize::from(kw.end())]
                            .to_owned()
                    ),
                )
                .with_label(p.span(file, *prev), "first declared here"),
            );
        } else {
            first.insert(kind, kw);
        }
    }
    let viewful = matches!(d.named_kind(), Some(K::Component | K::Page | K::Layout));
    if viewful
        && !first.contains_key(&SyntaxKind::ViewBlock)
        && !is_endpoint_page(cx, decl)
        && !d.has_annotation("Native")
        && !p.file(file).stdlib
    {
        cx.error(
            file,
            codes::MISSING_VIEW,
            d.name_range,
            format!("`{}` needs a `view` block", d.name),
        );
    }
    let _ = DeclKind::Extension;
}

/// Checks an annotation's arguments like a call of its initializer.
fn check_annotation_args(cx: &Cx<'_, '_>, owner: &BodyOwner, a: &ResolvedAnnotation) {
    let mut checker = Checker::new(cx, owner.clone());
    checker.check_annotation(&a.node, a.decl);
    checker.finish();
}

// ------------------------------------------------------------ themes

/// The category words of theme tokens, for K3304's closest-name fix.
const TOKEN_CATEGORIES: &[&str] = &[
    "color", "duration", "font", "fontSize", "radius", "shadow", "space",
];

/// Finds the token `name` visible to every theme: the theme's own tokens
/// first, then every other theme's (libraries included).
fn find_token(cx: &Cx<'_, '_>, theme: DeclId, name: &str) -> Option<MemberId> {
    let own = cx.p.decl(theme).members.iter().copied().find(|m| {
        let m = cx.p.member(*m);
        m.kind == MemberKind::Token && m.name == name
    });
    own.or_else(|| {
        cx.tokens
            .values()
            .flatten()
            .copied()
            .filter(|m| cx.p.member(*m).name == name)
            .min_by_key(|m| (cx.p.member(*m).owner == theme, m.0))
    })
}

/// The theme checks (sema.md §12.5): categories (K3304) and their types
/// (K3305), values (K2001), duplicates (K3307), re-declared library tokens
/// (K3302), overrides of unknown tokens (K3301), variants named `base`
/// (K3306) and `matches` conditions.
pub(crate) fn check_theme(cx: &Cx<'_, '_>, decl: DeclId, owner: &BodyOwner) {
    let p = cx.p;
    let d = p.decl(decl);
    let file = d.file;
    let Some(list) = d
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList)
    else {
        return;
    };
    let mut checker = Checker::new(cx, owner.clone());
    let mut tokens: HashMap<String, TextRange> = HashMap::new();
    let mut variants: HashMap<String, TextRange> = HashMap::new();
    let mut overrides: HashMap<String, TextRange> = HashMap::new();
    for child in list.children() {
        match child.kind() {
            SyntaxKind::ThemeToken => theme_token(cx, &mut checker, decl, &child, &mut tokens),
            SyntaxKind::ThemeOverride => {
                theme_override(cx, &mut checker, decl, &child, &mut overrides)
            }
            SyntaxKind::ThemeVariant => {
                let Some(v) = ast::ThemeVariant::cast(child.clone()) else {
                    continue;
                };
                if let Some(name) = ast::HasName::name(&v) {
                    if name.text() == "base" {
                        cx.error(
                            file,
                            codes::VARIANT_NAMED_BASE,
                            name.text_range(),
                            "`base` means \"no variant\"; name the variant something else",
                        );
                    }
                    duplicate(cx, file, &mut variants, name.text(), name.text_range());
                }
                // `matches .condition` takes a `MediaCondition`.
                if let Some(cond) = child.children().find_map(ast::Expr::cast) {
                    let ty = p
                        .lookup_qualified("k.core.MediaCondition")
                        .map_or(Ty::Error, Ty::named);
                    checker.check_initializer(&cond, Some(&ty));
                }
                let mut seen: HashMap<String, TextRange> = HashMap::new();
                for o in child
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::ThemeOverride)
                {
                    theme_override(cx, &mut checker, decl, &o, &mut seen);
                }
            }
            _ => {}
        }
    }
    checker.finish();
}

/// K3307 for a name seen before in the same scope.
fn duplicate(
    cx: &Cx<'_, '_>,
    file: crate::program::FileIx,
    seen: &mut HashMap<String, TextRange>,
    name: &str,
    range: TextRange,
) -> bool {
    if let Some(first) = seen.get(name) {
        cx.report(
            file,
            Diagnostic::new(
                codes::DUPLICATE_THEME_MEMBER,
                cx.p.span(file, range),
                format!("`{name}` is declared twice"),
            )
            .with_label(cx.p.span(file, *first), "first declared here"),
        );
        return true;
    }
    seen.insert(name.to_owned(), range);
    false
}

/// A token declaration (`color primary: Color = …`): its category (K3304, K3305), duplicates (K3307), re-declared library tokens (K3302) and its value (K2001).
fn theme_token(
    cx: &Cx<'_, '_>,
    checker: &mut Checker<'_, '_, '_>,
    theme: DeclId,
    node: &SyntaxNode,
    seen: &mut HashMap<String, TextRange>,
) {
    let p = cx.p;
    let file = p.decl(theme).file;
    let idents: Vec<k_syntax::SyntaxToken> = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident)
        .collect();
    let (Some(category), Some(name)) = (idents.first(), idents.get(1)) else {
        return;
    };
    let ty_node = node.children().find_map(ast::Type::cast);
    let declared = ty_node.as_ref().map(|t| cx.resolve_type(file, &[], t));
    let category_ty = cx.token_category(category.text());
    match category_ty {
        None => {
            let mut d = Diagnostic::new(
                codes::UNKNOWN_TOKEN_CATEGORY,
                p.span(file, category.text_range()),
                format!("`{}` is not a token category", category.text()),
            );
            if let Some(c) = crate::cx::closest(category.text(), TOKEN_CATEGORIES.iter().copied()) {
                d = d.with_fix(
                    Fix::replace(
                        format!("Replace `{}` with `{c}`", category.text()),
                        p.span(file, category.text_range()),
                        c,
                    )
                    .maybe_incorrect(),
                );
            }
            cx.report(file, d);
        }
        Some(cat) => {
            checker.note_resolution(
                name.text_range(),
                Resolution::Token {
                    category: p.decl_name(cat),
                    name: name.text().to_owned(),
                },
            );
            if let (Some(t), Some(declared)) = (&ty_node, &declared) {
                if !declared.contains_error() && declared.decl() != Some(cat) {
                    let cname = p.decl(cat).name.clone();
                    let written = declared.render(p);
                    cx.report(
                        file,
                        Diagnostic::new(
                            codes::TOKEN_CATEGORY_MISMATCH,
                            p.span(file, t.syntax().text_range()),
                            format!("`{}` tokens are `{cname}`", category.text()),
                        )
                        .with_fix(Fix::replace(
                            format!("Replace `{written}` with `{cname}`"),
                            p.span(file, t.syntax().text_range()),
                            cname,
                        )),
                    );
                }
            }
        }
    }
    // Tokens are members of their category: `space md` and `radius md`
    // don't clash.
    let key = format!(
        "{}.{}",
        category_ty.map_or(String::new(), |c| p.decl(c).name.clone()),
        name.text()
    );
    if let Some(first) = seen.get(&key) {
        cx.report(
            file,
            Diagnostic::new(
                codes::DUPLICATE_THEME_MEMBER,
                cx.p.span(file, name.text_range()),
                format!("`{}` is declared twice", name.text()),
            )
            .with_label(cx.p.span(file, *first), "first declared here"),
        );
        return;
    }
    seen.insert(key, name.text_range());
    // A library already declares the token: override it instead (K3302).
    if !p.file(file).stdlib {
        let library = cx.tokens.values().flatten().copied().find(|m| {
            let m = cx.p.member(*m);
            m.name == name.text() && cx.p.file(cx.p.decl(m.owner).file).stdlib
        });
        if let Some(lib) = library {
            let lm = p.member(lib);
            let lfile = p.decl(lm.owner).file;
            let eq = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::Eq);
            let end = eq.map_or(node.text_range().end(), |t| t.text_range().start());
            let edit = TextRange::new(category.text_range().start(), end);
            cx.report(
                file,
                Diagnostic::new(
                    codes::REDECLARED_THEME_TOKEN,
                    p.span(file, name.text_range()),
                    format!("`{}` is declared by a library; override it", name.text()),
                )
                .with_label(p.span(lfile, lm.name_range), "declared here")
                .with_fix(Fix::replace(
                    "Rewrite as an override",
                    p.span(file, edit),
                    format!("{} ", name.text()),
                )),
            );
            return;
        }
    }
    let expected = declared.or_else(|| category_ty.map(Ty::named));
    if let Some(value) = node.children().find_map(ast::Expr::cast) {
        checker.check_initializer(&value, expected.as_ref());
    }
}

/// An override (`name = value`): an existing token (K3301, closest-name fix) and a value of its type (K2001); duplicates in one scope are K3307.
fn theme_override(
    cx: &Cx<'_, '_>,
    checker: &mut Checker<'_, '_, '_>,
    theme: DeclId,
    node: &SyntaxNode,
    seen: &mut HashMap<String, TextRange>,
) {
    let p = cx.p;
    let file = p.decl(theme).file;
    let Some(name) = node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident)
    else {
        return;
    };
    if duplicate(cx, file, seen, name.text(), name.text_range()) {
        return;
    }
    let value = node.children().find_map(ast::Expr::cast);
    let Some(token) = find_token(cx, theme, name.text()) else {
        let mut d = Diagnostic::new(
            codes::UNKNOWN_THEME_TOKEN,
            p.span(file, name.text_range()),
            format!("no theme declares the token `{}`", name.text()),
        );
        let names: Vec<&str> = cx
            .tokens
            .values()
            .flatten()
            .map(|m| cx.p.member(*m).name.as_str())
            .collect();
        if let Some(c) = crate::cx::closest(name.text(), names.into_iter()) {
            d = d.with_fix(
                Fix::replace(
                    format!("Replace `{}` with `{c}`", name.text()),
                    p.span(file, name.text_range()),
                    c,
                )
                .maybe_incorrect(),
            );
        }
        cx.report(file, d);
        if let Some(v) = value {
            checker.silently(|c| {
                c.check_initializer(&v, None);
            });
        }
        return;
    };
    checker.note_resolution(name.text_range(), cx.member_resolution(token));
    let ty = cx.field_ty(token);
    if let Some(v) = value {
        checker.check_initializer(&v, Some(&ty));
    }
}

// ------------------------------------------------------------ forms

/// The text a string literal stands for, for `@Matches`: escapes decoded,
/// `{n}` kept as regex text. `Err` with the range to report (K7005) when the
/// literal interpolates anything but an integer literal.
fn pattern_text(lit: &SyntaxNode) -> Result<String, TextRange> {
    let mut out = String::new();
    for el in lit.children_with_tokens() {
        match el {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::StringText => out.push_str(t.text()),
                SyntaxKind::StringEscape => out.push_str(&unescape(t.text())),
                _ => {}
            },
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::Interpolation => {
                let inner = n.children().find_map(ast::Expr::cast);
                let digits = match &inner {
                    Some(ast::Expr::Literal(l)) => l
                        .syntax()
                        .first_token()
                        .filter(|t| t.kind() == SyntaxKind::IntLit)
                        .map(|t| t.text().to_owned()),
                    _ => None,
                };
                match digits {
                    // `"[A-Z]{3}"`: a quantifier, not an interpolation.
                    Some(d) => {
                        out.push('{');
                        out.push_str(&d);
                        out.push('}');
                    }
                    None => return Err(lit.text_range()),
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Decodes one string escape (`{{`, `\\n`, `\\u{41}`).
fn unescape(escape: &str) -> String {
    match escape {
        "{{" => "{".to_owned(),
        "}}" => "}".to_owned(),
        "\\n" => "\n".to_owned(),
        "\\t" => "\t".to_owned(),
        "\\r" => "\r".to_owned(),
        "\\0" => "\0".to_owned(),
        e if e.starts_with("\\u{") => {
            u32::from_str_radix(e.trim_start_matches("\\u{").trim_end_matches('}'), 16)
                .ok()
                .and_then(char::from_u32)
                .map_or_else(String::new, |c| c.to_string())
        }
        e => e.chars().skip(1).collect(),
    }
}

/// The type an annotated field holds, without optionals and `Secret`.
fn constrained(cx: &Cx<'_, '_>, ty: &Ty) -> Ty {
    let ty = match ty {
        Ty::Optional(inner) => (**inner).clone(),
        other => other.clone(),
    };
    match &ty {
        Ty::Named(d, args) if cx.p.lookup_qualified("k.core.Secret") == Some(*d) => {
            args.first().cloned().unwrap_or(Ty::Error)
        }
        _ => ty,
    }
}

/// The form and constraint rules of an annotated field (sema.md §12.10):
/// field types (K3002), `@Format` on text (K7003), `@Email` on
/// `EmailAddress` (K7004), `@Matches` literals (K7005, K7006), `@Form` of a
/// `Validatable` struct (K2009). Returns `false` when the annotation was
/// rejected (K3002).
fn constraint(
    cx: &Cx<'_, '_>,
    file: crate::program::FileIx,
    m: MemberId,
    a: &ResolvedAnnotation,
) -> bool {
    let p = cx.p;
    let name = p.decl(a.decl).name.as_str();
    let ty = cx.field_ty(m);
    if ty.contains_error() {
        return true;
    }
    let inner = constrained(cx, &ty);
    let is = |q: &str| inner.decl().is_some() && inner.decl() == p.lookup_qualified(q);
    let b = &cx.b;
    let text = inner.decl().is_some() && inner.decl() == b.string;
    let fits = match name {
        "Length" => text || inner.decl().is_some() && inner.decl() == b.list,
        "Range" => [b.int, b.double, b.decimal].contains(&inner.decl()) && inner.decl().is_some(),
        "Matches" | "NotBlank" => text,
        "Email" => text || is("k.text.EmailAddress"),
        _ => true,
    };
    if !fits {
        not_allowed(cx, file, a);
        return false;
    }
    match name {
        "Format" if text => {
            cx.error(
                file,
                codes::FORMAT_ON_STRING,
                a.range,
                "text always parses: `@Format` belongs on fields of parsed types",
            );
        }
        "Email" if is("k.text.EmailAddress") => {
            let text = p.file(file).text;
            let has_args = a.node.arg_list().is_some_and(|l| l.args().next().is_some());
            let fix = if has_args {
                let name_range = a.node.name().map_or(a.range, |t| t.text_range());
                Fix::replace(
                    "Replace with `@Format(message:)`",
                    p.span(file, name_range),
                    "Format",
                )
            } else {
                let end = next_non_space(text, a.node.syntax().text_range().end());
                Fix::replace(
                    "Remove `@Email`",
                    p.span(file, TextRange::new(a.range.start(), end)),
                    "",
                )
            };
            cx.report(
                file,
                Diagnostic::new(
                    codes::REDUNDANT_EMAIL,
                    p.span(file, a.range),
                    "an `EmailAddress` validates itself; `@Email` is redundant",
                )
                .with_fix(fix),
            );
        }
        "Matches" => {
            let Some(arg) = a
                .node
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|x| x.value())
            else {
                return true;
            };
            let pattern = match &arg {
                ast::Expr::StringLit(lit) => pattern_text(lit.syntax()),
                other => Err(other.syntax().text_range()),
            };
            match pattern {
                Err(range) => cx.error(
                    file,
                    codes::MATCHES_NOT_LITERAL,
                    range,
                    "`@Matches` takes a string literal, known at compile time",
                ),
                Ok(pattern) => {
                    if let Err(why) = crate::regex::validate(&pattern) {
                        cx.error(
                            file,
                            codes::INVALID_REGEX,
                            arg.syntax().text_range(),
                            format!("invalid regular expression: {why}"),
                        );
                    }
                }
            }
        }
        "Form" => {
            let validatable = p.lookup_qualified("k.validation.Validatable");
            if validatable.is_some_and(|v| !cx.implements(&ty, v)) {
                let range = p
                    .member(m)
                    .node
                    .as_ref()
                    .and_then(|n| ast::FieldDecl::cast(n.clone()))
                    .and_then(|f| f.ty())
                    .map_or(p.member(m).name_range, |t| t.syntax().text_range());
                cx.error(
                    file,
                    codes::UNSATISFIED_CONSTRAINT,
                    range,
                    format!(
                        "`@Form` wraps a `Validatable` struct; `{}` isn't",
                        ty.render(p)
                    ),
                );
            }
        }
        _ => {}
    }
    true
}

// ------------------------------------------------------------ @Load

/// Records what a `@Load` initializer throws.
pub(crate) fn record_load(cx: &Cx<'_, '_>, m: MemberId, escaped: Vec<Ty>, range: TextRange) {
    cx.fw
        .loads
        .borrow_mut()
        .insert(m, (escaped.into_iter().next(), range));
}

/// The `@Load` fields of a declaration.
fn load_fields(cx: &Cx<'_, '_>, decl: DeclId) -> Vec<MemberId> {
    cx.p.decl(decl)
        .members
        .iter()
        .copied()
        .filter(|m| {
            let m = cx.p.member(*m);
            m.kind == MemberKind::Field && m.has_annotation("Load")
        })
        .collect()
}

/// The error type `failed(let error)` binds: the loads' error, `{error}`
/// when no load throws.
pub(crate) fn load_error(cx: &Cx<'_, '_>, decl: DeclId) -> Ty {
    let loads = cx.fw.loads.borrow();
    load_fields(cx, decl)
        .iter()
        .find_map(|m| loads.get(m).and_then(|(e, _)| e.clone()))
        .unwrap_or(Ty::Error)
}

/// The `@Load` rules of a declaration (sema.md §12.9): a `loading` block
/// (K3022), `Codable` values (K3021) and errors (K3024), one error type
/// (K2001), no cycles between loads (K3020), no member named `reload`
/// (K3023).
pub(crate) fn load_rules(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    let file = d.file;
    let loads = load_fields(cx, decl);
    // `@Load` elsewhere is K3002; its other rules don't apply.
    if loads.is_empty() || !matches!(d.named_kind(), Some(K::Page | K::Component | K::Layout)) {
        return;
    }
    let list = d
        .node
        .children()
        .find(|c| c.kind() == SyntaxKind::MemberList);
    let has_loading = list
        .as_ref()
        .is_some_and(|l| l.children().any(|c| c.kind() == SyntaxKind::LoadingBlock));
    if !has_loading && !is_endpoint_page(cx, decl) {
        let mut diag = Diagnostic::new(
            codes::MISSING_LOADING_BLOCK,
            p.span(file, d.name_range),
            format!(
                "`{}` has `@Load` fields, so it needs a `loading` block",
                d.name
            ),
        );
        if let Some(close) = list.as_ref().and_then(|l| {
            l.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() == SyntaxKind::RBrace)
                .last()
        }) {
            diag = diag.with_fix(
                Fix::replace(
                    "Add an empty `loading { }`",
                    k_diagnostics::Span::empty(p.file(file).id, close.text_range().start()),
                    "\n    loading {\n    }\n",
                )
                .maybe_incorrect(),
            );
        }
        cx.report(file, diag);
    }
    let codable = cx.b.codable;
    let is_codable = |t: &Ty| t.contains_error() || codable.is_none_or(|c| cx.implements(t, c));
    for &m in &loads {
        let ty = cx.field_ty(m);
        if !is_codable(&ty) {
            let range = p
                .member(m)
                .node
                .as_ref()
                .and_then(|n| ast::FieldDecl::cast(n.clone()))
                .and_then(|f| f.ty())
                .map_or(p.member(m).name_range, |t| t.syntax().text_range());
            cx.error(
                file,
                codes::LOAD_NOT_CODABLE,
                range,
                format!(
                    "`@Load` values are sent to the browser, so they are `Codable`; `{}` isn't",
                    ty.render(p)
                ),
            );
        }
    }
    let recorded = cx.fw.loads.borrow().clone();
    let mut first: Option<Ty> = None;
    for &m in &loads {
        let Some((Some(error), range)) = recorded.get(&m).cloned() else {
            continue;
        };
        if !is_codable(&error) {
            cx.error(
                file,
                codes::LOAD_ERROR_NOT_CODABLE,
                range,
                format!(
                    "a load's error is sent to `failed`, so it is `Codable`; `{}` isn't",
                    error.render(p)
                ),
            );
        }
        match &first {
            None => first = Some(error),
            Some(f) if *f != error => cx.error(
                file,
                codes::TYPE_MISMATCH,
                range,
                format!(
                    "the loads of `{}` throw `{}`; this one throws `{}` (wrap errors, don't mix them)",
                    d.name,
                    f.render(p),
                    error.render(p)
                ),
            ),
            Some(_) => {}
        }
    }
    // Ordering graph: which loads and `@Derived` fields each initializer
    // reads. `@Derived` fields are intermediate nodes: `a` reading `d`
    // reading `b` makes `a` wait for `b` (14-async-data.md).
    let mut nodes: Vec<MemberId> = loads.clone();
    nodes.extend(d.members.iter().copied().filter(|m| {
        let member = p.member(*m);
        member.kind == MemberKind::Field && member.has_annotation("Derived")
    }));
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, m)| (p.member(*m).name.as_str(), i))
        .collect();
    let mut edges = vec![Vec::new(); nodes.len()];
    let mut self_loop = vec![false; nodes.len()];
    for (i, m) in nodes.iter().enumerate() {
        let Some(init) = p
            .member(*m)
            .node
            .as_ref()
            .and_then(|n| ast::FieldDecl::cast(n.clone()))
            .and_then(|f| f.initializer())
        else {
            continue;
        };
        let callee = |n: &SyntaxNode| {
            n.parent().is_some_and(|c| {
                c.kind() == SyntaxKind::CallExpr && c.first_child().as_ref() == Some(n)
            })
        };
        for n in init
            .syntax()
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::NameExpr && !callee(n))
        {
            if let Some(&j) = n.first_token().and_then(|t| index.get(t.text())) {
                edges[i].push(j);
                if i == j {
                    self_loop[i] = true;
                }
            }
        }
    }
    let comp = crate::check::tarjan(&edges);
    let mut comp_size: HashMap<usize, usize> = HashMap::new();
    for &c in &comp {
        *comp_size.entry(c).or_default() += 1;
    }
    // Only the loads are reported: a cycle among `@Derived` fields alone
    // has no code yet (03-reactivity.md).
    for (i, m) in loads.iter().enumerate() {
        let in_cycle = self_loop[i] || comp_size.get(&comp[i]).is_some_and(|n| *n > 1);
        if in_cycle {
            cx.error(
                file,
                codes::LOAD_CYCLE,
                p.member(*m).name_range,
                format!(
                    "the `@Load` field `{}` waits for itself through a cycle of loads",
                    p.member(*m).name
                ),
            );
        }
    }
    if let Some(&r) = d
        .member_names
        .get("reload")
        .or_else(|| d.funcs.get("reload").and_then(|f| f.first()))
    {
        cx.error(
            file,
            codes::RESERVED_RELOAD,
            p.member(r).name_range,
            "`reload` is generated for declarations with `@Load` fields",
        );
    }
}

// ------------------------------------------------------------ persistence

/// The collection name of an entity inside its repository: the plural in
/// lower camel case (`Note` → `notes`, `Category` → `categories`).
pub(crate) fn collection_name(entity: &str) -> String {
    let mut name = entity.to_owned();
    if let Some(first) = name.get(..1) {
        name = first.to_lowercase() + &name[1..];
    }
    let consonant_y = name.ends_with('y')
        && !name.ends_with("ay")
        && !name.ends_with("ey")
        && !name.ends_with("oy")
        && !name.ends_with("uy");
    if consonant_y {
        name.pop();
        name.push_str("ies");
    } else if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|e| name.ends_with(e))
    {
        name.push_str("es");
    } else {
        name.push('s');
    }
    name
}

/// The table name of an entity: snake case of its name.
fn table_name(entity: &str) -> String {
    let mut out = String::new();
    for (i, c) in entity.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns `true` for types an entity column can hold.
fn column_type(cx: &Cx<'_, '_>, ty: &Ty) -> bool {
    let b = &cx.b;
    match ty {
        Ty::Error => true,
        Ty::Optional(inner) => column_type(cx, inner),
        Ty::Named(d, args) if Some(*d) == b.list => args.first().is_some_and(|a| {
            a.decl()
                .is_some_and(|e| cx.p.decl(e).named_kind() == Some(K::Entity))
        }),
        Ty::Named(d, _) => {
            let decl = cx.p.decl(*d);
            if decl.named_kind() == Some(K::Entity) {
                return true;
            }
            if cx.is_enum(*d) && !cx.p.is_stdlib(*d) {
                return cx.cases(*d).iter().all(|c| !cx.case_has_payload(*c));
            }
            // The app's user id type (`AccountId`), stored as its value
            // (`docs/design/26-auth-and-sessions.md`).
            if cx
                .p
                .lookup_qualified("k.auth.AuthUserId")
                .is_some_and(|a| cx.declares(*d, a))
            {
                return true;
            }
            [
                b.int, b.double, b.decimal, b.bool_, b.string, b.money, b.duration,
            ]
            .contains(&Some(*d))
                || [
                    "k.core.Date",
                    "k.crypto.Uuid",
                    "k.text.EmailAddress",
                    "k.core.Url",
                ]
                .iter()
                .any(|q| cx.p.lookup_qualified(q) == Some(*d))
        }
        _ => false,
    }
}

/// Entity rules (sema.md §12.12): column types (K6002) and the reserved
/// `k_` table prefix (K6003).
pub(crate) fn check_entity(cx: &Cx<'_, '_>, decl: DeclId) {
    let p = cx.p;
    let d = p.decl(decl);
    if d.named_kind() != Some(K::Entity) {
        return;
    }
    if table_name(&d.name).starts_with("k_") {
        cx.error(
            d.file,
            codes::RESERVED_TABLE_PREFIX,
            d.name_range,
            format!(
                "`{}` would be the table `{}`; the `k_` prefix is reserved",
                d.name,
                table_name(&d.name)
            ),
        );
    }
    for &m in &d.members {
        let member = p.member(m);
        if member.kind != MemberKind::Field || member.is_static {
            continue;
        }
        let ty = cx.field_ty(m);
        if column_type(cx, &ty) {
            continue;
        }
        let range = member
            .node
            .as_ref()
            .and_then(|n| ast::FieldDecl::cast(n.clone()))
            .and_then(|f| f.ty())
            .map_or(member.name_range, |t| t.syntax().text_range());
        cx.error(
            d.file,
            codes::UNSUPPORTED_COLUMN_TYPE,
            range,
            format!("an entity column can't hold `{}`", ty.render(p)),
        );
    }
}
