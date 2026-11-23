//! `preview` declarations (`docs/design/57-previews.md`;
//! `docs/compiler/sema.md` §12.14, "Previews").
//!
//! A preview renders a view in a chosen state, next to the code that
//! defines it. Its body is test code inside a production file: the
//! program puts a production file's previews in the file's *preview scope*
//! (`program::FileInfo::scope_of`), a test file of the same module that
//! shares the file's tree and imports, so its names resolve as in a
//! `.test.k` file and nothing else in the file sees them.
//!
//! The checks:
//!
//! * a parameter has a default (**K7504**) and a type the panel has a
//!   control for (**K7505**, [`Control`]);
//! * the body is setup lines (`use` rules, `@Inject` fields, statements
//!   with `await` and `try`) and then the view: an empty body or one that
//!   ends in another statement is **K7509**, a last expression that isn't a
//!   view **K7508**;
//! * a file has at most one unnamed preview (**K7506**) and no two
//!   previews of the same name (**K7507**);
//! * a production file may import a `testing` module only for its
//!   previews: each use outside them is K1104 (`check::preview_imports`);
//! * without `[package] preview = ["view-previews"]`, each preview is
//!   K8108 at its keyword (`crate::stability`).
//!
//! [`previews`] enumerates a file's previews for the tools, with what the
//! panel needs: the label, the keyword's range and each parameter's
//! control and default. Code generation builds the runnable entry
//! (`k_codegen::Entry::Previews`).

use k_diagnostics::{Diagnostic, FileId, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::body::{BodyOwner, Checker};
use crate::cx::{Cx, SigKey};
use crate::lookup::Resolution;
use crate::program::{DeclId, MemberKind};
use crate::stdlib::DeclarationKind;
use crate::ty::Ty;
use crate::view::{self, ProgramView, Resolved, Type};

// ------------------------------------------------------------------ checks

/// Checks one preview declaration's parameters and body.
pub(crate) fn check(cx: &Cx<'_, '_>, decl: DeclId, owner: BodyOwner) {
    let p = cx.p;
    let d = p.decl(decl);
    let file = d.file;
    let Some(preview) = ast::PreviewDecl::cast(d.node.clone()) else {
        return;
    };
    let mut sig = (*cx.sig(SigKey::Decl(decl))).clone();
    sig.is_async = true;
    if let Some(list) = preview.param_list() {
        for (param, sp) in list.params().zip(sig.params.iter()) {
            if param.default_value().is_none() {
                cx.error(
                    file,
                    codes::PREVIEW_PARAM_WITHOUT_DEFAULT,
                    sp.name_range,
                    format!(
                        "`{}` needs a default value: a preview renders before anyone touches its controls",
                        sp.name
                    ),
                );
            }
            if !has_control(cx, &sp.ty) {
                let range = param
                    .ty()
                    .map_or(sp.name_range, |t| t.syntax().text_range());
                cx.report(
                    file,
                    Diagnostic::new(
                        codes::PREVIEW_PARAM_TYPE,
                        p.span(file, range),
                        format!(
                            "the preview panel has no control for `{}`",
                            sp.ty.render(p)
                        ),
                    )
                    .with_help(
                        "a preview parameter is a `Bool`, an `Int`, a `Double`, a `String`, an enum whose cases have no payload, or a type with a literal syntax (`Money`, `Duration`, `Date`, `Url`, …)",
                    ),
                );
            }
        }
    }
    let Some(body) = preview.body() else {
        return;
    };
    let mut checker = Checker::new(cx, owner);
    let last = checker.check_preview(&d.node, &sig, &body);
    checker.finish();
    match last {
        None => cx.error(
            file,
            codes::PREVIEW_WITHOUT_VIEW,
            d.name_range,
            "this preview renders nothing: its body ends with the view it shows",
        ),
        Some((ty, range)) => {
            let shows_view = cx.b.view.is_some_and(|v| cx.implements(&ty, v));
            if !shows_view {
                cx.error(
                    file,
                    codes::PREVIEW_NOT_A_VIEW,
                    range,
                    format!(
                        "a preview ends with the view it renders, not a value of type `{}`",
                        ty.render(p)
                    ),
                );
            }
        }
    }
}

/// K7506 and K7507 over the previews among `decls` (one file's).
pub(crate) fn check_names(cx: &Cx<'_, '_>, decls: &[DeclId]) {
    let p = cx.p;
    let mut unnamed: Option<DeclId> = None;
    let mut named: Vec<(String, TextRange, DeclId)> = Vec::new();
    for &d in decls {
        let decl = p.decl(d);
        if decl.named_kind() != Some(DeclarationKind::Preview) {
            continue;
        }
        let Some(preview) = ast::PreviewDecl::cast(decl.node.clone()) else {
            continue;
        };
        match preview.name() {
            None => match unnamed {
                None => unnamed = Some(d),
                Some(first) => cx.report(
                    decl.file,
                    Diagnostic::new(
                        codes::SECOND_UNNAMED_PREVIEW,
                        p.span(decl.file, decl.name_range),
                        "a file has at most one unnamed preview: name this one, as in `preview \"empty\" { … }`",
                    )
                    .with_label(
                        p.span(p.decl(first).file, p.decl(first).name_range),
                        "the file's unnamed preview",
                    ),
                ),
            },
            Some(name) => {
                let Some(text) = name.plain_value() else {
                    continue;
                };
                let range = name.syntax().text_range();
                if let Some((_, first, _)) = named.iter().find(|(n, _, _)| *n == text) {
                    cx.report(
                        decl.file,
                        Diagnostic::new(
                            codes::DUPLICATE_PREVIEW_NAME,
                            p.span(decl.file, range),
                            format!("this file already has a preview named \"{text}\""),
                        )
                        .with_label(p.span(decl.file, *first), "first named here"),
                    );
                } else {
                    named.push((text, range, d));
                }
            }
        }
    }
}

/// `true` when the preview panel has a control for a parameter of type
/// `ty` ([`Control`]). An erroneous type has one, so it isn't reported
/// twice.
fn has_control(cx: &Cx<'_, '_>, ty: &Ty) -> bool {
    let b = &cx.b;
    let d = match ty {
        Ty::Error => return true,
        Ty::Named(d, args) if args.is_empty() => *d,
        _ => return false,
    };
    if [b.bool_, b.int, b.double, b.string, b.money, b.duration].contains(&Some(d)) {
        return true;
    }
    let p = cx.p;
    if cx.is_enum(d) {
        let cases = cx.cases(d);
        return !cases.is_empty() && cases.iter().all(|&c| !cx.case_has_payload(c));
    }
    p.decl(d).members.iter().any(|&m| {
        let member = p.member(m);
        member.kind == MemberKind::Func && member.is_static && member.name == "parse" && {
            let sig = cx.sig(SigKey::Member(m));
            sig.params.len() == 1
                && sig.params[0].label.is_none()
                && sig.params[0].ty.decl() == b.string
        }
    })
}

// ------------------------------------------------------------------ the API

/// One `preview` declaration of a file, as the tools see it.
#[derive(Clone, PartialEq, Debug)]
pub struct Preview {
    /// The declaration (its `named_kind` is [`DeclarationKind::Preview`]).
    pub decl: view::DeclId,
    /// The file that declares it.
    pub file: FileId,
    /// The written name (`preview "sold out"`), if any.
    pub name: Option<String>,
    /// What the switcher shows: the name, or for an unnamed preview the
    /// name of the declaration whose view it renders (`Stepper`), or
    /// `"Preview"` when that isn't known.
    pub label: String,
    /// The whole declaration.
    pub range: TextRange,
    /// The `preview` keyword: where `k check` reports a failure (K7520) and
    /// where an editor puts the preview's CodeLens.
    pub keyword: TextRange,
    /// The declaration the preview renders (a component, layout or page),
    /// when its last line constructs one.
    pub view: Option<view::DeclId>,
    /// The parameters, in order.
    pub params: Vec<PreviewParam>,
}

/// One parameter of a [`Preview`]: a control of the panel.
#[derive(Clone, PartialEq, Debug)]
pub struct PreviewParam {
    /// The name, which is also the key of its value in the preview entry
    /// (`k_codegen`'s `$mount`) and what a rebuild keeps control values by.
    pub name: String,
    /// The name's range.
    pub name_range: TextRange,
    /// The type as written in diagnostics (`Int`, `Size`).
    pub type_name: String,
    /// The control the panel shows.
    pub control: Control,
    /// The default value's source text; empty without one (K7504).
    pub default_source: String,
    /// The default value, when it is a literal the panel can show without
    /// running the preview.
    pub default: Option<ControlValue>,
}

/// The control of a preview parameter, chosen by its type
/// (docs/design/57-previews.md, "The panel").
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Control {
    /// `Bool`: a toggle.
    Toggle,
    /// `Int`: a number field with steppers.
    Integer,
    /// `Double`: a number field with steppers.
    Number,
    /// `String`: a text field.
    Text,
    /// An enum without payloads: a dropdown of its cases, in order.
    Choice(Vec<String>),
    /// A type with a literal syntax (`Money`, `Duration`, a type with
    /// `static func parse(_ text: String) -> T?`): a text field parsed by
    /// the type's parser; the qualified type name.
    Parsed(String),
    /// A type the panel has no control for (K7505 in a checked program).
    Unsupported,
}

/// A literal default of a preview parameter.
#[derive(Clone, PartialEq, Debug)]
pub enum ControlValue {
    /// `true` or `false`.
    Bool(bool),
    /// An integer literal.
    Int(i64),
    /// A floating-point literal.
    Double(f64),
    /// A plain string literal.
    String(String),
    /// An enum case (`.large`), by name.
    Case(String),
}

/// The previews of `file`, in source order, from the program view, the
/// file's syntax tree and the resolution at a range.
pub(crate) fn previews(
    view: &ProgramView,
    file: FileId,
    root: &SyntaxNode,
    resolve: &dyn Fn(TextRange) -> Option<Resolution>,
) -> Vec<Preview> {
    let mut out = Vec::new();
    let decls = view
        .decls
        .iter()
        .enumerate()
        .filter(|(_, d)| d.file == file && d.named_kind() == Some(DeclarationKind::Preview));
    for (i, info) in decls {
        let id = view::DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let Some(node) = root
            .descendants()
            .filter_map(ast::PreviewDecl::cast)
            .find(|n| n.syntax().text_range() == info.range)
        else {
            continue;
        };
        let name = node.name().and_then(|s| s.plain_value());
        let shown = rendered(&node).and_then(|tok| match resolve(tok) {
            Some(r) => match view.resolve(&r) {
                Some(Resolved::Decl(d)) => Some(d),
                _ => None,
            },
            None => None,
        });
        let label = name.clone().unwrap_or_else(|| {
            shown.map_or_else(|| "Preview".to_owned(), |d| view.decl(d).name.clone())
        });
        let sig_params = info
            .signature
            .as_ref()
            .map(|s| s.params.clone())
            .unwrap_or_default();
        let mut params = Vec::new();
        let nodes: Vec<ast::Param> = node
            .param_list()
            .map(|l| l.params().collect())
            .unwrap_or_default();
        for (param, sp) in nodes.iter().zip(sig_params) {
            let default = param.default_value();
            let control = control(view, &sp.ty);
            params.push(PreviewParam {
                name: sp.name.clone(),
                name_range: sp.name_range,
                type_name: type_name(view, &sp.ty),
                default_source: default
                    .as_ref()
                    .map(|e| e.syntax().text().to_string())
                    .unwrap_or_default(),
                default: default.as_ref().and_then(literal),
                control,
            });
        }
        out.push(Preview {
            decl: id,
            file,
            name,
            label,
            range: info.range,
            keyword: info.name_range,
            view: shown,
            params,
        });
    }
    out
}

/// The name token's range of the declaration a preview's last line
/// constructs: `Stepper` in `Stepper(value: bind value).max(99)`.
fn rendered(node: &ast::PreviewDecl) -> Option<TextRange> {
    let last = node.body()?.statements().last()?;
    let ast::Stmt::Expr(mut e) = last else {
        return None;
    };
    loop {
        match e {
            ast::Expr::CallExpr(c) => e = c.callee()?,
            ast::Expr::MemberExpr(m) => e = m.receiver()?,
            ast::Expr::NameExpr(n) => return Some(n.name()?.text_range()),
            _ => return None,
        }
    }
}

/// The control for a parameter of type `ty` (see [`Control`]).
#[must_use]
pub fn control(view: &ProgramView, ty: &Type) -> Control {
    let Type::Named(d, args) = ty else {
        return Control::Unsupported;
    };
    if !args.is_empty() {
        return Control::Unsupported;
    }
    let q = view.qualified_name(*d);
    match q.as_str() {
        "k.core.Bool" => return Control::Toggle,
        "k.core.Int" => return Control::Integer,
        "k.core.Double" => return Control::Number,
        "k.core.String" => return Control::Text,
        "k.core.Money" | "k.core.Duration" => return Control::Parsed(q),
        _ => {}
    }
    let decl = view.decl(*d);
    let members = || decl.members.iter().map(|&m| view.member(m));
    if decl.named_kind() == Some(DeclarationKind::Enum) {
        let cases: Vec<_> = members()
            .filter(|m| m.kind == view::MemberKind::Case)
            .collect();
        let plain = cases
            .iter()
            .all(|m| m.signature.as_ref().is_none_or(|s| s.params.is_empty()));
        return if plain && !cases.is_empty() {
            Control::Choice(cases.iter().map(|m| m.name.clone()).collect())
        } else {
            Control::Unsupported
        };
    }
    let parses = members().any(|m| {
        m.kind == view::MemberKind::Func
            && m.is_static
            && m.name == "parse"
            && m.signature.as_ref().is_some_and(|s| {
                s.params.len() == 1
                    && s.params[0].label.is_none()
                    && s.params[0]
                        .ty
                        .decl()
                        .is_some_and(|t| view.qualified_name(t) == "k.core.String")
            })
    });
    if parses {
        Control::Parsed(q)
    } else {
        Control::Unsupported
    }
}

/// A type's name as diagnostics show it.
fn type_name(view: &ProgramView, ty: &Type) -> String {
    match ty {
        Type::Named(d, _) => view.decl(*d).name.clone(),
        Type::Optional(t) => format!("{}?", type_name(view, t)),
        _ => "{error}".to_owned(),
    }
}

/// The value of a literal default: `true`, `3`, `-1.5`, `"text"`, `.case`.
fn literal(e: &ast::Expr) -> Option<ControlValue> {
    match e {
        ast::Expr::Literal(l) => {
            let tok = l.syntax().first_token()?;
            let text = tok.text().replace('_', "");
            match tok.kind() {
                SyntaxKind::TrueKw => Some(ControlValue::Bool(true)),
                SyntaxKind::FalseKw => Some(ControlValue::Bool(false)),
                SyntaxKind::IntLit => text.parse().ok().map(ControlValue::Int),
                SyntaxKind::FloatLit => text.parse().ok().map(ControlValue::Double),
                _ => None,
            }
        }
        ast::Expr::PrefixExpr(p) if p.syntax().first_token()?.kind() == SyntaxKind::Minus => {
            match literal(&p.syntax().children().find_map(ast::Expr::cast)?)? {
                ControlValue::Int(i) => Some(ControlValue::Int(-i)),
                ControlValue::Double(x) => Some(ControlValue::Double(-x)),
                _ => None,
            }
        }
        ast::Expr::StringLit(s) => s.plain_value().map(ControlValue::String),
        ast::Expr::ImplicitMemberExpr(m) => Some(ControlValue::Case(m.name()?.text().to_owned())),
        _ => None,
    }
}
