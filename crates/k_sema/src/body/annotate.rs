//! Checking annotation arguments (sema.md §12.1): an annotation's arguments
//! are checked like a call of its declaring type's initializer, except for
//! parameters whose type is a compile-time name (`DeclarationName`, as in
//! `@Layout(Shell)`) or a reactive dependency (`Dependency`, as in
//! `@Effect(on: [userId])`).

use k_diagnostics::codes;
use k_syntax::ast::{self, AstNode};

use super::{Checker, Target};
use crate::program::DeclId;
use crate::ty::Ty;

/// The annotations that name reactive places: their fields.
const REACTIVE: &[&str] = &[
    "State", "Prop", "Binding", "Derived", "Load", "Form", "Param",
];

impl Checker<'_, '_, '_> {
    /// Checks the arguments of annotation `node`, declared by `decl`.
    pub(crate) fn check_annotation(&mut self, node: &ast::Annotation, decl: DeclId) {
        self.push_fn(Self::plain_fn());
        self.check_annotation_inner(node, decl);
        self.pop_fn();
    }

    /// The context of code outside any function: not async, throws
    /// nothing.
    fn plain_fn() -> super::FnCx {
        super::FnCx {
            is_async: false,
            throws: None,
            lambda_throws: None,
            ret: None,
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        }
    }

    /// [`Checker::check_annotation`] inside its function context.
    fn check_annotation_inner(&mut self, node: &ast::Annotation, decl: DeclId) {
        let Some(list) = node.arg_list() else { return };
        let name = self.cx.p.decl(decl).name.clone();
        match name.as_str() {
            "Effect" => {
                for arg in list.args() {
                    let Some(value) = arg.value() else { continue };
                    let is_on = arg.label().is_some_and(|l| l.text() == "on");
                    if !is_on {
                        self.infer(&value, None);
                        continue;
                    }
                    match &value {
                        ast::Expr::ListExpr(l) => {
                            for item in l.syntax().children().filter_map(ast::Expr::cast) {
                                self.dependency(&item);
                            }
                        }
                        other => {
                            self.infer(other, None);
                        }
                    }
                }
            }
            "Layout" => {
                for arg in list.args() {
                    let Some(value) = arg.value() else { continue };
                    let t = self.infer(&value, None);
                    let is_layout = match self.resolve(&t) {
                        Ty::Meta(inner) => inner.decl().is_some_and(|d| {
                            self.cx.p.decl(d).named_kind()
                                == Some(crate::stdlib::DeclarationKind::Layout)
                        }),
                        Ty::Error => true,
                        _ => false,
                    };
                    if !is_layout {
                        self.err(
                            codes::TYPE_MISMATCH,
                            value.syntax().text_range(),
                            "`@Layout` takes the name of a layout",
                        );
                    }
                }
            }
            "TestDefault" | "Check" => {
                for arg in list.args() {
                    if let Some(value) = arg.value() {
                        self.infer(&value, None);
                    }
                }
            }
            _ => {
                let range = node
                    .name()
                    .map_or(node.syntax().text_range(), |t| t.text_range());
                let args = self.annotation_args(&list);
                self.construct(node.syntax(), decl, range, &args, None);
            }
        }
    }

    /// Checks `Name` or `Name(args)` written outside a body (a `requires`
    /// guard) like a call of `decl`'s initializer.
    pub(crate) fn check_construct(
        &mut self,
        node: &k_syntax::SyntaxNode,
        decl: DeclId,
        name_range: k_diagnostics::TextRange,
    ) {
        let args = match node.children().find_map(ast::ArgList::cast) {
            Some(list) => self.annotation_args(&list),
            None => Vec::new(),
        };
        self.push_fn(Self::plain_fn());
        self.construct(node, decl, name_range, &args, None);
        self.pop_fn();
    }

    /// The arguments of an annotation's argument list, in the shape calls use.
    fn annotation_args(&self, list: &ast::ArgList) -> Vec<super::Arg> {
        list.args()
            .map(|a| {
                let label = a.label().map(|l| (l.text().to_owned(), l.text_range()));
                let expr = a.value();
                let is_lambda = matches!(expr, Some(ast::Expr::LambdaExpr(_)));
                super::Arg {
                    label,
                    expr,
                    range: a.syntax().text_range(),
                    is_lambda,
                }
            })
            .collect()
    }

    /// One element of `@Effect(on: [...])`: a reactive place (K2001
    /// otherwise).
    fn dependency(&mut self, e: &ast::Expr) {
        let t = self.infer(e, None);
        if self.resolve(&t).contains_error() {
            return;
        }
        if self.is_reactive_place(e) {
            return;
        }
        self.err(
            codes::TYPE_MISMATCH,
            e.syntax().text_range(),
            "expected a reactive value (`@State`, `@Prop`, `@Derived`, …)",
        );
    }

    /// Returns `true` for a reactive place: a field with a reactive
    /// annotation, a path into one, or a service's `var` or reactive field
    /// read through an injected field.
    pub(crate) fn is_reactive_place(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::ParenExpr(p) => p
                .syntax()
                .children()
                .find_map(ast::Expr::cast)
                .is_some_and(|i| self.is_reactive_place(&i)),
            ast::Expr::NameExpr(n) => match self.targets.get(&n.syntax().text_range()) {
                Some(Target::Member(m)) => {
                    let member = self.cx.p.member(*m);
                    REACTIVE.iter().any(|a| member.has_annotation(a))
                }
                _ => false,
            },
            ast::Expr::MemberExpr(m) => {
                let Some(recv) = m.receiver() else {
                    return false;
                };
                if self.is_reactive_place(&recv) {
                    return true;
                }
                // `cart.items`, `room.isSeen`: a service's `var` field or
                // reactive one (`@State`, `@Derived`, …), which its
                // injected instance shares with every reader.
                let Some(name) = m.name() else { return false };
                match self.targets.get(&name.text_range()) {
                    Some(Target::Member(f)) => {
                        let member = self.cx.p.member(*f);
                        let owner = self.cx.p.decl(member.owner).named_kind();
                        owner == Some(crate::stdlib::DeclarationKind::Service)
                            && (member.is_var || REACTIVE.iter().any(|a| member.has_annotation(a)))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}
