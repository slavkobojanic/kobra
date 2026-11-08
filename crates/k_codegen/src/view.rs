//! Views: DOM creation, SSR emission and hydration markers
//! (`docs/compiler/codegen.md` §11).
//!
//! A view block is first turned into a [`VNode`] tree (elements with their
//! static and dynamic attributes, texts, blocks, child components, slots).
//! Both renderers are printed from that one tree, so they walk the same
//! structure in the same order and can't drift:
//!
//! * the **client** factory: each maximal static subtree with one root
//!   element becomes a `$k.template(…)` created once per module; dynamic
//!   parts are reached by paths and bound with one render effect each;
//!   blocks and children sit between `<!--[-->`/`<!--]-->` markers;
//! * the **server** renderer: static HTML as `out.raw(…)` escaped at build
//!   time, every dynamic value through `out.text`/`out.attribute`, blocks
//!   between `$s.OPEN`/`$s.CLOSE`.

use k_sema::Resolution;
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, Resolved, Type};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::{Target, has_token, mangle, token};
use crate::js::{self, Expr, Param, Prop, Stmt, TemplatePart, bin, call, id, member, rt};
use crate::lower::Body;

/// One node of a view.
#[derive(Clone, Debug)]
pub(crate) enum VNode {
    /// An element.
    Elem(Box<Elem>),
    /// Static text.
    Text(String),
    /// A dynamic text: the expression's interpolation text.
    Dyn(ast::Expr),
    /// `if` (an `IfExpr` node).
    If(SyntaxNode),
    /// `match` (a `MatchExpr` node).
    Match(SyntaxNode),
    /// `for … key` (a `ForStmt` node).
    For(SyntaxNode),
    /// A child component instance.
    Component(Box<Comp>),
    /// A slot call: `content`, `row(item)`.
    Slot(ast::Expr),
    /// `metadata { … }` inside a view (a `failed` block's): the entries'
    /// call nodes. Server renderers set them; the client ignores them.
    Meta(Vec<SyntaxNode>),
    /// `ErrorBoundary { content } .fallback { … }`: the content, and the
    /// fallback that replaces it after a panic (09-errors.md, "Panics").
    Boundary(Vec<VNode>, Option<Vec<VNode>>),
    /// Adjacent texts of which at least one is static (`"a" title`): one
    /// text node, whose text is the pieces joined. HTML parsing merges
    /// them into one node anyway, so the client must render one node too,
    /// or its element paths and hydration would count nodes that don't
    /// exist ([`merge_texts`]).
    Mixed(Vec<Piece>),
}

/// One piece of a [`VNode::Mixed`] text.
#[derive(Clone, Debug)]
pub(crate) enum Piece {
    /// Static text.
    Static(String),
    /// A dynamic text: the expression's interpolation text.
    Dyn(ast::Expr),
}

/// Merges each run of adjacent texts that holds a static text into one
/// node: static texts are joined, empty ones dropped, and a run with a
/// dynamic text becomes a [`VNode::Mixed`]. Runs of dynamic texts only
/// stay apart (a `<!---->` separates them on both sides).
pub(crate) fn merge_texts(nodes: Vec<VNode>) -> Vec<VNode> {
    fn flush(run: &mut Vec<VNode>, out: &mut Vec<VNode>) {
        let run = std::mem::take(run);
        if !run.iter().any(|n| matches!(n, VNode::Text(_))) {
            out.extend(run);
            return;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for n in run {
            match n {
                VNode::Text(t) if t.is_empty() => {}
                VNode::Text(t) => match pieces.last_mut() {
                    Some(Piece::Static(p)) => p.push_str(&t),
                    _ => pieces.push(Piece::Static(t)),
                },
                VNode::Dyn(e) => pieces.push(Piece::Dyn(e)),
                _ => {}
            }
        }
        match pieces.len() {
            0 => {}
            1 => out.push(match pieces.pop() {
                Some(Piece::Static(t)) => VNode::Text(t),
                Some(Piece::Dyn(e)) => VNode::Dyn(e),
                None => return,
            }),
            _ => out.push(VNode::Mixed(pieces)),
        }
    }
    let mut out = Vec::with_capacity(nodes.len());
    let mut run = Vec::new();
    for n in nodes {
        if matches!(n, VNode::Text(_) | VNode::Dyn(_)) {
            run.push(n);
        } else {
            flush(&mut run, &mut out);
            out.push(n);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The text of `pieces`, joined: static pieces as literals, dynamic ones
/// through their interpolation text.
fn pieces_text(b: &mut Body<'_, '_>, pieces: &[Piece]) -> Expr {
    let mut parts = Vec::new();
    for p in pieces {
        parts.push(match p {
            Piece::Static(t) => js::str(t.clone()),
            Piece::Dyn(e) => {
                let t = b.ty(e.syntax());
                let x = b.expr(e);
                b.text_of(x, &t)
            }
        });
    }
    parts
        .into_iter()
        .reduce(|a, c| bin("+", a, c))
        .unwrap_or_else(|| js::str(""))
}

/// `true` for a node rendered as one dynamic text node.
fn is_dyn_text(n: &VNode) -> bool {
    matches!(n, VNode::Dyn(_) | VNode::Mixed(_))
}

/// An attribute value.
#[derive(Clone, Debug)]
pub(crate) enum AttrVal {
    /// A static value (`None`: a boolean attribute that is present).
    Static(Option<String>),
    /// A dynamic value; `aria` writes booleans as `"true"`/`"false"`.
    Dyn(ast::Expr, AttrKind),
    /// A generated expression (a page's `href`).
    Gen(ast::Expr),
    /// `selected` on the option whose value the dropdown's place holds
    /// (server only; the client sets the `value` property).
    Selected(ast::Expr, String),
}

/// How a dynamic attribute value is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttrKind {
    /// A text value, written as is.
    Text,
    /// A boolean attribute (present or absent).
    Bool,
    /// `"true"`/`"false"` (ARIA states).
    Aria,
}

/// A class of `.class(…)`.
#[derive(Clone, Debug)]
pub(crate) enum ClassPart {
    /// A hashed (or global) class name.
    Static(String),
    /// `.name when condition`.
    When(String, ast::Expr),
    /// Class names from an expression (a `@Stylable` value, `global(…)`).
    Dyn(ast::Expr),
}

/// A two-way control binding: a DOM property kept in sync with a place.
#[derive(Clone, Debug)]
pub(crate) struct Control {
    /// The DOM property (`value`, `checked`).
    pub prop: &'static str,
    /// The event that writes back (`input`, `change`).
    pub event: &'static str,
    /// The bound place (the operand of `bind`).
    pub place: ast::Expr,
    /// `true` for numeric fields: the text is parsed back.
    pub number: bool,
    /// How `place` is read and written.
    pub mode: ControlMode,
}

/// How a [`Control`]'s `place` is read and written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ControlMode {
    /// `bind place`: the place is read and assigned.
    #[default]
    Place,
    /// A `Binding` value (`Binding(get:set:)`, a `@Binding` field): read
    /// with `get()`, written with `set(_:)`.
    Binding,
    /// A plain value (`isChecked: todo.isDone`): read only, never written.
    OneWay,
}

/// The current value of a control's place: the place itself, or a
/// `Binding`'s `get()`.
fn control_value(b: &mut Body<'_, '_>, place: &ast::Expr, mode: ControlMode) -> Expr {
    let x = b.expr(place);
    if mode == ControlMode::Binding {
        call(member(x, "get"), vec![])
    } else {
        x
    }
}

/// An element of a view.
#[derive(Clone, Debug, Default)]
pub(crate) struct Elem {
    /// The HTML tag name, `button` or `h1`.
    pub tag: String,
    /// The attributes in source order, by HTML attribute name.
    pub attrs: Vec<(String, AttrVal)>,
    /// The classes: scoped static names, conditional ones and expressions.
    pub classes: Vec<ClassPart>,
    /// The event handlers, by DOM event name (`click`).
    pub events: Vec<(String, ast::Expr)>,
    /// The child nodes, in document order.
    pub children: Vec<VNode>,
    /// The two-way binding of a form control (`bind`), if any.
    pub control: Option<Control>,
    /// The visible `<label>` text of a form control.
    pub label: Option<String>,
    /// Reactive style values: `(custom property, expression)`.
    pub styles: Vec<(String, ast::Expr)>,
}

/// A child component call.
#[derive(Clone, Debug)]
pub(crate) struct Comp {
    /// The component declaration instantiated.
    pub decl: DeclId,
    /// Prop name → value (with `bind` kept).
    pub args: Vec<(String, ast::Expr)>,
    /// Events: name → handler.
    pub events: Vec<(String, ast::Expr)>,
    /// Slots: name → lambda.
    pub slots: Vec<(String, ast::LambdaExpr)>,
    /// Stylable values: name → class expression.
    pub stylables: Vec<(String, ast::Expr)>,
}

/// Builds the node tree of a view's statements.
pub(crate) struct Tree<'b, 'g, 'a> {
    /// The body the view belongs to, which lowers the expressions in it.
    pub b: &'b mut Body<'g, 'a>,
    /// The component whose view this is.
    pub owner: DeclId,
}

/// The HTML tag of a built-in element.
fn tag_of(name: &str) -> String {
    match name {
        "heading" => "h1".into(),
        "item" => "li".into(),
        "term" => "dt".into(),
        "definition" => "dd".into(),
        "row" => "tr".into(),
        "cell" => "td".into(),
        "headerCell" => "th".into(),
        "emphasis" => "em".into(),
        "codeBlock" => "pre".into(),
        "code" => "code".into(),
        "caption" => "figcaption".into(),
        _ => match k_sema::stdlib::elements::element(name) {
            Some(def) if def.html.chars().all(|c| c.is_ascii_alphanumeric()) => def.html.to_owned(),
            _ => name.to_ascii_lowercase(),
        },
    }
}

/// `true` for elements whose `.label` is a visible `<label>`.
fn is_form_control(name: &str) -> bool {
    matches!(
        name,
        "textField"
            | "passwordField"
            | "numberField"
            | "dateField"
            | "checkbox"
            | "textArea"
            | "dropdown"
            | "emailField"
    )
}

/// The ARIA states written as `"true"`/`"false"`.
fn aria_name(label: &str) -> Option<&'static str> {
    Some(match label {
        "pressed" => "aria-pressed",
        "expanded" => "aria-expanded",
        "selected" => "aria-selected",
        "checked" => "aria-checked",
        "current" => "aria-current",
        "busy" => "aria-busy",
        "invalid" => "aria-invalid",
        _ => return None,
    })
}

/// The entry calls of `metadata { … }` written as a call with a trailing
/// lambda inside a view, or `None` for any other call.
fn metadata_call(c: &ast::CallExpr) -> Option<Vec<SyntaxNode>> {
    let ast::Expr::NameExpr(n) = c.callee()? else {
        return None;
    };
    if n.name()?.text() != "metadata" {
        return None;
    }
    let lambda = c.trailing_lambda()?;
    Some(
        lambda
            .syntax()
            .children()
            .filter(|x| x.kind() == SyntaxKind::CallExpr)
            .collect(),
    )
}

/// HTML boolean attributes.
fn is_boolean_attr(name: &str) -> bool {
    matches!(
        name,
        "disabled"
            | "hidden"
            | "required"
            | "readonly"
            | "checked"
            | "selected"
            | "multiple"
            | "autofocus"
            | "open"
    )
}

impl Tree<'_, '_, '_> {
    /// The nodes of a block of view statements (a `Block`, a lambda body).
    pub(crate) fn nodes(&mut self, stmts: &[SyntaxNode]) -> Vec<VNode> {
        let mut out = Vec::new();
        for s in stmts {
            if let Some(n) = self.node(s) {
                out.push(n);
            }
        }
        merge_texts(out)
    }

    /// The node of one statement of a view: a block (`if`, `match`, `for`),
    /// static or dynamic text, an element or component call, a slot call or a
    /// `metadata` call; `None` for a statement that renders nothing.
    fn node(&mut self, s: &SyntaxNode) -> Option<VNode> {
        match s.kind() {
            SyntaxKind::ForStmt => return Some(VNode::For(s.clone())),
            SyntaxKind::IfExpr => return Some(VNode::If(s.clone())),
            SyntaxKind::MatchExpr => return Some(VNode::Match(s.clone())),
            _ => {}
        }
        let e = ast::Expr::cast(s.clone())?;
        match &e {
            ast::Expr::StringLit(lit) => {
                if let Some(v) = lit.plain_value() {
                    return Some(VNode::Text(v));
                }
                Some(VNode::Dyn(e.clone()))
            }
            ast::Expr::CallExpr(c) => {
                if let Some(calls) = metadata_call(c) {
                    return Some(VNode::Meta(calls));
                }
                if let Some(n) = self.call_node(&e) {
                    return Some(n);
                }
                if self.is_slot_call(&e) {
                    return Some(VNode::Slot(e.clone()));
                }
                Some(VNode::Dyn(e.clone()))
            }
            ast::Expr::NameExpr(_) | ast::Expr::MemberExpr(_) => {
                if let Some(n) = self.call_node(&e) {
                    return Some(n);
                }
                if self.is_slot_call(&e) {
                    return Some(VNode::Slot(e.clone()));
                }
                Some(VNode::Dyn(e.clone()))
            }
            ast::Expr::ParenExpr(p) => {
                let inner = p
                    .syntax()
                    .children()
                    .find(|c| ast::Expr::can_cast(c.kind()))?;
                self.node(&inner)
            }
            _ => Some(VNode::Dyn(e.clone())),
        }
    }

    /// `true` when `e` invokes a `@Slot` of the component.
    fn is_slot_call(&self, e: &ast::Expr) -> bool {
        let name = match e {
            ast::Expr::NameExpr(n) => n.name(),
            ast::Expr::CallExpr(c) => match c.callee() {
                Some(ast::Expr::NameExpr(n)) => n.name(),
                _ => None,
            },
            _ => None,
        };
        let Some(name) = name else { return false };
        match self.b.g.resolved(self.b.file, name.text_range()) {
            Some(Resolved::Member(m)) => self.b.g.view.member(m).has_annotation("Slot"),
            _ => {
                // A local bound by `if let footer` over a slot.
                matches!(
                    self.b.g.resolution(self.b.file, name.text_range()),
                    Some(Resolution::Local { .. })
                ) && self.slot_named(name.text()).is_some()
            }
        }
    }

    /// The `@Slot` member of the owner named `name`, if any.
    fn slot_named(&self, name: &str) -> Option<MemberId> {
        self.b
            .g
            .view
            .decl(self.owner)
            .members
            .iter()
            .copied()
            .find(|&m| {
                let mi = self.b.g.view.member(m);
                mi.name == name && mi.has_annotation("Slot")
            })
    }

    /// Peels chained modifiers off an element or component call.
    pub(crate) fn call_node(&mut self, e: &ast::Expr) -> Option<VNode> {
        let mut modifiers: Vec<(String, Option<ast::ArgList>, Option<ast::LambdaExpr>)> =
            Vec::new();
        let mut cur = e.clone();
        loop {
            match &cur {
                ast::Expr::CallExpr(c) => match c.callee()? {
                    ast::Expr::MemberExpr(me) => {
                        let name = me.name()?.text().to_owned();
                        let recv = me.receiver()?;
                        if !self.is_view_call(&recv) {
                            return None;
                        }
                        modifiers.push((name, c.arg_list(), c.trailing_lambda()));
                        cur = recv;
                    }
                    ast::Expr::NameExpr(_) => break,
                    _ => return None,
                },
                ast::Expr::MemberExpr(me) => {
                    // `.footer` without arguments is not a modifier.
                    let _ = me;
                    return None;
                }
                _ => return None,
            }
        }
        modifiers.reverse();
        let ast::Expr::CallExpr(base) = &cur else {
            return None;
        };
        let ast::Expr::NameExpr(n) = base.callee()? else {
            return None;
        };
        let name = n.name()?;
        match self.b.g.resolution(self.b.file, name.text_range()) {
            Some(Resolution::Element { name: el }) => Some(self.element(&el, base, &modifiers)),
            Some(r) => match self.b.g.view.resolve(&r) {
                Some(Resolved::Decl(d))
                    if matches!(
                        self.b.g.view.decl(d).named_kind(),
                        Some(DeclarationKind::Component | DeclarationKind::Layout)
                    ) =>
                {
                    Some(self.component(d, base, &modifiers))
                }
                _ => None,
            },
            None => {
                if k_sema::stdlib::elements::element(name.text()).is_some() || name.text() == "item"
                {
                    Some(self.element(name.text(), base, &modifiers))
                } else {
                    None
                }
            }
        }
    }

    /// `true` if `e` is a call chain rooted at a built-in element or a
    /// component or layout (`button("Go").disabled(x)`), so it renders a
    /// node rather than a text.
    fn is_view_call(&self, e: &ast::Expr) -> bool {
        let mut cur = e.clone();
        loop {
            match &cur {
                ast::Expr::CallExpr(c) => match c.callee() {
                    Some(ast::Expr::MemberExpr(me)) => match me.receiver() {
                        Some(r) => cur = r,
                        None => return false,
                    },
                    Some(ast::Expr::NameExpr(n)) => {
                        let Some(name) = n.name() else { return false };
                        return match self.b.g.resolution(self.b.file, name.text_range()) {
                            Some(Resolution::Element { .. }) => true,
                            Some(r) => matches!(
                                self.b.g.view.resolve(&r),
                                Some(Resolved::Decl(d)) if matches!(self.b.g.view.decl(d).named_kind(), Some(DeclarationKind::Component | DeclarationKind::Layout))
                            ),
                            None => k_sema::stdlib::elements::element(name.text()).is_some(),
                        };
                    }
                    _ => return false,
                },
                _ => return false,
            }
        }
    }

    /// The nodes of a content lambda's body (its parameters skipped); empty
    /// without one.
    fn content(&mut self, lambda: Option<ast::LambdaExpr>) -> Vec<VNode> {
        let Some(l) = lambda else { return Vec::new() };
        let stmts: Vec<SyntaxNode> = l
            .syntax()
            .children()
            .filter(|c| c.kind() != SyntaxKind::LambdaParamList)
            .collect();
        self.nodes(&stmts)
    }

    /// The node of a built-in element call with its modifier chain: the tag,
    /// attributes, classes, events, a form control's binding and label, and
    /// the children from its content lambda.
    #[allow(clippy::too_many_lines)]
    fn element(
        &mut self,
        name: &str,
        call: &ast::CallExpr,
        modifiers: &[(String, Option<ast::ArgList>, Option<ast::LambdaExpr>)],
    ) -> VNode {
        let mut el = Elem {
            tag: tag_of(name),
            ..Elem::default()
        };
        match name {
            "passwordField" => el
                .attrs
                .push(("type".into(), AttrVal::Static(Some("password".into())))),
            "numberField" => el
                .attrs
                .push(("type".into(), AttrVal::Static(Some("number".into())))),
            "checkbox" => el
                .attrs
                .push(("type".into(), AttrVal::Static(Some("checkbox".into())))),
            "dateField" => el
                .attrs
                .push(("type".into(), AttrVal::Static(Some("date".into())))),
            "emailField" => el
                .attrs
                .push(("type".into(), AttrVal::Static(Some("email".into())))),
            _ => {}
        }
        if let Some(list) = call.arg_list() {
            for a in list.args() {
                let (Some(label), Some(v)) = (a.label(), a.value()) else {
                    continue;
                };
                match label.text() {
                    "level" => {
                        if let ast::Expr::Literal(l) = &v {
                            if let Some(t) = l.token() {
                                el.tag = format!("h{}", t.text().trim());
                            }
                        }
                    }
                    "value" | "text" | "selection" | "isChecked"
                        if !matches!(v, ast::Expr::BindExpr(_)) =>
                    {
                        // A `Binding` value is two-way through its `get` and
                        // `set`; a checkbox's plain value sets `checked`.
                        let t = self.b.ty(v.syntax());
                        let is_binding = t
                            .decl()
                            .is_some_and(|d| self.b.g.view.qualified_name(d) == "k.core.Binding");
                        let checkbox = label.text() == "isChecked";
                        if is_binding || checkbox {
                            el.control = Some(Control {
                                prop: if checkbox { "checked" } else { "value" },
                                event: if checkbox || label.text() == "selection" {
                                    "change"
                                } else {
                                    "input"
                                },
                                place: v.clone(),
                                number: name == "numberField",
                                mode: if is_binding {
                                    ControlMode::Binding
                                } else {
                                    ControlMode::OneWay
                                },
                            });
                        } else {
                            el.attrs
                                .push(("value".into(), self.attr_value(&v, AttrKind::Text)));
                        }
                    }
                    "value" | "text" | "selection" | "isChecked" => {
                        if let ast::Expr::BindExpr(be) = &v {
                            if let Some(place) = be.syntax().children().find_map(ast::Expr::cast) {
                                let checkbox = label.text() == "isChecked";
                                el.control = Some(Control {
                                    prop: if checkbox { "checked" } else { "value" },
                                    event: if checkbox || label.text() == "selection" {
                                        "change"
                                    } else {
                                        "input"
                                    },
                                    place,
                                    number: name == "numberField",
                                    mode: ControlMode::Place,
                                });
                            }
                        }
                    }
                    "source" => el
                        .attrs
                        .push(("src".into(), self.attr_value(&v, AttrKind::Text))),
                    "description" => el
                        .attrs
                        .push(("alt".into(), self.attr_value(&v, AttrKind::Text))),
                    "url" => el
                        .attrs
                        .push(("href".into(), self.attr_value(&v, AttrKind::Text))),
                    "to" => {
                        let href = crate::app::static_href(self.b, &v);
                        match href {
                            Some(h) => el.attrs.push(("href".into(), AttrVal::Static(Some(h)))),
                            None => el.attrs.push(("href".into(), AttrVal::Gen(v.clone()))),
                        }
                    }
                    other => {
                        let attr = crate::css::kebab(other);
                        el.attrs.push((attr, self.attr_value(&v, AttrKind::Text)));
                    }
                }
            }
        }
        el.children = self.content(call.trailing_lambda());
        if el.tag == "table"
            && el
                .children
                .iter()
                .any(|c| matches!(c, VNode::Elem(e) if e.tag == "tr"))
        {
            // Browsers put rows in a `<tbody>`; the template must match.
            let rows = std::mem::take(&mut el.children);
            el.children = vec![VNode::Elem(Box::new(Elem {
                tag: "tbody".into(),
                children: rows,
                ..Elem::default()
            }))];
        }
        for (m, args, lambda) in modifiers {
            let first = args.as_ref().and_then(|l| l.args().next());
            let value = first.as_ref().and_then(ast::Arg::value);
            match m.as_str() {
                "on" => {
                    if let Some(list) = args {
                        for a in list.args() {
                            if let (Some(l), Some(v)) = (a.label(), a.value()) {
                                el.events.push((l.text().to_ascii_lowercase(), v));
                            }
                        }
                    }
                }
                "class" => {
                    if let Some(v) = value {
                        self.classes(&v, &mut el.classes);
                    }
                }
                "label" => {
                    // A form control's literal label is a visible
                    // `<label>`; a computed one names it through
                    // `aria-label`, so the control is never unnamed.
                    let literal = match &value {
                        Some(ast::Expr::StringLit(s)) if is_form_control(name) => s.plain_value(),
                        _ => None,
                    };
                    if literal.is_some() {
                        el.label = literal;
                    } else if let Some(v) = &value {
                        el.attrs
                            .push(("aria-label".into(), self.attr_value(v, AttrKind::Text)));
                    }
                }
                _ => {
                    let Some(v) = value else {
                        let _ = lambda;
                        continue;
                    };
                    if let Some(aria) = aria_name(m) {
                        el.attrs
                            .push((aria.into(), self.attr_value(&v, AttrKind::Aria)));
                    } else if is_boolean_attr(m) {
                        el.attrs
                            .push((m.clone(), self.attr_value(&v, AttrKind::Bool)));
                    } else {
                        el.attrs
                            .push((crate::css::kebab(m), self.attr_value(&v, AttrKind::Text)));
                    }
                }
            }
        }
        // Reactive style values of the element's classes.
        let style_vars = crate::css::reactive_values(self.b, self.owner, &el.classes);
        el.styles = style_vars;
        VNode::Elem(Box::new(el))
    }

    /// A static attribute value when `v` is a literal, a dynamic one
    /// otherwise.
    fn attr_value(&mut self, v: &ast::Expr, kind: AttrKind) -> AttrVal {
        match v {
            ast::Expr::StringLit(s) => {
                if let Some(t) = s.plain_value() {
                    return AttrVal::Static(Some(t));
                }
            }
            ast::Expr::Literal(l) => {
                if let Some(t) = l.token() {
                    match t.kind() {
                        SyntaxKind::TrueKw => {
                            return match kind {
                                AttrKind::Aria => AttrVal::Static(Some("true".into())),
                                _ => AttrVal::Static(None),
                            };
                        }
                        SyntaxKind::FalseKw => {
                            return match kind {
                                AttrKind::Aria => AttrVal::Static(Some("false".into())),
                                AttrKind::Bool => AttrVal::Static(Some("\u{0}absent".into())),
                                AttrKind::Text => AttrVal::Static(Some("false".into())),
                            };
                        }
                        SyntaxKind::IntLit | SyntaxKind::FloatLit => {
                            return AttrVal::Static(Some(t.text().to_owned()));
                        }
                        _ => {}
                    }
                }
            }
            ast::Expr::ImplicitMemberExpr(m) => {
                if let Some(n) = m.name() {
                    return AttrVal::Static(Some(crate::css::kebab(n.text())));
                }
            }
            _ => {}
        }
        AttrVal::Dyn(v.clone(), kind)
    }

    /// The classes of `.class(…)`: one class, a list, `when` conditions,
    /// `global(…)` names and `@Stylable` values.
    fn classes(&mut self, v: &ast::Expr, out: &mut Vec<ClassPart>) {
        match v {
            ast::Expr::ListExpr(l) => {
                for item in l.syntax().children().filter_map(ast::Expr::cast) {
                    self.classes(&item, out);
                }
            }
            ast::Expr::ImplicitMemberExpr(m) => {
                if let Some(n) = m.name() {
                    out.push(ClassPart::Static(crate::css::class_name(
                        self.b.g,
                        self.owner,
                        n.text(),
                    )));
                }
            }
            ast::Expr::WhenExpr(w) => {
                let mut es = w.syntax().children().filter_map(ast::Expr::cast);
                if let (Some(ast::Expr::ImplicitMemberExpr(m)), Some(c)) = (es.next(), es.next()) {
                    if let Some(n) = m.name() {
                        out.push(ClassPart::When(
                            crate::css::class_name(self.b.g, self.owner, n.text()),
                            c,
                        ));
                    }
                }
            }
            ast::Expr::CallExpr(c) => {
                let is_global = matches!(c.callee(), Some(ast::Expr::NameExpr(n)) if n.name().is_some_and(|t| t.text() == "global"));
                if is_global {
                    if let Some(ast::Expr::StringLit(s)) = c
                        .arg_list()
                        .and_then(|l| l.args().next())
                        .and_then(|a| a.value())
                    {
                        if let Some(t) = s.plain_value() {
                            out.push(ClassPart::Static(t));
                            return;
                        }
                    }
                }
                out.push(ClassPart::Dyn(v.clone()));
            }
            _ => out.push(ClassPart::Dyn(v.clone())),
        }
    }

    /// The node of a child component call: its props (with `bind` kept),
    /// events, slots (the trailing lambda fills the first `@Slot`, or
    /// `content`) and stylable values.
    fn component(
        &mut self,
        d: DeclId,
        call: &ast::CallExpr,
        modifiers: &[(String, Option<ast::ArgList>, Option<ast::LambdaExpr>)],
    ) -> VNode {
        let mut comp = Comp {
            decl: d,
            args: Vec::new(),
            events: Vec::new(),
            slots: Vec::new(),
            stylables: Vec::new(),
        };
        if let Some(list) = call.arg_list() {
            for a in list.args() {
                if let (Some(l), Some(v)) = (a.label(), a.value()) {
                    comp.args.push((l.text().to_owned(), v));
                }
            }
        }
        if let Some(l) = call.trailing_lambda() {
            let name = self.default_slot(d).unwrap_or_else(|| "content".into());
            comp.slots.push((name, l));
        }
        let native = self.b.g.view.decl(d).has_annotation("Native");
        for (m, args, lambda) in modifiers {
            if m == "on" {
                if let Some(list) = args {
                    for a in list.args() {
                        if let (Some(l), Some(v)) = (a.label(), a.value()) {
                            comp.events.push((l.text().to_owned(), v));
                        }
                    }
                }
                continue;
            }
            let member = self.b.g.find_member(d, m);
            let is_slot = member.is_some_and(|x| self.b.g.view.member(x).has_annotation("Slot"));
            let is_stylable =
                member.is_some_and(|x| self.b.g.view.member(x).has_annotation("Stylable"));
            if is_slot {
                if let Some(l) = lambda {
                    comp.slots.push((m.clone(), l.clone()));
                }
                continue;
            }
            // A chained option of a function type may take its lambda as a
            // trailing block (`.itemLabel { card => card.title }`).
            let value = args
                .as_ref()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())
                .or_else(|| lambda.clone().map(ast::Expr::LambdaExpr));
            if let Some(v) = value {
                if is_stylable {
                    comp.stylables.push((m.clone(), v));
                } else {
                    comp.args.push((m.clone(), v));
                }
            }
        }
        if native {
            if let Some(n) = self.native_component(&comp) {
                return n;
            }
        }
        VNode::Component(Box::new(comp))
    }

    /// The stdlib's `@Native` components, which have no generated body:
    /// they lower to plain elements (`Stack`, `Grid`, `Spacer`,
    /// `VisuallyHidden`, `Icon`) or to a boundary block (`ErrorBoundary`)
    /// in place (05-views.md, "The stdlib provides layout components").
    /// Layout values are read at compile time: an `axis`, a `gap` token
    /// (`.md`) and an integer `columns` literal; any other expression keeps
    /// the default (codegen.md §10).
    fn native_component(&mut self, comp: &Comp) -> Option<VNode> {
        let name = self.b.g.view.qualified_name(comp.decl);
        let arg = |n: &str| {
            comp.args
                .iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
        };
        let slot = |n: &str| {
            comp.slots
                .iter()
                .find(|(k, _)| k == n)
                .map(|(_, l)| l.clone())
        };
        // `.name` → `name`, for an enum case or a theme token argument.
        let case = |e: Option<ast::Expr>| -> Option<String> {
            let text = e?.syntax().text().to_string();
            let t = text.trim().strip_prefix('.')?;
            t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                .then(|| t.to_owned())
        };
        let gap = case(arg("gap"))
            .map(|t| format!("var(--k-space-{})", crate::css::kebab(&t)))
            .unwrap_or_else(|| "0".to_owned());
        let styled = |tag: &str, style: String, children: Vec<VNode>| Elem {
            tag: tag.to_owned(),
            attrs: vec![("style".into(), AttrVal::Static(Some(style)))],
            children,
            ..Elem::default()
        };
        let el = match name.as_str() {
            "k.ui.ErrorBoundary" => {
                let content = self.content(slot("content"));
                let fallback = slot("fallback").map(|l| self.content(Some(l)));
                return Some(VNode::Boundary(content, fallback));
            }
            "k.ui.Stack" => {
                let dir = if case(arg("axis")).as_deref() == Some("horizontal") {
                    "row"
                } else {
                    "column"
                };
                let children = self.content(slot("content"));
                styled(
                    "div",
                    format!("display: flex; flex-direction: {dir}; gap: {gap}"),
                    children,
                )
            }
            "k.ui.Grid" => {
                let columns = arg("columns")
                    .and_then(|e| e.syntax().text().to_string().trim().parse::<u32>().ok())
                    .filter(|&n| (1..=64).contains(&n))
                    .unwrap_or(2);
                let children = self.content(slot("content"));
                styled(
                    "div",
                    format!(
                        "display: grid; grid-template-columns: repeat({columns}, minmax(0, 1fr)); gap: {gap}"
                    ),
                    children,
                )
            }
            "k.ui.Spacer" => {
                let mut e = styled("div", "flex: 1 1 auto".to_owned(), Vec::new());
                e.attrs
                    .push(("aria-hidden".into(), AttrVal::Static(Some("true".into()))));
                e
            }
            "k.ui.VisuallyHidden" => {
                let children = self.content(slot("content"));
                styled(
                    "span",
                    "position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0".to_owned(),
                    children,
                )
            }
            "k.ui.Icon" => {
                let mut e = Elem {
                    tag: "span".into(),
                    ..Elem::default()
                };
                if let Some(n) = case(arg("name")) {
                    e.attrs.push(("data-icon".into(), AttrVal::Static(Some(n))));
                }
                match arg("label") {
                    Some(l) if l.syntax().text().to_string().trim() != "nil" => {
                        e.attrs
                            .push(("role".into(), AttrVal::Static(Some("img".into()))));
                        let v = self.attr_value(&l, AttrKind::Text);
                        e.attrs.push(("aria-label".into(), v));
                    }
                    _ => e
                        .attrs
                        .push(("aria-hidden".into(), AttrVal::Static(Some("true".into())))),
                }
                e
            }
            _ => return None,
        };
        Some(VNode::Elem(Box::new(el)))
    }

    /// The name of the first `@Slot` member of component `d`, which a trailing
    /// content lambda fills.
    fn default_slot(&self, d: DeclId) -> Option<String> {
        self.b.g.view.decl(d).members.iter().copied().find_map(|m| {
            let mi = self.b.g.view.member(m);
            mi.has_annotation("Slot").then(|| mi.name.clone())
        })
    }
}

// ------------------------------------------------------------------ html

/// Escapes static text for HTML.
pub(crate) fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escapes a static attribute value.
pub(crate) fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

/// The HTML void elements, which have no end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The static start tag of an element (static attributes and classes).
fn start_tag(el: &Elem) -> String {
    let mut s = format!("<{}", el.tag);
    let statics: Vec<&str> = el
        .classes
        .iter()
        .filter_map(|c| match c {
            ClassPart::Static(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    let dynamic_classes = el
        .classes
        .iter()
        .any(|c| !matches!(c, ClassPart::Static(_)));
    if !statics.is_empty() && !dynamic_classes {
        s.push_str(&format!(" class=\"{}\"", escape_attr(&statics.join(" "))));
    }
    for (name, v) in &el.attrs {
        if let AttrVal::Static(v) = v {
            match v {
                None => s.push_str(&format!(" {name}")),
                Some(t) if t == "\u{0}absent" => {}
                Some(t) => s.push_str(&format!(" {name}=\"{}\"", escape_attr(t))),
            }
        }
    }
    s.push('>');
    s
}

/// The end tag of an element; empty for a void element ([`VOID`]).
fn end_tag(el: &Elem) -> String {
    if VOID.contains(&el.tag.as_str()) {
        String::new()
    } else {
        format!("</{}>", el.tag)
    }
}

/// One step of a path from a template root.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// `.firstChild`: down to the first child.
    First,
    /// `.nextSibling`: on to the next sibling.
    Next,
}

/// The expression that reaches a node of a cloned template from its root
/// `root` by following `path`, `root.firstChild.nextSibling…`.
fn path_expr(root: &str, path: &[Step]) -> Expr {
    let mut e = id(root);
    for s in path {
        e = member(
            e,
            match s {
                Step::First => "firstChild",
                Step::Next => "nextSibling",
            },
        );
    }
    e
}

// ---------------------------------------------------------------- client

/// Renders view nodes on the client.
pub(crate) struct Client<'b, 'g, 'a> {
    /// The body the view belongs to.
    pub b: &'b mut Body<'g, 'a>,
    /// The component whose view is rendered.
    pub owner: DeclId,
    /// The component's async-handler context name, when one is needed.
    pub lifetime: Option<String>,
}

/// A dynamic part found while building a template.
enum Part {
    /// A dynamic text node.
    Text(ast::Expr),
    /// A text node joined from static and dynamic pieces.
    Mixed(Vec<Piece>),
    /// An element with dynamic attributes, classes, styles, events or a
    /// control.
    Elem(Box<Elem>),
    /// A block, child component or slot, rendered between markers.
    Block(VNode),
    /// The `<label>` of the labeled control that follows it in the
    /// template: both get one fresh `uid()` (`for` and `id`).
    Label,
}

impl Client<'_, '_, '_> {
    /// The module-level `template(…)` constant of `html`, shared by equal
    /// templates of one declaration. Its name carries the declaration's
    /// (`Header$t0`), because a chunk concatenates the preludes of many
    /// declarations from several files.
    fn template_const(&mut self, html: String) -> String {
        let owner = self.b.owner.map(|d| self.b.g.name(d)).unwrap_or_default();
        let n = format!("{owner}$t{}", self.b.prelude.len());
        let existing = self.b.prelude.iter().find_map(|s| match s {
            Stmt::Const(name, Expr::Call(_, args)) if matches!(args.first(), Some(Expr::Html(h)) if *h == html) => Some(name.clone()),
            _ => None,
        });
        if let Some(e) = existing {
            return e;
        }
        self.b.prelude.push(Stmt::Const(
            n.clone(),
            rt("template", vec![Expr::Html(html)]),
        ));
        n
    }

    /// Renders a fragment of nodes; returns the expression of its node (an
    /// element, a fragment, or `null`).
    pub(crate) fn fragment(&mut self, nodes: &[VNode]) -> Expr {
        let mut roots: Vec<Expr> = Vec::new();
        let mut prev_dyn = false;
        for n in nodes {
            let is_dyn = is_dyn_text(n);
            if is_dyn && prev_dyn {
                // `<!---->` between two dynamic texts, as the server writes.
                let sep = self.b.fresh("sep");
                self.b
                    .stmts
                    .push(Stmt::Const(sep.clone(), rt("separator", vec![])));
                roots.push(id(sep));
            }
            if let Some(r) = self.root(n) {
                roots.push(r);
            }
            prev_dyn = is_dyn;
        }
        match roots.len() {
            0 => Expr::Null,
            1 => roots.into_iter().next().unwrap_or(Expr::Null),
            _ => {
                let f = self.b.fresh("f");
                self.b.stmts.push(Stmt::Const(
                    f.clone(),
                    call(member(id("document"), "createDocumentFragment"), vec![]),
                ));
                for r in roots {
                    self.b
                        .stmts
                        .push(Stmt::Expr(call(member(id(&f), "appendChild"), vec![r])));
                }
                id(f)
            }
        }
    }

    /// Renders one root node.
    fn root(&mut self, n: &VNode) -> Option<Expr> {
        match n {
            // A `failed` block's metadata: its title, while the block is
            // mounted (runtime.md §10); no node.
            VNode::Meta(calls) => {
                let set = crate::server::client_title(self.b, calls, None);
                self.b.stmts.extend(set);
                None
            }
            VNode::Elem(el) => Some(self.element_root(el)),
            // `textNode` claims the server's text node while hydrating, so
            // it is created in order with the other roots' claims.
            VNode::Text(t) => {
                let node = self.b.fresh("n");
                self.b.stmts.push(Stmt::Const(
                    node.clone(),
                    rt("textNode", vec![js::str(t.clone())]),
                ));
                Some(id(node))
            }
            VNode::Dyn(e) => {
                let node = self.b.fresh("n");
                self.b
                    .stmts
                    .push(Stmt::Const(node.clone(), rt("textNode", vec![js::str("")])));
                self.text_binding(&id(&node), e);
                Some(id(node))
            }
            VNode::Mixed(pieces) => {
                let node = self.b.fresh("n");
                self.b
                    .stmts
                    .push(Stmt::Const(node.clone(), rt("textNode", vec![js::str("")])));
                self.pieces_binding(&id(&node), pieces);
                Some(id(node))
            }
            other => {
                let m = self.b.fresh("m");
                self.b
                    .stmts
                    .push(Stmt::Const(m.clone(), rt("markers", vec![])));
                self.block(other, &member(id(&m), "start"), &member(id(&m), "end"));
                Some(member(id(m), "node"))
            }
        }
    }

    /// Renders an element with its static subtree as one module-level
    /// template: clones it, reaches every dynamic part by its path and binds
    /// it. A visible label gets a template of its own, cloned first. Returns
    /// the expression of the created node (a fragment when there is a label).
    fn element_root(&mut self, el: &Elem) -> Expr {
        let mut html = String::new();
        let mut parts: Vec<(Vec<Step>, Part)> = Vec::new();
        let label = el.label.clone();
        build(el, &mut html, &mut Vec::new(), &mut parts, true);
        let t = self.template_const(html);
        // A visible label precedes its control in the document, so its
        // template is claimed first during hydration.
        let label_node = label.as_ref().map(|text| {
            let lt = self.template_const(format!("<label>{}</label>", escape_text(text)));
            let l = self.b.fresh("label");
            self.b
                .stmts
                .push(Stmt::Const(l.clone(), call(id(lt), vec![])));
            l
        });
        let root = self.b.fresh("root");
        self.b
            .stmts
            .push(Stmt::Const(root.clone(), call(id(t), vec![])));
        // Resolve every node reference before any block renders.
        let mut refs = Vec::new();
        for (path, part) in parts {
            let node = self.b.fresh("e");
            self.b
                .stmts
                .push(Stmt::Const(node.clone(), path_expr(&root, &path)));
            refs.push((node, part));
        }
        let mut block_ends: Vec<String> = Vec::new();
        let mut label_id: Option<String> = None;
        for (node, part) in refs {
            match part {
                Part::Text(e) => self.text_binding(&id(&node), &e),
                Part::Mixed(p) => self.pieces_binding(&id(&node), &p),
                Part::Label => {
                    let uid = self.b.fresh("id");
                    self.b
                        .stmts
                        .push(Stmt::Const(uid.clone(), rt("uid", vec![])));
                    self.b.stmts.push(Stmt::Expr(rt(
                        "attr",
                        vec![id(&node), js::str("for"), id(&uid)],
                    )));
                    label_id = Some(uid);
                }
                Part::Elem(el) => {
                    if el.label.is_some() {
                        if let Some(uid) = label_id.take() {
                            self.b.stmts.push(Stmt::Expr(rt(
                                "attr",
                                vec![id(&node), js::str("id"), id(uid)],
                            )));
                        }
                    }
                    self.element_bindings(&id(&node), &el);
                }
                Part::Block(b) => {
                    let end = self.b.fresh("end");
                    self.b
                        .stmts
                        .push(Stmt::Const(end.clone(), member(id(&node), "nextSibling")));
                    block_ends.push(end.clone());
                    self.block(&b, &id(&node), &id(&end));
                }
            }
        }
        if let Some(l) = label_node {
            // A visible label linked to the control.
            let uid = self.b.fresh("id");
            self.b
                .stmts
                .push(Stmt::Const(uid.clone(), rt("uid", vec![])));
            self.b.stmts.push(Stmt::Expr(rt(
                "attr",
                vec![id(&l), js::str("for"), id(&uid)],
            )));
            self.b.stmts.push(Stmt::Expr(rt(
                "attr",
                vec![id(&root), js::str("id"), id(&uid)],
            )));
            let f = self.b.fresh("f");
            self.b.stmts.push(Stmt::Const(
                f.clone(),
                call(member(id("document"), "createDocumentFragment"), vec![]),
            ));
            self.b.stmts.push(Stmt::Expr(call(
                member(id(&f), "appendChild"),
                vec![id(&l)],
            )));
            self.b.stmts.push(Stmt::Expr(call(
                member(id(&f), "appendChild"),
                vec![id(&root)],
            )));
            return id(f);
        }
        id(root)
    }

    /// Binds the text of `node` to the joined `pieces`, with one render effect.
    fn pieces_binding(&mut self, node: &Expr, pieces: &[Piece]) {
        let (pre, value) = self.b.capture(|b| pieces_text(b, pieces));
        let body = rt("text", vec![node.clone(), value]);
        let f = if pre.is_empty() {
            js::arrow(vec![], body)
        } else {
            let mut s = pre;
            s.push(Stmt::Expr(body));
            js::arrow_block(vec![], s, false)
        };
        self.b.stmts.push(Stmt::Expr(rt("renderEffect", vec![f])));
    }

    /// Binds the text of `node` to the interpolation text of `e`, with one
    /// render effect.
    fn text_binding(&mut self, node: &Expr, e: &ast::Expr) {
        let (pre, value) = self.b.capture(|b| {
            let t = b.ty(e.syntax());
            let x = b.expr(e);
            b.text_of(x, &t)
        });
        let body = rt("text", vec![node.clone(), value]);
        let f = if pre.is_empty() {
            js::arrow(vec![], body)
        } else {
            let mut s = pre;
            s.push(Stmt::Expr(body));
            js::arrow_block(vec![], s, false)
        };
        self.b.stmts.push(Stmt::Expr(rt("renderEffect", vec![f])));
    }

    /// Emits a render effect whose body is the expression `f` lowers (after
    /// any statements it needs).
    fn effect_of(&mut self, f: impl FnOnce(&mut Body<'_, '_>) -> Expr) {
        let (pre, body) = self.b.capture(f);
        let f = if pre.is_empty() {
            js::arrow(vec![], body)
        } else {
            let mut s = pre;
            s.push(Stmt::Expr(body));
            js::arrow_block(vec![], s, false)
        };
        self.b.stmts.push(Stmt::Expr(rt("renderEffect", vec![f])));
    }

    /// Binds the dynamic parts of element `node`: attributes, classes and
    /// reactive styles each in a render effect, a control's property and its
    /// write-back event, then the event handlers.
    #[allow(clippy::too_many_lines)]
    fn element_bindings(&mut self, node: &Expr, el: &Elem) {
        for (name, v) in &el.attrs {
            match v {
                AttrVal::Static(_) => {}
                AttrVal::Dyn(e, kind) => {
                    let e = e.clone();
                    let kind = *kind;
                    let name = name.clone();
                    let node = node.clone();
                    self.effect_of(move |b| {
                        let x = b.expr(&e);
                        let x = match kind {
                            AttrKind::Aria => call(id("String"), vec![x]),
                            AttrKind::Bool => x,
                            AttrKind::Text => {
                                let t = b.ty(e.syntax());
                                if b.g.is_string(&t) || b.g.is_css_text(&t) || t.decl() == b.g.b.url
                                {
                                    x
                                } else {
                                    b.text_of(x, &t)
                                }
                            }
                        };
                        rt("attr", vec![node, js::str(name), x])
                    });
                }
                AttrVal::Selected(..) => {}
                AttrVal::Gen(e) => {
                    let e = e.clone();
                    let name = name.clone();
                    let node = node.clone();
                    self.effect_of(move |b| {
                        let href = crate::app::href_expr(b, &e);
                        rt("attr", vec![node, js::str(name), href])
                    });
                }
            }
        }
        let dynamic_classes = el
            .classes
            .iter()
            .any(|c| !matches!(c, ClassPart::Static(_)));
        if dynamic_classes {
            let statics: Vec<String> = el
                .classes
                .iter()
                .filter_map(|c| match c {
                    ClassPart::Static(n) => Some(n.clone()),
                    _ => None,
                })
                .collect();
            let classes = el.classes.clone();
            let node = node.clone();
            self.effect_of(move |b| {
                let mut dynamic = Vec::new();
                for c in &classes {
                    match c {
                        ClassPart::Static(_) => {}
                        ClassPart::When(n, cond) => {
                            let x = b.expr(cond);
                            dynamic.push(bin("&&", x, js::str(n.clone())));
                        }
                        ClassPart::Dyn(e) => dynamic.push(b.expr(e)),
                    }
                }
                rt(
                    "classList",
                    vec![node, js::str(statics.join(" ")), Expr::Array(dynamic)],
                )
            });
        }
        for (prop, e) in &el.styles {
            let e = e.clone();
            let prop = prop.clone();
            let node = node.clone();
            self.effect_of(move |b| {
                let t = b.ty(e.syntax());
                let x = b.expr(&e);
                let x = b.text_of(x, &t);
                rt("style", vec![node, js::str(prop), x])
            });
        }
        if let Some(c) = &el.control {
            let path = if c.mode == ControlMode::Place {
                self.b.store_path(&c.place)
            } else {
                None
            };
            if let Some((st, path)) = path {
                // A path-store field: the binding pair of that path.
                let pair = self.b.fresh("b");
                self.b.stmts.push(Stmt::Const(
                    pair.clone(),
                    call(
                        member(id(st), "binding"),
                        vec![Expr::Array(path.into_iter().map(js::str).collect())],
                    ),
                ));
                let n2 = node.clone();
                let prop = c.prop;
                let get = call(member(id(&pair), "get"), vec![]);
                self.b.stmts.push(Stmt::Expr(rt(
                    "renderEffect",
                    vec![js::arrow(vec![], rt("prop", vec![n2, js::str(prop), get]))],
                )));
                let read = member(node.clone(), prop);
                let read = if c.number {
                    call(id("Number"), vec![read])
                } else {
                    read
                };
                let set = js::arrow(vec![], call(member(id(&pair), "set"), vec![read]));
                self.b.stmts.push(Stmt::Expr(rt(
                    "on",
                    vec![node.clone(), js::str(c.event), set],
                )));
                return self.element_events(node, el);
            }
            // A `@Form` field: the form wires the control (value, input,
            // blur) and its error message (15-forms-and-validation.md).
            if c.mode == ControlMode::Place && c.prop == "value" {
                let (pre, probe) = self.b.capture(|b| b.expr(&c.place));
                if let (true, Expr::Call(f, args)) = (pre.is_empty(), &probe) {
                    if let (Expr::Member(field, m), true) = (&**f, args.is_empty()) {
                        if m == "value"
                            && matches!(&**field, Expr::Call(g, _)
                                if matches!(&**g, Expr::Member(_, n) if n == "field"))
                        {
                            self.b.stmts.push(Stmt::Expr(call(
                                member((**field).clone(), "attach"),
                                vec![node.clone()],
                            )));
                            return self.element_events(node, el);
                        }
                    }
                }
            }
            let place = c.place.clone();
            let n2 = node.clone();
            let prop = c.prop;
            let number = c.number;
            let mode = c.mode;
            self.effect_of(move |b| {
                let x = control_value(b, &place, mode);
                let x = if number {
                    call(id("String"), vec![x])
                } else {
                    x
                };
                rt("prop", vec![n2, js::str(prop), x])
            });
            let read = member(node.clone(), prop);
            let read = if number {
                call(id("Number"), vec![read])
            } else {
                read
            };
            if c.mode == ControlMode::OneWay {
                return self.element_events(node, el);
            }
            let (stmts, ()) = if c.mode == ControlMode::Binding {
                let binding = self.b.expr(&c.place);
                (
                    vec![Stmt::Expr(call(member(binding, "set"), vec![read]))],
                    (),
                )
            } else {
                self.b.capture(|b| b.store(&c.place, read))
            };
            let handler = match stmts.as_slice() {
                [Stmt::Expr(e)] => js::arrow(vec![], e.clone()),
                _ => js::arrow_block(vec![], stmts, false),
            };
            self.b.stmts.push(Stmt::Expr(rt(
                "on",
                vec![node.clone(), js::str(c.event), handler],
            )));
        }
        self.element_events(node, el);
    }

    /// Attaches the element's event handlers with `$k.on`.
    fn element_events(&mut self, node: &Expr, el: &Elem) {
        for (event, h) in &el.events {
            let handler = self.handler(h);
            let event = match event.as_str() {
                "mouseenter" | "mouseleave" | "keydown" | "keyup" | "click" | "focus" | "blur"
                | "submit" | "input" | "change" => event.clone(),
                other => other.to_owned(),
            };
            self.b.stmts.push(Stmt::Expr(rt(
                "on",
                vec![node.clone(), js::str(event), handler],
            )));
        }
    }

    /// An event handler: a lambda, a function reference (async ones get
    /// the component's lifetime as `$c`), or an expression giving one.
    fn handler(&mut self, h: &ast::Expr) -> Expr {
        if let ast::Expr::NameExpr(n) = h {
            if let Some(tok) = n.name() {
                if let Some(Resolved::Member(m)) = self.b.g.resolved(self.b.file, tok.text_range())
                {
                    let mi = self.b.g.view.member(m).clone();
                    if mi.kind == k_sema::view::MemberKind::Func {
                        let f = id(mangle(&mi.name));
                        if mi.signature.as_ref().is_some_and(|s| s.is_async) {
                            let lt = self.lifetime_name();
                            return js::arrow(vec![], call(f, vec![id(lt)]));
                        }
                        return f;
                    }
                }
            }
        }
        if let ast::Expr::LambdaExpr(l) = h {
            let awaits = l
                .syntax()
                .descendants()
                .any(|n| n.kind() == SyntaxKind::AwaitExpr);
            if awaits {
                let name = self.lifetime_name();
                let x = self.b.lambda(l, None);
                return js::arrow(vec![], call(x, vec![id(name)]));
            }
            return self.b.sync_lambda(l);
        }
        self.b.expr(h)
    }

    /// The name of the cancellation context async handlers run under: the
    /// component's lifetime, or `$life`.
    fn lifetime_name(&mut self) -> String {
        self.b.uses_life = true;
        if let Some(n) = &self.lifetime {
            return n.clone();
        }
        "$life".to_owned()
    }

    /// Renders a block, child component or slot between two markers.
    #[allow(clippy::too_many_lines)]
    fn block(&mut self, n: &VNode, start: &Expr, end: &Expr) {
        match n {
            VNode::If(node) => {
                let s = self.if_block(node);
                let (cond, then, otherwise) = s;
                let mut args = vec![start.clone(), end.clone(), cond, then];
                args.push(otherwise.unwrap_or(Expr::Null));
                self.b.stmts.push(Stmt::Expr(rt("when", args)));
            }
            VNode::Match(node) => {
                let (select, arms) = self.match_block(node);
                self.b.stmts.push(Stmt::Expr(rt(
                    "branch",
                    vec![start.clone(), end.clone(), select, Expr::Array(arms)],
                )));
            }
            VNode::For(node) => {
                let e = self.for_block(node, start, end);
                self.b.stmts.push(Stmt::Expr(e));
            }
            VNode::Component(c) => {
                let e = self.component_call(c);
                let f = js::arrow(vec![], rt("scope", vec![js::arrow(vec![], e)]));
                self.b
                    .stmts
                    .push(Stmt::Expr(rt("child", vec![start.clone(), end.clone(), f])));
            }
            VNode::Slot(e) => {
                let with_args = matches!(e, ast::Expr::CallExpr(c)
                    if c.arg_list().is_some_and(|l| l.args().next().is_some()));
                let x = self.slot_value(e);
                if with_args {
                    // `row(item)`: re-render when the arguments change, so
                    // an item that changes under its key (a rename)
                    // reaches the slot's view (runtime.md §5.3). The arm
                    // index flips whenever an argument is a new value.
                    let args = match &x {
                        Expr::Call(_, a) => a.clone(),
                        _ => Vec::new(),
                    };
                    let sel = self.b.fresh("sel");
                    let last = self.b.fresh("last");
                    let n = self.b.fresh("n");
                    let now = self.b.fresh("now");
                    let changed = (0..args.len())
                        .map(|i| {
                            bin(
                                "!==",
                                Expr::Index(Box::new(id(&now)), Box::new(Expr::Num(i as f64))),
                                Expr::Index(Box::new(id(&last)), Box::new(Expr::Num(i as f64))),
                            )
                        })
                        .reduce(|a, b| bin("||", a, b))
                        .unwrap_or(Expr::Bool(false));
                    self.b.stmts.push(Stmt::Let(last.clone(), Some(Expr::Null)));
                    self.b
                        .stmts
                        .push(Stmt::Let(n.clone(), Some(Expr::Num(1.0))));
                    self.b.stmts.push(Stmt::Const(
                        sel.clone(),
                        js::arrow_block(
                            vec![],
                            vec![
                                Stmt::Const(now.clone(), Expr::Array(args)),
                                Stmt::If(
                                    bin("||", bin("===", id(&last), Expr::Null), changed),
                                    vec![
                                        Stmt::Expr(js::assign(id(&last), id(&now))),
                                        Stmt::Expr(js::assign(
                                            id(&n),
                                            bin("-", Expr::Num(1.0), id(&n)),
                                        )),
                                    ],
                                    None,
                                ),
                                Stmt::Return(Some(id(&n))),
                            ],
                            false,
                        ),
                    ));
                    let arm = js::arrow(vec![], x);
                    self.b.stmts.push(Stmt::Expr(rt(
                        "branch",
                        vec![
                            start.clone(),
                            end.clone(),
                            id(sel),
                            Expr::Array(vec![arm.clone(), arm]),
                        ],
                    )));
                } else {
                    self.b.stmts.push(Stmt::Expr(rt(
                        "child",
                        vec![start.clone(), end.clone(), js::arrow(vec![], x)],
                    )));
                }
            }
            VNode::Boundary(content, fallback) => {
                // A branch between the content (0) and the fallback (1): a
                // panic while the content renders, or later in one of its
                // effects (`catchError`), shows the fallback.
                let failed = self.b.fresh("failed");
                self.b.stmts.push(Stmt::Const(
                    failed.clone(),
                    rt("signal", vec![Expr::Bool(false)]),
                ));
                let fb = self.b.fresh("fallback");
                let fallback_fn = match fallback {
                    Some(nodes) => self.render_fn(nodes, Vec::new(), Vec::new()),
                    None => Expr::Null,
                };
                self.b.stmts.push(Stmt::Const(fb.clone(), fallback_fn));
                let content_fn = self.render_fn(content, Vec::new(), Vec::new());
                // try { const n = catchError(content, fail); if (!failed.get()) return n; }
                // catch (e) { fail(); } return fallback ? fallback() : null;
                // An effect of the content that panics while it is created
                // shows the fallback at once, too (the arm runs untracked).
                let fail = call(member(id(&failed), "set"), vec![Expr::Bool(true)]);
                let node = self.b.fresh("node");
                let arm = js::arrow_block(
                    vec![],
                    vec![
                        Stmt::Try(
                            vec![
                                Stmt::Const(
                                    node.clone(),
                                    rt(
                                        "catchError",
                                        vec![content_fn, js::arrow(vec![], fail.clone())],
                                    ),
                                ),
                                Stmt::If(
                                    js::un("!", call(member(id(&failed), "get"), vec![])),
                                    vec![Stmt::Return(Some(id(node)))],
                                    None,
                                ),
                            ],
                            "e".into(),
                            vec![Stmt::Expr(fail)],
                        ),
                        Stmt::Return(Some(Expr::Cond(
                            Box::new(id(&fb)),
                            Box::new(call(id(&fb), vec![])),
                            Box::new(Expr::Null),
                        ))),
                    ],
                    false,
                );
                let select = js::arrow(
                    vec![],
                    Expr::Cond(
                        Box::new(call(member(id(&failed), "get"), vec![])),
                        Box::new(Expr::Num(1.0)),
                        Box::new(Expr::Num(0.0)),
                    ),
                );
                self.b.stmts.push(Stmt::Expr(rt(
                    "branch",
                    vec![
                        start.clone(),
                        end.clone(),
                        select,
                        Expr::Array(vec![arm, id(fb)]),
                    ],
                )));
            }
            VNode::Elem(_) | VNode::Text(_) | VNode::Dyn(_) | VNode::Mixed(_) | VNode::Meta(_) => {}
        }
    }

    /// The value of a slot call: `props.content()`, `props.row(item)`.
    fn slot_value(&mut self, e: &ast::Expr) -> Expr {
        match e {
            ast::Expr::NameExpr(n) => {
                let Some(t) = n.name() else { return Expr::Null };
                match self.b.g.resolution(self.b.file, t.text_range()) {
                    Some(Resolution::Local { name, definition }) => {
                        call(id(self.b.local_js(&name, definition.range)), vec![])
                    }
                    _ => call(member(id("props"), t.text().to_owned()), vec![]),
                }
            }
            ast::Expr::CallExpr(c) => {
                let name = match c.callee() {
                    Some(ast::Expr::NameExpr(n)) => {
                        n.name().map(|t| t.text().to_owned()).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                let mut args = Vec::new();
                if let Some(list) = c.arg_list() {
                    for a in list.args() {
                        if let Some(v) = a.value() {
                            args.push(self.b.expr(&v));
                        }
                    }
                }
                call(member(id("props"), name), args)
            }
            other => self.b.expr(other),
        }
    }

    /// The render function of a branch: a fresh statement list returning
    /// its fragment.
    pub(crate) fn render_fn(
        &mut self,
        nodes: &[VNode],
        params: Vec<Param>,
        prefix: Vec<Stmt>,
    ) -> Expr {
        let saved = std::mem::take(&mut self.b.stmts);
        self.b.stmts.extend(prefix);
        let node = self.fragment(nodes);
        let mut body = std::mem::replace(&mut self.b.stmts, saved);
        if body.is_empty() {
            return js::arrow(params, node);
        }
        body.push(Stmt::Return(Some(node)));
        js::arrow_block(params, body, false)
    }

    /// The nodes of a block's statements.
    fn block_nodes(&mut self, block: &SyntaxNode) -> Vec<VNode> {
        let stmts: Vec<SyntaxNode> = block.children().collect();
        let mut t = Tree {
            b: self.b,
            owner: self.owner,
        };
        t.nodes(&stmts)
    }

    /// The arguments of `$k.when` for an `if` block: the condition as a
    /// thunk, the render function of the `then` branch and that of the `else`
    /// branch (an `else if` nests another `when`), if any.
    fn if_block(&mut self, node: &SyntaxNode) -> (Expr, Expr, Option<Expr>) {
        let blocks: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::Block)
            .collect();
        let else_if = node
            .children()
            .skip_while(|c| c.kind() != SyntaxKind::Block)
            .find(|c| c.kind() == SyntaxKind::IfExpr);
        let conds: Vec<SyntaxNode> = node
            .children()
            .take_while(|c| c.kind() != SyntaxKind::Block)
            .filter(|c| c.kind() == SyntaxKind::LetCondition || ast::Expr::can_cast(c.kind()))
            .collect();
        let mut test: Option<Expr> = None;
        for c in &conds {
            let x = if c.kind() == SyntaxKind::LetCondition {
                let name = token(c, SyntaxKind::Ident);
                let value = c.children().find_map(ast::Expr::cast);
                let v = match (&value, &name) {
                    (Some(v), _) => self.b.expr(v),
                    // `if let footer` over a slot or a member.
                    (None, Some(n)) => self.shorthand(n),
                    (None, None) => continue,
                };
                bin("!==", v, Expr::Null)
            } else if let Some(e) = ast::Expr::cast(c.clone()) {
                self.b.expr(&e)
            } else {
                continue;
            };
            test = Some(match test {
                Some(a) => bin("&&", a, x),
                None => x,
            });
        }
        let cond = js::arrow(vec![], test.unwrap_or(Expr::Bool(true)));
        // Bindings of `if let`: the branch renders again only when a value
        // turns nil, so each binding is read through a getter, `x$()`, as a
        // `for` row reads its item: its uses follow the value while the
        // branch shows. The getter keeps the last non-nil value, so an
        // effect that runs before a branch going away is disposed never
        // sees nil.
        let mut prefix = Vec::new();
        let mut bound = Vec::new();
        for c in &conds {
            if c.kind() != SyntaxKind::LetCondition {
                continue;
            }
            let Some(n) = token(c, SyntaxKind::Ident) else {
                continue;
            };
            let value = c.children().find_map(ast::Expr::cast);
            let (mut body, v) = match &value {
                Some(v) => self.b.capture(|b| b.expr(v)),
                None => (Vec::new(), self.shorthand(&n)),
            };
            let js_name = self.b.bind(&n);
            // An optional slot (`if let footer`) is placed, not read: it
            // keeps its plain binding.
            if value.is_none() && self.is_slot(&n) {
                prefix.extend(body);
                prefix.push(Stmt::Const(js_name, v));
                continue;
            }
            let last = format!("{js_name}$last");
            body.push(Stmt::Const("$v".into(), v));
            body.push(Stmt::If(
                bin("!==", id("$v"), Expr::Null),
                vec![Stmt::Expr(js::assign(id(&last), id("$v")))],
                None,
            ));
            body.push(Stmt::Return(Some(id(&last))));
            prefix.push(Stmt::Let(last, Some(Expr::Null)));
            prefix.push(Stmt::Const(
                format!("{js_name}$"),
                js::arrow_block(vec![], body, false),
            ));
            if self.b.signal_locals.insert(js_name.clone()) {
                bound.push(js_name);
            }
        }
        let then_nodes = blocks
            .first()
            .map(|b| self.block_nodes(b))
            .unwrap_or_default();
        let then = self.render_fn(&then_nodes, Vec::new(), prefix);
        for name in bound {
            self.b.signal_locals.remove(&name);
        }
        let otherwise = if let Some(ei) = else_if {
            let nested = VNode::If(ei);
            Some(self.render_fn(&[nested], Vec::new(), Vec::new()))
        } else {
            blocks.get(1).map(|b| {
                let nodes = self.block_nodes(b);
                self.render_fn(&nodes, Vec::new(), Vec::new())
            })
        };
        (cond, then, otherwise)
    }

    /// `true` when a shorthand `if let x` names a `@Slot` of the owner.
    fn is_slot(&self, n: &k_syntax::SyntaxToken) -> bool {
        self.b.g.view.decl(self.owner).members.iter().any(|&m| {
            let mi = self.b.g.view.member(m);
            mi.name == n.text() && mi.has_annotation("Slot")
        })
    }

    /// The value a shorthand `if let x` reads: the member or slot `x`.
    fn shorthand(&mut self, n: &k_syntax::SyntaxToken) -> Expr {
        let owner = self.owner;
        let m = self
            .b
            .g
            .view
            .decl(owner)
            .members
            .iter()
            .copied()
            .find(|&m| self.b.g.view.member(m).name == n.text());
        match m {
            Some(m) => {
                if self.b.g.view.member(m).has_annotation("Slot") {
                    return member(id("props"), n.text().to_owned());
                }
                self.b.implicit_self_member(m)
            }
            None => id(self
                .b
                .local_by_name(n.text())
                .unwrap_or_else(|| mangle(n.text()))),
        }
    }

    /// The arguments of `$k.branch` for a `match` block: a thunk selecting the
    /// index of the arm that matches, and each arm's render function.
    fn match_block(&mut self, node: &SyntaxNode) -> (Expr, Vec<Expr>) {
        let scrutinee = node.children().find_map(ast::Expr::cast);
        let arms: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MatchArm)
            .collect();
        let Some(scrutinee) = scrutinee else {
            return (Expr::Null, Vec::new());
        };
        let sty = self.b.ty(scrutinee.syntax());
        let sv = self.b.expr(&scrutinee);
        // The arm index by case tag (enums), or by an if chain.
        let enum_decl = sty
            .unwrap_optional()
            .decl()
            .filter(|_| self.b.g.kind_of(&sty) == Some(DeclarationKind::Enum));
        let mut index_of_tag: Vec<i64> = Vec::new();
        if let Some(d) = enum_decl {
            index_of_tag = vec![-1; self.b.g.cases(d).len()];
        }
        let mut chain: Vec<(Option<Expr>, usize)> = Vec::new();
        let mut rendered = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let pats: Vec<ast::Pattern> = arm.children().filter_map(ast::Pattern::cast).collect();
            // Bindings: read from the scrutinee when the arm renders.
            let mut prefix = Vec::new();
            let mut tests = Vec::new();
            let mut always = false;
            for p in &pats {
                let tmp = id("$v");
                let (t, binds) = self.b.pattern_test(p, &tmp, &sty);
                for (n, e) in binds {
                    prefix.push(Stmt::Const(n, replace_ident(&e, "$v", &sv)));
                }
                match t {
                    Some(t) => tests.push(replace_ident(&t, "$v", &sv)),
                    None => always = true,
                }
                if let (Some(d), ast::Pattern::EnumCasePattern(ep)) = (enum_decl, p) {
                    let name = ep
                        .syntax()
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .find(|t| t.kind().is_name_like());
                    if let Some(name) = name {
                        if let Some(case) = self.b.g.find_member(d, name.text()) {
                            let tag = self.b.g.case_tag(case);
                            if let Some(slot) = index_of_tag.get_mut(tag) {
                                if *slot < 0 {
                                    *slot = i as i64;
                                }
                            }
                        }
                    }
                }
                if let (Some(_), ast::Pattern::WildcardPattern(_)) = (enum_decl, p) {
                    for slot in &mut index_of_tag {
                        if *slot < 0 {
                            *slot = i as i64;
                        }
                    }
                }
            }
            let mut test = if always {
                None
            } else {
                tests.into_iter().reduce(|a, b| bin("||", a, b))
            };
            // `case .loaded(let places) where places.isEmpty`: the guard
            // reads the arm's bindings straight from the scrutinee.
            if let Some(guard) = arm
                .children()
                .find(|c| c.kind() == SyntaxKind::MatchGuard)
                .and_then(|g| g.children().find_map(ast::Expr::cast))
            {
                let (_, mut g) = self.b.capture(|b| b.expr(&guard));
                for st in &prefix {
                    if let Stmt::Const(n, e) = st {
                        g = replace_ident(&g, n, e);
                    }
                }
                test = Some(match test {
                    Some(t) => bin("&&", t, g),
                    None => g,
                });
            }
            chain.push((test, i));
            let block = arm.children().find(|c| c.kind() == SyntaxKind::Block);
            let nodes = block.map(|b| self.block_nodes(&b)).unwrap_or_default();
            rendered.push(self.render_fn(&nodes, Vec::new(), prefix));
        }
        let select = if enum_decl.is_some()
            && !index_of_tag.contains(&-1)
            && !arms
                .iter()
                .any(|a| a.children().any(|c| c.kind() == SyntaxKind::MatchGuard))
        {
            Expr::Index(
                Box::new(Expr::Array(
                    index_of_tag.iter().map(|&i| Expr::Num(i as f64)).collect(),
                )),
                Box::new(member(sv.clone(), "$")),
            )
        } else {
            let mut e = Expr::Num(-1.0);
            for (t, i) in chain.into_iter().rev() {
                e = match t {
                    Some(t) => Expr::Cond(Box::new(t), Box::new(Expr::Num(i as f64)), Box::new(e)),
                    None => Expr::Num(i as f64),
                };
            }
            e
        };
        (js::arrow(vec![], select), rendered)
    }

    /// The `$k.each` call for a `for … key` block between `start` and `end`:
    /// the items as a thunk, the key function and the row render function,
    /// whose item is read through a signal accessor so a row updates in
    /// place when its item changes under the same key.
    fn for_block(&mut self, node: &SyntaxNode, start: &Expr, end: &Expr) -> Expr {
        let pattern = node.children().find_map(ast::Pattern::cast);
        let iter = node.children().find_map(ast::Expr::cast);
        let key = node
            .children()
            .find(|c| c.kind() == SyntaxKind::KeyClause)
            .and_then(|k| k.children().find_map(ast::Expr::cast));
        let block = node.children().find(|c| c.kind() == SyntaxKind::Block);
        let items = iter.as_ref().map_or(Expr::Array(Vec::new()), |i| {
            let (pre, x) = self.b.capture(|b| b.expr(i));
            let _ = pre;
            x
        });
        let name = match &pattern {
            Some(ast::Pattern::IdentPattern(p)) => p.syntax().first_token(),
            _ => None,
        };
        let var = match &name {
            Some(t) => self.b.bind(t),
            None => self.b.fresh("x"),
        };
        let key_fn = match &key {
            Some(k) => {
                let x = self.b.expr(k);
                js::arrow(vec![Param::new(var.clone())], x)
            }
            None => js::arrow(vec![Param::new(var.clone())], id(&var)),
        };
        // The row reads its item through the item signal (`row$()`) at each
        // use, so its bindings track the item: a new item under the same
        // key re-renders the row in place.
        let item = format!("{var}$");
        let nodes = block.map(|b| self.block_nodes(&b)).unwrap_or_default();
        let fresh = self.b.signal_locals.insert(var.clone());
        let row = self.render_fn(&nodes, vec![Param::new(item)], Vec::new());
        if fresh {
            self.b.signal_locals.remove(&var);
        }
        rt(
            "each",
            vec![
                start.clone(),
                end.clone(),
                js::arrow(vec![], items),
                key_fn,
                row,
            ],
        )
    }

    /// The call of a child component: `Badge({ label: () => "new" }, ctx)`.
    pub(crate) fn component_call(&mut self, c: &Comp) -> Expr {
        let props = props_object_inner(self.b, c, Target::Client);
        let f = self.b.decl_ref(c.decl);
        call(f, vec![props, id("ctx")])
    }
}

/// Replaces the identifier `name` in `e` with `with`.
fn replace_ident(e: &Expr, name: &str, with: &Expr) -> Expr {
    let r = |x: &Expr| replace_ident(x, name, with);
    match e {
        Expr::Ident(n) if n == name => with.clone(),
        Expr::Member(o, n) => member(r(o), n.clone()),
        Expr::OptMember(o, n) => Expr::OptMember(Box::new(r(o)), n.clone()),
        Expr::Index(o, i) => Expr::Index(Box::new(r(o)), Box::new(r(i))),
        Expr::Call(f, a) => call(r(f), a.iter().map(r).collect()),
        Expr::Binary(op, a, b) => bin(op, r(a), r(b)),
        Expr::Unary(op, a) => Expr::Unary(op, Box::new(r(a))),
        other => other.clone(),
    }
}

/// The props object: getters for props and options, binding pairs, event
/// handlers `on$name`, slot functions, stylable class getters.
pub(crate) fn props_object_inner(b: &mut Body<'_, '_>, c: &Comp, target: Target) -> Expr {
    let mut props = Vec::new();
    for (name, v) in &c.args {
        let member = b.g.find_member(c.decl, name);
        let is_binding = member.is_some_and(|m| b.g.view.member(m).has_annotation("Binding"));
        let value = if is_binding || matches!(v, ast::Expr::BindExpr(_)) {
            match v {
                ast::Expr::BindExpr(be) => match be.syntax().children().find_map(ast::Expr::cast) {
                    Some(place) => b.binding_pair(&place),
                    None => Expr::Null,
                },
                other => b.expr(other),
            }
        } else {
            let (pre, x) = b.capture(|b| {
                // A lambda for a prop of function type: the child calls it
                // with its K arguments alone.
                let x = match v {
                    ast::Expr::LambdaExpr(l) if target == Target::Client => b.owned_lambda(l),
                    other => b.expr(other),
                };
                let from = b.ty(v.syntax());
                let to = member
                    .and_then(|m| b.g.view.member(m).ty.clone())
                    .unwrap_or(Type::Error);
                b.coerce(x, &from, &to)
            });
            if pre.is_empty() {
                js::arrow(vec![], x)
            } else {
                let mut s = pre;
                s.push(Stmt::Return(Some(x)));
                js::arrow_block(vec![], s, false)
            }
        };
        props.push(Prop::KeyValue(name.clone(), value));
    }
    if target == Target::Client {
        for (name, h) in &c.events {
            // The child fires the event with its payload alone: a handler
            // that awaits is bound to the listening component.
            let v = match h {
                ast::Expr::LambdaExpr(l)
                    if l.syntax()
                        .descendants()
                        .any(|n| n.kind() == SyntaxKind::AwaitExpr) =>
                {
                    b.owned_lambda(l)
                }
                ast::Expr::LambdaExpr(l) => b.sync_lambda(l),
                other => b.expr(other),
            };
            props.push(Prop::KeyValue(format!("on${name}"), v));
        }
    }
    for (name, l) in &c.slots {
        let v = slot_fn(b, l, target);
        props.push(Prop::KeyValue(name.clone(), v));
    }
    for (name, v) in &c.stylables {
        let mut classes = Vec::new();
        {
            let owner = b_owner(b);
            let mut t = Tree { b, owner };
            t.classes(v, &mut classes);
        }
        let mut parts = Vec::new();
        for cl in classes {
            match cl {
                ClassPart::Static(n) => parts.push(js::str(n)),
                ClassPart::When(n, cond) => {
                    let x = b.expr(&cond);
                    parts.push(Expr::Cond(
                        Box::new(x),
                        Box::new(js::str(n)),
                        Box::new(js::str("")),
                    ));
                }
                ClassPart::Dyn(e) => parts.push(b.expr(&e)),
            }
        }
        let joined = match parts.len() {
            1 => parts.remove(0),
            _ => call(member(Expr::Array(parts), "join"), vec![js::str(" ")]),
        };
        props.push(Prop::KeyValue(name.clone(), js::arrow(vec![], joined)));
    }
    Expr::Object(props, false)
}

/// The owner of `b`, or the first declaration when it has none.
fn b_owner(b: &Body<'_, '_>) -> DeclId {
    b.owner.unwrap_or(DeclId(0))
}

/// A slot's function: the lambda's view rendered by the side's renderer.
fn slot_fn(b: &mut Body<'_, '_>, l: &ast::LambdaExpr, target: Target) -> Expr {
    let owner = b_owner(b);
    let mut params = Vec::new();
    let mut prefix = Vec::new();
    if let Some(list) = l.param_list() {
        for p in list
            .syntax()
            .children()
            .filter(|c| c.kind() == SyntaxKind::LambdaParam)
        {
            if let Some(t) = token(&p, SyntaxKind::Ident) {
                params.push(Param::new(b.bind(&t)));
            }
        }
    }
    let stmts: Vec<SyntaxNode> = l
        .syntax()
        .children()
        .filter(|c| c.kind() != SyntaxKind::LambdaParamList)
        .collect();
    let nodes = {
        let mut t = Tree { b, owner };
        t.nodes(&stmts)
    };
    match target {
        Target::Client => {
            let mut c = Client {
                b,
                owner,
                lifetime: None,
            };
            c.render_fn(&nodes, params, std::mem::take(&mut prefix))
        }
        Target::Server => {
            params.push(Param::new("out"));
            let mut s = Ssr { b, owner };
            let saved = std::mem::take(&mut s.b.stmts);
            s.nodes(&nodes);
            let body = std::mem::replace(&mut s.b.stmts, saved);
            js::arrow_block(params, body, false)
        }
    }
}

// ---------------------------------------------------------------- server

/// Renders view nodes as SSR string building.
pub(crate) struct Ssr<'b, 'g, 'a> {
    /// The body the view belongs to.
    pub b: &'b mut Body<'g, 'a>,
    /// The component whose view is rendered.
    pub owner: DeclId,
}

impl Ssr<'_, '_, '_> {
    /// Emits `out.raw(html)` for markup escaped at build time, merging it
    /// into the preceding `out.raw` when there is one.
    fn raw(&mut self, html: &str) {
        if html.is_empty() {
            return;
        }
        // Merge with a preceding raw chunk.
        if let Some(Stmt::Expr(Expr::Call(f, args))) = self.b.stmts.last_mut() {
            if matches!(&**f, Expr::Member(o, n) if matches!(&**o, Expr::Ident(x) if x == "out") && n == "raw")
            {
                if let Some(Expr::Html(prev)) = args.first_mut() {
                    prev.push_str(html);
                    return;
                }
            }
        }
        self.b.stmts.push(Stmt::Expr(call(
            member(id("out"), "raw"),
            vec![Expr::Html(html.to_owned())],
        )));
    }

    /// Emits `out.raw($s.WHICH)`: a hydration marker of the server runtime
    /// (`OPEN`, `CLOSE`, `SEPARATOR`).
    fn marker(&mut self, which: &str) {
        self.b.stmts.push(Stmt::Expr(call(
            member(id("out"), "raw"),
            vec![member(id("$s"), which.to_owned())],
        )));
    }

    /// Emits the nodes.
    pub(crate) fn nodes(&mut self, nodes: &[VNode]) {
        let mut prev_dyn = false;
        for n in nodes {
            let is_dyn = is_dyn_text(n);
            if is_dyn && prev_dyn {
                self.marker("SEPARATOR");
            }
            self.node(n);
            prev_dyn = is_dyn;
        }
    }

    /// Emits one node: static text, dynamic text through `out.text`, an
    /// element, a block between markers, a child component's renderer or a
    /// slot.
    #[allow(clippy::too_many_lines)]
    fn node(&mut self, n: &VNode) {
        match n {
            VNode::Text(t) => self.raw(&escape_text(t)),
            VNode::Dyn(e) => {
                let (pre, x) = self.b.capture(|b| {
                    let t = b.ty(e.syntax());
                    let x = b.expr(e);
                    b.text_of(x, &t)
                });
                self.b.stmts.extend(pre);
                self.b
                    .stmts
                    .push(Stmt::Expr(call(member(id("out"), "text"), vec![x])));
            }
            VNode::Mixed(pieces) => {
                let (pre, x) = self.b.capture(|b| pieces_text(b, pieces));
                self.b.stmts.extend(pre);
                self.b
                    .stmts
                    .push(Stmt::Expr(call(member(id("out"), "text"), vec![x])));
            }
            VNode::Elem(el) => self.element(el),
            VNode::Meta(calls) => {
                let stmts = crate::server::meta_calls(self.b, calls, &member(id("ctx"), "meta"));
                self.b.stmts.extend(stmts);
            }
            VNode::If(node) => {
                self.marker("OPEN");
                let s = self.if_stmt(node);
                self.b.stmts.push(s);
                self.marker("CLOSE");
            }
            VNode::Match(node) => {
                self.marker("OPEN");
                self.match_stmt(node);
                self.marker("CLOSE");
            }
            VNode::For(node) => {
                self.marker("OPEN");
                self.for_stmt(node);
                self.marker("CLOSE");
            }
            VNode::Boundary(content, fallback) => {
                // The content, or after a panic the fallback in its place:
                // what the content wrote so far is taken back.
                self.marker("OPEN");
                let n = self.b.fresh("n");
                self.b.stmts.push(Stmt::Const(
                    n.clone(),
                    member(member(id("out"), "chunks"), "length"),
                ));
                let saved = std::mem::take(&mut self.b.stmts);
                self.nodes(content);
                let tried = std::mem::take(&mut self.b.stmts);
                self.b.stmts.push(Stmt::Expr(js::assign(
                    member(member(id("out"), "chunks"), "length"),
                    id(&n),
                )));
                if let Some(f) = fallback {
                    self.nodes(f);
                }
                let caught = std::mem::replace(&mut self.b.stmts, saved);
                self.b.stmts.push(Stmt::Try(tried, "e".into(), caught));
                self.marker("CLOSE");
            }
            VNode::Component(c) => {
                self.marker("OPEN");
                let props = props_object_inner(self.b, c, Target::Server);
                let name = format!("{}$ssr", self.b.g.name(c.decl));
                let f = self.b.use_decl(c.decl, name);
                let async_child = crate::ui::has_loads(self.b.g, c.decl);
                let e = call(f, vec![props, id("ctx"), id("out")]);
                self.b.stmts.push(Stmt::Expr(if async_child {
                    Expr::Await(Box::new(e))
                } else {
                    e
                }));
                self.marker("CLOSE");
            }
            VNode::Slot(e) => {
                self.marker("OPEN");
                let x = match e {
                    ast::Expr::NameExpr(nm) => {
                        let t = nm.name().map(|t| t.text().to_owned()).unwrap_or_default();
                        let callee = match self.b.local_by_name(&t) {
                            Some(l) => id(l),
                            None => member(id("props"), t),
                        };
                        call(callee, vec![id("out")])
                    }
                    ast::Expr::CallExpr(c) => {
                        let name = match c.callee() {
                            Some(ast::Expr::NameExpr(n)) => {
                                n.name().map(|t| t.text().to_owned()).unwrap_or_default()
                            }
                            _ => String::new(),
                        };
                        let mut args = Vec::new();
                        if let Some(list) = c.arg_list() {
                            for a in list.args() {
                                if let Some(v) = a.value() {
                                    args.push(self.b.expr(&v));
                                }
                            }
                        }
                        args.push(id("out"));
                        call(member(id("props"), name), args)
                    }
                    other => self.b.expr(other),
                };
                self.b.stmts.push(Stmt::Expr(x));
                self.marker("CLOSE");
            }
        }
    }

    /// Emits an element: its label (with a per-request id), its start tag,
    /// its children or its control's value, and its end tag.
    #[allow(clippy::too_many_lines)]
    fn element(&mut self, el: &Elem) {
        if let Some(text) = &el.label {
            // Per-request ids (`Out.uid`), never an isolate-global counter.
            let uid = self.b.fresh("id");
            self.b.stmts.push(Stmt::Const(
                uid.clone(),
                call(member(id("out"), "uid"), vec![]),
            ));
            self.raw("<label");
            self.b.stmts.push(Stmt::Expr(call(
                member(id("out"), "attribute"),
                vec![js::str("for"), id(&uid)],
            )));
            self.raw(&format!(">{}</label>", escape_text(text)));
            self.open(el, Some(uid));
        } else {
            self.open(el, None);
        }
        let mut prev_dyn = false;
        for c in &el.children {
            let is_dyn = is_dyn_text(c);
            if is_dyn && prev_dyn {
                self.raw("<!---->");
            }
            // The option matching a dropdown's value is `selected`.
            if let (VNode::Elem(opt), Some(ctl)) = (c, &el.control) {
                if el.tag == "select" && opt.tag == "option" {
                    let value = opt.attrs.iter().find_map(|(n, v)| match v {
                        AttrVal::Static(Some(t)) if n == "value" => Some(t.clone()),
                        _ => None,
                    });
                    if let Some(v) = value {
                        let mut o = (**opt).clone();
                        o.attrs
                            .push(("selected".into(), AttrVal::Selected(ctl.place.clone(), v)));
                        self.element(&o);
                        prev_dyn = false;
                        continue;
                    }
                }
            }
            self.node(c);
            prev_dyn = is_dyn;
        }
        // A textarea's value is its content.
        if el.tag == "textarea" {
            if let Some(c) = &el.control {
                let x = control_value(self.b, &c.place, c.mode);
                self.b
                    .stmts
                    .push(Stmt::Expr(call(member(id("out"), "text"), vec![x])));
            }
        }
        self.raw(&end_tag(el));
    }

    /// Emits the start tag of `el`: the static part as raw markup, and each
    /// dynamic attribute, class list and style through `out.attribute`.
    fn open(&mut self, el: &Elem, id_attr: Option<String>) {
        let tag = start_tag(el);
        let dynamic_classes = el
            .classes
            .iter()
            .any(|c| !matches!(c, ClassPart::Static(_)));
        let has_dynamic = dynamic_classes
            || el
                .attrs
                .iter()
                .any(|(_, v)| !matches!(v, AttrVal::Static(_)))
            || !el.styles.is_empty()
            || el
                .control
                .as_ref()
                .is_some_and(|c| el.tag != "textarea" && c.prop == "value" && el.tag != "select")
            || el.control.as_ref().is_some_and(|c| c.prop == "checked")
            || id_attr.is_some();
        if !has_dynamic {
            self.raw(&tag);
            return;
        }
        // The static part without the closing `>`.
        self.raw(&tag[..tag.len() - 1]);
        if let Some(uid) = id_attr {
            self.b.stmts.push(Stmt::Expr(call(
                member(id("out"), "attribute"),
                vec![js::str("id"), id(uid)],
            )));
        }
        if dynamic_classes {
            let mut parts = Vec::new();
            for c in &el.classes {
                match c {
                    ClassPart::Static(n) => parts.push(js::str(n.clone())),
                    ClassPart::When(n, cond) => {
                        let x = self.b.expr(cond);
                        parts.push(Expr::Cond(
                            Box::new(x),
                            Box::new(js::str(n.clone())),
                            Box::new(js::str("")),
                        ));
                    }
                    ClassPart::Dyn(e) => parts.push(self.b.expr(e)),
                }
            }
            let joined = rt("joinClasses", parts);
            self.b.stmts.push(Stmt::Expr(call(
                member(id("out"), "attribute"),
                vec![js::str("class"), joined],
            )));
        }
        for (name, v) in &el.attrs {
            let x = match v {
                AttrVal::Static(_) => continue,
                AttrVal::Dyn(e, kind) => {
                    let x = self.b.expr(e);
                    match kind {
                        AttrKind::Aria => call(id("String"), vec![x]),
                        AttrKind::Bool => x,
                        AttrKind::Text => {
                            let t = self.b.ty(e.syntax());
                            if self.b.g.is_string(&t)
                                || self.b.g.is_css_text(&t)
                                || t.decl() == self.b.g.b.url
                            {
                                x
                            } else {
                                self.b.text_of(x, &t)
                            }
                        }
                    }
                }
                AttrVal::Gen(e) => crate::app::href_expr(self.b, e),
                AttrVal::Selected(place, v) => {
                    let x = self.b.expr(place);
                    bin("===", x, js::str(v.clone()))
                }
            };
            self.b.stmts.push(Stmt::Expr(call(
                member(id("out"), "attribute"),
                vec![js::str(name.clone()), x],
            )));
        }
        if !el.styles.is_empty() {
            let mut parts = Vec::new();
            for (i, (prop, e)) in el.styles.iter().enumerate() {
                let t = self.b.ty(e.syntax());
                let x = self.b.expr(e);
                let x = self.b.text_of(x, &t);
                let sep = if i == 0 { "" } else { "; " };
                parts.push(TemplatePart::Text(format!("{sep}{prop}: ")));
                parts.push(TemplatePart::Expr(x));
            }
            self.b.stmts.push(Stmt::Expr(call(
                member(id("out"), "attribute"),
                vec![js::str("style"), Expr::Template(parts)],
            )));
        }
        if let Some(c) = &el.control {
            if el.tag != "textarea" && el.tag != "select" {
                let x = control_value(self.b, &c.place, c.mode);
                let (name, x) = if c.prop == "checked" {
                    ("checked", x)
                } else if c.number {
                    ("value", call(id("String"), vec![x]))
                } else {
                    ("value", x)
                };
                self.b.stmts.push(Stmt::Expr(call(
                    member(id("out"), "attribute"),
                    vec![js::str(name), x],
                )));
            }
        }
        self.raw(">");
    }

    /// The nodes of a block's statements.
    fn block_nodes(&mut self, block: &SyntaxNode) -> Vec<VNode> {
        let stmts: Vec<SyntaxNode> = block.children().collect();
        let mut t = Tree {
            b: self.b,
            owner: self.owner,
        };
        t.nodes(&stmts)
    }

    /// Runs `f` with a fresh statement buffer and returns what it emitted.
    fn sub(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Stmt> {
        let saved = std::mem::take(&mut self.b.stmts);
        f(self);
        std::mem::replace(&mut self.b.stmts, saved)
    }

    /// The `if` statement that renders an `if` block's branches, with its
    /// `let` bindings.
    fn if_stmt(&mut self, node: &SyntaxNode) -> Stmt {
        let blocks: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::Block)
            .collect();
        let else_if = node
            .children()
            .skip_while(|c| c.kind() != SyntaxKind::Block)
            .find(|c| c.kind() == SyntaxKind::IfExpr);
        let conds: Vec<SyntaxNode> = node
            .children()
            .take_while(|c| c.kind() != SyntaxKind::Block)
            .filter(|c| c.kind() == SyntaxKind::LetCondition || ast::Expr::can_cast(c.kind()))
            .collect();
        let mut test: Option<Expr> = None;
        let mut prefix = Vec::new();
        for c in &conds {
            let x = if c.kind() == SyntaxKind::LetCondition {
                let name = token(c, SyntaxKind::Ident);
                let value = c.children().find_map(ast::Expr::cast);
                let v = match (&value, &name) {
                    (Some(v), _) => self.b.expr(v),
                    (None, Some(n)) => {
                        let m = self
                            .b
                            .g
                            .view
                            .decl(self.owner)
                            .members
                            .iter()
                            .copied()
                            .find(|&m| self.b.g.view.member(m).name == n.text());
                        match m {
                            Some(m) if self.b.g.view.member(m).has_annotation("Slot") => {
                                member(id("props"), n.text().to_owned())
                            }
                            Some(m) => self.b.implicit_self_member(m),
                            None => id(self
                                .b
                                .local_by_name(n.text())
                                .unwrap_or_else(|| mangle(n.text()))),
                        }
                    }
                    (None, None) => continue,
                };
                if let Some(n) = name {
                    let js_name = self.b.bind(&n);
                    prefix.push(Stmt::Const(js_name.clone(), v));
                    bin("!==", id(js_name), Expr::Null)
                } else {
                    bin("!==", v, Expr::Null)
                }
            } else if let Some(e) = ast::Expr::cast(c.clone()) {
                self.b.expr(&e)
            } else {
                continue;
            };
            test = Some(match test {
                Some(a) => bin("&&", a, x),
                None => x,
            });
        }
        let then_nodes = blocks
            .first()
            .map(|b| self.block_nodes(b))
            .unwrap_or_default();
        let then = self.sub(|s| s.nodes(&then_nodes));
        let otherwise = if let Some(ei) = else_if {
            Some(self.sub(|s| {
                let st = s.if_stmt(&ei);
                s.b.stmts.push(st);
            }))
        } else {
            blocks.get(1).map(|b| {
                let nodes = self.block_nodes(b);
                self.sub(|s| s.nodes(&nodes))
            })
        };
        let s = Stmt::If(test.unwrap_or(Expr::Bool(true)), then, otherwise);
        if prefix.is_empty() {
            s
        } else {
            prefix.push(s);
            Stmt::Block(prefix)
        }
    }

    /// Emits the `if` chain that renders the arm of a `match` block that
    /// matches.
    fn match_stmt(&mut self, node: &SyntaxNode) {
        let scrutinee = node.children().find_map(ast::Expr::cast);
        let arms: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MatchArm)
            .collect();
        let Some(scrutinee) = scrutinee else { return };
        let sty = self.b.ty(scrutinee.syntax());
        let mut sv = self.b.expr(&scrutinee);
        if !matches!(sv, Expr::Ident(_)) {
            let t = self.b.fresh("m");
            self.b.stmts.push(Stmt::Const(t.clone(), sv));
            sv = id(t);
        }
        let is_enum = self.b.g.kind_of(&sty) == Some(DeclarationKind::Enum)
            && !matches!(sty, Type::Optional(_));
        if is_enum {
            let mut cases = Vec::new();
            for arm in &arms {
                let pats: Vec<ast::Pattern> =
                    arm.children().filter_map(ast::Pattern::cast).collect();
                let mut tests = Vec::new();
                let mut binds = Vec::new();
                let mut wildcard = false;
                for p in &pats {
                    match p {
                        ast::Pattern::WildcardPattern(_) => wildcard = true,
                        ast::Pattern::EnumCasePattern(ep) => {
                            let (_, bs) = self.b.pattern_test(p, &sv, &sty);
                            binds.extend(bs);
                            let name = ep
                                .syntax()
                                .children_with_tokens()
                                .filter_map(rowan::NodeOrToken::into_token)
                                .find(|t| t.kind().is_name_like());
                            if let (Some(name), Some(d)) = (name, sty.decl()) {
                                if let Some(case) = self.b.g.find_member(d, name.text()) {
                                    tests.push(Expr::Num(self.b.g.case_tag(case) as f64));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let block = arm.children().find(|c| c.kind() == SyntaxKind::Block);
                let nodes = block.map(|b| self.block_nodes(&b)).unwrap_or_default();
                let mut body: Vec<Stmt> =
                    binds.into_iter().map(|(n, e)| Stmt::Const(n, e)).collect();
                body.extend(self.sub(|s| s.nodes(&nodes)));
                body.push(Stmt::Break(None));
                cases.push(js::Case {
                    tests: if wildcard { Vec::new() } else { tests },
                    body,
                });
            }
            self.b.stmts.push(Stmt::Switch(member(sv, "$"), cases));
            return;
        }
        let mut otherwise: Option<Vec<Stmt>> = None;
        let mut built = Vec::new();
        for arm in &arms {
            let pats: Vec<ast::Pattern> = arm.children().filter_map(ast::Pattern::cast).collect();
            let mut tests = Vec::new();
            let mut binds = Vec::new();
            let mut always = false;
            for p in &pats {
                let (t, bs) = self.b.pattern_test(p, &sv, &sty);
                binds.extend(bs);
                match t {
                    Some(t) => tests.push(t),
                    None => always = true,
                }
            }
            let block = arm.children().find(|c| c.kind() == SyntaxKind::Block);
            let nodes = block.map(|b| self.block_nodes(&b)).unwrap_or_default();
            let mut body: Vec<Stmt> = binds.into_iter().map(|(n, e)| Stmt::Const(n, e)).collect();
            body.extend(self.sub(|s| s.nodes(&nodes)));
            built.push((
                if always {
                    None
                } else {
                    tests.into_iter().reduce(|a, b| bin("||", a, b))
                },
                body,
            ));
        }
        for (t, body) in built.into_iter().rev() {
            otherwise = Some(match t {
                Some(t) => vec![Stmt::If(t, body, otherwise)],
                None => body,
            });
        }
        if let Some(s) = otherwise {
            self.b.stmts.extend(s);
        }
    }

    /// Emits a `for … of` loop that renders a `for` block's rows.
    fn for_stmt(&mut self, node: &SyntaxNode) {
        let pattern = node.children().find_map(ast::Pattern::cast);
        let iter = node.children().find_map(ast::Expr::cast);
        let block = node.children().find(|c| c.kind() == SyntaxKind::Block);
        let items = iter
            .as_ref()
            .map_or(Expr::Array(Vec::new()), |i| self.b.expr(i));
        let var = match &pattern {
            Some(ast::Pattern::IdentPattern(p)) => match p.syntax().first_token() {
                Some(t) => self.b.bind(&t),
                None => self.b.fresh("x"),
            },
            _ => self.b.fresh("x"),
        };
        let nodes = block.map(|b| self.block_nodes(&b)).unwrap_or_default();
        let body = self.sub(|s| s.nodes(&nodes));
        self.b.stmts.push(Stmt::ForOf(var, items, body, false));
    }
}

/// Builds the template HTML of an element and records its dynamic parts
/// with their paths.
fn build(
    el: &Elem,
    html: &mut String,
    path: &mut Vec<Step>,
    parts: &mut Vec<(Vec<Step>, Part)>,
    root: bool,
) {
    let _ = root;
    html.push_str(&start_tag(el));
    let needs_bindings = el
        .attrs
        .iter()
        .any(|(_, v)| !matches!(v, AttrVal::Static(_)))
        || el
            .classes
            .iter()
            .any(|c| !matches!(c, ClassPart::Static(_)))
        || !el.styles.is_empty()
        || !el.events.is_empty()
        || el.control.is_some()
        || (!root && el.label.is_some());
    if needs_bindings {
        let mut bare = el.clone();
        bare.children = Vec::new();
        parts.push((path.clone(), Part::Elem(Box::new(bare))));
    }
    let mut first = true;
    let mut prev_dyn = false;
    for c in &el.children {
        let is_dyn = is_dyn_text(c);
        if is_dyn && prev_dyn {
            // `<!---->` keeps two dynamic texts apart.
            step(path, &mut first);
            html.push_str("<!---->");
        }
        step(path, &mut first);
        match c {
            VNode::Elem(child) => {
                if let Some(text) = &child.label {
                    // A nested labeled control: its `<label>` comes first,
                    // as the server renders it.
                    html.push_str(&format!("<label>{}</label>", escape_text(text)));
                    parts.push((path.clone(), Part::Label));
                    path.push(Step::Next);
                }
                build(child, html, path, parts, false);
            }
            VNode::Text(t) => html.push_str(&escape_text(t)),
            VNode::Dyn(e) => {
                html.push(' ');
                parts.push((path.clone(), Part::Text(e.clone())));
            }
            VNode::Mixed(p) => {
                html.push(' ');
                parts.push((path.clone(), Part::Mixed(p.clone())));
            }
            other => {
                html.push_str("<!--[--><!--]-->");
                parts.push((path.clone(), Part::Block(other.clone())));
                // The end marker is a sibling too.
                path.push(Step::Next);
            }
        }
        prev_dyn = is_dyn;
    }
    if !first {
        // Leave the children level.
        while matches!(path.last(), Some(Step::Next)) {
            path.pop();
        }
        path.pop();
    }
    html.push_str(&end_tag(el));
}

/// Advances `path` to the next child: `firstChild` for the first, then
/// `nextSibling`.
fn step(path: &mut Vec<Step>, first: &mut bool) {
    if *first {
        path.push(Step::First);
        *first = false;
    } else {
        path.push(Step::Next);
    }
}

/// `true` if a view statement list renders anything.
#[allow(dead_code)]
pub(crate) fn has_token_kind(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    has_token(node, kind)
}
