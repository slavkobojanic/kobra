//! Checking `view` content (sema.md §12.2, §12.4): built-in elements from the
//! element table, component calls against their props, slots, bindings and
//! events, chained attributes, `.class(...)` references, and view loops.
//!
//! View content is ordinary statements of a block, checked in *view mode*
//! ([`Checker::view_on`]): every expression statement is a view node.
//! A node is a postfix chain `base(args) { content } .link(args) …` whose
//! base decides what the chain is:
//!
//! * a lowercase name that isn't a local, member or declaration is a
//!   built-in element ([`Checker::element_node`]); an unknown one is K3101;
//! * a name resolving to a component, page or layout is a component call
//!   ([`Checker::component_node`]);
//! * anything else is a value placed in the view (text, a slot, a scoped
//!   slot call) and is checked as an ordinary expression.
//!
//! `if`, `match` and `for` keep view mode in their bodies; lambdas passed as
//! handlers leave it, while trailing blocks and slot builders enter it again
//! ([`Checker::view_lambda`]).

use std::collections::HashMap;

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use super::{Arg, Checker, Target};
use crate::cx::{Found, MemberFound, builtin, closest, token};
use crate::lookup::Resolution;
use crate::program::{DeclId, MemberId, MemberKind};
use crate::stdlib::DeclarationKind;
use crate::stdlib::elements::{self, Content, ElementDef};
use crate::ty::Ty;

/// What the content being checked may hold, from its enclosing element.
#[derive(Clone, Default)]
pub(crate) struct ViewCx {
    /// The child elements allowed (`Content::Only`), or `None` for any.
    only: Option<&'static [&'static str]>,
    /// `true` inside an element that takes no content (each child is K3107).
    none: bool,
    /// The value type of a `dropdown`'s options.
    option_ty: Option<Ty>,
}

/// One chained call after a node's base: `.name(args)`.
struct Link {
    /// The name after `.`.
    name: SyntaxToken,
    /// The call, `None` for a bare `.name`.
    call: Option<ast::CallExpr>,
}

/// The kinds of fields a component call can set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PropKind {
    /// `@Prop`: a one-way input.
    Prop,
    /// `@Binding`: a two-way input passed with `bind`.
    Binding,
    /// `@Slot`: content.
    Slot,
    /// `@Stylable`: a class hook.
    Stylable,
}

/// A settable field of a component.
#[derive(Clone)]
struct PropInfo {
    /// The field.
    member: MemberId,
    /// Its name: the argument label or chained name.
    name: String,
    /// Its annotation.
    kind: PropKind,
    /// Its type, with the component's type arguments applied.
    ty: Ty,
    /// `true` when the call must set it (no default, not optional).
    required: bool,
}

/// Roles that a semantic element expresses: `.role(.navigation)` → `nav`.
const SEMANTIC_ROLES: &[(&str, &str)] = &[
    ("article", "article"),
    ("banner", "header"),
    ("button", "button"),
    ("complementary", "aside"),
    ("contentinfo", "footer"),
    ("form", "form"),
    ("heading", "heading"),
    ("img", "image"),
    ("link", "link"),
    ("list", "list"),
    ("listitem", "item"),
    ("main", "main"),
    ("navigation", "nav"),
    ("region", "section"),
    ("table", "table"),
];

/// Splits a view node into its base and the chained links after it, in
/// source order.
fn decompose(e: &ast::Expr) -> (ast::Expr, Vec<Link>) {
    let mut links = Vec::new();
    let mut current = e.clone();
    loop {
        match &current {
            ast::Expr::CallExpr(c) => {
                let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
                    break;
                };
                let (Some(name), Some(recv)) = (m.name(), m.receiver()) else {
                    break;
                };
                links.push(Link {
                    name,
                    call: Some(c.clone()),
                });
                current = recv;
            }
            ast::Expr::MemberExpr(m) => {
                let (Some(name), Some(recv)) = (m.name(), m.receiver()) else {
                    break;
                };
                links.push(Link { name, call: None });
                current = recv;
            }
            _ => break,
        }
    }
    links.reverse();
    (current, links)
}

/// The base name and call of a node base: `name`, `name(…)`, `name { … }`.
fn base_name(base: &ast::Expr) -> Option<(SyntaxToken, Option<ast::CallExpr>)> {
    match base {
        ast::Expr::NameExpr(n) => n
            .syntax()
            .first_token()
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .map(|t| (t, None)),
        ast::Expr::CallExpr(c) => match c.callee() {
            Some(ast::Expr::NameExpr(n)) => n
                .syntax()
                .first_token()
                .filter(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| (t, Some(c.clone()))),
            _ => None,
        },
        _ => None,
    }
}

/// Returns `true` for a call whose argument list isn't closed (a syntax
/// error the parser reported): what follows the first argument is not
/// checked.
fn broken(call: &ast::CallExpr) -> bool {
    call.arg_list()
        .is_some_and(|l| token(l.syntax(), SyntaxKind::RParen).is_none())
}

/// Returns `true` for names that start with a lowercase letter (built-in elements).
fn is_lower(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

impl Checker<'_, '_, '_> {
    // ------------------------------------------------------------ entry

    /// Checks a `view`, `loading` or `failed` block (and a `metadata` block
    /// quietly reuses the same entry): its statements are view content.
    pub(crate) fn check_view_block(&mut self, block: &ast::Block) {
        self.push_fn(super::FnCx {
            is_async: true,
            throws: Some(Ty::Error),
            lambda_throws: None,
            ret: None,
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        });
        let saved = std::mem::replace(&mut self.view_on, true);
        let saved_cx = std::mem::take(&mut self.view_ctx);
        let saved_ssr = std::mem::replace(&mut self.ssr, true);
        self.block_stmts(block);
        self.ssr = saved_ssr;
        self.view_ctx = saved_cx;
        self.view_on = saved;
        self.pop_fn();
    }

    /// Checks a page-level `metadata { … }` block: its statements call the
    /// entries of `k.ui.MetadataEntries`; `status(_:)` belongs inside
    /// `failed` only (K3025).
    pub(crate) fn check_metadata_block(&mut self, block: &ast::Block) {
        self.push_fn(super::FnCx {
            is_async: true,
            throws: Some(Ty::Error),
            lambda_throws: None,
            ret: None,
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        });
        let stmts: Vec<ast::Stmt> = block.statements().collect();
        self.push_scope();
        self.metadata_stmts(&stmts, false);
        self.pop_scope();
        self.pop_fn();
    }

    /// The statements of a metadata block; `allow_status` inside `failed`.
    fn metadata_stmts(&mut self, stmts: &[ast::Stmt], allow_status: bool) {
        for stmt in stmts {
            match stmt {
                ast::Stmt::Expr(e) => self.metadata_call(e, allow_status, true),
                other => self.stmt(other),
            }
        }
    }

    /// The `MetadataEntries` function named `name`.
    fn metadata_entry(&self, name: &str) -> Option<MemberId> {
        let entries = self.cx.p.lookup_qualified("k.ui.MetadataEntries")?;
        self.cx
            .p
            .decl(entries)
            .funcs
            .get(name)
            .and_then(|f| f.first().copied())
    }

    /// One metadata entry call (`title("…")`); `status(…)` outside a
    /// `failed` metadata block is K3025. With `in_block` off, the call is
    /// view content (`status` directly in `failed`).
    fn metadata_call(&mut self, e: &ast::Expr, allow_status: bool, in_block: bool) {
        let ast::Expr::CallExpr(c) = e else {
            self.infer(e, None);
            return;
        };
        let Some(ast::Expr::NameExpr(n)) = c.callee() else {
            self.infer(e, None);
            return;
        };
        let Some(name) = n.syntax().first_token() else {
            return;
        };
        let shadowed =
            self.find_local(name.text()).is_some() || self.self_member(name.text()).is_some();
        let Some(entry) = self.metadata_entry(name.text()).filter(|_| !shadowed) else {
            self.infer(e, None);
            return;
        };
        self.note_res(name.text_range(), self.cx.member_resolution(entry));
        let args = self.collect_args(c);
        if name.text() == "status" && (!allow_status || !in_block) {
            self.err(
                codes::STATUS_OUTSIDE_FAILED,
                c.syntax().text_range(),
                "`status(_:)` is set in a `metadata` block inside `failed`",
            );
            self.quiet += 1;
            self.args_unchecked(&args);
            self.quiet -= 1;
            return;
        }
        let sig = self.cx.sig(crate::cx::SigKey::Member(entry));
        self.apply_sig(c.syntax(), &sig, &[], &args, None, super::CallKind::Plain);
    }

    /// Runs `f` with view mode off: for expressions inside view content
    /// that are values (conditions, arguments, initializers).
    pub(crate) fn plain<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = std::mem::replace(&mut self.view_on, false);
        let t = f(self);
        self.view_on = saved;
        t
    }

    /// Checks the statements of view content.
    pub(crate) fn view_stmts(&mut self, stmts: &[ast::Stmt]) {
        for stmt in stmts {
            self.view_stmt(stmt);
        }
    }

    /// Checks one statement of view content.
    fn view_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Expr(e) => self.view_node(e),
            ast::Stmt::For(_) => {
                self.content_rule(stmt.syntax(), None, true);
                self.stmt(stmt);
            }
            ast::Stmt::Let(_) => self.plain(|s| s.stmt(stmt)),
            _ => self.stmt(stmt),
        }
    }

    /// Checks a child against the enclosing element's content rule
    /// (K3107). `element` is the child's element name when it is one;
    /// `structural` marks `if`, `match` and `for`, whose bodies are checked
    /// against the same rule instead.
    fn content_rule(&mut self, child: &SyntaxNode, element: Option<&str>, structural: bool) {
        if structural {
            return;
        }
        if self.view_ctx.none {
            self.err(
                codes::CONTENT_NOT_ALLOWED,
                child.text_range(),
                "this element takes no content",
            );
            return;
        }
        if let Some(only) = self.view_ctx.only {
            if !element.is_some_and(|e| only.contains(&e)) {
                let names: Vec<String> = only.iter().map(|n| format!("`{n}`")).collect();
                self.err(
                    codes::CONTENT_NOT_ALLOWED,
                    child.text_range(),
                    format!("only {} may appear here", names.join(", ")),
                );
            }
        }
    }

    /// Checks one view node.
    fn view_node(&mut self, e: &ast::Expr) {
        if matches!(e, ast::Expr::IfExpr(_) | ast::Expr::MatchExpr(_)) {
            self.infer(e, None);
            return;
        }
        let (base, links) = decompose(e);
        let Some((name, call)) = base_name(&base) else {
            self.value_node(e);
            return;
        };
        // `metadata { … }` inside `failed` (or `loading`) content is the
        // statement form of a metadata block (sema.md §12.9), where
        // `status(_:)` is allowed.
        if name.text() == "metadata" && links.is_empty() && self.find_local("metadata").is_none() {
            if let Some(l) = call.as_ref().and_then(ast::CallExpr::trailing_lambda) {
                let stmts: Vec<ast::Stmt> = l.statements().collect();
                let saved = std::mem::replace(&mut self.view_on, false);
                self.push_scope();
                self.metadata_stmts(&stmts, true);
                self.pop_scope();
                self.view_on = saved;
                return;
            }
        }
        if name.text() == "status"
            && self.metadata_entry("status").is_some()
            && self.find_local("status").is_none()
            && self.self_member("status").is_none()
        {
            self.metadata_call(e, true, false);
            return;
        }
        let text = name.text().to_owned();
        let has_block = call.as_ref().is_some_and(|c| c.trailing_lambda().is_some());
        // `item.name` is a value: an element or component is followed by
        // chained calls only.
        if links.iter().any(|l| l.call.is_none()) {
            self.value_node(e);
            return;
        }
        if is_lower(&text) {
            let shadowed = self.find_local(&text).is_some()
                || self.self_member(&text).is_some()
                || matches!(self.cx.lookup(self.file(), &text), Found::Decl(_));
            if !shadowed || (has_block && elements::element(&text).is_some()) {
                self.content_rule(e.syntax(), Some(&text), false);
                self.element_node(e, &name, call.as_ref(), &links);
                return;
            }
        } else if let Found::Decl(d) = self.cx.lookup(self.file(), &text) {
            if matches!(
                self.cx.p.decl(d).named_kind(),
                Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout)
            ) && self.find_local(&text).is_none()
            {
                if self.view_ctx.none {
                    self.content_rule(e.syntax(), None, false);
                }
                self.note_res(name.text_range(), self.cx.decl_resolution(d));
                self.component_node(e, &base, &name, d, call.as_ref(), &links);
                return;
            }
        } else if self.find_local(&text).is_none()
            && self.self_member(&text).is_none()
            && matches!(self.cx.lookup(self.file(), &text), Found::Missing)
        {
            // An unknown component: K1201 at the name; what is passed to it
            // is resolved but not checked, and its content is view content.
            self.content_rule(e.syntax(), None, false);
            self.plain(|s| s.name_value(&text, name.text_range(), false));
            self.unknown_node(call.as_ref(), &links);
            return;
        }
        self.value_node(e);
    }

    /// A value placed in a view: text, or an opaque `View` (a slot, a
    /// scoped slot's result) that may hold whatever the content rule
    /// allows, so only the no-content rule applies to it.
    fn value_node(&mut self, e: &ast::Expr) {
        let t = self.plain(|s| s.infer(e, None));
        let t = self.resolve(&t);
        if self.is_view(&self.deopt(&t)) && !self.view_ctx.none {
            return;
        }
        self.content_rule(e.syntax(), None, false);
    }

    /// Returns the member of the enclosing declaration named `name`.
    fn self_member(&self, name: &str) -> Option<MemberId> {
        let self_ty = self.owner.self_ty.clone()?;
        for statics in [false, true] {
            if let MemberFound::Member(m, _) =
                self.cx
                    .lookup_member(self.file(), self.owner.decl, &self_ty, name, statics)
            {
                return Some(m);
            }
        }
        None
    }

    /// Resolves what is passed to an unknown element or component without
    /// checking it against anything; its content is still view content.
    fn unknown_node(&mut self, call: Option<&ast::CallExpr>, links: &[Link]) {
        if let Some(c) = call {
            let args: Vec<Arg> = self
                .collect_args(c)
                .into_iter()
                .filter(|a| !self.is_trailing(c, a))
                .collect();
            self.plain(|s| s.args_unchecked(&args));
            if let Some(l) = c.trailing_lambda() {
                self.view_lambda(&l, Vec::new(), ViewCx::default());
            }
        }
        for link in links {
            let Some(c) = &link.call else { continue };
            let args = self.collect_args(c);
            if link.name.text() == "class" {
                for a in &args {
                    if let Some(e) = &a.expr {
                        self.class_value(e, false);
                    }
                }
                continue;
            }
            self.plain(|s| s.args_unchecked(&args));
        }
    }

    /// Returns `true` if `a` is the call's trailing block.
    fn is_trailing(&self, c: &ast::CallExpr, a: &Arg) -> bool {
        c.trailing_lambda()
            .is_some_and(|l| l.syntax().text_range() == a.range)
    }

    /// Checks a trailing block or slot builder as view content with `ctx`'s
    /// content rule; `params` are its parameters (a scoped slot's).
    pub(crate) fn view_lambda(
        &mut self,
        l: &ast::LambdaExpr,
        params: Vec<(String, TextRange, Ty)>,
        ctx: ViewCx,
    ) {
        let saved = std::mem::replace(&mut self.view_on, true);
        let saved_cx = std::mem::replace(&mut self.view_ctx, ctx);
        self.lambda_body(l, params, None, None, true, None);
        self.view_ctx = saved_cx;
        self.view_on = saved;
        self.note_type(l.syntax().text_range(), &builtin(self.cx.b.view));
    }

    // ------------------------------------------------------------ elements

    /// A built-in element: its arguments, content rule, chained attributes and events; an unknown name is K3101.
    fn element_node(
        &mut self,
        whole: &ast::Expr,
        name: &SyntaxToken,
        call: Option<&ast::CallExpr>,
        links: &[Link],
    ) {
        let text = name.text();
        let Some(def) = elements::element(text) else {
            self.unknown_element(name, call);
            self.unknown_node(call, links);
            return;
        };
        self.note_res(
            name.text_range(),
            Resolution::Element {
                name: def.name.to_owned(),
            },
        );
        let view = builtin(self.cx.b.view);
        if let Some(c) = call {
            self.note_type(c.syntax().text_range(), &view);
        }
        self.note_type(whole.syntax().text_range(), &view);
        let param = if def.name == "option" {
            self.view_ctx
                .option_ty
                .clone()
                .unwrap_or_else(|| self.fresh())
        } else {
            self.fresh()
        };
        self.element_args(def, name, call, &param);
        if let Some(l) = call.and_then(ast::CallExpr::trailing_lambda) {
            let ctx = match def.content {
                Content::None => ViewCx {
                    none: true,
                    ..ViewCx::default()
                },
                Content::Any => ViewCx::default(),
                Content::Only(only) => ViewCx {
                    only: Some(only),
                    none: false,
                    option_ty: (def.name == "dropdown").then(|| param.clone()),
                },
            };
            self.view_lambda(&l, Vec::new(), ctx);
        }
        let mut set: HashMap<String, TextRange> = HashMap::new();
        for link in links {
            self.element_link(def, link, &mut set);
        }
        if def.name == "video" && !set.contains_key("captions") {
            if let Some(c) = call {
                self.missing_captions(c);
            }
        }
    }

    /// K3112: a `video` playing a file or a URL without `.captions(…)`
    /// (WCAG 1.2.2; `docs/design/46-media-and-rtc.md`, "Showing and
    /// playing media"). A live stream is exempt, and so is a source the
    /// compiler can't see the case of.
    fn missing_captions(&mut self, call: &ast::CallExpr) {
        let Some(list) = call.arg_list() else { return };
        let Some(value) = list
            .args()
            .find(|a| a.label().is_some_and(|l| l.text() == "source"))
            .and_then(|a| a.value())
        else {
            return;
        };
        let callee = match &value {
            ast::Expr::CallExpr(c) => c.callee(),
            other => Some(other.clone()),
        };
        let case = callee.and_then(|e| {
            e.syntax()
                .last_token()
                .filter(|t| t.kind().is_name_like())
                .map(|t| t.text().to_owned())
        });
        if !matches!(case.as_deref(), Some("file" | "url")) {
            return;
        }
        let at = list.syntax().text_range().end();
        let file = self.span(call.syntax().text_range()).file;
        self.report(
            Diagnostic::new(
                codes::MISSING_CAPTIONS,
                self.span(value.syntax().text_range()),
                "a video from a file or URL needs captions",
            )
            .with_help("add `.captions(.track(url:language:label:))`, or `.captions(.notNeeded)` for a video without speech")
            .with_fix(
                Fix::replace(
                    "Insert `.captions(.notNeeded)`",
                    Span::empty(file, at),
                    ".captions(.notNeeded)",
                )
                .maybe_incorrect(),
            ),
        );
    }

    /// K3101 with the K element an HTML name stands for (`a` → `link`, `h2` → `heading(level: 2)`), or the closest element.
    fn unknown_element(&mut self, name: &SyntaxToken, call: Option<&ast::CallExpr>) {
        let text = name.text();
        let range = name.text_range();
        let mut d = Diagnostic::new(
            codes::UNKNOWN_ELEMENT,
            self.span(range),
            format!("`{text}` is not a built-in element"),
        );
        if let Some(k) = elements::html_suggestion(text) {
            let mut replacement = k.to_owned();
            if k == "heading" && call.is_none_or(|c| c.arg_list().is_none()) {
                if let Some(level) = text.strip_prefix('h') {
                    replacement = format!("heading(level: {level})");
                }
            }
            d = d.with_fix(
                Fix::replace(
                    format!("Replace `{text}` with `{replacement}`"),
                    self.span(range),
                    replacement,
                )
                .maybe_incorrect(),
            );
        } else if let Some(c) = closest(text, elements::ELEMENTS.iter().map(|e| e.name)) {
            d = d.with_fix(
                Fix::replace(format!("Replace `{text}` with `{c}`"), self.span(range), c)
                    .maybe_incorrect(),
            );
        }
        self.report(d);
    }

    /// The insertion point for a missing argument: before `)`, or right
    /// after `name` without an argument list.
    fn insertion_point(&self, name_end: TextSize, call: Option<&ast::CallExpr>) -> TextSize {
        call.and_then(ast::CallExpr::arg_list)
            .and_then(|l| token(l.syntax(), SyntaxKind::RParen))
            .map_or(name_end, |t| t.text_range().start())
    }

    /// An element's named arguments against its table forms (K2004, K3102), with `heading(level:)` (K3109) and binding arguments.
    fn element_args(
        &mut self,
        def: &'static ElementDef,
        name: &SyntaxToken,
        call: Option<&ast::CallExpr>,
        param: &Ty,
    ) {
        let args: Vec<Arg> = match call {
            Some(c) => self
                .collect_args(c)
                .into_iter()
                .filter(|a| !self.is_trailing(c, a))
                .collect(),
            None => Vec::new(),
        };
        let labels: Vec<&str> = args
            .iter()
            .filter_map(|a| a.label.as_ref().map(|l| l.0.as_str()))
            .collect();
        // The form whose labels the call uses; the first by default.
        let form: &[elements::Param] = def
            .forms
            .iter()
            .copied()
            .find(|f| !labels.is_empty() && labels.iter().all(|l| f.iter().any(|p| p.label == *l)))
            .or_else(|| def.forms.first().copied())
            .unwrap_or(&[]);
        let mut unknown = false;
        let mut given: Vec<&str> = Vec::new();
        for a in &args {
            let Some((label, lrange)) = &a.label else {
                if let Some(e) = &a.expr {
                    self.plain(|s| s.infer(e, None));
                }
                continue;
            };
            let Some(p) = form.iter().find(|p| p.label == label.as_str()) else {
                unknown = true;
                let all: Vec<&str> = def
                    .forms
                    .iter()
                    .flat_map(|f| f.iter().map(|p| p.label))
                    .collect();
                // A label of another form (`link(to:)` vs `link(url:)`):
                // the two are alternatives, not a misspelling.
                let alternative = def
                    .forms
                    .iter()
                    .find(|f| f.iter().any(|p| p.label == label.as_str()))
                    .and_then(|other| {
                        labels
                            .iter()
                            .find(|l| !other.iter().any(|p| p.label == **l))
                            .copied()
                    });
                if let Some(alt) = alternative {
                    self.err(
                        codes::UNKNOWN_ARGUMENT_LABEL,
                        *lrange,
                        format!("`{alt}:` and `{label}:` are alternatives; pass only one"),
                    );
                    if let Some(e) = &a.expr {
                        self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                        let _ = e;
                    }
                    continue;
                }
                let mut d = Diagnostic::new(
                    codes::UNKNOWN_ARGUMENT_LABEL,
                    self.span(*lrange),
                    format!("`{}` has no argument labelled `{label}`", def.name),
                );
                let hint = elements::html_attribute(label).filter(|h| all.contains(h));
                if let Some(c) = hint.or_else(|| closest(label, all.iter().copied())) {
                    d = d.with_fix(
                        Fix::replace(
                            format!("Replace `{label}` with `{c}`"),
                            self.span(*lrange),
                            c,
                        )
                        .maybe_incorrect(),
                    );
                }
                self.report(d);
                if let Some(e) = &a.expr {
                    self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                    let _ = e;
                }
                continue;
            };
            given.push(p.label);
            self.note_res(
                *lrange,
                Resolution::Attribute {
                    element: def.name.to_owned(),
                    name: p.label.to_owned(),
                },
            );
            let Some(e) = &a.expr else { continue };
            if def.name == "heading" && p.label == "level" {
                self.heading_level(e);
                continue;
            }
            let ty = self.cx.fw.table_ty(self.cx, p.ty, param);
            self.value_arg(e, &ty);
        }
        if unknown {
            return;
        }
        let missing: Vec<&str> = form
            .iter()
            .map(|p| p.label)
            .filter(|l| !given.contains(l))
            .collect();
        if !missing.is_empty() {
            let at = self.insertion_point(name.text_range().end(), call);
            let list = if def.forms.len() > 1 {
                def.forms
                    .iter()
                    .map(|f| {
                        let labels: Vec<String> =
                            f.iter().map(|p| format!("{}:", p.label)).collect();
                        format!("`{}({})`", def.name, labels.join(""))
                    })
                    .collect::<Vec<_>>()
                    .join(" or ")
            } else {
                missing
                    .iter()
                    .map(|l| format!("`{l}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            self.report(Diagnostic::new(
                codes::MISSING_ELEMENT_ARGUMENT,
                Span::empty(self.span(name.text_range()).file, at),
                format!("`{}` needs {list}", def.name),
            ));
        }
    }

    /// `heading(level:)` takes an integer literal from 1 to 6 (K3109).
    fn heading_level(&mut self, e: &ast::Expr) {
        let ok = match e {
            ast::Expr::Literal(_) => self
                .text(e.syntax().text_range())
                .parse::<u32>()
                .is_ok_and(|n| (1..=6).contains(&n)),
            _ => false,
        };
        if ok {
            self.check(e, &builtin(self.cx.b.int));
            return;
        }
        self.quiet += 1;
        self.plain(|s| s.infer(e, None));
        self.quiet -= 1;
        self.err(
            codes::INVALID_HEADING_LEVEL,
            e.syntax().text_range(),
            "a heading level is an integer literal from 1 to 6",
        );
    }

    /// Checks an argument against a parameter type, with the binding rules
    /// for `Binding<T>` parameters (K3015, K3016, K3018).
    fn value_arg(&mut self, e: &ast::Expr, ty: &Ty) {
        let binding = self.cx.p.lookup_qualified("k.core.Binding");
        let inner = match self.resolve(ty) {
            Ty::Named(d, args) if Some(d) == binding => args.first().cloned(),
            _ => None,
        };
        match inner {
            Some(inner) => self.binding_arg(e, &inner, ty),
            None => {
                if let ast::Expr::BindExpr(b) = e {
                    self.unexpected_bind(b);
                    if let Some(place) = b.syntax().children().find_map(ast::Expr::cast) {
                        self.plain(|s| s.check(&place, ty));
                    }
                    return;
                }
                self.plain(|s| s.check(e, ty));
            }
        }
    }

    /// K3018: `bind` where no binding is expected, with a fix removing it.
    fn unexpected_bind(&mut self, b: &ast::BindExpr) {
        let start = b.syntax().text_range().start();
        let end = b
            .syntax()
            .children()
            .find_map(ast::Expr::cast)
            .map_or(b.syntax().text_range().end(), |e| {
                e.syntax().text_range().start()
            });
        let range = TextRange::new(start, end);
        self.report(
            Diagnostic::new(
                codes::UNEXPECTED_BIND,
                self.span(range),
                "`bind` is only for `@Binding` props and `Binding` arguments",
            )
            .with_fix(Fix::replace("Remove `bind `", self.span(range), "")),
        );
    }

    /// An argument for a two-way binding of `inner` (`binding` is the
    /// parameter's full type, `Binding<inner>`).
    fn binding_arg(&mut self, e: &ast::Expr, inner: &Ty, binding: &Ty) {
        if let ast::Expr::BindExpr(b) = e {
            let Some(place) = b.syntax().children().find_map(ast::Expr::cast) else {
                return;
            };
            self.in_bind = true;
            let t = self.plain(|s| s.infer(&place, Some(inner)));
            self.in_bind = false;
            let t = self.resolve(&t);
            if t.contains_error() {
                return;
            }
            if !self.bindable(&place) {
                self.err(
                    codes::NOT_BINDABLE,
                    place.syntax().text_range(),
                    "only `@State`, `@Binding` and `@Form` values (and paths into them) can be bound",
                );
                return;
            }
            // A `@Form` field binds through its text draft: a text input
            // binds any field the form parses (`bind signup.email` for an
            // `EmailAddress`, sema.md §12.10).
            let text_draft = self.is_string(inner) && self.form_rooted(&place);
            if !text_draft && !self.assignable(&t, inner, None) {
                self.mismatch(&place, &t, inner);
            }
            return;
        }
        let t = self.plain(|s| s.infer(e, Some(binding)));
        let t = self.resolve(&t);
        if t.contains_error() {
            return;
        }
        if self.assignable_quiet(&t, binding) {
            return;
        }
        let mut d = Diagnostic::new(
            codes::MISSING_BIND,
            self.span(e.syntax().text_range()),
            "a two-way binding is passed with `bind`",
        );
        if self.bindable(e) {
            d = d.with_fix(Fix::replace(
                "Insert `bind `",
                Span::empty(
                    self.span(e.syntax().text_range()).file,
                    e.syntax().text_range().start(),
                ),
                "bind ",
            ));
        }
        self.report(d);
    }

    /// Returns `true` for `String`.
    fn is_string(&self, t: &Ty) -> bool {
        let t = self.resolve(t);
        t.decl().is_some() && t.decl() == self.cx.b.string
    }

    /// Returns `true` if `place` is a path into a `@Form` field.
    fn form_rooted(&self, place: &ast::Expr) -> bool {
        self.place_root_member(place)
            .is_some_and(|m| self.cx.p.member(m).has_annotation("Form"))
    }

    /// Returns `true` if `place` is bindable: a `@State`, `@Binding` or
    /// `@Form` field, a path into one, or a loop variable over a bindable
    /// list.
    pub(crate) fn bindable(&self, place: &ast::Expr) -> bool {
        let mut current = place.clone();
        loop {
            match &current {
                ast::Expr::ParenExpr(p) => match p.syntax().children().find_map(ast::Expr::cast) {
                    Some(inner) => current = inner,
                    None => return false,
                },
                ast::Expr::MemberExpr(m) => match m.receiver() {
                    Some(r) => current = r,
                    None => return false,
                },
                ast::Expr::NameExpr(n) => {
                    return match self.targets.get(&n.syntax().text_range()) {
                        Some(Target::Local(id)) => self.bindable_locals.contains(id),
                        Some(Target::Member(m)) => {
                            let member = self.cx.p.member(*m);
                            member.has_annotation("State")
                                || member.has_annotation("Binding")
                                || member.has_annotation("Form")
                        }
                        _ => false,
                    };
                }
                _ => return false,
            }
        }
    }

    /// One chained call on an element: `.on(…)`, `.class(…)` or an attribute from the table (K3103, K3104, K3110).
    fn element_link(
        &mut self,
        def: &'static ElementDef,
        link: &Link,
        set: &mut HashMap<String, TextRange>,
    ) {
        let name = link.name.text();
        let range = link.name.text_range();
        let Some(call) = &link.call else {
            return;
        };
        let args = self.collect_args(call);
        if name == "on" {
            let events: Vec<(String, Ty)> = def
                .all_events()
                .map(|e| {
                    let payload = self.cx.fw.table_ty(self.cx, e.payload, &Ty::Error);
                    (e.name.to_owned(), payload)
                })
                .collect();
            let owner = def.name.to_owned();
            self.on_call(call, &args, &events, &owner, set);
            return;
        }
        if self.duplicate(set, name, range) {
            self.plain(|s| s.args_unchecked(&args));
            return;
        }
        if name == "class" {
            self.class_arg(call, &args);
            return;
        }
        let Some(attr) = def.attribute(name) else {
            let candidates = elements::UNIVERSAL_ATTRIBUTES
                .iter()
                .chain(def.attributes.iter())
                .map(|p| p.label);
            let mut d = Diagnostic::new(
                codes::UNKNOWN_ATTRIBUTE,
                self.span(range),
                format!("`{}` has no attribute `{name}`", def.name),
            );
            if let Some(c) = closest(name, candidates) {
                d = d.with_fix(
                    Fix::replace(format!("Replace `{name}` with `{c}`"), self.span(range), c)
                        .maybe_incorrect(),
                );
            }
            self.report(d);
            self.quiet += 1;
            self.plain(|s| s.args_unchecked(&args));
            self.quiet -= 1;
            return;
        };
        self.note_res(
            range,
            Resolution::Attribute {
                element: def.name.to_owned(),
                name: attr.label.to_owned(),
            },
        );
        let ty = self.cx.fw.table_ty(self.cx, attr.ty, &Ty::Error);
        let Some(value) = args.first().and_then(|a| a.expr.clone()) else {
            self.missing_value(call, attr.label);
            return;
        };
        self.value_arg(&value, &ty);
        if attr.label == "role" {
            self.redundant_role(&value);
        }
        if !broken(call) {
            self.plain(|s| s.args_unchecked(&args[1..]));
        }
    }

    /// K3110: `.role(.navigation)` and the like, which a semantic element
    /// expresses.
    fn redundant_role(&mut self, value: &ast::Expr) {
        let ast::Expr::ImplicitMemberExpr(m) = value else {
            return;
        };
        let Some(case) = m
            .syntax()
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
        else {
            return;
        };
        if let Some((_, element)) = SEMANTIC_ROLES.iter().find(|(r, _)| *r == case.text()) {
            let range = m.syntax().text_range();
            self.report(
                Diagnostic::new(
                    codes::REDUNDANT_ROLE,
                    self.span(range),
                    format!("the role `{}` has a semantic element", case.text()),
                )
                .with_help(format!("use `{element}`")),
            );
        }
    }

    /// K2003 for a chained option without its value (`.disabled()`).
    fn missing_value(&mut self, call: &ast::CallExpr, label: &str) {
        let at = call
            .arg_list()
            .and_then(|l| token(l.syntax(), SyntaxKind::RParen))
            .map_or(call.syntax().text_range().end(), |t| t.text_range().start());
        self.report(Diagnostic::new(
            codes::MISSING_ARGUMENT,
            Span::empty(self.span(call.syntax().text_range()).file, at),
            format!("`.{label}(…)` needs a value"),
        ));
    }

    /// Records a chained name; K3104 when it was set before.
    fn duplicate(
        &mut self,
        set: &mut HashMap<String, TextRange>,
        name: &str,
        range: TextRange,
    ) -> bool {
        if let Some(first) = set.get(name) {
            let first = *first;
            self.report(
                Diagnostic::new(
                    codes::DUPLICATE_ATTRIBUTE,
                    self.span(range),
                    format!("`{name}` is set twice"),
                )
                .with_label(self.span(first), "first set here"),
            );
            return true;
        }
        set.insert(name.to_owned(), range);
        false
    }

    /// `.on(name: handler)`: one event per call (K3108), known events
    /// (K3105), each event once (K3104), handlers that fit the payload and
    /// don't throw (K2001, K3017).
    fn on_call(
        &mut self,
        call: &ast::CallExpr,
        args: &[Arg],
        events: &[(String, Ty)],
        owner: &str,
        set: &mut HashMap<String, TextRange>,
    ) {
        for (i, a) in args.iter().enumerate() {
            let Some((label, lrange)) = &a.label else {
                self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                continue;
            };
            if i > 0 {
                let mut d = Diagnostic::new(
                    codes::MULTIPLE_EVENTS_IN_ON,
                    self.span(*lrange),
                    "`.on(…)` takes one event; chain another `.on(…)`",
                );
                if let Some(prev) = args.get(i - 1) {
                    let gap = TextRange::new(prev.range.end(), a.range.start());
                    d = d.with_fix(Fix::replace(
                        "Split into one `.on(…)` per event",
                        self.span(gap),
                        ").on(",
                    ));
                }
                self.report(d);
            }
            let Some((_, payload)) = events.iter().find(|(n, _)| n == label) else {
                let names: Vec<String> = events.iter().map(|(n, _)| format!("`{n}`")).collect();
                let hint = if names.is_empty() {
                    "it fires no events".to_owned()
                } else {
                    format!("its events are {}", names.join(", "))
                };
                self.err(
                    codes::UNKNOWN_EVENT,
                    *lrange,
                    format!("`{owner}` has no event `{label}`: {hint}"),
                );
                self.quiet += 1;
                self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                self.quiet -= 1;
                continue;
            };
            let payload = payload.clone();
            self.note_res(
                *lrange,
                Resolution::Event {
                    owner: owner.to_owned(),
                    name: label.clone(),
                },
            );
            let key = format!("on:{label}");
            if self.duplicate_event(set, &key, label, *lrange) {
                self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                continue;
            }
            if let Some(e) = &a.expr {
                self.handler(e, &payload);
            }
        }
        let _ = call;
    }

    /// Records an event listened to on one call site; K3104 when it was before.
    fn duplicate_event(
        &mut self,
        set: &mut HashMap<String, TextRange>,
        key: &str,
        label: &str,
        range: TextRange,
    ) -> bool {
        if let Some(first) = set.get(key) {
            let first = *first;
            self.report(
                Diagnostic::new(
                    codes::DUPLICATE_ATTRIBUTE,
                    self.span(range),
                    format!("`{label}` is listened to twice"),
                )
                .with_label(self.span(first), "first set here"),
            );
            return true;
        }
        set.insert(key.to_owned(), range);
        false
    }

    /// Checks an event handler for `payload` (`Void` for none): a function
    /// that takes the payload, may be `async` and can't throw.
    fn handler(&mut self, e: &ast::Expr, payload: &Ty) {
        let void = builtin(self.cx.b.void);
        let params = if self.is_void(payload) {
            Vec::new()
        } else {
            vec![payload.clone()]
        };
        let expected = Ty::func(params, true, None, void);
        // A handler runs in the browser only.
        let saved_ssr = std::mem::replace(&mut self.ssr, false);
        if matches!(e, ast::Expr::LambdaExpr(_)) {
            self.plain(|s| s.check(e, &expected));
            self.ssr = saved_ssr;
            return;
        }
        self.ssr = saved_ssr;
        let t = self.plain(|s| s.infer(e, Some(&expected)));
        let t = self.resolve(&t);
        if t.contains_error() {
            return;
        }
        if let Ty::Func(f) = &t {
            if f.throws.as_ref().is_some_and(|t| !matches!(t, Ty::Never)) {
                self.err(
                    codes::THROWING_HANDLER,
                    e.syntax().text_range(),
                    "an event handler can't throw: nothing could catch its error",
                );
                return;
            }
        }
        if !self.assignable(&t, &expected, None) {
            self.mismatch(e, &t, &expected);
        }
    }

    // ------------------------------------------------------------ classes

    /// `.class(...)` on an element: class references of the enclosing
    /// declaration's style block.
    fn class_arg(&mut self, call: &ast::CallExpr, args: &[Arg]) {
        let Some(value) = args.first().and_then(|a| a.expr.clone()) else {
            self.missing_value(call, "class");
            return;
        };
        self.class_value(&value, true);
        if !broken(call) {
            self.plain(|s| s.args_unchecked(&args[1..]));
        }
    }

    /// Checks a class value: `.name` references, lists, `x when c`, or any
    /// `Classes` value. With `report` off, references are only recorded
    /// as used (the attribute itself was already reported).
    pub(crate) fn class_value(&mut self, e: &ast::Expr, report: bool) {
        match e {
            ast::Expr::ImplicitMemberExpr(m) => {
                let Some(name) = m
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind().is_name_like())
                else {
                    return;
                };
                self.class_ref(&name, report);
            }
            ast::Expr::WhenExpr(w) => {
                let mut exprs = w.syntax().children().filter_map(ast::Expr::cast);
                if let Some(v) = exprs.next() {
                    self.class_value(&v, report);
                }
                if let Some(c) = exprs.next() {
                    let b = builtin(self.cx.b.bool_);
                    if report {
                        self.plain(|s| s.check(&c, &b));
                    } else {
                        self.plain(|s| s.infer(&c, Some(&b)));
                    }
                }
            }
            ast::Expr::ListExpr(l) => {
                for item in l.syntax().children().filter_map(ast::Expr::cast) {
                    self.class_value(&item, report);
                }
            }
            ast::Expr::ParenExpr(p) => {
                if let Some(inner) = p.syntax().children().find_map(ast::Expr::cast) {
                    self.class_value(&inner, report);
                }
            }
            _ => {
                let classes = self
                    .cx
                    .p
                    .lookup_qualified("k.core.Classes")
                    .map_or(Ty::Error, Ty::named);
                if report {
                    self.plain(|s| s.check(e, &classes));
                } else {
                    self.plain(|s| s.infer(e, None));
                }
                if let ast::Expr::NameExpr(n) = e {
                    if let Some(Target::Member(m)) = self.targets.get(&n.syntax().text_range()) {
                        if self.cx.p.member(*m).has_annotation("Stylable") {
                            self.cx.fw.applied_stylables.borrow_mut().insert(*m);
                        }
                    }
                }
            }
        }
    }

    /// A `.name` class reference against the enclosing declaration's style
    /// block (K3203).
    fn class_ref(&mut self, name: &SyntaxToken, report: bool) {
        let Some(decl) = self.style_owner() else {
            return;
        };
        let info = self.cx.fw.style(self.cx, decl);
        let text = name.text();
        if info.has_class(text) {
            self.cx
                .fw
                .used_classes
                .borrow_mut()
                .insert((decl, text.to_owned()));
            self.note_res(
                name.text_range(),
                Resolution::Class {
                    component: self.cx.p.decl_name(decl),
                    name: text.to_owned(),
                },
            );
            return;
        }
        if !report {
            return;
        }
        let range = name.text_range();
        let mut d = Diagnostic::new(
            codes::UNKNOWN_CLASS,
            self.span(range),
            if info.has_style {
                format!("the style block has no class `{text}`")
            } else {
                format!("there is no class `{text}`: this declaration has no style block")
            },
        );
        if let Some(c) = closest(text, info.classes.iter().map(|(n, _)| n.as_str())) {
            d = d.with_fix(
                Fix::replace(format!("Replace `{text}` with `{c}`"), self.span(range), c)
                    .maybe_incorrect(),
            );
        }
        self.report(d);
    }

    /// The declaration whose style block class references resolve against.
    pub(crate) fn style_owner(&self) -> Option<DeclId> {
        self.owner.decl.filter(|d| {
            matches!(
                self.cx.p.decl(*d).named_kind(),
                Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout)
            )
        })
    }

    // ------------------------------------------------------------ components

    /// The settable fields of a component and its events.
    fn component_fields(
        &mut self,
        d: DeclId,
        name: &SyntaxToken,
    ) -> (Vec<PropInfo>, Vec<(String, MemberId, Ty)>) {
        let p = self.cx.p;
        // A generic component (`LineChart<X>`) gets fresh type arguments
        // per call, inferred from its props, unless they are written out
        // (`Picker<Fruit>(…)`).
        let list = name
            .parent()
            .and_then(|n| n.children().find_map(ast::GenericArgList::cast));
        let explicit = self.type_args(list);
        let subst = match explicit.and_then(|ta| self.explicit_subst(d, &ta)) {
            Some(subst) => subst,
            None => self.fresh_subst(Some(d)),
        };
        let event = p.lookup_qualified("k.core.Event");
        let mut props = Vec::new();
        let mut events = Vec::new();
        for &m in &p.decl(d).members {
            let member = p.member(m);
            if member.kind != MemberKind::Field {
                continue;
            }
            let kind = if member.has_annotation("Prop") {
                PropKind::Prop
            } else if member.has_annotation("Binding") {
                PropKind::Binding
            } else if member.has_annotation("Slot") {
                PropKind::Slot
            } else if member.has_annotation("Stylable") {
                PropKind::Stylable
            } else if member.has_annotation("Emits") {
                let ty = crate::cx::apply(&self.cx.field_ty(m), &subst);
                let payload = match &ty {
                    Ty::Named(e, args) if Some(*e) == event => {
                        args.first().cloned().unwrap_or(Ty::Error)
                    }
                    _ => Ty::Error,
                };
                events.push((member.name.clone(), m, payload));
                continue;
            } else {
                continue;
            };
            let ty = crate::cx::apply(&self.cx.field_ty(m), &subst);
            let required =
                !member.has_default && !matches!(ty, Ty::Optional(_)) && kind != PropKind::Stylable;
            props.push(PropInfo {
                member: m,
                name: member.name.clone(),
                kind,
                ty,
                required,
            });
        }
        (props, events)
    }

    /// A component call: named props (K2003, K2004), the trailing block as its slot, chained options and events, and binding rules for `@Binding` props.
    fn component_node(
        &mut self,
        whole: &ast::Expr,
        base: &ast::Expr,
        name: &SyntaxToken,
        d: DeclId,
        call: Option<&ast::CallExpr>,
        links: &[Link],
    ) {
        let view = builtin(self.cx.b.view);
        self.note_type(base.syntax().text_range(), &view);
        self.note_type(whole.syntax().text_range(), &view);
        let (props, events) = self.component_fields(d, name);
        let mut set: HashMap<String, TextRange> = HashMap::new();
        let mut unknown = false;
        let mut given: Vec<MemberId> = Vec::new();
        if let Some(c) = call {
            let args = self.collect_args(c);
            let named: Vec<&str> = args
                .iter()
                .filter_map(|a| a.label.as_ref().map(|l| l.0.as_str()))
                .collect();
            let free =
                |p: &&PropInfo| p.kind == PropKind::Slot && !named.contains(&p.name.as_str());
            let trailing_slot = props
                .iter()
                .filter(free)
                .find(|p| p.required && self.is_view(&p.ty))
                .or_else(|| {
                    props
                        .iter()
                        .filter(free)
                        .find(|p| self.is_view(&self.deopt(&p.ty)))
                })
                .map(|p| p.member);
            for a in &args {
                if self.is_trailing(c, a) {
                    let Some(ast::Expr::LambdaExpr(l)) = &a.expr else {
                        continue;
                    };
                    match trailing_slot {
                        Some(m) => {
                            given.push(m);
                            let ty = props
                                .iter()
                                .find(|p| p.member == m)
                                .map_or(Ty::Error, |p| p.ty.clone());
                            self.slot_lambda(l, &ty);
                        }
                        None => self.view_lambda(l, Vec::new(), ViewCx::default()),
                    }
                    continue;
                }
                let Some((label, lrange)) = &a.label else {
                    self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                    continue;
                };
                let Some(prop) = props.iter().find(|p| &p.name == label).cloned() else {
                    unknown = true;
                    let mut diag = Diagnostic::new(
                        codes::UNKNOWN_ARGUMENT_LABEL,
                        self.span(*lrange),
                        format!("`{}` has no prop `{label}`", self.cx.p.decl(d).name),
                    );
                    if let Some(c) = closest(label, props.iter().map(|p| p.name.as_str())) {
                        diag = diag.with_fix(
                            Fix::replace(
                                format!("Replace `{label}` with `{c}`"),
                                self.span(*lrange),
                                c,
                            )
                            .maybe_incorrect(),
                        );
                    }
                    self.report(diag);
                    self.plain(|s| s.args_unchecked(std::slice::from_ref(a)));
                    continue;
                };
                self.note_res(*lrange, self.cx.member_resolution(prop.member));
                set.insert(prop.name.clone(), *lrange);
                given.push(prop.member);
                if let Some(e) = &a.expr {
                    self.prop_value(&prop, e);
                }
            }
        }
        for link in links {
            self.component_link(d, link, &props, &events, &mut set, &mut given);
        }
        if unknown {
            return;
        }
        let missing: Vec<&PropInfo> = props
            .iter()
            .filter(|p| p.required && !given.contains(&p.member))
            .collect();
        if !missing.is_empty() {
            let at = self.insertion_point(name.text_range().end(), call);
            let names: Vec<String> = missing.iter().map(|p| format!("`{}`", p.name)).collect();
            self.report(Diagnostic::new(
                codes::MISSING_ARGUMENT,
                Span::empty(self.span(name.text_range()).file, at),
                format!("missing argument(s) {}", names.join(", ")),
            ));
        }
    }

    /// Returns `true` for `View`.
    fn is_view(&self, ty: &Ty) -> bool {
        ty.decl().is_some() && ty.decl() == self.cx.b.view
    }

    /// Checks a value passed for a prop, binding, slot or class hook.
    fn prop_value(&mut self, prop: &PropInfo, e: &ast::Expr) {
        match prop.kind {
            PropKind::Binding => {
                let binding = self
                    .cx
                    .p
                    .lookup_qualified("k.core.Binding")
                    .map_or(Ty::Error, |b| Ty::generic(b, vec![prop.ty.clone()]));
                let inner = prop.ty.clone();
                self.binding_arg(e, &inner, &binding);
            }
            PropKind::Stylable => self.class_value(e, true),
            PropKind::Slot => {
                if let ast::Expr::LambdaExpr(l) = e {
                    self.slot_lambda(l, &prop.ty);
                } else {
                    self.plain(|s| s.check(e, &prop.ty));
                }
            }
            PropKind::Prop => self.value_arg(e, &prop.ty),
        }
    }

    /// A builder for a slot of type `ty`: `View`, `View?` or a function
    /// returning `View` whose parameters the lambda takes.
    fn slot_lambda(&mut self, l: &ast::LambdaExpr, ty: &Ty) {
        let ty = self.deopt(ty);
        let declared = super::lambda_param_names(l);
        let params: Vec<(String, TextRange, Ty)> = match &ty {
            Ty::Func(f) => declared
                .into_iter()
                .enumerate()
                .map(|(i, (n, r, _))| (n, r, f.params.get(i).cloned().unwrap_or(Ty::Error)))
                .collect(),
            _ => declared,
        };
        self.view_lambda(l, params, ViewCx::default());
    }

    /// One chained call on a component: `.on(…)`, an optional prop, slot or class hook (K3103 for unknown names, K3104 for repeats).
    fn component_link(
        &mut self,
        d: DeclId,
        link: &Link,
        props: &[PropInfo],
        events: &[(String, MemberId, Ty)],
        set: &mut HashMap<String, TextRange>,
        given: &mut Vec<MemberId>,
    ) {
        let name = link.name.text();
        let range = link.name.text_range();
        let Some(call) = &link.call else { return };
        let args = self.collect_args(call);
        if name == "on" {
            let list: Vec<(String, Ty)> = events
                .iter()
                .map(|(n, _, t)| (n.clone(), t.clone()))
                .collect();
            let owner = self.cx.p.decl_name(d);
            self.on_call(call, &args, &list, &owner, set);
            return;
        }
        let Some(prop) = props.iter().find(|p| p.name == name).cloned() else {
            let candidates = props.iter().map(|p| p.name.as_str());
            let mut diag = Diagnostic::new(
                codes::UNKNOWN_ATTRIBUTE,
                self.span(range),
                format!("`{}` has no option `{name}`", self.cx.p.decl(d).name),
            );
            if let Some(c) = closest(name, candidates) {
                diag = diag.with_fix(
                    Fix::replace(format!("Replace `{name}` with `{c}`"), self.span(range), c)
                        .maybe_incorrect(),
                );
            }
            self.report(diag);
            if name == "class" {
                for a in &args {
                    if let Some(e) = &a.expr {
                        self.class_value(e, false);
                    }
                }
                return;
            }
            self.quiet += 1;
            self.plain(|s| s.args_unchecked(&args));
            self.quiet -= 1;
            return;
        };
        if self.duplicate(set, name, range) {
            self.quiet += 1;
            self.plain(|s| s.args_unchecked(&args));
            self.quiet -= 1;
            return;
        }
        self.note_res(range, self.cx.member_resolution(prop.member));
        given.push(prop.member);
        let Some(value) = args.first().and_then(|a| a.expr.clone()) else {
            self.missing_value(call, &prop.name);
            return;
        };
        self.prop_value(&prop, &value);
        if !broken(call) {
            self.plain(|s| s.args_unchecked(&args[1..]));
        }
    }
}
