//! Checking one body: name resolution in lexical scopes (sema.md §7.3),
//! bidirectional type inference (§9.6), effects (§10) and control flow
//! (§11).
//!
//! A [`Checker`] walks the typed syntax tree of one declaration's body
//! (a function, initializer, computed property, field initializer, test case
//! or view). Expected types flow down ([`Checker::check`]) and synthesized
//! types flow up ([`Checker::infer`]); generic arguments are inference
//! variables bound by unification. The same walk tracks
//!
//! * the effect context: `async`, the innermost `try`, `await` and `do`,
//!   and where a thrown error goes (a `do`, a `try … else`, the function's
//!   `throws` type or a lambda's inferred error type);
//! * the flow state: whether the current point is reachable and which
//!   deferred-initialization bindings are definitely (and possibly)
//!   assigned, which drives K2305, K2306, K2307 and K2011.
//!
//! `{error}` suppresses every diagnostic that depends on it, so one mistake
//! is reported once.

use std::collections::HashMap;
use std::rc::Rc;

use k_diagnostics::{Diagnostic, Fix, Span, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode, Visibility};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::cx::{
    Cx, Found, MUTATING_METHODS, MemberFound, Sig, SigKey, SigParam, SigUse, apply, builtin,
    closest, has_token, token,
};
use crate::lookup::Resolution;
use crate::pattern::{self, Pat};
use crate::program::{DeclId, DeclKind, FileIx, GenId, MemberId, MemberKind};
use crate::stdlib::DeclarationKind;
use crate::ty::{FnTy, Ty};

mod annotate;
mod literal;
mod style;
mod view;

/// The deepest expression nesting the checker follows (see
/// [`Checker::infer`]).
const MAX_EXPR_DEPTH: u32 = 1500;

/// Index of a local binding of the body.
pub(crate) type LocalId = usize;

/// A local binding: a `let`/`var`, a parameter, a pattern binding.
#[derive(Clone, Debug)]
pub(crate) struct Local {
    /// The bound name.
    pub(crate) name: String,
    /// The binding's type.
    pub(crate) ty: Ty,
    /// The range of the binding name.
    pub(crate) def: TextRange,
    /// `true` for function parameters (`param` in resolutions).
    pub(crate) is_param: bool,
    /// `true` for `var` bindings.
    pub(crate) mutable: bool,
    /// The `let`/`var` keyword, for K2011's label and fix.
    pub(crate) keyword: Option<TextRange>,
    /// `true` for `let x: T` without initializer (definite initialization).
    pub(crate) deferred: bool,
    /// `true` once a name resolved to the binding (K2304).
    pub(crate) used: bool,
    /// `true` once a read before initialization was reported (K2306 once).
    pub(crate) reported: bool,
}

/// What a name or member expression refers to, for mutability checks.
#[derive(Clone, Copy, Debug)]
enum Target {
    /// A local binding of the body.
    Local(LocalId),
    /// A member (field, property, function) of a declaration.
    Member(MemberId),
    /// A top-level declaration.
    Decl(DeclId),
}

/// An error thrown inside a `do` block, with the cases inner `do`s already
/// handled.
#[derive(Clone, Debug)]
struct Thrown {
    /// The type of the thrown error.
    ty: Ty,
    /// The pattern rows (one pattern per row) that nested `do … catch`
    /// blocks already handled; the outer block's catches only need to
    /// cover the rest.
    covered: Vec<Vec<Pat>>,
}

/// The state of one `do` block being checked.
#[derive(Default)]
struct DoCx {
    /// Every error the block can throw, in source order.
    thrown: Vec<Thrown>,
    /// `true` once the block threw (or called something that throws) an
    /// error of an unknown (`{error}`) type: exhaustiveness and "can't
    /// throw" checks are skipped so the first mistake is reported once.
    saw_unknown: bool,
    /// `true` for the block of `expect throws`, which records whatever is
    /// thrown instead of checking catches (K7502).
    accepts_any: bool,
}

/// The innermost `try`.
struct TryCx {
    /// The range of the whole `try` expression, where errors it marks are
    /// reported.
    range: TextRange,
    /// `true` for `try … else { … }`: the `else` block handles every error,
    /// so nothing propagates.
    has_else: bool,
    /// `true` once a throwing call was seen in the operand (else K2202,
    /// needless `try`).
    saw_throwing: bool,
    /// `true` once a call of unknown type was seen in the operand, which
    /// suppresses the needless-`try` report.
    saw_unknown: bool,
    /// `true` once an unhandled error was reported for this `try`, so one
    /// `try` yields at most one K2203.
    reported: bool,
}

/// The function (or lambda) whose body is being checked.
struct FnCx {
    /// `true` when `await` is allowed (an `async` function, a lambda in an
    /// async context, an initializer).
    is_async: bool,
    /// The declared error type; `Some(Error)` accepts anything.
    throws: Option<Ty>,
    /// The inferred error type of a lambda (an inference variable).
    lambda_throws: Option<Ty>,
    /// The declared (or expected) return type; `None` for initializers and
    /// bodies whose value is inferred.
    ret: Option<Ty>,
    /// `true` for a lambda body: `return` is K2308 there.
    is_lambda: bool,
    /// Depth of enclosing loops: `break` and `continue` need one.
    loops: u32,
    /// The enclosing `do` blocks, innermost last.
    dos: Vec<DoCx>,
    /// The enclosing `try` expressions, innermost last.
    tries: Vec<TryCx>,
    /// Depth of enclosing `await` operands: an async call outside one is
    /// K2204.
    awaits: u32,
    /// The type whose initializer this is, `None` elsewhere: an
    /// initializer may assign its type's non-static `let` fields.
    init_of: Option<DeclId>,
    /// The declaration whose signature this context checks (a function,
    /// method or initializer); `None` for lambdas, initializers of fields
    /// and other bodies without a signature of their own. Labels and fixes
    /// that point at the signature (K2203, K2205) use it.
    decl_node: Option<SyntaxNode>,
}

/// Returns `true` if `e` constructs the enum case `name`: `.name`,
/// `.name(…)`, `T.name` or `T.name(…)`, possibly parenthesized.
fn constructs_case(e: &ast::Expr, name: &str) -> bool {
    match e {
        ast::Expr::ParenExpr(p) => p
            .syntax()
            .children()
            .find_map(ast::Expr::cast)
            .is_some_and(|inner| constructs_case(&inner, name)),
        ast::Expr::CallExpr(c) => c
            .callee()
            .is_some_and(|callee| constructs_case(&callee, name)),
        ast::Expr::MemberExpr(m) => m.name().is_some_and(|n| n.text() == name),
        ast::Expr::ImplicitMemberExpr(m) => m
            .syntax()
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::Ident && t.text() == name),
        _ => false,
    }
}

/// The note of K2301/K2206 listing the missing patterns: at most eight,
/// then how many more.
fn missing_note(missing: &[String]) -> String {
    const SHOWN: usize = 8;
    let mut note = String::from("missing: ");
    for (i, pat) in missing.iter().take(SHOWN).enumerate() {
        if i > 0 {
            note.push_str(", ");
        }
        note.push('`');
        note.push_str(pat);
        note.push('`');
    }
    if missing.len() > SHOWN {
        note.push_str(&format!(" and {} more", missing.len() - SHOWN));
    }
    note
}

/// Returns where an inserted `try ` (`for_try`) or `await ` covering `expr`
/// must go: the start of the outermost expression that `expr` is an operand
/// or postfix receiver of.
///
/// `try` and `await` must start the expression they cover (grammar.md
/// §13.2, K0114), so in `a + load()` the fix goes before `a`, not before
/// `load()`. The walk stops at anything that starts a new expression
/// context: an argument, a list element, parentheses, a statement or a
/// lambda body. A `try` also covers an enclosing `await` (`try await x`),
/// while an `await` stops below an enclosing `try`, so both fixes keep the
/// `try await` order.
fn covering_expr_start(expr: &SyntaxNode, for_try: bool) -> TextSize {
    let mut node = expr.clone();
    while let Some(parent) = node.parent() {
        let walk = match parent.kind() {
            SyntaxKind::BinaryExpr | SyntaxKind::PrefixExpr => true,
            // Only the receiver or callee of a postfix chain: arguments and
            // indexes start their own expressions.
            SyntaxKind::CallExpr | SyntaxKind::MemberExpr | SyntaxKind::IndexExpr => {
                parent.children().next().is_some_and(|first| first == node)
            }
            SyntaxKind::AwaitExpr => for_try,
            _ => false,
        };
        if !walk {
            break;
        }
        node = parent;
    }
    node.text_range().start()
}

/// The places in a function, method or initializer signature that effect
/// diagnostics point at (K2203, K2205).
struct SigPoints {
    /// The declaration's name, or the `init` keyword.
    name: TextRange,
    /// Just after the parameter list's `)`: where `async` goes.
    after_params: TextSize,
    /// Just after `async` when present, else after the parameter list:
    /// where a `throws` clause goes.
    after_effects: TextSize,
    /// `true` when the signature already has a `throws` clause.
    has_throws: bool,
}

impl SigPoints {
    /// Finds the points of a `FuncDecl`, `InitDecl` or `RpcDecl` node;
    /// `None` when the signature is too broken to edit (no parameter list).
    fn of(node: &SyntaxNode) -> Option<SigPoints> {
        let name = node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::InitKw))?
            .text_range();
        let params = node
            .children()
            .find(|c| c.kind() == SyntaxKind::ParamList)?;
        let after_params = params.text_range().end();
        let after_effects = token(node, SyntaxKind::AsyncKw)
            .filter(|t| t.text_range().start() >= after_params)
            .map_or(after_params, |t| t.text_range().end());
        let has_throws = node
            .children()
            .any(|c| c.kind() == SyntaxKind::ThrowsClause);
        Some(SigPoints {
            name,
            after_params,
            after_effects,
            has_throws,
        })
    }
}

/// The flow state at a point of the body.
#[derive(Clone, Debug, PartialEq)]
struct Flow {
    /// `false` after a `return`, `throw`, `break` or `continue` (or a call
    /// returning `Never`) on every path to this point.
    reachable: bool,
    /// Deferred bindings definitely assigned.
    init: Vec<LocalId>,
    /// Deferred bindings possibly assigned.
    maybe: Vec<LocalId>,
}

impl Flow {
    /// Returns the state at the start of a body: reachable, nothing
    /// assigned.
    fn start() -> Flow {
        Flow {
            reachable: true,
            init: Vec::new(),
            maybe: Vec::new(),
        }
    }

    /// Joins the states at the end of alternative branches: reachable if
    /// any branch is, definitely assigned if assigned on every reachable
    /// branch, possibly assigned if on any.
    ///
    /// When no branch is reachable the result is the first branch's state
    /// (or [`Flow::start`]) marked unreachable. Quadratic in the number of
    /// deferred bindings, which is small.
    fn merge(branches: &[Flow]) -> Flow {
        let live: Vec<&Flow> = branches.iter().filter(|f| f.reachable).collect();
        if live.is_empty() {
            let mut f = branches.first().cloned().unwrap_or_else(Flow::start);
            f.reachable = false;
            return f;
        }
        let mut init = live[0].init.clone();
        init.retain(|l| live.iter().all(|f| f.init.contains(l)));
        let mut maybe = Vec::new();
        for f in &live {
            for l in &f.maybe {
                if !maybe.contains(l) {
                    maybe.push(*l);
                }
            }
        }
        Flow {
            reachable: true,
            init,
            maybe,
        }
    }
}

/// A call argument.
struct Arg {
    /// The argument label and its range, `None` for positional arguments.
    label: Option<(String, TextRange)>,
    /// The argument value, `None` when the parser recovered from a missing
    /// one.
    expr: Option<ast::Expr>,
    /// The range of the whole argument (label included).
    range: TextRange,
    /// `true` for a lambda argument (including a trailing lambda), whose
    /// parameter types come from the selected overload.
    is_lambda: bool,
}

/// Where a body is checked: the declaration that owns `self` and its
/// private members, and the generic parameters in scope.
#[derive(Clone)]
pub(crate) struct BodyOwner {
    /// The file of the body.
    pub(crate) file: FileIx,
    /// The declaration whose private members the body sees (the extension
    /// itself for extension members).
    pub(crate) decl: Option<DeclId>,
    /// The type of `self`, `None` outside types and extensions.
    pub(crate) self_ty: Option<Ty>,
    /// The generic parameters in scope.
    pub(crate) generics: Vec<GenId>,
}

/// The checker of one body.
///
/// A checker is single-threaded and short-lived: one is created per body
/// (see [`Checker::new`]), driven through one of its entry points and
/// consumed by [`Checker::finish`], which hands its diagnostics and types
/// to the shared [`Cx`].
pub(crate) struct Checker<'c, 'p, 'a> {
    /// The shared checking context (program, signatures, results).
    cx: &'c Cx<'p, 'a>,
    /// Where the body is checked.
    owner: BodyOwner,
    /// Every local binding of the body, indexed by [`LocalId`].
    pub(crate) locals: Vec<Local>,
    /// The lexical scopes, innermost last: the names they bind, in order.
    scopes: Vec<Vec<(String, LocalId)>>,
    /// The enclosing functions and lambdas, innermost last.
    fns: Vec<FnCx>,
    /// The flow state at the current point.
    flow: Flow,
    /// The inference variables: `vars[v]` is what `Ty::Var(v)` is bound
    /// to, `None` while unbound.
    vars: Vec<Option<Ty>>,
    /// Where each inference variable was bound, for K2002 labels; parallel
    /// to [`Checker::vars`].
    origins: Vec<Option<TextRange>>,
    /// Types of expressions to record for tooling, resolved and handed to
    /// the context by [`Checker::finish`].
    node_types: Vec<(TextRange, Ty)>,
    /// What each name or member expression refers to (by range), for
    /// mutability and framework checks.
    targets: HashMap<TextRange, Target>,
    /// Suppression depth: while positive, diagnostics are dropped (used
    /// for speculative inference such as overload selection).
    quiet: u32,
    /// Lambdas that are the value of a lambda whose parameter types
    /// couldn't be inferred (K2002 already reported for the outer one).
    uninferred_values: std::collections::HashSet<TextRange>,
    /// `true` when types and resolutions are recorded for tooling; off for
    /// silent and speculative passes.
    record: bool,
    /// The diagnostics reported so far, handed over by [`Checker::finish`].
    diags: Vec<Diagnostic>,
    /// The latest type inferred for each expression range (unresolved),
    /// read back by later checks on the same node.
    last_types: HashMap<TextRange, Ty>,
    /// While positive, an implicit member (`.name`) without an expected
    /// type is `{error}` silently instead of K2007.
    no_implicit: u32,
    /// Set by [`Checker::allow_throws`]: the next initializer may throw
    /// any error.
    initializer_throws: bool,
    /// Member and call expressions inside an optional chain (`a?.b.c`):
    /// the rest of the chain reads the unwrapped value.
    chained: std::collections::HashSet<TextRange>,
    /// Set when a call turns out to be a chained prop or slot of a
    /// component: its arguments are checked quietly by the framework
    /// layer instead.
    quiet_args: bool,
    /// The current expression nesting depth, bounded by [`MAX_EXPR_DEPTH`].
    depth: u32,
    /// Memo of [`Checker::implicit_chain`] per node of a postfix chain.
    implicit_chains: HashMap<TextRange, bool>,
    /// `true` while the statements being checked are view content
    /// ([`view`]).
    view_on: bool,
    /// The content rule of the element whose content is being checked.
    view_ctx: view::ViewCx,
    /// `true` while the operand of `bind` is inferred: a `@Form` field
    /// reads as its struct there (`bind signup.email`).
    in_bind: bool,
    /// Loop variables over bindable lists (`bind item.quantity`).
    bindable_locals: std::collections::HashSet<LocalId>,
    /// `true` while a `@Derived` initializer is checked: writes to state
    /// are K3011 there.
    in_derived: bool,
    /// `true` while code that may run during server rendering is checked:
    /// views and field initializers (K4012, K4013), not event handlers.
    ssr: bool,
    /// `true` while a `@Form` field's initializer is checked (`.empty` is
    /// valid only there, K7002).
    form_init: bool,
    /// The member function whose body is checked (K7001).
    current_fn: Option<MemberId>,
    /// Depth of lambdas passed to `form.handle(…)` (they read `value`).
    handle_depth: u32,
    /// Errors that escaped to an initializer allowed to throw (a `@Load`'s
    /// errors go to `failed`).
    escaped: Vec<Ty>,
    /// Depth of lambdas passed to query methods (`filter`, `first`, …),
    /// which are translated to SQL (K6001).
    query_lambda_depth: u32,
    /// Depth of `update { row => … }` lambdas, whose row is writable.
    query_update_depth: u32,
}

impl<'c, 'p, 'a> Checker<'c, 'p, 'a> {
    /// Creates a checker for a body owned by `owner`.
    pub(crate) fn new(cx: &'c Cx<'p, 'a>, owner: BodyOwner) -> Checker<'c, 'p, 'a> {
        Checker {
            cx,
            owner,
            locals: Vec::new(),
            scopes: vec![Vec::new()],
            fns: Vec::new(),
            flow: Flow::start(),
            vars: Vec::new(),
            origins: Vec::new(),
            node_types: Vec::new(),
            targets: HashMap::new(),
            quiet: 0,
            uninferred_values: std::collections::HashSet::new(),
            record: true,
            diags: Vec::new(),
            last_types: HashMap::new(),
            no_implicit: 0,
            initializer_throws: false,
            chained: std::collections::HashSet::new(),
            quiet_args: false,
            depth: 0,
            implicit_chains: HashMap::new(),
            view_on: false,
            view_ctx: view::ViewCx::default(),
            in_bind: false,
            bindable_locals: std::collections::HashSet::new(),
            in_derived: false,
            ssr: false,
            form_init: false,
            current_fn: None,
            handle_depth: 0,
            escaped: Vec::new(),
            query_lambda_depth: 0,
            query_update_depth: 0,
        }
    }

    /// The query collection a repository sees (sema.md §12.12): `notes` in
    /// `repository NoteRepository for Note`, a `Query<Note>`.
    fn repository_collection(&self, name: &str) -> Option<Ty> {
        let d = self.owner.decl?;
        let decl = self.cx.p.decl(d);
        if decl.named_kind() != Some(DeclarationKind::Repository) {
            return None;
        }
        let entity_node = decl
            .node
            .children()
            .find(|c| c.kind() == SyntaxKind::ForClause)
            .and_then(|c| c.children().find_map(ast::Type::cast))?;
        let entity = self.cx.resolve_type_quiet(decl.file, &[], &entity_node);
        let e = entity.decl()?;
        if crate::framework::collection_name(&self.cx.p.decl(e).name) != name {
            return None;
        }
        let query = self.cx.p.lookup_qualified("k.db.Query")?;
        Some(Ty::generic(query, vec![entity]))
    }

    /// K6001: a query lambda calls a function SQL can't express.
    fn untranslatable_call(&mut self, node: &SyntaxNode, user_function: bool) {
        if self.query_lambda_depth == 0 || !user_function {
            return;
        }
        self.err(
            codes::UNTRANSLATABLE_QUERY,
            node.text_range(),
            "a query lambda is translated to SQL, which can't call this function",
        );
    }

    /// Returns `true` in test files and test-only modules.
    fn in_test_code(&self) -> bool {
        let file = self.cx.p.file(self.file());
        file.is_test
            || file
                .module
                .is_some_and(|m| self.cx.p.modules[m.0 as usize].test_only)
    }

    /// Returns `true` for a `Secret<T>` operand or one computed by
    /// `reveal()`.
    fn secret_operand(&self, e: &ast::Expr, t: &Ty) -> bool {
        let secret = self.cx.p.lookup_qualified("k.core.Secret");
        if secret.is_some() && self.resolve(t).decl() == secret {
            return true;
        }
        e.syntax()
            .descendants()
            .filter_map(ast::MemberExpr::cast)
            .any(|m| {
                m.name().is_some_and(|n| n.text() == "reveal")
                    && m.receiver().is_some_and(|r| {
                        self.type_of_node(r.syntax().text_range())
                            .is_some_and(|t| secret.is_some() && t.decl() == secret)
                    })
            })
    }

    /// K3013: an effect that discards the `TimerHandle` of `clock.every`
    /// or `clock.after` can't cancel the timer.
    fn discarded_timer(&mut self, e: &ast::Expr, t: &Ty) {
        let handle = self.cx.p.lookup_qualified("k.time.TimerHandle");
        if handle.is_none() || t.decl() != handle {
            return;
        }
        let effect = self
            .current_fn
            .is_some_and(|m| self.cx.p.member(m).has_annotation("Effect"));
        if !effect || self.fcx_ref().is_lambda {
            return;
        }
        self.report(
            Diagnostic::new(
                codes::DISCARDED_TIMER_HANDLE,
                self.span(e.syntax().text_range()),
                "the timer is never cancelled: keep its `TimerHandle` and cancel it in `onCleanup`",
            )
            .with_fix(
                Fix::replace(
                    "Bind the handle and cancel it in `onCleanup`",
                    self.span(e.syntax().text_range()),
                    format!(
                        "let timer = {}\n        onCleanup {{ timer.cancel() }}",
                        self.text(e.syntax().text_range())
                    ),
                )
                .maybe_incorrect(),
            ),
        );
    }

    /// The errors that escaped the initializer, first one first.
    pub(crate) fn take_escaped(&mut self) -> Vec<Ty> {
        std::mem::take(&mut self.escaped)
    }

    /// Binds the names of a `failed(let error)` pattern with type `ty`.
    pub(crate) fn bind_names_typed(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        for name in pattern
            .syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::Ident)
        {
            self.note_type(name.text_range(), ty);
            self.add_local(Local {
                name: name.text().to_owned(),
                ty: ty.clone(),
                def: name.text_range(),
                is_param: false,
                mutable: false,
                keyword: None,
                deferred: false,
                used: true,
                reported: false,
            });
        }
    }

    /// Marks the body as a `@Form` field's initializer.
    pub(crate) fn form_initializer(&mut self) {
        self.form_init = true;
    }

    /// Records the member function whose body is checked.
    pub(crate) fn in_function(&mut self, m: MemberId) {
        self.current_fn = Some(m);
    }

    /// K7001: `form.value` is read only in the action passed to
    /// `form.handle(…)` and in functions only it calls.
    fn form_value(&mut self, name: &SyntaxToken) {
        let Some(Target::Member(m)) = self.targets.get(&name.text_range()).copied() else {
            return;
        };
        let member = self.cx.p.member(m);
        if member.name != "value"
            || self.cx.p.lookup_qualified("k.validation.Form") != Some(member.owner)
        {
            return;
        }
        if self.handle_depth > 0 {
            return;
        }
        let allowed = match (self.owner.decl, self.current_fn) {
            (Some(d), Some(f)) => self.cx.fw.form_actions(self.cx, d).contains(&f),
            _ => false,
        };
        if !allowed {
            self.err(
                codes::FORM_VALUE_OUTSIDE_HANDLE,
                name.text_range(),
                "`value` is read only inside the action passed to `handle(…)`, where every field parses",
            );
        }
    }

    /// Marks the body as code that may run during server rendering
    /// (a field initializer).
    pub(crate) fn server_rendered(&mut self) {
        self.ssr = true;
    }

    /// K4012: a client-only interface called while the server renders.
    fn client_only_call(&mut self, member: MemberId, call: &SyntaxNode) {
        if !self.ssr {
            return;
        }
        let owner = self.cx.p.member(member).owner;
        // The platform's client-only APIs (`LocalStorage`, `Theme`); nothing
        // runs during server rendering in a client-rendered app.
        if !self.cx.p.is_stdlib(owner) || crate::di::client_rendered(self.cx) {
            return;
        }
        let side = crate::di::side(self.cx, owner);
        if side.client && !side.server {
            self.err(
                codes::CLIENT_ONLY_DURING_SSR,
                call.text_range(),
                format!(
                    "`{}` is client-only, but this code may run while the server renders",
                    self.cx.p.decl_name(owner)
                ),
            );
        }
    }

    /// Records what the name at `range` resolves to (declaration-level
    /// checks outside bodies use it).
    pub(crate) fn note_resolution(&self, range: TextRange, resolution: Resolution) {
        self.note_res(range, resolution);
    }

    /// Runs `f` with reporting off.
    pub(crate) fn silently(&mut self, f: impl FnOnce(&mut Self)) {
        self.quiet += 1;
        f(self);
        self.quiet -= 1;
    }

    /// Marks the body as a `@Derived` initializer (K3011 for state
    /// writes).
    pub(crate) fn derived(&mut self) {
        self.in_derived = true;
    }

    /// The field of the enclosing declaration a place starts from
    /// (`count` in `count.x = 1`), if any.
    fn place_root_member(&self, place: &ast::Expr) -> Option<MemberId> {
        let mut current = place.clone();
        loop {
            match &current {
                ast::Expr::ParenExpr(p) => {
                    current = p.syntax().children().find_map(ast::Expr::cast)?
                }
                ast::Expr::MemberExpr(m) => current = m.receiver()?,
                ast::Expr::IndexExpr(i) => {
                    current = i.syntax().children().find_map(ast::Expr::cast)?
                }
                ast::Expr::NameExpr(n) => {
                    return match self.targets.get(&n.syntax().text_range()) {
                        Some(Target::Member(m))
                            if self.cx.p.member(*m).kind == MemberKind::Field =>
                        {
                            Some(*m)
                        }
                        _ => None,
                    };
                }
                _ => return None,
            }
        }
    }

    /// K3011 when `place` is state written while a `@Derived` value is
    /// computed; returns `true` when reported.
    fn derived_write(&mut self, place: &ast::Expr, at: TextRange) -> bool {
        if !self.in_derived || self.place_root_member(place).is_none() {
            return false;
        }
        self.err(
            codes::STATE_WRITE_IN_DERIVED,
            at,
            "a `@Derived` value can't write state while it is computed",
        );
        true
    }

    /// K3011 at a call, made while a `@Derived` value is computed, of a
    /// function of the same declaration that writes its fields (directly or
    /// through its own calls; see `FwState::state_writers`).
    fn derived_call(&mut self, callee: MemberId, at: TextRange) {
        if !self.in_derived {
            return;
        }
        let Some(decl) = self.owner.decl else { return };
        if self.cx.p.member(callee).owner != decl
            || !self.cx.fw.state_writers(self.cx, decl).contains(&callee)
        {
            return;
        }
        self.err(
            codes::STATE_WRITE_IN_DERIVED,
            at,
            format!(
                "`{}` writes state, so a `@Derived` value can't call it while it is computed",
                self.cx.p.member(callee).name
            ),
        );
    }

    /// Turns off reporting and recording (for inferring a field's type on
    /// demand; the body pass reports later).
    pub(crate) fn silent(mut self) -> Self {
        self.quiet = 1;
        self.record = false;
        self
    }

    /// Returns the file of the body being checked.
    fn file(&self) -> FileIx {
        self.owner.file
    }

    /// Returns the source text of `range` in the body's file.
    ///
    /// # Panics
    ///
    /// Panics if `range` is not within the file or does not fall on UTF-8
    /// boundaries; callers pass ranges of the file's own syntax tree.
    fn text(&self, range: TextRange) -> &'a str {
        let text = self.cx.p.file(self.file()).text;
        &text[usize::from(range.start())..usize::from(range.end())]
    }

    /// Returns the [`Span`] of `range` in the body's file.
    fn span(&self, range: TextRange) -> Span {
        self.cx.p.span(self.file(), range)
    }

    /// Collects `diagnostic`, unless reporting is suppressed
    /// ([`Checker::quiet`] is positive).
    fn report(&mut self, diagnostic: Diagnostic) {
        if self.quiet == 0 {
            self.diags.push(diagnostic);
        }
    }

    /// Reports a diagnostic with `code` and `message` at `range`, without
    /// labels or fixes.
    fn err(&mut self, code: k_diagnostics::Code, range: TextRange, message: impl Into<String>) {
        let d = Diagnostic::new(code, self.span(range), message);
        self.report(d);
    }

    /// Hands the collected diagnostics and recorded types to the context.
    pub(crate) fn finish(mut self) {
        if self.record {
            let types = std::mem::take(&mut self.node_types);
            for (range, ty) in types {
                let ty = self.resolve(&ty);
                self.cx.record_type(self.file(), range, ty);
            }
        }
        let file = self.file();
        for d in self.diags.drain(..) {
            self.cx.report(file, d);
        }
    }

    /// Records `ty` as the type of the node at `range` for tooling, when
    /// recording is on.
    fn note_type(&mut self, range: TextRange, ty: &Ty) {
        if self.record {
            self.node_types.push((range, ty.clone()));
        }
    }

    /// Records what the name at `range` resolves to for tooling, when
    /// recording is on.
    fn note_res(&self, range: TextRange, resolution: Resolution) {
        if self.record {
            self.cx.record_resolution(self.file(), range, resolution);
        }
    }

    // ------------------------------------------------------------ entry points

    /// Checks a function-like body: parameters, effects, return type.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_function(
        &mut self,
        node: &SyntaxNode,
        sig: &Sig,
        body: Option<ast::Block>,
        is_init: bool,
        tests: bool,
    ) {
        self.push_fn(FnCx {
            is_async: sig.is_async || tests,
            throws: if tests {
                Some(Ty::Error)
            } else {
                sig.throws.clone()
            },
            lambda_throws: None,
            ret: if is_init { None } else { Some(sig.ret.clone()) },
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: if is_init { self.owner.decl } else { None },
            decl_node: Some(node.clone()),
        });
        if let Some(list) = node.children().find_map(ast::ParamList::cast) {
            for (param, sp) in list.params().zip(sig.params.iter()) {
                if let Some(default) = param.default_value() {
                    self.check(&default, &sp.ty);
                }
                self.bind_param(sp);
            }
        }
        if let Some(body) = body {
            self.block_stmts(&body);
            let ret = sig.ret.clone();
            let void = self.is_void(&ret);
            if self.flow.reachable && !is_init && !void && !ret.is_error() && !tests {
                if let Some(close) = token(body.syntax(), SyntaxKind::RBrace) {
                    self.err(
                        codes::MISSING_RETURN,
                        close.text_range(),
                        format!(
                            "missing `return` in a function returning `{}`",
                            ret.render(self.cx.p)
                        ),
                    );
                }
            }
        }
        self.pop_fn();
    }

    /// Checks a block whose last expression is a value of `expected`
    /// (computed properties).
    pub(crate) fn check_value_block(&mut self, block: &ast::Block, expected: &Ty) {
        self.push_fn(FnCx {
            is_async: false,
            throws: None,
            lambda_throws: None,
            ret: Some(expected.clone()),
            is_lambda: false,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        });
        let stmts: Vec<ast::Stmt> = block.statements().collect();
        self.value_stmts(&stmts, Some(expected), true);
        self.pop_fn();
    }

    /// Lets the next initializer throw (an `@Load` initializer: its errors
    /// go to the `failed` block).
    pub(crate) fn allow_throws(&mut self) {
        self.initializer_throws = true;
    }

    /// K2002 when an initializer's type still has unbound variables
    /// (`Fetch.idle` without context).
    fn report_unknown(&mut self, e: &ast::Expr, t: &Ty) {
        if t.has_vars_in(&self.vars) && !t.contains_error() {
            self.err(
                codes::CANNOT_INFER,
                e.syntax().text_range(),
                format!(
                    "the type of this expression can't be inferred (`{}`)",
                    t.render(self.cx.p)
                ),
            );
        }
    }

    /// Checks an initializer expression against `expected` (or infers it).
    pub(crate) fn check_initializer(&mut self, expr: &ast::Expr, expected: Option<&Ty>) -> Ty {
        let throws = if self.initializer_throws {
            Some(Ty::Error)
        } else {
            None
        };
        self.push_fn(FnCx {
            is_async: true,
            throws,
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
        let ty = match expected {
            Some(e) => {
                self.check(expr, e);
                e.clone()
            }
            None => {
                let t = self.infer(expr, None);
                let t = self.resolve(&t);
                self.report_unknown(expr, &t);
                t
            }
        };
        self.pop_fn();
        self.resolve(&ty)
    }

    /// Enters a function or lambda context.
    fn push_fn(&mut self, f: FnCx) {
        self.fns.push(f);
    }

    /// Leaves the innermost function or lambda context.
    fn pop_fn(&mut self) {
        self.fns.pop();
    }

    /// Returns the innermost function context, mutably.
    ///
    /// # Panics
    ///
    /// Panics outside every function context; each entry point pushes one
    /// before checking statements or expressions.
    fn fcx(&mut self) -> &mut FnCx {
        let len = self.fns.len();
        &mut self.fns[len - 1]
    }

    /// Returns the innermost function context.
    ///
    /// # Panics
    ///
    /// Panics outside every function context, like [`Checker::fcx`].
    fn fcx_ref(&self) -> &FnCx {
        &self.fns[self.fns.len() - 1]
    }

    /// Binds a parameter of the signature as an immutable local of the
    /// current scope.
    fn bind_param(&mut self, p: &SigParam) {
        let id = self.add_local(Local {
            name: p.name.clone(),
            ty: p.ty.clone(),
            def: p.name_range,
            is_param: true,
            mutable: false,
            keyword: None,
            deferred: false,
            used: false,
            reported: false,
        });
        let _ = id;
    }

    /// Adds `local` to the body and binds its name in the innermost scope;
    /// returns its id.
    fn add_local(&mut self, local: Local) -> LocalId {
        let id = self.locals.len();
        let name = local.name.clone();
        self.locals.push(local);
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name, id));
        }
        id
    }

    /// Opens a lexical scope.
    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    /// Closes the innermost lexical scope; its bindings stop being visible
    /// (they stay in [`Checker::locals`]).
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Looks `name` up in the lexical scopes, innermost (and latest binding)
    /// first; `None` when no local has that name.
    ///
    /// Linear in the number of visible bindings.
    fn find_local(&self, name: &str) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            if let Some((_, id)) = scope.iter().rev().find(|(n, _)| n == name) {
                return Some(*id);
            }
        }
        None
    }

    // ------------------------------------------------------------ inference vars

    /// Returns a new, unbound inference variable.
    fn fresh(&mut self) -> Ty {
        let id = u32::try_from(self.vars.len()).unwrap_or(u32::MAX);
        self.vars.push(None);
        self.origins.push(None);
        Ty::Var(id)
    }

    /// Replaces bound variables by their values, deeply.
    ///
    /// A type without bound variables is returned as is (its `Arc`s are
    /// shared, nothing is allocated), so resolving a large, already
    /// resolved type costs one traversal and no allocation.
    pub(crate) fn resolve(&self, ty: &Ty) -> Ty {
        self.resolve_changed(ty).unwrap_or_else(|| ty.clone())
    }

    /// [`resolve`](Checker::resolve), but `None` when `ty` has no bound
    /// variable (the resolved type is `ty` itself).
    fn resolve_changed(&self, ty: &Ty) -> Option<Ty> {
        /// Resolves `items`; `None` when none of them changed.
        fn items(checker: &Checker<'_, '_, '_>, items: &[Ty]) -> Option<Vec<Ty>> {
            let first = items
                .iter()
                .enumerate()
                .find_map(|(i, t)| Some((i, checker.resolve_changed(t)?)))?;
            let (index, changed) = first;
            let mut out = Vec::with_capacity(items.len());
            out.extend_from_slice(&items[..index]);
            out.push(changed);
            out.extend(items[index + 1..].iter().map(|t| checker.resolve(t)));
            Some(out)
        }
        match ty {
            Ty::Var(v) => {
                let bound = self.vars.get(*v as usize)?.as_ref()?;
                Some(self.resolve(bound))
            }
            Ty::Error | Ty::Never | Ty::Param(_) => None,
            Ty::Named(d, args) => Some(Ty::Named(*d, items(self, args)?.into())),
            Ty::Tuple(elements) => Some(Ty::Tuple(items(self, elements)?.into())),
            Ty::Optional(inner) => Some(Ty::optional(self.resolve_changed(inner)?)),
            Ty::Meta(inner) => Some(Ty::Meta(std::sync::Arc::new(self.resolve_changed(inner)?))),
            Ty::Func(f) => {
                let params = items(self, &f.params);
                let throws = f.throws.as_ref().and_then(|t| self.resolve_changed(t));
                let ret = self.resolve_changed(&f.ret);
                if params.is_none() && throws.is_none() && ret.is_none() {
                    return None;
                }
                Some(Ty::func(
                    params.unwrap_or_else(|| f.params.clone()),
                    f.is_async,
                    throws.or_else(|| f.throws.clone()),
                    ret.unwrap_or_else(|| f.ret.clone()),
                ))
            }
        }
    }

    /// Follows bound variables at the top of `ty` only: the result is not a
    /// bound variable, but its components may contain some.
    fn shallow(&self, ty: &Ty) -> Ty {
        let mut ty = ty;
        while let Ty::Var(v) = ty {
            match self.vars.get(*v as usize) {
                Some(Some(bound)) => ty = bound,
                _ => break,
            }
        }
        ty.clone()
    }

    /// Binds every unbound variable in `ty` to `{error}`.
    fn poison(&mut self, ty: &Ty) {
        match self.resolve(ty) {
            Ty::Var(v) => self.bind(v, Ty::Error, None),
            Ty::Named(_, args) | Ty::Tuple(args) => {
                for a in args.iter() {
                    self.poison(a);
                }
            }
            Ty::Optional(inner) | Ty::Meta(inner) => self.poison(&inner),
            Ty::Func(f) => {
                for p in &f.params {
                    self.poison(p);
                }
                self.poison(&f.ret);
            }
            _ => {}
        }
    }

    /// Binds the inference variable `v` to `ty`, remembering `origin` as
    /// where it was first bound.
    ///
    /// Binding a variable to itself is a no-op (no cycle is created); an
    /// unknown `v` is ignored.
    fn bind(&mut self, v: u32, ty: Ty, origin: Option<TextRange>) {
        if let Ty::Var(w) = ty {
            if w == v {
                return;
            }
        }
        if let Some(slot) = self.vars.get_mut(v as usize) {
            *slot = Some(ty);
        }
        if let Some(slot) = self.origins.get_mut(v as usize) {
            if slot.is_none() {
                *slot = origin;
            }
        }
    }

    /// Unifies two types invariantly, binding variables.
    ///
    /// Each level resolves only the top of both types, so unifying two
    /// types is linear in their size (resolving them deeply at every level
    /// would be quadratic in their depth).
    fn unify(&mut self, a: &Ty, b: &Ty, origin: Option<TextRange>) -> bool {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), _) => {
                self.bind(*x, b.clone(), origin);
                true
            }
            (_, Ty::Var(y)) => {
                self.bind(*y, a.clone(), origin);
                true
            }
            (Ty::Named(d1, a1), Ty::Named(d2, a2)) => {
                d1 == d2
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2.iter())
                        .all(|(x, y)| self.unify(x, y, origin))
            }
            (Ty::Optional(x), Ty::Optional(y)) => self.unify(x, y, origin),
            (Ty::Tuple(x), Ty::Tuple(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|(p, q)| self.unify(p, q, origin))
            }
            (Ty::Func(f), Ty::Func(g)) => {
                f.params.len() == g.params.len()
                    && f.params
                        .iter()
                        .zip(g.params.iter())
                        .all(|(p, q)| self.unify(p, q, origin))
                    && self.unify(&f.ret, &g.ret, origin)
            }
            _ => self.resolve(&a) == self.resolve(&b),
        }
    }

    /// Returns `true` if a value of `actual` is accepted where `expected`
    /// is expected, binding variables.
    ///
    /// Like [`unify`](Checker::unify), each level resolves only the top of
    /// both types; the branches that inspect a whole type (`implements`)
    /// resolve it deeply.
    fn assignable(&mut self, actual: &Ty, expected: &Ty, origin: Option<TextRange>) -> bool {
        let a = self.shallow(actual);
        let e = self.shallow(expected);
        let wild = |t: &Ty| match t {
            Ty::Named(d, _) => self.cx.is_wildcard(*d),
            Ty::Optional(inner) => matches!(&**inner, Ty::Named(d, _) if self.cx.is_wildcard(*d)),
            _ => false,
        };
        if wild(&a) || wild(&e) {
            return true;
        }
        if let (Ty::Named(d, _), Some(v)) = (&a, self.deopt(&e).decl()) {
            if Some(v) == self.cx.b.view && self.cx.implements(&a, v) && *d != v {
                // Components, pages and layouts are values of `View`.
                return true;
            }
        }
        match (&a, &e) {
            (Ty::Error, Ty::Var(v)) | (Ty::Var(v), Ty::Error) => {
                // An erroneous argument fixes the variable to `{error}`, so
                // nothing downstream reports it again.
                self.bind(*v, Ty::Error, origin);
                true
            }
            (Ty::Error, _) => {
                self.poison(&e);
                true
            }
            (_, Ty::Error) => {
                self.poison(&a);
                true
            }
            (Ty::Never, _) => true,
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (_, Ty::Var(y)) => {
                self.bind(*y, a.clone(), origin);
                true
            }
            (Ty::Var(x), _) => {
                self.bind(*x, e.clone(), origin);
                true
            }
            (Ty::Optional(x), Ty::Optional(y)) => self.assignable(x, y, origin),
            (Ty::Optional(_), _) => false,
            (_, Ty::Optional(y)) => self.assignable(&a, y, origin),
            (Ty::Named(d1, _), Ty::Named(d2, _)) if d1 == d2 => self.unify(&a, &e, origin),
            (_, Ty::Named(i, _)) if self.cx.is_interface(*i) => {
                if let Ty::Named(d, _) = &a {
                    if self.cx.is_interface(*d) && !self.cx.declares(*d, *i) {
                        return false;
                    }
                }
                self.cx.implements(&self.resolve(&a), *i)
            }
            (Ty::Named(d1, _), Ty::Named(d2, _)) if self.token_compatible(*d1, *d2) => true,
            (Ty::Func(f), Ty::Func(g)) => {
                if f.params.len() != g.params.len() {
                    return false;
                }
                if f.is_async && !g.is_async {
                    return false;
                }
                let params_ok = f
                    .params
                    .iter()
                    .zip(g.params.iter())
                    .all(|(p, q)| self.assignable(q, p, origin));
                if !params_ok || !self.assignable(&f.ret, &g.ret, origin) {
                    return false;
                }
                match (&f.throws, &g.throws) {
                    // A function that can't throw is a function that
                    // throws anything (subtyping, sema.md §9.6). An error
                    // variable stays open: a later argument may fix it
                    // (`twice(a: { 1 }, b: { try fe() })`); the call binds
                    // it to `Never` if nothing does (`call_effects`).
                    (None, Some(_)) => true,
                    (None, None) => true,
                    (Some(_), None) => false,
                    (Some(x), Some(y)) => self.unify(x, y, origin),
                }
            }
            _ => self.unify(&a, &e, origin),
        }
    }

    /// Returns `true` if a value of the style-token type `a` is accepted
    /// where `b` is expected: `Length` and the length-like tokens (`Spacing`,
    /// `Radius`, `FontSize`) convert to and from `Length`, not to each other.
    fn token_compatible(&self, a: DeclId, b: DeclId) -> bool {
        let b_ = &self.cx.b;
        let lengthy = [b_.length, b_.spacing, b_.radius, b_.font_size];
        lengthy.contains(&Some(a))
            && lengthy.contains(&Some(b))
            && (Some(a) == b_.length || Some(b) == b_.length)
    }

    /// Returns `true` if `ty` is `Void`.
    fn is_void(&self, ty: &Ty) -> bool {
        ty.decl().is_some() && ty.decl() == self.cx.b.void
    }

    /// Returns `true` if `ty` is `Bool`.
    fn is_bool(&self, ty: &Ty) -> bool {
        ty.decl().is_some() && ty.decl() == self.cx.b.bool_
    }

    /// Returns `ty` resolved, without one level of optionality (`T?` gives
    /// `T`, anything else itself).
    fn deopt(&self, ty: &Ty) -> Ty {
        match self.resolve(ty) {
            Ty::Optional(inner) => (*inner).clone(),
            other => other,
        }
    }

    // ------------------------------------------------------------ statements

    /// Checks the statements of `block` in a new scope (as view content when
    /// [`Checker::view_on`] is set).
    fn block_stmts(&mut self, block: &ast::Block) {
        self.push_scope();
        if self.view_on {
            let stmts: Vec<ast::Stmt> = block.statements().collect();
            self.view_stmts(&stmts);
        } else {
            for stmt in block.statements() {
                self.stmt(&stmt);
            }
        }
        self.pop_scope();
    }

    /// Checks statements of a block used as a value; returns the value type.
    fn value_stmts(&mut self, stmts: &[ast::Stmt], expected: Option<&Ty>, report: bool) -> Ty {
        if self.view_on {
            self.push_scope();
            self.view_stmts(stmts);
            self.pop_scope();
            return builtin(self.cx.b.view);
        }
        self.push_scope();
        let mut ty = builtin(self.cx.b.void);
        for (i, stmt) in stmts.iter().enumerate() {
            let last = i + 1 == stmts.len();
            if last {
                ty = match stmt {
                    ast::Stmt::Expr(e) => {
                        let (t, value) = match expected {
                            Some(exp) if report => {
                                let t = self.check(e, exp);
                                let t = self.resolve(&t);
                                let value = if matches!(t, Ty::Never) {
                                    Ty::Never
                                } else {
                                    exp.clone()
                                };
                                (t, value)
                            }
                            _ => {
                                let t = self.infer(e, expected);
                                (self.resolve(&t), t)
                            }
                        };
                        // As for a statement (`Stmt::Expr`): a value of
                        // type `Never` (`fail()`) ends the path, so the
                        // branch doesn't leave a deferred `let` unassigned.
                        let structural =
                            matches!(e, ast::Expr::IfExpr(_) | ast::Expr::MatchExpr(_));
                        if !structural && matches!(t, Ty::Never) {
                            self.flow.reachable = false;
                        }
                        value
                    }
                    ast::Stmt::Return(r) if self.fcx_ref().is_lambda => {
                        self.stmt(stmt);
                        match r.syntax().children().find_map(ast::Expr::cast) {
                            Some(_) => expected.cloned().unwrap_or(Ty::Error),
                            None => builtin(self.cx.b.void),
                        }
                    }
                    _ => {
                        self.stmt(stmt);
                        if self.flow.reachable {
                            builtin(self.cx.b.void)
                        } else {
                            Ty::Never
                        }
                    }
                };
            } else {
                self.stmt(stmt);
            }
        }
        if stmts.is_empty() {
            if let (Some(exp), true) = (expected, report) {
                let void = builtin(self.cx.b.void);
                let _ = self.assignable(&void, exp, None);
            }
        }
        self.pop_scope();
        ty
    }

    /// Checks one statement, dispatching on its kind.
    fn stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Let(l) => self.let_stmt(l.syntax()),
            ast::Stmt::Assign(a) => self.assign_stmt(a.syntax()),
            ast::Stmt::Return(r) => self.return_stmt(r.syntax()),
            ast::Stmt::Throw(t) => self.throw_stmt(t.syntax()),
            ast::Stmt::Break(b) => self.jump(b.syntax()),
            ast::Stmt::Continue(c) => self.jump(c.syntax()),
            ast::Stmt::Guard(g) => self.guard_stmt(g.syntax()),
            ast::Stmt::While(w) => self.while_stmt(w.syntax()),
            ast::Stmt::For(f) => self.for_stmt(f.syntax()),
            ast::Stmt::Do(d) => self.do_stmt(d.syntax()),
            ast::Stmt::Expect(e) => {
                if let Some(expr) = e.syntax().children().find_map(ast::Expr::cast) {
                    let t = self.infer(&expr, Some(&builtin(self.cx.b.bool_)));
                    let t = self.resolve(&t);
                    if !t.contains_error() && !self.is_bool(&t) {
                        self.err(
                            codes::EXPECT_NOT_BOOL,
                            expr.syntax().text_range(),
                            format!("`expect` takes a `Bool`, found `{}`", t.render(self.cx.p)),
                        );
                    }
                }
            }
            ast::Stmt::ExpectThrows(e) => self.expect_throws(e.syntax()),
            ast::Stmt::Expr(e) => {
                let t = self.infer(e, None);
                let t = self.resolve(&t);
                self.discarded_timer(e, &t);
                // `if` and `match` compute their flow structurally; any other
                // expression of type `Never` (or `{error}`, which might be)
                // ends the path.
                let structural = matches!(e, ast::Expr::IfExpr(_) | ast::Expr::MatchExpr(_));
                if !structural && matches!(t, Ty::Never | Ty::Error) {
                    self.flow.reachable = false;
                }
            }
        }
    }

    /// Checks a `let` or `var` statement: the initializer is checked against
    /// the annotation (or inferred) and the name is bound in the current
    /// scope, as a deferred binding when there is no initializer.
    fn let_stmt(&mut self, node: &SyntaxNode) {
        let Some(name) = token(node, SyntaxKind::Ident) else {
            if let Some(e) = node.children().find_map(ast::Expr::cast) {
                self.infer(&e, None);
            }
            return;
        };
        let keyword = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| matches!(t.kind(), SyntaxKind::LetKw | SyntaxKind::VarKw));
        let mutable = keyword
            .as_ref()
            .is_some_and(|k| k.kind() == SyntaxKind::VarKw);
        let annotated = node
            .children()
            .find_map(ast::Type::cast)
            .map(|t| self.cx.resolve_type(self.file(), &self.owner.generics, &t));
        let init = node.children().find_map(ast::Expr::cast);
        let ty = match (&annotated, &init) {
            (Some(t), Some(e)) => {
                self.check(e, t);
                t.clone()
            }
            (Some(t), None) => t.clone(),
            (None, Some(e)) => {
                let t = self.infer(e, None);
                let t = self.resolve(&t);
                self.report_unknown(e, &t);
                t
            }
            (None, None) => Ty::Error,
        };
        let ty = self.resolve(&ty);
        self.note_type(name.text_range(), &ty);
        self.add_local(Local {
            name: name.text().to_owned(),
            ty,
            def: name.text_range(),
            is_param: false,
            mutable,
            keyword: keyword.map(|k| k.text_range()),
            deferred: init.is_none(),
            used: false,
            reported: false,
        });
    }

    /// Checks an assignment (`=` or a compound `+=`, `-=`, `*=`, `/=`, `%=`).
    ///
    /// The value is checked against the target's type. A plain assignment to
    /// a deferred `let` is its initialization (K2011 when it may already be
    /// assigned); any other target must be writable (`var`, a writable
    /// member) and not written while a `@Derived` value is computed.
    fn assign_stmt(&mut self, node: &SyntaxNode) {
        let mut exprs = node.children().filter_map(ast::Expr::cast);
        let lhs = exprs.next();
        let rhs = exprs.next();
        let op = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::Eq
                        | SyntaxKind::PlusEq
                        | SyntaxKind::MinusEq
                        | SyntaxKind::StarEq
                        | SyntaxKind::SlashEq
                        | SyntaxKind::PercentEq
                )
            });
        let compound = op.as_ref().is_some_and(|o| o.kind() != SyntaxKind::Eq);
        let Some(lhs) = lhs else {
            if let Some(rhs) = rhs {
                self.infer(&rhs, None);
            }
            return;
        };
        // A plain assignment to a deferred local is a write, not a read.
        let mut deferred_target = None;
        if let ast::Expr::NameExpr(n) = &lhs {
            if let Some(name) = n.name() {
                if let Some(id) = self.find_local(name.text()) {
                    if self.locals[id].deferred && !compound {
                        deferred_target = Some(id);
                    }
                }
            }
        }
        let lhs_ty = if let Some(id) = deferred_target {
            let range = lhs.syntax().text_range();
            self.locals[id].used = true;
            let local = &self.locals[id];
            let resolution = if local.is_param {
                Resolution::Parameter {
                    name: local.name.clone(),
                    definition: self.span(local.def),
                }
            } else {
                Resolution::Local {
                    name: local.name.clone(),
                    definition: self.span(local.def),
                }
            };
            self.note_res(range, resolution);
            self.targets.insert(range, Target::Local(id));
            self.locals[id].ty.clone()
        } else {
            self.infer(&lhs, None)
        };
        if let Some(rhs) = &rhs {
            if compound {
                let r = self.infer(rhs, Some(&lhs_ty));
                let op_kind = op.as_ref().map(SyntaxToken::kind);
                let result = self.arith(op_kind, &lhs_ty, &r, node.text_range(), rhs, &lhs);
                if !result.is_error() {
                    let _ = self.assignable(&result, &lhs_ty, None);
                }
            } else {
                self.check(rhs, &lhs_ty);
            }
        }
        if let Some(id) = deferred_target {
            let local = &self.locals[id];
            if !local.mutable && self.flow.maybe.contains(&id) && self.flow.reachable {
                let range = lhs.syntax().text_range();
                self.err(
                    codes::ASSIGNMENT_TO_LET,
                    range,
                    format!("`{}` is a `let` and is already assigned", local.name),
                );
            }
            if !self.flow.init.contains(&id) {
                self.flow.init.push(id);
            }
            if !self.flow.maybe.contains(&id) {
                self.flow.maybe.push(id);
            }
            return;
        }
        if self.derived_write(&lhs, node.text_range()) {
            return;
        }
        self.check_mutable(&lhs);
    }

    /// Reports K2011 when `place` can't be assigned or mutated.
    fn check_mutable(&mut self, place: &ast::Expr) {
        let range = place.syntax().text_range();
        let mut current = place.clone();
        loop {
            match &current {
                ast::Expr::ParenExpr(p) => {
                    let Some(inner) = p.syntax().children().find_map(ast::Expr::cast) else {
                        return;
                    };
                    current = inner;
                }
                ast::Expr::NameExpr(n) => {
                    let target = self.targets.get(&n.syntax().text_range()).copied();
                    match target {
                        Some(Target::Local(id)) => {
                            let local = self.locals[id].clone();
                            if local.mutable || local.deferred {
                                return;
                            }
                            let mut d = Diagnostic::new(
                                codes::ASSIGNMENT_TO_LET,
                                self.span(range),
                                format!("cannot assign through `{}`: it is a `let`", local.name),
                            );
                            if let Some(kw) = local.keyword {
                                d = d
                                    .with_label(self.span(kw), "declared with `let` here")
                                    .with_fix(
                                        Fix::replace("Change `let` to `var`", self.span(kw), "var")
                                            .maybe_incorrect(),
                                    );
                            }
                            self.report(d);
                            return;
                        }
                        Some(Target::Member(m)) => {
                            self.member_mutable(m, range, true);
                            return;
                        }
                        Some(Target::Decl(d)) => {
                            let decl = self.cx.p.decl(d);
                            if decl.named_kind() == Some(DeclarationKind::Constant) {
                                let kw = token(&decl.node, SyntaxKind::LetKw);
                                let mut diag = Diagnostic::new(
                                    codes::ASSIGNMENT_TO_LET,
                                    self.span(range),
                                    format!("cannot assign to `{}`: it is a `let`", decl.name),
                                );
                                if let Some(kw) = kw {
                                    diag = diag.with_label(
                                        self.cx.p.span(decl.file, kw.text_range()),
                                        "declared with `let` here",
                                    );
                                }
                                self.report(diag);
                            }
                            return;
                        }
                        None => return,
                    }
                }
                ast::Expr::MemberExpr(m) => {
                    let Some(name) = m.name() else { return };
                    let Some(Target::Member(member)) =
                        self.targets.get(&name.text_range()).copied()
                    else {
                        return;
                    };
                    if !self.member_mutable(member, range, false) {
                        return;
                    }
                    let Some(recv) = m.receiver() else { return };
                    // Through a class reference, the receiver's binding
                    // doesn't matter.
                    let recv_ty = self.type_of_node(recv.syntax().text_range());
                    if let Some(d) = recv_ty.as_ref().and_then(|t| self.deopt(t).decl()) {
                        if self.cx.is_class(d) {
                            return;
                        }
                    }
                    if matches!(recv_ty, Some(Ty::Meta(_))) {
                        return;
                    }
                    if let ast::Expr::NameExpr(n) = &recv {
                        if n.syntax()
                            .first_token()
                            .is_some_and(|t| t.kind() == SyntaxKind::SelfKw)
                        {
                            return;
                        }
                    }
                    current = recv;
                }
                _ => return,
            }
        }
    }

    /// Checks one member link of a place; returns `true` to keep walking.
    fn member_mutable(&mut self, member: MemberId, range: TextRange, unqualified: bool) -> bool {
        let m = self.cx.p.member(member);
        let file = self.cx.p.decl(m.owner).file;
        if m.kind == MemberKind::Field && !m.is_static {
            // Only a service's own methods write its fields (K4006).
            let owner_kind = self.cx.p.decl(m.owner).named_kind();
            if owner_kind == Some(DeclarationKind::Service) && self.owner.decl != Some(m.owner) {
                self.report(
                    Diagnostic::new(
                        codes::FOREIGN_STATE_WRITE,
                        self.span(range),
                        format!(
                            "only `{}`'s own methods write its field `{}`",
                            self.cx.p.decl(m.owner).name,
                            m.name
                        ),
                    )
                    .with_label(self.cx.p.span(file, m.name_range), "declared here"),
                );
                return false;
            }
            if m.has_annotation("Prop") && !m.is_var {
                let mut d = Diagnostic::new(
                    codes::WRITE_TO_PROP,
                    self.span(range),
                    format!("`{}` is a `@Prop`: the parent owns it", m.name),
                )
                .with_label(self.cx.p.span(file, m.name_range), "declared `@Prop` here");
                let annotation = m.node.as_ref().and_then(|n| {
                    n.children()
                        .filter_map(ast::Annotation::cast)
                        .find(|a| a.name().is_some_and(|t| t.text() == "Prop"))
                });
                if let (Some(a), Some(kw)) = (annotation, m.keyword_range) {
                    let start = a.syntax().text_range().start();
                    let end = a
                        .name()
                        .map_or(a.syntax().text_range().end(), |t| t.text_range().end());
                    let mut fix = Fix::replace(
                        "Change `@Prop let` to `@Binding var`",
                        self.cx.p.span(file, TextRange::new(start, end)),
                        "@Binding",
                    )
                    .maybe_incorrect();
                    fix.edits.push(k_diagnostics::TextEdit {
                        span: self.cx.p.span(file, kw),
                        replacement: "var".to_owned(),
                    });
                    d = d.with_fix(fix);
                }
                self.report(d);
                return false;
            }
            if m.has_annotation("Derived") && !m.is_var {
                self.report(
                    Diagnostic::new(
                        codes::WRITE_TO_DERIVED,
                        self.span(range),
                        format!("`{}` is `@Derived`: it is computed, never assigned", m.name),
                    )
                    .with_label(
                        self.cx.p.span(file, m.name_range),
                        "declared `@Derived` here",
                    ),
                );
                return false;
            }
            if (m.has_annotation("Prop") || m.has_annotation("Derived")) && m.is_var {
                // A wrongly declared `@Prop var` is reported once (K3001).
                return false;
            }
        }
        if m.kind == MemberKind::Field && !m.is_var {
            let in_init = self.fns.iter().any(|f| f.init_of == Some(m.owner));
            if in_init && !m.is_static {
                return false;
            }
            let mut d = Diagnostic::new(
                codes::ASSIGNMENT_TO_LET,
                self.span(range),
                format!("cannot assign to `{}`: it is a `let`", m.name),
            );
            if let Some(kw) = m.keyword_range {
                let file = self.cx.p.decl(m.owner).file;
                d = d.with_label(self.cx.p.span(file, kw), "declared with `let` here");
            }
            self.report(d);
            return false;
        }
        !unqualified
    }

    /// Returns the latest type inferred for the expression at `range`,
    /// resolved; `None` if it was not inferred.
    fn type_of_node(&self, range: TextRange) -> Option<Ty> {
        self.last_types.get(&range).map(|t| self.resolve(t))
    }

    /// Checks a `return` statement against the function's return type and
    /// makes the rest of the block unreachable; in a lambda it is K2308 (with
    /// a fix when it is the lambda's last statement).
    fn return_stmt(&mut self, node: &SyntaxNode) {
        let value = node.children().find_map(ast::Expr::cast);
        if self.fcx_ref().is_lambda {
            let kw =
                token(node, SyntaxKind::ReturnKw).map_or(node.text_range(), |t| t.text_range());
            let is_last = node
                .next_sibling()
                .is_none_or(|s| !ast::Stmt::can_cast(s.kind()))
                && node
                    .parent()
                    .is_some_and(|p| p.kind() == SyntaxKind::LambdaExpr);
            let mut d = Diagnostic::new(
                codes::RETURN_IN_LAMBDA,
                self.span(kw),
                "`return` inside a lambda: a lambda's value is its last expression",
            );
            if is_last && value.is_some() {
                let end = value
                    .as_ref()
                    .map_or(kw.end(), |v| v.syntax().text_range().start());
                d = d.with_fix(Fix::replace(
                    "Remove `return `",
                    self.span(TextRange::new(kw.start(), end)),
                    "",
                ));
            }
            self.report(d);
            if let Some(v) = &value {
                let ret = self.fcx_ref().ret.clone();
                match ret {
                    Some(r) => {
                        self.check(v, &r);
                    }
                    None => {
                        self.infer(v, None);
                    }
                }
            }
            self.flow.reachable = false;
            return;
        }
        let ret = self.fcx_ref().ret.clone();
        if let Some(v) = &value {
            match &ret {
                Some(r) if !self.is_void(r) => {
                    self.check(v, r);
                }
                _ => {
                    self.infer(v, None);
                }
            }
        }
        self.flow.reachable = false;
    }

    /// Checks a `throw` statement: infers the error (with the function's
    /// declared error type as a hint), sends it to its handler and makes the
    /// rest of the block unreachable.
    ///
    /// A protocol handler throwing `rpcFailure` itself is K5506.
    fn throw_stmt(&mut self, node: &SyntaxNode) {
        let Some(value) = node.children().find_map(ast::Expr::cast) else {
            self.flow.reachable = false;
            return;
        };
        let hint = self
            .fns
            .iter()
            .rev()
            .find_map(|f| f.throws.clone().filter(|t| !t.is_error()));
        let ty = self.infer(&value, hint.as_ref());
        let ty = self.resolve(&ty);
        if !ty.contains_error() && self.in_rpc_handler() && constructs_case(&value, "rpcFailure") {
            self.err(
                codes::HANDLER_THROWS_RPC_FAILURE,
                node.text_range(),
                "only the runtime produces `rpcFailure`; a handler throws its own cases",
            );
        }
        if !ty.contains_error() {
            self.throw_error(ty, Vec::new(), node.text_range(), false);
        } else if let Some(d) = self.fcx().dos.last_mut() {
            d.saw_unknown = true;
        }
        self.flow.reachable = false;
    }

    /// Returns `true` inside the body of a protocol handler: an `rpc`
    /// member with a body (in the service implementing the protocol).
    fn in_rpc_handler(&self) -> bool {
        self.current_fn.is_some_and(|m| {
            let member = self.cx.p.member(m);
            member.kind == MemberKind::Rpc
                && member
                    .node
                    .as_ref()
                    .is_some_and(|n| n.children().any(|c| c.kind() == SyntaxKind::Block))
        })
    }

    /// Sends a thrown error to its handler (sema.md §10).
    fn throw_error(&mut self, ty: Ty, covered: Vec<Vec<Pat>>, at: TextRange, from_try: bool) {
        let fcx = self.fcx();
        if from_try {
            if let Some(t) = fcx.tries.last() {
                if t.has_else {
                    return;
                }
            }
        }
        if let Some(d) = fcx.dos.last_mut() {
            if d.accepts_any {
                // `expect throws`: record what the block can throw (K7502).
                d.thrown.push(Thrown { ty, covered });
                return;
            }
            d.thrown.push(Thrown { ty, covered });
            return;
        }
        if let Some(lt) = fcx.lambda_throws.clone() {
            let lt_resolved = self.resolve(&lt);
            if let Ty::Var(v) = lt_resolved {
                self.bind(v, ty, Some(at));
                return;
            }
            if self.unify(&lt_resolved, &ty, None) {
                return;
            }
            self.unhandled(&ty, at, from_try);
            return;
        }
        match fcx.throws.clone() {
            Some(Ty::Error) => {
                if self.initializer_throws && self.fns.len() == 1 && !ty.contains_error() {
                    self.escaped.push(ty);
                }
            }
            Some(t) => {
                let t = self.resolve(&t);
                if t != ty && !self.unify(&t, &ty, None) {
                    self.unhandled(&ty, at, from_try);
                }
            }
            None => self.unhandled(&ty, at, from_try),
        }
    }

    /// The fix of K2003/K2008: inserts `label: <#Type#>` for each missing
    /// parameter at `at` (just before `)`, or after the callee when the
    /// call has no argument list, which the fix then adds).
    ///
    /// Placeholders use the `<#…#>` form editors render as tab stops; the
    /// fix is maybe-incorrect, since the values are the user's to choose.
    fn missing_args_fix(&self, call: &SyntaxNode, at: TextSize, missing: &[&SigParam]) -> Fix {
        let source = crate::ty::SourceNames(self.cx.p);
        let parts: Vec<String> = missing
            .iter()
            .map(|p| {
                let placeholder = format!("<#{}#>", p.ty.render(&source));
                match &p.label {
                    Some(label) => format!("{label}: {placeholder}"),
                    None => placeholder,
                }
            })
            .collect();
        let list = call.children().find(|c| c.kind() == SyntaxKind::ArgList);
        let text = match &list {
            None => format!("({})", parts.join(", ")),
            Some(list) => {
                // What precedes the insertion point inside the list.
                let before = list
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| !t.kind().is_trivia() && t.text_range().end() <= at)
                    .last()
                    .map(|t| t.kind());
                let has_args = list.children().next().is_some();
                let sep = match before {
                    Some(SyntaxKind::Comma) => " ",
                    _ if has_args => ", ",
                    _ => "",
                };
                format!("{sep}{}", parts.join(", "))
            }
        };
        Fix::replace(
            "Insert the missing arguments",
            Span::empty(self.span(call.text_range()).file, at),
            text,
        )
        .maybe_incorrect()
    }

    /// Returns `true` if an error of type `ty` thrown here would reach a
    /// handler: an enclosing `do`/`catch`, a lambda whose error type is
    /// still being inferred, or a `throws` clause that names it (or accepts
    /// any error).
    ///
    /// A conservative estimate without side effects (no inference variable
    /// is bound): it decides only whether the K2201 fix is
    /// machine-applicable, never whether an error is reported.
    fn error_can_propagate(&self, ty: &Ty) -> bool {
        let fcx = self.fcx_ref();
        if !fcx.dos.is_empty() {
            return true;
        }
        let accepts = |t: &Ty| {
            let t = self.resolve(t);
            matches!(t, Ty::Error | Ty::Var(_) | Ty::Param(_)) || t == *ty
        };
        if let Some(lt) = &fcx.lambda_throws {
            return accepts(lt);
        }
        fcx.throws.as_ref().is_some_and(accepts)
    }

    /// Reports K2203: an error of type `ty` thrown at `at` is neither caught
    /// nor declared, with labels and a fix pointing at the signature.
    ///
    /// With `from_try`, a `try` reports at most once.
    fn unhandled(&mut self, ty: &Ty, at: TextRange, from_try: bool) {
        if from_try {
            if let Some(t) = self.fcx().tries.last_mut() {
                if t.reported {
                    return;
                }
                t.reported = true;
            }
        }
        let rendered = ty.render(self.cx.p);
        let mut d = Diagnostic::new(
            codes::UNHANDLED_ERROR,
            self.span(at),
            format!("`{rendered}` is neither caught nor declared in `throws`"),
        );
        let points = self.fcx_ref().decl_node.as_ref().and_then(SigPoints::of);
        if let Some(points) = points {
            if points.has_throws {
                d = d.with_label(self.span(points.name), "declared here");
            } else {
                let source = ty.render(&crate::ty::SourceNames(self.cx.p));
                d = d
                    .with_label(
                        self.span(points.name),
                        format!("declared here without `throws {rendered}`"),
                    )
                    .with_fix(
                        Fix::replace(
                            format!("Add `throws {source}`"),
                            self.span(TextRange::empty(points.after_effects)),
                            format!(" throws {source}"),
                        )
                        .maybe_incorrect(),
                    );
            }
        }
        self.report(d);
    }

    /// Checks a `break` or `continue` (K2309 outside a loop) and makes the
    /// rest of the block unreachable.
    fn jump(&mut self, node: &SyntaxNode) {
        if self.fcx_ref().loops == 0 {
            let kw = node
                .first_token()
                .map_or(node.text_range(), |t| t.text_range());
            let what = if node.kind() == SyntaxKind::BreakStmt {
                "break"
            } else {
                "continue"
            };
            self.err(
                codes::MISPLACED_JUMP,
                kw,
                format!("`{what}` outside a loop"),
            );
        }
        self.flow.reachable = false;
    }

    /// Checks a condition list, binding `let` conditions in the current
    /// scope.
    fn conditions(&mut self, node: &SyntaxNode) {
        let bool_ty = builtin(self.cx.b.bool_);
        for child in node.children() {
            if matches!(child.kind(), SyntaxKind::Block | SyntaxKind::KeyClause) {
                break;
            }
            if child.kind() == SyntaxKind::LetCondition {
                let Some(name) = token(&child, SyntaxKind::Ident) else {
                    continue;
                };
                let value_ty = match child.children().find_map(ast::Expr::cast) {
                    Some(e) => self.infer(&e, None),
                    None => {
                        // `if let x` is `if let x = x`.
                        self.name_value(name.text(), name.text_range(), false)
                    }
                };
                let value_ty = self.resolve(&value_ty);
                let bound = match &value_ty {
                    Ty::Optional(inner) => (**inner).clone(),
                    Ty::Error => Ty::Error,
                    other => other.clone(),
                };
                self.note_type(name.text_range(), &bound);
                self.add_local(Local {
                    name: name.text().to_owned(),
                    ty: bound,
                    def: name.text_range(),
                    is_param: false,
                    mutable: false,
                    keyword: None,
                    deferred: false,
                    used: false,
                    reported: false,
                });
            } else if let Some(e) = ast::Expr::cast(child.clone()) {
                self.check(&e, &bool_ty);
            }
        }
    }

    /// Checks a `guard … else { … }` statement: the conditions' bindings are
    /// visible after the statement but not in `else`, and the `else` block
    /// must exit (K2305).
    fn guard_stmt(&mut self, node: &SyntaxNode) {
        // The conditions bind in the enclosing scope, but not in `else`.
        let else_block = node.children().find_map(ast::Block::cast);
        let mark = self.scopes.last().map_or(0, Vec::len);
        self.conditions(node);
        let bound: Vec<(String, LocalId)> = self
            .scopes
            .last_mut()
            .map(|s| s.split_off(mark))
            .unwrap_or_default();
        let before = self.flow.clone();
        if let Some(block) = else_block {
            self.block_stmts(&block);
            if self.flow.reachable {
                if let Some(kw) = token(node, SyntaxKind::ElseKw) {
                    self.err(
                        codes::GUARD_DOES_NOT_EXIT,
                        kw.text_range(),
                        "the `else` block of a `guard` must exit",
                    );
                }
            }
        }
        self.flow = before;
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(bound);
        }
    }

    /// Checks a `while` loop: its conditions bind in the loop's scope, and
    /// after the loop the flow is the state before it, reachable, plus what
    /// the body possibly assigned.
    fn while_stmt(&mut self, node: &SyntaxNode) {
        self.push_scope();
        self.conditions(node);
        let before = self.flow.clone();
        if let Some(block) = node.children().find_map(ast::Block::cast) {
            self.fcx().loops += 1;
            self.block_stmts(&block);
            self.fcx().loops -= 1;
        }
        let maybe = self.flow.maybe.clone();
        self.flow = before;
        self.flow.reachable = true;
        for m in maybe {
            if !self.flow.maybe.contains(&m) {
                self.flow.maybe.push(m);
            }
        }
        self.pop_scope();
    }

    /// Checks a `for` (or `for await`) loop over a list, set, range, map or
    /// stream.
    ///
    /// A `Stream` needs `for await` and a collection must not use it; loop
    /// variables over a bindable list are themselves bindable. Flow after the
    /// loop is as for [`Checker::while_stmt`].
    fn for_stmt(&mut self, node: &SyntaxNode) {
        let is_await = has_token(node, SyntaxKind::AwaitKw);
        let pattern = node.children().find_map(ast::Pattern::cast);
        let iterable = node.children().find_map(ast::Expr::cast);
        let in_view = self.view_on;
        self.view_on = false;
        let iter_ty = iterable.as_ref().map_or(Ty::Error, |e| self.infer(e, None));
        self.view_on = in_view;
        let iter_ty = self.resolve(&iter_ty);
        let b = &self.cx.b;
        let element = match &iter_ty {
            Ty::Named(d, args)
                if [b.list, b.set, b.half_open, b.closed, b.stream].contains(&Some(*d)) =>
            {
                args.first().cloned().unwrap_or(Ty::Error)
            }
            _ => Ty::Error,
        };
        let is_stream = iter_ty.decl().is_some() && iter_ty.decl() == b.stream;
        let is_collection = matches!(&iter_ty, Ty::Named(d, _) if [b.list, b.set, b.half_open, b.closed, b.map].contains(&Some(*d)));
        let in_async = self.fcx_ref().is_async;
        if is_stream && !is_await {
            if let Some(kw) = token(node, SyntaxKind::ForKw) {
                let mut d = Diagnostic::new(
                    codes::FOR_OVER_STREAM,
                    self.span(kw.text_range()),
                    "a `Stream` is read with `for await`",
                );
                let fix = Fix::replace(
                    "Insert `await ` after `for`",
                    Span::empty(
                        self.span(kw.text_range()).file,
                        kw.text_range().end() + TextSize::from(1),
                    ),
                    "await ",
                );
                d = d.with_fix(if in_async { fix } else { fix.maybe_incorrect() });
                self.report(d);
            }
        } else if is_await {
            if let Some(kw) = token(node, SyntaxKind::AwaitKw) {
                if is_collection {
                    let end = kw.text_range().end() + TextSize::from(1);
                    let d = Diagnostic::new(
                        codes::FOR_AWAIT_NOT_STREAM,
                        self.span(kw.text_range()),
                        "`for await` reads a `Stream`, not a collection",
                    )
                    .with_fix(Fix::replace(
                        "Remove `await `",
                        self.span(TextRange::new(kw.text_range().start(), end)),
                        "",
                    ));
                    self.report(d);
                } else if is_stream && !in_async {
                    self.err(
                        codes::FOR_AWAIT_NOT_STREAM,
                        kw.text_range(),
                        "`for await` is allowed only in an async function",
                    );
                }
            }
        }
        self.push_scope();
        let first_local = self.locals.len();
        if let Some(p) = &pattern {
            self.bind_for_pattern(p, &element);
        }
        if iterable.as_ref().is_some_and(|e| self.bindable(e)) {
            for id in first_local..self.locals.len() {
                self.bindable_locals.insert(id);
            }
        }
        self.for_key(node, iterable.as_ref(), pattern.as_ref(), &element, in_view);
        let before = self.flow.clone();
        if let Some(block) = node.children().find_map(ast::Block::cast) {
            self.fcx().loops += 1;
            self.block_stmts(&block);
            self.fcx().loops -= 1;
        }
        let maybe = self.flow.maybe.clone();
        self.flow = before;
        self.flow.reachable = true;
        for m in maybe {
            if !self.flow.maybe.contains(&m) {
                self.flow.maybe.push(m);
            }
        }
        self.pop_scope();
    }

    /// The `key` clause of a `for`: required in views (K3106), typed and
    /// `Hashable` there (K2009), not allowed elsewhere (K3111).
    fn for_key(
        &mut self,
        node: &SyntaxNode,
        iterable: Option<&ast::Expr>,
        pattern: Option<&ast::Pattern>,
        element: &Ty,
        in_view: bool,
    ) {
        let clause = node.children().find(|c| c.kind() == SyntaxKind::KeyClause);
        match clause {
            Some(clause) => {
                let key = clause.children().find_map(ast::Expr::cast);
                if in_view {
                    let Some(key) = key else { return };
                    self.view_on = false;
                    let t = self.infer(&key, None);
                    self.view_on = in_view;
                    let t = self.resolve(&t);
                    if t.contains_error() {
                        return;
                    }
                    if let Some(h) = self.cx.b.hashable {
                        if !self.cx.implements(&t, h) {
                            self.err(
                                codes::UNSATISFIED_CONSTRAINT,
                                key.syntax().text_range(),
                                format!(
                                    "a `key` must be `Hashable`; `{}` isn't",
                                    t.render(self.cx.p)
                                ),
                            );
                        }
                    }
                    return;
                }
                if let Some(key) = &key {
                    self.infer(key, None);
                }
                let range = clause.text_range();
                let text = self.cx.p.file(self.file()).text;
                let start = usize::from(range.start());
                let trimmed = text[..start].trim_end_matches([' ', '\t']).len();
                let removal = TextRange::new(
                    TextSize::from(u32::try_from(trimmed).unwrap_or(0)),
                    range.end(),
                );
                self.report(
                    Diagnostic::new(
                        codes::KEY_OUTSIDE_VIEW,
                        self.span(range),
                        "`key` belongs to loops in views",
                    )
                    .with_fix(Fix::replace(
                        "Remove the `key` clause",
                        self.span(removal),
                        "",
                    )),
                );
            }
            None if in_view => {
                let Some(iterable) = iterable else { return };
                let at = iterable.syntax().text_range().end();
                let mut d = Diagnostic::new(
                    codes::MISSING_FOR_KEY,
                    Span::empty(self.span(at_range(at)).file, at),
                    "a `for` in a view needs a `key` that identifies each item",
                );
                let var = pattern.and_then(|p| match p {
                    ast::Pattern::IdentPattern(i) => token(i.syntax(), SyntaxKind::Ident),
                    _ => None,
                });
                if let Some(var) = var {
                    let elem = self.resolve(element);
                    let has_id = elem.decl().is_some_and(|d| {
                        self.cx.p.decl(d).member_names.get("id").is_some_and(|m| {
                            matches!(
                                self.cx.p.member(*m).kind,
                                MemberKind::Field | MemberKind::Computed
                            )
                        })
                    });
                    if has_id {
                        let text = format!(" key {}.id", var.text());
                        d = d.with_fix(
                            Fix::replace(
                                format!("Insert `{text}`"),
                                Span::empty(self.span(at_range(at)).file, at),
                                text,
                            )
                            .maybe_incorrect(),
                        );
                    }
                }
                self.report(d);
            }
            None => {}
        }
    }

    /// Binds the loop variables of a `for` pattern (a name, a tuple of
    /// patterns or a binding) to the element type `ty`, as immutable locals of
    /// the current scope; other patterns bind nothing.
    fn bind_for_pattern(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        match pattern {
            ast::Pattern::IdentPattern(p) => {
                if let Some(name) = token(p.syntax(), SyntaxKind::Ident) {
                    self.note_type(name.text_range(), ty);
                    self.add_local(Local {
                        name: name.text().to_owned(),
                        ty: ty.clone(),
                        def: name.text_range(),
                        is_param: false,
                        mutable: false,
                        keyword: None,
                        deferred: false,
                        used: false,
                        reported: false,
                    });
                }
            }
            ast::Pattern::TuplePattern(t) => {
                let items: Vec<ast::Pattern> = t
                    .syntax()
                    .children()
                    .filter_map(ast::Pattern::cast)
                    .collect();
                let tys: Vec<Ty> = match self.resolve(ty) {
                    Ty::Tuple(ts) => ts.to_vec(),
                    _ => vec![Ty::Error; items.len()],
                };
                for (i, item) in items.iter().enumerate() {
                    let t = tys.get(i).cloned().unwrap_or(Ty::Error);
                    self.bind_for_pattern(item, &t);
                }
            }
            ast::Pattern::BindingPattern(p) => {
                if let Some(name) = token(p.syntax(), SyntaxKind::Ident) {
                    self.add_local(Local {
                        name: name.text().to_owned(),
                        ty: ty.clone(),
                        def: name.text_range(),
                        is_param: false,
                        mutable: false,
                        keyword: None,
                        deferred: false,
                        used: false,
                        reported: false,
                    });
                }
            }
            _ => {}
        }
    }

    /// Checks an `expect throws [Error.case] { … }` test statement.
    ///
    /// The block records what it can throw instead of propagating it, and is
    /// K7502 when it can throw neither the named error nor anything of
    /// unknown type. Flow after the statement is the state before the block.
    fn expect_throws(&mut self, node: &SyntaxNode) {
        let mut expected_error: Option<DeclId> = None;
        if let Some(p) = node.children().find_map(ast::Pattern::cast) {
            if let ast::Pattern::EnumCasePattern(e) = &p {
                // `expect throws CartError.outOfStock`: resolve the enum.
                let idents: Vec<SyntaxToken> = e
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind() == SyntaxKind::Ident)
                    .collect();
                if idents.len() >= 2 {
                    if let Found::Decl(d) = self.cx.lookup(self.file(), idents[0].text()) {
                        expected_error = Some(d);
                        self.note_res(idents[0].text_range(), self.cx.decl_resolution(d));
                        let ty = Ty::named(d);
                        self.quiet += 1;
                        let mut binds = Vec::new();
                        let _ = self.pattern(&p, &ty, &mut binds);
                        self.quiet -= 1;
                    }
                }
            }
        }
        if let Some(block) = node.children().find_map(ast::Block::cast) {
            self.fcx().dos.push(DoCx {
                accepts_any: true,
                ..DoCx::default()
            });
            let before = self.flow.clone();
            self.block_stmts(&block);
            self.flow = before;
            let done = self.fcx().dos.pop().unwrap_or_default();
            // K7502: the block must be able to throw the expected error.
            let can_throw = done.thrown.iter().any(|t| {
                let t = self.resolve(&t.ty);
                t.contains_error() || expected_error.is_none_or(|e| t.decl() == Some(e))
            });
            if !can_throw && !done.saw_unknown {
                if let Some(kw) = token(node, SyntaxKind::ExpectKw) {
                    self.err(
                        codes::EXPECT_THROWS_CANNOT_THROW,
                        kw.text_range(),
                        "this block can't throw the expected error",
                    );
                }
            }
        }
    }

    /// Checks a `do { … } catch … { … }` statement.
    ///
    /// Errors thrown in the block are grouped by type and matched against the
    /// `catch` clauses: a clause that can never run is K2302, an unused
    /// catch-all binding K2304, and whatever the clauses do not cover
    /// propagates to the enclosing handler. Exhaustiveness is skipped when the
    /// block has syntax errors or threw something of unknown type.
    fn do_stmt(&mut self, node: &SyntaxNode) {
        let Some(block) = node.children().find_map(ast::Block::cast) else {
            return;
        };
        let damaged = block
            .syntax()
            .descendants()
            .any(|n| n.kind() == SyntaxKind::ErrorNode)
            || node
                .descendants_with_tokens()
                .any(|t| t.kind() == SyntaxKind::Error);
        let entry = self.flow.clone();
        self.fcx().dos.push(DoCx::default());
        self.block_stmts(&block);
        let after_do = self.flow.clone();
        let mut docx = self.fcx().dos.pop().unwrap_or_default();
        let clauses: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::CatchClause)
            .collect();
        let mut branches = vec![after_do];
        // Group the thrown errors by type.
        let mut types: Vec<Ty> = Vec::new();
        for t in &docx.thrown {
            if !types.contains(&t.ty) {
                types.push(t.ty.clone());
            }
        }
        let do_kw = token(node, SyntaxKind::DoKw).map_or(node.text_range(), |t| t.text_range());
        let mut rows_per_type: Vec<pattern::Rows> = types
            .iter()
            .map(|ty| {
                let mut rows = pattern::Rows::new();
                for t in docx.thrown.iter().filter(|t| &t.ty == ty) {
                    for row in &t.covered {
                        rows.push(self, row.clone(), std::slice::from_ref(ty));
                    }
                }
                rows
            })
            .collect();
        let mut broken = damaged || docx.saw_unknown;
        let mut catch_all_seen = false;
        for clause in &clauses {
            let catch_kw =
                token(clause, SyntaxKind::CatchKw).map_or(clause.text_range(), |t| t.text_range());
            let patterns: Vec<ast::Pattern> =
                clause.children().filter_map(ast::Pattern::cast).collect();
            let guard = clause
                .children()
                .find(|c| c.kind() == SyntaxKind::MatchGuard);
            self.flow = entry.clone();
            self.push_scope();
            let mut binds = Vec::new();
            let mut clause_useful = false;
            let mut clause_rows: Vec<(usize, Pat)> = Vec::new();
            let mut catch_all_binding: Option<LocalId> = None;
            let is_catch_all = patterns.len() == 1
                && matches!(
                    patterns[0],
                    ast::Pattern::WildcardPattern(_) | ast::Pattern::BindingPattern(_)
                );
            for p in &patterns {
                // Which thrown type does the pattern apply to?
                let mut applied = false;
                if types.is_empty() {
                    let ty = Ty::Error;
                    let _ = self.pattern(p, &ty, &mut binds);
                    continue;
                }
                let target = if is_catch_all {
                    None
                } else {
                    types.iter().position(|t| self.pattern_fits(p, t))
                };
                match target {
                    Some(i) => {
                        let ty = types[i].clone();
                        match self.pattern(p, &ty, &mut binds) {
                            Some(pat) => {
                                clause_rows.push((i, pat));
                                applied = true;
                            }
                            None => broken = true,
                        }
                    }
                    None if is_catch_all => {
                        let ty = if types.len() == 1 {
                            types[0].clone()
                        } else {
                            builtin(self.cx.b.error)
                        };
                        let _ = self.pattern(p, &ty, &mut binds);
                        for i in 0..types.len() {
                            clause_rows.push((i, Pat::Wild));
                        }
                        applied = true;
                    }
                    None => {
                        let ty = types[0].clone();
                        let _ = self.pattern(p, &ty, &mut binds);
                        broken = true;
                    }
                }
                let _ = applied;
            }
            // Reachability.
            if !broken {
                if types.is_empty() {
                    clause_useful = false;
                } else {
                    for (i, pat) in &clause_rows {
                        let ty = types[*i].clone();
                        if rows_per_type[*i].useful(
                            self,
                            std::slice::from_ref(pat),
                            std::slice::from_ref(&ty),
                        ) {
                            clause_useful = true;
                        }
                    }
                }
                if catch_all_seen || !clause_useful {
                    let d = Diagnostic::new(
                        codes::UNREACHABLE_ARM,
                        self.span(catch_kw),
                        "this `catch` clause can never run",
                    )
                    .with_fix(self.remove_arm_fix(clause));
                    self.report(d);
                }
            }
            if guard.is_none() {
                for (i, pat) in clause_rows {
                    let ty = types[i].clone();
                    rows_per_type[i].push(self, vec![pat], std::slice::from_ref(&ty));
                }
                if is_catch_all {
                    catch_all_seen = true;
                }
            }
            for (name, ty, range) in binds {
                let id = self.add_local(Local {
                    name,
                    ty: ty.clone(),
                    def: range,
                    is_param: false,
                    mutable: false,
                    keyword: None,
                    deferred: false,
                    used: false,
                    reported: false,
                });
                self.note_type(range, &ty);
                if is_catch_all {
                    catch_all_binding = Some(id);
                }
            }
            if let Some(g) = &guard {
                if let Some(e) = g.children().find_map(ast::Expr::cast) {
                    self.check(&e, &builtin(self.cx.b.bool_));
                }
            }
            if let Some(b) = clause.children().find_map(ast::Block::cast) {
                self.block_stmts(&b);
            }
            if let Some(id) = catch_all_binding {
                if !self.locals[id].used && guard.is_none() && !broken {
                    let range = patterns[0].syntax().text_range();
                    let d = Diagnostic::new(
                        codes::UNUSED_CATCH_ALL_BINDING,
                        self.span(range),
                        "the catch-all binding is never used",
                    )
                    .with_fix(Fix::replace(
                        "Replace the pattern with `_`",
                        self.span(range),
                        "_",
                    ));
                    if catch_all_seen && rows_per_type.len() == types.len() {
                        self.report(d);
                    }
                }
            }
            self.pop_scope();
            branches.push(self.flow.clone());
        }
        // Exhaustiveness: what isn't caught propagates.
        if !broken {
            let mut missing = false;
            let mut patterns: Vec<String> = Vec::new();
            for (i, ty) in types.iter().enumerate() {
                let exhaustive =
                    !rows_per_type[i].useful(self, &[Pat::Wild], std::slice::from_ref(ty));
                if exhaustive {
                    continue;
                }
                if self.propagates(ty) {
                    let covered = rows_per_type[i].rows().to_vec();
                    self.throw_error(ty.clone(), covered, do_kw, false);
                } else {
                    missing = true;
                    for pat in rows_per_type[i].missing(self, ty) {
                        if !patterns.contains(&pat) {
                            patterns.push(pat);
                        }
                    }
                }
            }
            if missing {
                let mut d = Diagnostic::new(
                    codes::CATCH_NOT_EXHAUSTIVE,
                    self.span(do_kw),
                    "the `catch` clauses don't handle every error",
                )
                .with_note(missing_note(&patterns));
                if let Some(fix) = self.missing_catches_fix(node, &clauses, &patterns) {
                    d = d.with_fix(fix);
                }
                self.report(d);
            }
        } else {
            for (i, ty) in types.iter().enumerate() {
                let covered = rows_per_type
                    .get(i)
                    .map(|rows| rows.rows().to_vec())
                    .unwrap_or_default();
                if self.propagates(ty) {
                    self.throw_error(ty.clone(), covered, do_kw, false);
                }
            }
            if let Some(d) = self.fcx().dos.last_mut() {
                d.saw_unknown = true;
            }
        }
        docx.thrown.clear();
        self.flow = Flow::merge(&branches);
    }

    /// The indentation (leading spaces and tabs) of the line holding
    /// `offset`.
    fn line_indent(&self, offset: TextSize) -> &'a str {
        let text = self.cx.p.file(self.file()).text;
        let at = usize::from(offset).min(text.len());
        let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let rest = &text[start..];
        let len = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        &rest[..len]
    }

    /// The K2301 fix: inserts an empty arm per missing pattern (`default`
    /// for `_`) before the `match`'s closing `}`, indented like the first
    /// arm. Maybe-incorrect: the arms' bodies are the user's to write.
    fn missing_arms_fix(
        &self,
        node: &SyntaxNode,
        arms: &[SyntaxNode],
        missing: &[String],
    ) -> Option<Fix> {
        let close = token(node, SyntaxKind::RBrace)?.text_range().start();
        let indent = match arms.first() {
            Some(arm) => self.line_indent(arm.text_range().start()).to_owned(),
            None => format!("{}    ", self.line_indent(node.text_range().start())),
        };
        let own_line = self.line_indent(close).len() == usize::from(close) - self.line_start(close);
        let mut text = String::new();
        if !own_line {
            text.push('\n');
        }
        for pat in missing {
            text.push_str(&indent);
            if pat == "_" {
                text.push_str("default { }\n");
            } else {
                text.push_str(&format!("case {pat} {{ }}\n"));
            }
        }
        let at = if own_line {
            TextSize::try_from(self.line_start(close)).unwrap_or(close)
        } else {
            close
        };
        if !own_line {
            text.push_str(self.line_indent(node.text_range().start()));
        }
        Some(
            Fix::replace(
                "Insert the missing arms",
                self.span(TextRange::empty(at)),
                text,
            )
            .maybe_incorrect(),
        )
    }

    /// The K2206 fix: appends an empty `catch` clause per missing pattern
    /// after the last clause (or the `do` block). Maybe-incorrect.
    fn missing_catches_fix(
        &self,
        node: &SyntaxNode,
        clauses: &[SyntaxNode],
        missing: &[String],
    ) -> Option<Fix> {
        let after = match clauses.last() {
            Some(c) => c.text_range().end(),
            None => node
                .children()
                .find_map(ast::Block::cast)?
                .syntax()
                .text_range()
                .end(),
        };
        let indent = self.line_indent(node.text_range().start());
        let mut text = String::new();
        for pat in missing {
            text.push_str(&format!(" catch {pat} {{\n{indent}}}"));
        }
        Some(
            Fix::replace(
                "Insert the missing `catch` clauses",
                self.span(TextRange::empty(after)),
                text,
            )
            .maybe_incorrect(),
        )
    }

    /// The K2302 fix: removes an unreachable `match` arm or `catch`
    /// clause. An arm that fills its lines is removed with them; a clause
    /// sharing a line (`} catch .a { }`) is removed with the whitespace
    /// before it. Maybe-incorrect: the arm may hold code the user wants to
    /// keep elsewhere.
    fn remove_arm_fix(&self, arm: &SyntaxNode) -> Fix {
        let text = self.cx.p.file(self.file()).text;
        let range = arm.text_range();
        let start = self.line_start(range.start());
        let end = usize::from(range.end());
        let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
        let alone = text[start..usize::from(range.start())].trim().is_empty()
            && text[end..line_end].trim().is_empty();
        let remove = if alone {
            TextRange::new(
                TextSize::try_from(start).unwrap_or(range.start()),
                TextSize::try_from(line_end).unwrap_or(range.end()),
            )
        } else {
            let before = text[..usize::from(range.start())].trim_end_matches([' ', '\t']);
            TextRange::new(
                TextSize::try_from(before.len()).unwrap_or(range.start()),
                range.end(),
            )
        };
        Fix::replace("Remove the arm", self.span(remove), "").maybe_incorrect()
    }

    /// The offset of the start of the line holding `offset`.
    fn line_start(&self, offset: TextSize) -> usize {
        let text = self.cx.p.file(self.file()).text;
        let at = usize::from(offset).min(text.len());
        text[..at].rfind('\n').map_or(0, |i| i + 1)
    }

    /// Returns `true` if an error of `ty` escaping a `do` has a handler.
    fn propagates(&mut self, ty: &Ty) -> bool {
        let fcx = self.fcx_ref();
        if fcx.dos.last().is_some() {
            return true;
        }
        if let Some(lt) = fcx.lambda_throws.clone() {
            let r = self.resolve(&lt);
            return matches!(r, Ty::Var(_)) || self.resolve(&r) == *ty;
        }
        match &fcx.throws {
            Some(Ty::Error) => true,
            Some(t) => self.resolve(t) == *ty,
            None => false,
        }
    }

    /// Returns `true` if a catch pattern names a case of `ty`.
    fn pattern_fits(&self, pattern: &ast::Pattern, ty: &Ty) -> bool {
        let ast::Pattern::EnumCasePattern(e) = pattern else {
            return true;
        };
        let Some(name) = case_name(e.syntax()) else {
            return true;
        };
        let Some(d) = self.deopt(ty).decl() else {
            return false;
        };
        self.cx.p.decl(d).member_names.contains_key(name.text())
    }

    // ------------------------------------------------------------ expressions

    /// Checks `expr` against `expected`, reporting K2001 on a mismatch.
    pub(crate) fn check(&mut self, expr: &ast::Expr, expected: &Ty) -> Ty {
        let ty = self.infer(expr, Some(expected));
        let range = expr.syntax().text_range();
        if !self.assignable(&ty, expected, Some(range)) {
            self.mismatch(expr, &ty, expected);
        }
        ty
    }

    /// Reports K2001 at `expr`: `found` is not `expected`. Nothing is reported
    /// when either type contains `{error}`.
    fn mismatch(&mut self, expr: &ast::Expr, found: &Ty, expected: &Ty) {
        let found = self.resolve(found);
        let expected = self.resolve(expected);
        if found.contains_error() || expected.contains_error() {
            return;
        }
        let range = match expr {
            ast::Expr::LambdaExpr(_) => expr.syntax().text_range(),
            _ => expr.syntax().text_range(),
        };
        let message = format!(
            "expected `{}`, found `{}`",
            expected.render(self.cx.p),
            found.render(self.cx.p)
        );
        self.err(codes::TYPE_MISMATCH, range, message);
    }

    /// Infers the type of `expr`, using `expected` as a hint.
    pub(crate) fn infer(&mut self, expr: &ast::Expr, expected: Option<&Ty>) -> Ty {
        // Operator and postfix chains are unbounded in the grammar; past
        // this depth the rest of a chain is `{error}` (never a stack
        // overflow, nothing reported).
        if self.depth >= MAX_EXPR_DEPTH {
            return Ty::Error;
        }
        self.depth += 1;
        let ty = self.infer_inner(expr, expected);
        self.depth -= 1;
        let range = expr.syntax().text_range();
        if self.record {
            self.node_types.push((range, ty.clone()));
        }
        self.last_types.insert(range, ty.clone());
        ty
    }

    /// Synthesizes the type of `expr`, dispatching on its kind; the work of
    /// [`Checker::infer`] without the depth bound and the recording.
    fn infer_inner(&mut self, expr: &ast::Expr, expected: Option<&Ty>) -> Ty {
        let expected = expected.map(|e| self.resolve(e));
        let expected = expected.as_ref();
        match expr {
            ast::Expr::Literal(l) => self.literal(l, expected),
            ast::Expr::StringLit(s) => {
                for i in s.interpolations() {
                    if let Some(e) = i.expr() {
                        self.infer(&e, None);
                    }
                }
                builtin(self.cx.b.string)
            }
            ast::Expr::NameExpr(n) => {
                let Some(name) = n.syntax().first_token() else {
                    return Ty::Error;
                };
                self.name_value(
                    name.text(),
                    name.text_range(),
                    name.kind() == SyntaxKind::SelfKw,
                )
            }
            ast::Expr::ImplicitMemberExpr(m) => self.implicit_member(m.syntax(), expected, false).0,
            ast::Expr::ParenExpr(p) => match p.syntax().children().find_map(ast::Expr::cast) {
                Some(inner) => self.infer(&inner, expected),
                None => Ty::Error,
            },
            ast::Expr::TupleExpr(t) => {
                let items: Vec<Ty> = t
                    .syntax()
                    .children()
                    .filter_map(ast::Expr::cast)
                    .map(|e| self.infer(&e, None))
                    .collect();
                Ty::Tuple(items.into())
            }
            ast::Expr::ListExpr(l) => self.list_literal(l.syntax(), expected),
            ast::Expr::MapExpr(m) => self.map_literal(m.syntax(), expected),
            ast::Expr::LambdaExpr(l) => self.lambda(l, expected),
            ast::Expr::IfExpr(i) => self.if_expr(i.syntax(), expected),
            ast::Expr::MatchExpr(m) => self.match_expr(m.syntax(), expected),
            ast::Expr::TryExpr(t) => self.try_expr(t.syntax(), expected),
            ast::Expr::AwaitExpr(a) => self.await_expr(a.syntax(), expected),
            ast::Expr::PrefixExpr(p) => self.prefix(p, expected),
            ast::Expr::BinaryExpr(b) => self.binary(b, expected),
            ast::Expr::CallExpr(c) => self.call(c, expected),
            ast::Expr::MemberExpr(m) => self.member(m, expected),
            ast::Expr::IndexExpr(i) => {
                let mut exprs = i.syntax().children().filter_map(ast::Expr::cast);
                let base = exprs.next().map_or(Ty::Error, |e| self.infer(&e, None));
                let index = exprs.next();
                let base = self.resolve(&base);
                let b = &self.cx.b;
                match &base {
                    Ty::Named(d, args) if Some(*d) == b.list => {
                        if let Some(ix) = index {
                            self.check(&ix, &builtin(self.cx.b.int));
                        }
                        args.first().cloned().unwrap_or(Ty::Error)
                    }
                    Ty::Named(d, args) if Some(*d) == b.map => {
                        if let (Some(ix), Some(k)) = (index, args.first()) {
                            let k = k.clone();
                            self.check(&ix, &k);
                        }
                        args.get(1).cloned().map_or(Ty::Error, Ty::optional)
                    }
                    _ => {
                        if let Some(ix) = index {
                            self.infer(&ix, None);
                        }
                        Ty::Error
                    }
                }
            }
            ast::Expr::BindExpr(b) => match b.syntax().children().find_map(ast::Expr::cast) {
                Some(inner) => {
                    self.infer(&inner, expected);
                    Ty::Error
                }
                None => Ty::Error,
            },
            ast::Expr::WhenExpr(w) => {
                let mut exprs = w.syntax().children().filter_map(ast::Expr::cast);
                let value = exprs.next().map_or(Ty::Error, |e| self.infer(&e, expected));
                if let Some(cond) = exprs.next() {
                    self.check(&cond, &builtin(self.cx.b.bool_));
                }
                value
            }
        }
    }

    /// Returns the type of a literal: an integer literal is `Double` or
    /// `Decimal` where one is expected (else `Int`), a float literal `Decimal`
    /// where expected (else `Double`), and the other literals their own type.
    fn literal(&mut self, l: &ast::Literal, expected: Option<&Ty>) -> Ty {
        let Some(tok) = l.token() else {
            return Ty::Error;
        };
        let b = &self.cx.b;
        let expected_decl = expected.map(|e| self.deopt(e)).and_then(|e| e.decl());
        match tok.kind() {
            SyntaxKind::TrueKw | SyntaxKind::FalseKw => builtin(b.bool_),
            SyntaxKind::IntLit => {
                if expected_decl.is_some()
                    && (expected_decl == b.double || expected_decl == b.decimal)
                {
                    return Ty::named(expected_decl.unwrap_or(DeclId(0)));
                }
                builtin(b.int)
            }
            SyntaxKind::FloatLit => {
                if expected_decl.is_some() && expected_decl == b.decimal {
                    return builtin(b.decimal);
                }
                builtin(b.double)
            }
            SyntaxKind::ColorLit => builtin(b.color),
            SyntaxKind::UnitLit => {
                let text = tok.text();
                let unit: String = text
                    .chars()
                    .skip_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '_')
                    .collect();
                let decl = match unit.as_str() {
                    "%" => {
                        if expected_decl.is_some() && expected_decl == b.length {
                            b.length
                        } else {
                            b.percent
                        }
                    }
                    "ms" | "s" => b.duration,
                    "deg" | "rad" | "turn" | "grad" => b.angle,
                    _ => {
                        let tokens = [b.spacing, b.radius, b.font_size];
                        if expected_decl.is_some() && tokens.contains(&expected_decl) {
                            expected_decl
                        } else {
                            b.length
                        }
                    }
                };
                builtin(decl)
            }
            SyntaxKind::NilKw => match expected {
                Some(e) => {
                    let e = self.resolve(e);
                    match e {
                        Ty::Optional(_) | Ty::Error | Ty::Var(_) => e,
                        _ => {
                            self.err(
                                codes::TYPE_MISMATCH,
                                tok.text_range(),
                                format!("expected `{}`, found `nil`", e.render(self.cx.p)),
                            );
                            Ty::Error
                        }
                    }
                }
                None => {
                    self.err(
                        codes::CANNOT_INFER,
                        tok.text_range(),
                        "`nil` needs an optional type here",
                    );
                    Ty::Error
                }
            },
            _ => Ty::Error,
        }
    }

    /// Resolves a name used as a value.
    fn name_value(&mut self, name: &str, range: TextRange, is_self: bool) -> Ty {
        if is_self {
            return match self.owner.self_ty.clone() {
                Some(t) => t,
                None => {
                    self.err(
                        codes::UNRESOLVED_NAME,
                        range,
                        "`self` is available only inside a type or extension",
                    );
                    Ty::Error
                }
            };
        }
        if let Some(id) = self.find_local(name) {
            self.locals[id].used = true;
            let local = self.locals[id].clone();
            let resolution = if local.is_param {
                Resolution::Parameter {
                    name: local.name.clone(),
                    definition: self.span(local.def),
                }
            } else {
                Resolution::Local {
                    name: local.name.clone(),
                    definition: self.span(local.def),
                }
            };
            self.note_res(range, resolution);
            self.targets.insert(range, Target::Local(id));
            if local.deferred
                && !local.reported
                && self.flow.reachable
                && !self.flow.init.contains(&id)
            {
                let d = Diagnostic::new(
                    codes::USE_BEFORE_INITIALIZATION,
                    self.span(range),
                    format!("`{name}` is read before it is assigned"),
                )
                .with_label(self.span(local.def), "declared here");
                self.report(d);
                self.locals[id].reported = true;
            }
            return local.ty;
        }
        // Members of the enclosing type.
        if let Some(self_ty) = self.owner.self_ty.clone() {
            for statics in [false, true] {
                let found =
                    self.cx
                        .lookup_member(self.file(), self.owner.decl, &self_ty, name, statics);
                match found {
                    MemberFound::Member(m, subst) => {
                        self.note_res(range, self.cx.member_resolution(m));
                        self.targets.insert(range, Target::Member(m));
                        if self.in_bind && self.cx.p.member(m).has_annotation("Form") {
                            return self.cx.field_ty(m);
                        }
                        return self.member_ty_for_value(m, &subst);
                    }
                    MemberFound::Token(m) => {
                        self.note_res(range, self.cx.member_resolution(m));
                        return self.cx.field_ty(m);
                    }
                    MemberFound::NotVisible(m) => {
                        let member = self.cx.p.member(m).name.clone();
                        self.err(
                            codes::NOT_VISIBLE,
                            range,
                            format!("`{member}` is private to its declaration"),
                        );
                        return Ty::Error;
                    }
                    MemberFound::Unsatisfied(m) => {
                        self.note_res(range, self.cx.member_resolution(m));
                        return self.cx.member_value_ty(m, &[]);
                    }
                    _ => {}
                }
            }
        }
        if name == "reload" && self.owner_has_load() {
            return self.reload_ref(range);
        }
        if let Some(t) = self.repository_collection(name) {
            return t;
        }
        match self.cx.lookup(self.file(), name) {
            Found::Decl(d) => {
                self.note_res(range, self.cx.decl_resolution(d));
                self.targets.insert(range, Target::Decl(d));
                self.decl_value(d)
            }
            Found::NotVisible(d) => {
                self.err(
                    codes::NOT_VISIBLE,
                    range,
                    format!("`{name}` is private to its file"),
                );
                let _ = d;
                Ty::Error
            }
            Found::Silenced => Ty::Error,
            Found::Missing if self.owner.self_ty.as_ref().is_some_and(Ty::contains_error) => {
                Ty::Error
            }
            Found::Missing => {
                let mut d = Diagnostic::new(
                    codes::UNRESOLVED_NAME,
                    self.span(range),
                    format!("cannot find `{name}` in this scope"),
                );
                if let Some(fix) = self.cx.import_fix(self.file(), name) {
                    d = d.with_fix(fix);
                } else if let Some(c) = self.closest_in_scope(name) {
                    d = d.with_fix(
                        Fix::replace(format!("Replace with `{c}`"), self.span(range), c.as_str())
                            .maybe_incorrect(),
                    );
                }
                self.report(d);
                Ty::Error
            }
        }
    }

    /// The closest name to an unresolved value name: the locals in scope,
    /// the members of `self`'s type, and the declarations in the file's
    /// scope (K1201's closest-name fix).
    fn closest_in_scope(&self, name: &str) -> Option<String> {
        let mut extra: Vec<&str> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.iter().map(|(n, _)| n.as_str()))
            .collect();
        if let Some(d) = self.owner.decl {
            extra.extend(
                self.cx
                    .p
                    .decl(d)
                    .members
                    .iter()
                    .map(|m| self.cx.p.member(*m))
                    .filter(|m| m.kind != MemberKind::Init)
                    .map(|m| m.name.as_str()),
            );
        }
        self.cx.closest_visible(self.file(), name, false, &extra)
    }

    /// `reload`: the generated, synchronous `reload()` of a declaration
    /// with `@Load` fields (sema.md §12.9).
    fn reload_ref(&mut self, range: TextRange) -> Ty {
        if let Some(d) = self.owner.decl {
            self.note_res(
                range,
                Resolution::Member {
                    owner: self.cx.p.decl_name(d),
                    name: "reload()".to_owned(),
                },
            );
        }
        Ty::func(Vec::new(), false, None, builtin(self.cx.b.void))
    }

    /// Returns `true` if `m` is `k.auth.Session.user`, which is non-optional
    /// behind a `requires` guard (see [`Checker::guarded`]).
    fn is_session_user(&self, m: MemberId) -> bool {
        let member = self.cx.p.member(m);
        member.name == "user" && self.cx.p.lookup_qualified("k.auth.Session") == Some(member.owner)
    }

    /// Returns `true` if the body runs only after an `Authenticated` guard
    /// passed, so `session.user` is a `SessionUser` there rather than a
    /// `SessionUser?` (`docs/design/26-auth-and-sessions.md`, "Narrowing
    /// under `Authenticated`").
    ///
    /// Two kinds of body qualify, and nothing else does:
    ///
    /// - any body of a page whose own `requires` clause includes
    ///   `Authenticated` (its view, `@Load` initializers and functions);
    /// - the handler of an rpc whose protocol's `requires` clause, or whose
    ///   own `requires` clause in that protocol, includes `Authenticated`.
    ///   One handler serves the rpc of every implemented protocol that
    ///   declares it, so each of those protocols must guard it: a second
    ///   protocol that leaves the same rpc open lets guests run the body.
    ///
    /// These are exactly the clauses the server enforces before running the
    /// body (`k_codegen::server::guards`). Other guards don't narrow, even
    /// though `RateLimit` lets guests through and `Role` happens to refuse
    /// them; a `requires` clause written on the implementation's rpc
    /// doesn't either, because the server reads only the protocol's.
    /// Private helpers of a guarded service are never narrowed: they take
    /// the user as a parameter.
    fn guarded(&self) -> bool {
        let Some(d) = self.owner.decl else {
            return false;
        };
        let decl = self.cx.p.decl(d);
        match decl.named_kind() {
            Some(DeclarationKind::Page) => requires_authenticated(&decl.node),
            Some(DeclarationKind::Service) => {
                let Some(m) = self.current_fn.filter(|_| self.in_rpc_handler()) else {
                    return false;
                };
                let name = &self.cx.p.member(m).name;
                // Whether each implemented protocol that declares the rpc
                // guards it with `Authenticated`.
                let served: Vec<bool> = self
                    .cx
                    .implements_list(d)
                    .iter()
                    .filter_map(|(t, _)| {
                        let pd = self.cx.p.decl(t.decl()?);
                        if pd.named_kind() != Some(DeclarationKind::Protocol) {
                            return None;
                        }
                        let rpc = pd.members.iter().copied().find(|&pm| {
                            let pm = self.cx.p.member(pm);
                            pm.kind == MemberKind::Rpc && pm.name == *name
                        })?;
                        Some(
                            requires_authenticated(&pd.node)
                                || self
                                    .cx
                                    .p
                                    .member(rpc)
                                    .node
                                    .as_ref()
                                    .is_some_and(requires_authenticated),
                        )
                    })
                    .collect();
                !served.is_empty() && served.iter().all(|&g| g)
            }
            _ => false,
        }
    }

    /// Returns `true` if the body's owner has `@Load` fields (and so a
    /// generated `reload()`).
    fn owner_has_load(&self) -> bool {
        self.owner.decl.is_some_and(|d| {
            self.cx
                .p
                .decl(d)
                .members
                .iter()
                .any(|m| self.cx.p.member(*m).has_annotation("Load"))
        })
    }

    /// Returns the type of member `m` read as a value, with the owner's
    /// generics substituted by `subst`.
    fn member_ty_for_value(&mut self, m: MemberId, subst: &[(GenId, Ty)]) -> Ty {
        self.cx.member_value_ty(m, subst)
    }

    /// The value of a declaration name: a type, a function or a constant.
    fn decl_value(&mut self, d: DeclId) -> Ty {
        let decl = self.cx.p.decl(d);
        match decl.named_kind() {
            Some(DeclarationKind::Function) => {
                let sig = self.cx.sig(SigKey::Decl(d));
                let (params, ret, is_async, throws) = self.instantiate(&sig, &[]);
                Ty::func(params, is_async, throws, ret)
            }
            Some(DeclarationKind::Constant) => self.cx.constant_ty(d),
            _ if decl.is_type() => {
                let args: Vec<Ty> = decl.generics.iter().map(|_| self.fresh()).collect();
                Ty::Meta(std::sync::Arc::new(Ty::generic(d, args)))
            }
            _ => Ty::Error,
        }
    }

    /// Instantiates a signature's own generics with fresh variables.
    fn instantiate(&mut self, sig: &Sig, subst: &[(GenId, Ty)]) -> (Vec<Ty>, Ty, bool, Option<Ty>) {
        let (params, ret, is_async, throws, _) = self.instantiate_full(sig, subst);
        (params, ret, is_async, throws)
    }

    /// Instantiates a signature's own generics with fresh variables on top of
    /// `subst` (the owner's generics).
    ///
    /// Returns the parameter types, the return type, whether the signature is
    /// `async`, its error type and the full substitution (`subst` followed by
    /// the fresh variables), for callers that infer explicit type arguments.
    #[allow(clippy::type_complexity)]
    fn instantiate_full(
        &mut self,
        sig: &Sig,
        subst: &[(GenId, Ty)],
    ) -> (Vec<Ty>, Ty, bool, Option<Ty>, Vec<(GenId, Ty)>) {
        let mut full: Vec<(GenId, Ty)> = subst.to_vec();
        for g in &sig.generics {
            let v = self.fresh();
            full.push((*g, v));
        }
        let params = sig.params.iter().map(|p| apply(&p.ty, &full)).collect();
        let ret = apply(&sig.ret, &full);
        let throws = sig.throws.as_ref().map(|t| apply(t, &full));
        (params, ret, sig.is_async, throws, full)
    }

    /// Resolves `.name` against the expected type. Returns the type and,
    /// when the member is a payload case or static function, its signature.
    fn implicit_member(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Ty>,
        called: bool,
    ) -> (Ty, Option<SigUse>) {
        let Some(name) = token(node, SyntaxKind::Ident).or_else(|| {
            node.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| t.kind() != SyntaxKind::Dot && !t.kind().is_trivia())
                .last()
        }) else {
            return (Ty::Error, None);
        };
        let range = node.text_range();
        let Some(expected) = expected else {
            if self.no_implicit > 0 {
                return (Ty::Error, None);
            }
            self.err(
                codes::IMPLICIT_MEMBER_WITHOUT_TYPE,
                range,
                format!("`.{}` needs a type from context", name.text()),
            );
            return (Ty::Error, None);
        };
        let target = self.deopt(expected);
        if target.contains_error() || matches!(target, Ty::Var(_)) {
            return (Ty::Error, None);
        }
        let Ty::Named(decl, args) = &target else {
            return (Ty::Error, None);
        };
        let decl = *decl;
        let d = self.cx.p.decl(decl);
        let subst: Vec<(GenId, Ty)> = d
            .generics
            .iter()
            .copied()
            .zip(args.iter().cloned())
            .collect();
        let found = self
            .cx
            .lookup_member(self.file(), self.owner.decl, &target, name.text(), true);
        let candidate = match found {
            MemberFound::Member(m, s) => Some((m, s)),
            MemberFound::Token(m) => {
                self.note_res(name.text_range(), self.cx.member_resolution(m));
                return (target.clone(), None);
            }
            _ => None,
        };
        if let Some((m, s)) = candidate {
            let member = self.cx.p.member(m);
            let subst = if s.is_empty() { subst.clone() } else { s };
            match member.kind {
                MemberKind::Case => {
                    self.note_res(name.text_range(), self.cx.member_resolution(m));
                    if self.cx.case_has_payload(m) {
                        if !called {
                            let at = Span::empty(self.span(range).file, range.end());
                            self.report(Diagnostic::new(
                                codes::MISSING_ARGUMENT,
                                at,
                                format!("`.{}` needs its payload", name.text()),
                            ));
                        }
                        return (target.clone(), Some((self.cx.case_sig(m), subst)));
                    }
                    if called && !self.cx.case_sig(m).params.is_empty() {
                        // A page case whose parameters are all optional.
                        return (target.clone(), Some((self.cx.case_sig(m), subst)));
                    }
                    self.check_named_status(m, node);
                    return (target.clone(), None);
                }
                MemberKind::Field | MemberKind::Computed | MemberKind::Generated => {
                    let ty = self.cx.member_value_ty(m, &subst);
                    if self.assignable_quiet(&ty, &target) {
                        self.note_res(name.text_range(), self.cx.member_resolution(m));
                        return (target.clone(), None);
                    }
                }
                MemberKind::Func => {
                    let sig = self.cx.sig(SigKey::Member(m));
                    let ret = apply(&sig.ret, &subst);
                    if called && self.assignable_quiet(&ret, &target) {
                        self.note_res(name.text_range(), self.cx.member_resolution(m));
                        return (target.clone(), Some((sig, subst)));
                    }
                }
                _ => {}
            }
        }
        // `.empty` of a `Validatable` struct, valid only as a `@Form`
        // initializer (K7002); `.empty(field: value, …)` sets any fields.
        if name.text() == "empty" {
            if let Some(v) = self.cx.p.lookup_qualified("k.validation.Validatable") {
                if self.cx.declares(decl, v) {
                    if !self.form_init {
                        self.err(
                            codes::EMPTY_OUTSIDE_FORM,
                            range,
                            "`.empty` is valid only as a `@Form` field's initializer",
                        );
                    }
                    if !called {
                        return (target.clone(), None);
                    }
                    let params = self
                        .cx
                        .stored_fields(decl)
                        .iter()
                        .map(|f| {
                            let m = self.cx.p.member(*f);
                            SigParam {
                                label: Some(m.name.clone()),
                                name: m.name.clone(),
                                ty: apply(&self.cx.field_ty(*f), &subst),
                                has_default: true,
                                name_range: m.name_range,
                            }
                        })
                        .collect();
                    let sig = Rc::new(Sig {
                        generics: Vec::new(),
                        params,
                        is_async: false,
                        throws: None,
                        ret: target.clone(),
                    });
                    return (target.clone(), Some((sig, Vec::new())));
                }
            }
        }
        // Generated declarations whose members the checker doesn't know.
        if self.cx.p.decl(decl).has_annotation("Generated")
            && Some(decl) != self.cx.b.page
            && !self.cx.is_closed_generated(decl)
        {
            return (Ty::Error, None);
        }
        let candidates: Vec<String> = self.implicit_candidates(decl);
        let mut d = Diagnostic::new(
            codes::UNKNOWN_IMPLICIT_MEMBER,
            self.span(name.text_range()),
            format!(
                "`{}` has no member `{}`",
                target.render(self.cx.p),
                name.text()
            ),
        );
        let is_static_member = self.cx.p.decl(decl).members.iter().any(|m| {
            let m = self.cx.p.member(*m);
            m.is_static && m.kind != MemberKind::Init && m.name == name.text()
        });
        if is_static_member {
            let rendered = target.render(self.cx.p);
            d = d.with_note(format!(
                "`{}` is a static member of `{rendered}`, but it isn't a `{rendered}`",
                name.text()
            ));
        }
        if let Some(c) = closest(name.text(), candidates.iter().map(String::as_str)) {
            d = d.with_fix(
                Fix::replace(
                    format!("Replace with `{c}`"),
                    self.span(name.text_range()),
                    c,
                )
                .maybe_incorrect(),
            );
        }
        self.report(d);
        (target.clone(), None)
    }

    /// Returns the names an implicit member (`.name`) of `decl` may use: its
    /// static members other than initializers, then its style tokens; used
    /// for "did you mean" suggestions.
    fn implicit_candidates(&self, decl: DeclId) -> Vec<String> {
        let mut out: Vec<String> = self
            .cx
            .p
            .decl(decl)
            .members
            .iter()
            .map(|m| self.cx.p.member(*m))
            .filter(|m| m.is_static && m.kind != MemberKind::Init)
            .map(|m| m.name.clone())
            .collect();
        if let Some(tokens) = self.cx.tokens.get(&decl) {
            out.extend(tokens.iter().map(|t| self.cx.p.member(*t).name.clone()));
        }
        out
    }

    /// Returns `true` if `actual` is assignable to `expected`, binding
    /// inference variables only on success (the bindings are rolled back
    /// otherwise).
    ///
    /// Clones the variable table, so it costs O(number of variables).
    fn assignable_quiet(&mut self, actual: &Ty, expected: &Ty) -> bool {
        let saved = self.vars.clone();
        let ok = self.assignable(actual, expected, None);
        if !ok {
            self.vars = saved;
        }
        ok
    }

    /// Hook for checks on a payload-less enum case used as an implicit member
    /// (`.name`); currently does nothing.
    fn check_named_status(&mut self, _m: MemberId, _node: &SyntaxNode) {}

    /// Checks a list literal: against the element type of an expected
    /// `List<T>` or `Set<T>` (a set literal is written as a list), otherwise
    /// the first well-typed element fixes the element type and the others are
    /// checked against it; an empty literal without a type from context is
    /// K2002.
    fn list_literal(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let elements: Vec<ast::Expr> = node.children().filter_map(ast::Expr::cast).collect();
        let b = &self.cx.b;
        let expected = expected.map(|e| self.deopt(e));
        let (container, elem) = match &expected {
            Some(Ty::Named(d, args)) if Some(*d) == b.list || Some(*d) == b.set => {
                (*d, args.first().cloned())
            }
            _ => (b.list.unwrap_or(DeclId(0)), None),
        };
        if let Some(elem) = elem {
            for e in &elements {
                self.check(e, &elem);
            }
            return Ty::generic(container, vec![elem]);
        }
        if elements.is_empty() {
            if expected
                .as_ref()
                .is_some_and(|e| e.contains_error() || matches!(e, Ty::Var(_)))
            {
                return match expected {
                    Some(Ty::Var(_)) => {
                        let v = self.fresh();
                        Ty::generic(container, vec![v])
                    }
                    _ => Ty::Error,
                };
            }
            self.err(
                codes::CANNOT_INFER,
                node.text_range(),
                "the element type of `[]` can't be inferred here",
            );
            return Ty::Error;
        }
        let mut elem: Option<Ty> = None;
        for e in &elements {
            match &elem {
                Some(t) => {
                    let t = t.clone();
                    self.check(e, &t);
                }
                None => {
                    let t = self.infer(e, None);
                    let t = self.resolve(&t);
                    if !t.contains_error() {
                        elem = Some(t);
                    }
                }
            }
        }
        match elem {
            Some(t) => Ty::generic(container, vec![t]),
            None => Ty::generic(container, vec![Ty::Error]),
        }
    }

    /// Checks a map literal (`[key: value, …]`): against an expected
    /// `Map<K, V>`, otherwise the first entry fixes the key and value types
    /// (keys must be `Hashable`, else K2009) and the others
    /// are checked against them; an empty `[:]` without context is K2002.
    fn map_literal(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let entries: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MapEntry)
            .collect();
        let b = &self.cx.b;
        let map = b.map.unwrap_or(DeclId(0));
        let expected = expected.map(|e| self.deopt(e));
        if let Some(Ty::Named(d, args)) = &expected {
            if Some(*d) == b.map && args.len() == 2 {
                let (k, v) = (args[0].clone(), args[1].clone());
                for entry in &entries {
                    let mut exprs = entry.children().filter_map(ast::Expr::cast);
                    if let Some(key) = exprs.next() {
                        self.check(&key, &k);
                    }
                    if let Some(value) = exprs.next() {
                        self.check(&value, &v);
                    }
                }
                return Ty::generic(map, vec![k, v]);
            }
        }
        if entries.is_empty() {
            self.err(
                codes::CANNOT_INFER,
                node.text_range(),
                "the types of `[:]` can't be inferred here",
            );
            return Ty::Error;
        }
        let mut kt: Option<Ty> = None;
        let mut vt: Option<Ty> = None;
        for entry in &entries {
            let mut exprs = entry.children().filter_map(ast::Expr::cast);
            if let Some(key) = exprs.next() {
                match &kt {
                    Some(t) => {
                        let t = t.clone();
                        self.check(&key, &t);
                    }
                    None => {
                        let t = self.infer(&key, None);
                        let t = self.resolve(&t);
                        if let Some(h) = self.cx.b.hashable {
                            if !t.contains_error() && !self.cx.implements(&t, h) {
                                self.err(
                                    codes::UNSATISFIED_CONSTRAINT,
                                    key.syntax().text_range(),
                                    format!(
                                        "map keys must be `Hashable`; `{}` is not",
                                        t.render(self.cx.p)
                                    ),
                                );
                            }
                        }
                        kt = Some(t);
                    }
                }
            }
            if let Some(value) = exprs.next() {
                match &vt {
                    Some(t) => {
                        let t = t.clone();
                        self.check(&value, &t);
                    }
                    None => {
                        let t = self.infer(&value, None);
                        vt = Some(self.resolve(&t));
                    }
                }
            }
        }
        Ty::generic(map, vec![kt.unwrap_or(Ty::Error), vt.unwrap_or(Ty::Error)])
    }

    /// Checks a lambda against an expected function type (or infers it); the
    /// lambda's body is never view content.
    fn lambda(&mut self, l: &ast::LambdaExpr, expected: Option<&Ty>) -> Ty {
        let saved = std::mem::replace(&mut self.view_on, false);
        let t = self.lambda_inner(l, expected);
        self.view_on = saved;
        t
    }

    /// The work of [`Checker::lambda`]: takes parameter types from the
    /// annotations or the expected function type and checks the body in a
    /// new function context. A lambda where a non-function value is expected
    /// is K2001 (its body is still checked).
    fn lambda_inner(&mut self, l: &ast::LambdaExpr, expected: Option<&Ty>) -> Ty {
        let expected_fn = expected.map(|e| self.resolve(e)).and_then(|e| match e {
            Ty::Func(f) => Some(f),
            _ => None,
        });
        let range = l.syntax().text_range();
        if let (Some(e), None) = (expected, &expected_fn) {
            let e = self.resolve(e);
            if !e.contains_error() && !matches!(e, Ty::Var(_)) {
                // A lambda where a value is expected: check its body anyway,
                // then report the mismatch.
                let params = lambda_param_names(l);
                self.lambda_body(l, params, None, None, false, None);
                self.err(
                    codes::TYPE_MISMATCH,
                    range,
                    format!("expected `{}`, found a lambda", e.render(self.cx.p)),
                );
                return Ty::Error;
            }
        }
        let mut params: Vec<(String, TextRange, Option<Ty>)> = Vec::new();
        let mut declared_ret = None;
        if let Some(list) = l.param_list() {
            for p in list
                .syntax()
                .children()
                .filter(|c| c.kind() == SyntaxKind::LambdaParam)
            {
                let Some(name) = token(&p, SyntaxKind::Ident) else {
                    continue;
                };
                let ty = p
                    .children()
                    .find_map(ast::Type::cast)
                    .map(|t| self.cx.resolve_type(self.file(), &self.owner.generics, &t));
                params.push((name.text().to_owned(), name.text_range(), ty));
            }
            declared_ret = list
                .syntax()
                .children()
                .find_map(ast::ReturnType::cast)
                .and_then(|r| r.ty())
                .map(|t| self.cx.resolve_type(self.file(), &self.owner.generics, &t));
        }
        if let Some(f) = &expected_fn {
            if params.len() > f.params.len()
                || (!params.is_empty() && params.len() != f.params.len())
                || (params.is_empty() && !f.params.is_empty())
            {
                // Wrong arity: check the body for its own errors only.
                let tys: Vec<(String, TextRange, Ty)> = params
                    .iter()
                    .map(|(n, r, t)| (n.clone(), *r, t.clone().unwrap_or(Ty::Error)))
                    .collect();
                self.lambda_body(l, tys, None, None, false, None);
                self.err(
                    codes::TYPE_MISMATCH,
                    range,
                    format!(
                        "expected `{}`, found a lambda with {} parameter(s)",
                        Ty::Func(f.clone()).render(self.cx.p),
                        params.len()
                    ),
                );
                return Ty::Error;
            }
        }
        let mut param_tys = Vec::new();
        let mut unknown = false;
        for (i, (n, r, t)) in params.iter().enumerate() {
            let ty = match (t, &expected_fn) {
                (Some(t), _) => t.clone(),
                (None, Some(f)) => {
                    let pt = self.resolve(&f.params[i]);
                    if matches!(pt, Ty::Var(_)) {
                        unknown = true;
                        Ty::Error
                    } else {
                        pt
                    }
                }
                (None, None) => {
                    unknown = true;
                    Ty::Error
                }
            };
            param_tys.push((n.clone(), *r, ty));
        }
        if unknown && expected_fn.is_none() {
            // A lambda that is the value of a lambda already reported has
            // no type to infer from either: one K2002 for the whole nest
            // (`{ x => { y => { z => 1 } } }`), not one per level.
            if !self.uninferred_values.remove(&range) {
                self.err(
                    codes::CANNOT_INFER,
                    range,
                    "the lambda's parameter types can't be inferred here",
                );
            }
            if let Some(ast::Stmt::Expr(ast::Expr::LambdaExpr(inner))) = l.statements().last() {
                self.uninferred_values.insert(inner.syntax().text_range());
            }
            self.lambda_body(l, param_tys, None, None, false, None);
            return Ty::Error;
        }
        let (is_async, throws, ret) = match &expected_fn {
            Some(f) => (
                f.is_async,
                f.throws.clone(),
                declared_ret.clone().unwrap_or_else(|| f.ret.clone()),
            ),
            None => (
                false,
                None,
                declared_ret.clone().unwrap_or_else(|| self.fresh()),
            ),
        };
        let throws_var = match &throws {
            Some(t) => Some(t.clone()),
            None if expected_fn.is_none() => Some(self.fresh()),
            None => None,
        };
        let value = self.lambda_body(
            l,
            param_tys.clone(),
            Some(ret.clone()),
            throws_var.clone(),
            is_async,
            declared_ret.as_ref(),
        );
        let ret = self.resolve(&ret);
        let ret = if matches!(ret, Ty::Var(_)) {
            value
        } else {
            ret
        };
        let throws = throws_var.map(|t| self.resolve(&t)).and_then(|t| match t {
            Ty::Var(_) | Ty::Never => None,
            other => Some(other),
        });
        let throws = match (&expected_fn, throws) {
            (Some(f), t) if f.throws.is_some() => {
                let ft = self.resolve(f.throws.as_ref().unwrap_or(&Ty::Error));
                if matches!(ft, Ty::Var(_)) {
                    t
                } else {
                    Some(ft)
                }
            }
            (_, t) => t,
        };
        Ty::func(
            param_tys.into_iter().map(|(_, _, t)| t).collect(),
            is_async,
            throws,
            ret,
        )
    }

    /// Checks a lambda's body; returns its value type.
    fn lambda_body(
        &mut self,
        l: &ast::LambdaExpr,
        params: Vec<(String, TextRange, Ty)>,
        ret: Option<Ty>,
        throws: Option<Ty>,
        is_async: bool,
        declared_ret: Option<&Ty>,
    ) -> Ty {
        let saved_flow = self.flow.clone();
        self.push_fn(FnCx {
            is_async,
            throws: None,
            lambda_throws: throws.clone().or(Some(Ty::Never)),
            ret: ret.clone(),
            is_lambda: true,
            loops: 0,
            dos: Vec::new(),
            tries: Vec::new(),
            awaits: 0,
            init_of: None,
            decl_node: None,
        });
        if throws.is_none() {
            self.fcx().lambda_throws = None;
        }
        self.push_scope();
        // `update { row => row.x = … }`: the row is writable.
        let writable = self.query_update_depth > 0;
        for (n, r, t) in params {
            self.note_type(r, &t);
            self.add_local(Local {
                name: n,
                ty: t,
                def: r,
                is_param: false,
                mutable: writable,
                keyword: None,
                deferred: false,
                used: false,
                reported: false,
            });
        }
        let stmts: Vec<ast::Stmt> = l.statements().collect();
        let expected_value = ret.as_ref().map(|r| self.resolve(r));
        let report = declared_ret.is_some()
            || expected_value
                .as_ref()
                .is_some_and(|r| !matches!(r, Ty::Var(_)));
        let value = match &expected_value {
            Some(r) if self.is_void(r) => {
                self.value_stmts(&stmts, None, false);
                r.clone()
            }
            Some(r) if report => self.value_stmts(&stmts, Some(r), true),
            _ => {
                let v = self.value_stmts(&stmts, None, false);
                if let Some(r) = &ret {
                    let v = self.resolve(&v);
                    let origin = stmts.last().map(|s| s.syntax().text_range());
                    let _ = self.unify(r, &v, origin);
                }
                v
            }
        };
        self.pop_scope();
        self.pop_fn();
        self.flow = saved_flow;
        self.resolve(&value)
    }

    /// Checks an `if` (or `if let`) expression or statement: the conditions'
    /// bindings are visible in the first block only, the branches' flow
    /// states are merged, and used as a value the branches must agree on a
    /// type.
    fn if_expr(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let entry_scope = self.scopes.len();
        self.push_scope();
        let view = std::mem::replace(&mut self.view_on, false);
        self.conditions(node);
        self.view_on = view;
        let before_branches = self.flow.clone();
        let blocks: Vec<ast::Block> = node.children().filter_map(ast::Block::cast).collect();
        let else_if = node.children().find(|c| c.kind() == SyntaxKind::IfExpr);
        let mut value: Option<Ty> = None;
        let mut branches = Vec::new();
        if let Some(then) = blocks.first() {
            let stmts: Vec<ast::Stmt> = then.statements().collect();
            let t = self.value_stmts(&stmts, expected, expected.is_some());
            value = Some(t);
        }
        branches.push(self.flow.clone());
        self.scopes.truncate(entry_scope);
        self.flow = before_branches.clone();
        let has_else = blocks.len() > 1 || else_if.is_some();
        if let Some(else_if) = else_if {
            let t = self.if_expr(&else_if, expected.or(value.as_ref()).cloned().as_ref());
            if let Some(v) = &value {
                let v = self.resolve(v);
                if expected.is_none() && !matches!(v, Ty::Never) {
                    let _ = self.unify(&v, &t, None);
                }
            }
            branches.push(self.flow.clone());
        } else if let Some(else_block) = blocks.get(1) {
            let stmts: Vec<ast::Stmt> = else_block.statements().collect();
            let first = value.clone().map(|v| self.resolve(&v));
            let exp = expected.cloned().or_else(|| {
                first.filter(|v| !matches!(v, Ty::Never) && !self.is_void(v) && !v.contains_error())
            });
            let t = self.value_stmts(
                &stmts,
                exp.as_ref(),
                exp.is_some() && self.wants_value(node),
            );
            if value
                .as_ref()
                .is_some_and(|v| matches!(self.resolve(v), Ty::Never))
            {
                value = Some(t);
            }
            branches.push(self.flow.clone());
        } else {
            branches.push(before_branches);
        }
        self.flow = Flow::merge(&branches);
        if !has_else {
            return builtin(self.cx.b.void);
        }
        value.unwrap_or(Ty::Error)
    }

    /// Returns `true` if an `if`/`match` node is used as a value.
    fn wants_value(&self, node: &SyntaxNode) -> bool {
        let Some(parent) = node.parent() else {
            return false;
        };
        match parent.kind() {
            SyntaxKind::Block | SyntaxKind::LambdaExpr => {
                // The last statement of a value block.
                let is_last = node
                    .next_sibling()
                    .is_none_or(|s| !ast::Stmt::can_cast(s.kind()));
                is_last
                    && parent.parent().is_some_and(|gp| {
                        matches!(
                            gp.kind(),
                            SyntaxKind::FieldDecl | SyntaxKind::MatchArm | SyntaxKind::IfExpr
                        )
                    })
                    || (is_last && parent.kind() == SyntaxKind::LambdaExpr)
            }
            SyntaxKind::IfExpr => true,
            _ => true,
        }
    }

    /// Checks a `match`: each arm's pattern against the scrutinee's type,
    /// unreachable arms (K2302) and exhaustiveness, and, used as a value, the
    /// arms' common type. The arms' flow states are merged.
    fn match_expr(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let scrutinee = node.children().find_map(ast::Expr::cast);
        let view = std::mem::replace(&mut self.view_on, false);
        let subject = scrutinee
            .as_ref()
            .map_or(Ty::Error, |e| self.infer(e, None));
        self.view_on = view;
        let subject = self.resolve(&subject);
        let as_value = self.wants_value(node) && expected.is_some() || self.is_value_position(node);
        let match_kw =
            token(node, SyntaxKind::MatchKw).map_or(node.text_range(), |t| t.text_range());
        let arms: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MatchArm)
            .collect();
        let entry = self.flow.clone();
        let mut branches = Vec::new();
        let mut rows = pattern::Rows::new();
        let mut broken = subject.contains_error()
            || node
                .descendants()
                .any(|n| n.kind() == SyntaxKind::ErrorNode);
        let mut value: Option<Ty> = expected.cloned();
        let mut first_value: Option<Ty> = None;
        let mut catch_all_seen = false;
        for arm in &arms {
            self.flow = entry.clone();
            self.push_scope();
            let is_default = has_token(arm, SyntaxKind::DefaultKw);
            let patterns: Vec<ast::Pattern> =
                arm.children().filter_map(ast::Pattern::cast).collect();
            let guard = arm.children().find(|c| c.kind() == SyntaxKind::MatchGuard);
            let mut binds = Vec::new();
            let mut pats = Vec::new();
            for p in &patterns {
                match self.pattern(p, &subject, &mut binds) {
                    Some(pat) => pats.push((p.syntax().text_range(), pat)),
                    None => broken = true,
                }
            }
            let case_kw = token(arm, SyntaxKind::CaseKw);
            let top_wild =
                patterns.len() == 1 && matches!(patterns[0], ast::Pattern::WildcardPattern(_));
            if top_wild {
                if let Some(kw) = &case_kw {
                    let r = TextRange::new(
                        kw.text_range().start(),
                        patterns[0].syntax().text_range().end(),
                    );
                    let d = Diagnostic::new(
                        codes::WILDCARD_CASE_ARM,
                        self.span(r),
                        "write `default` instead of `case _`",
                    )
                    .with_fix(Fix::replace(
                        "Replace with `default`",
                        self.span(r),
                        "default",
                    ));
                    self.report(d);
                }
            }
            let mut unreachable = false;
            if !broken {
                if is_default {
                    if catch_all_seen
                        || !rows.useful(self, &[Pat::Wild], std::slice::from_ref(&subject))
                    {
                        let kw = token(arm, SyntaxKind::DefaultKw)
                            .map_or(arm.text_range(), |t| t.text_range());
                        let d = Diagnostic::new(
                            codes::UNREACHABLE_ARM,
                            self.span(kw),
                            "this `default` can never run",
                        )
                        .with_fix(self.remove_arm_fix(arm));
                        self.report(d);
                        unreachable = true;
                    }
                    if guard.is_none() {
                        rows.push(self, vec![Pat::Wild], std::slice::from_ref(&subject));
                    }
                } else {
                    for (r, pat) in &pats {
                        if catch_all_seen
                            || !rows.useful(
                                self,
                                std::slice::from_ref(pat),
                                std::slice::from_ref(&subject),
                            )
                        {
                            let mut d = Diagnostic::new(
                                codes::UNREACHABLE_ARM,
                                self.span(*r),
                                "this pattern can never match",
                            );
                            // Removing the arm is right only when it has no
                            // other pattern that could match.
                            if pats.len() == 1 {
                                d = d.with_fix(self.remove_arm_fix(arm));
                            }
                            self.report(d);
                            unreachable = true;
                        }
                        if guard.is_none() {
                            rows.push(self, vec![pat.clone()], std::slice::from_ref(&subject));
                        }
                    }
                }
            }
            let is_catch_all = is_default
                || (patterns.len() == 1
                    && matches!(
                        patterns[0],
                        ast::Pattern::WildcardPattern(_) | ast::Pattern::BindingPattern(_)
                    ));
            let mut binding_local = None;
            for (name, ty, range) in binds {
                self.note_type(range, &ty);
                let id = self.add_local(Local {
                    name,
                    ty,
                    def: range,
                    is_param: false,
                    mutable: false,
                    keyword: None,
                    deferred: false,
                    used: false,
                    reported: false,
                });
                if matches!(patterns.first(), Some(ast::Pattern::BindingPattern(_)))
                    && patterns.len() == 1
                {
                    binding_local = Some(id);
                }
            }
            if let Some(g) = &guard {
                if let Some(e) = g.children().find_map(ast::Expr::cast) {
                    self.check(&e, &builtin(self.cx.b.bool_));
                }
            }
            if let Some(block) = arm.children().find_map(ast::Block::cast) {
                let stmts: Vec<ast::Stmt> = block.statements().collect();
                if as_value {
                    let exp = value.clone().or_else(|| first_value.clone());
                    let t = self.value_stmts(&stmts, exp.as_ref(), exp.is_some());
                    let t = self.resolve(&t);
                    if exp.is_none() && !matches!(t, Ty::Never) && !t.contains_error() {
                        first_value = Some(t);
                    }
                } else {
                    self.value_stmts(&stmts, None, false);
                }
            }
            if let Some(id) = binding_local {
                if !self.locals[id].used && !unreachable && guard.is_none() {
                    if let (Some(kw), Some(p)) = (&case_kw, patterns.first()) {
                        let pr = p.syntax().text_range();
                        let head = TextRange::new(kw.text_range().start(), pr.end());
                        let d = Diagnostic::new(
                            codes::UNUSED_CATCH_ALL_BINDING,
                            self.span(pr),
                            "the catch-all binding is never used",
                        )
                        .with_fix(Fix::replace(
                            "Replace the arm head with `default`",
                            self.span(head),
                            "default",
                        ));
                        self.report(d);
                    }
                }
            }
            if is_catch_all && guard.is_none() {
                catch_all_seen = true;
            }
            self.pop_scope();
            branches.push(self.flow.clone());
        }
        if !broken && rows.useful(self, &[Pat::Wild], std::slice::from_ref(&subject)) {
            let missing = rows.missing(self, &subject);
            let mut d = Diagnostic::new(
                codes::MATCH_NOT_EXHAUSTIVE,
                self.span(match_kw),
                "the `match` doesn't handle every case",
            )
            .with_note(missing_note(&missing));
            if let Some(fix) = self.missing_arms_fix(node, &arms, &missing) {
                d = d.with_fix(fix);
            }
            self.report(d);
        }
        self.flow = if branches.is_empty() {
            entry
        } else {
            Flow::merge(&branches)
        };
        if value.is_none() {
            value = first_value;
        }
        if !as_value {
            return builtin(self.cx.b.void);
        }
        value.unwrap_or(Ty::Error)
    }

    /// Returns `true` if the `if`/`match` `node` is used as a value: anywhere
    /// outside a block, or as the last statement of a block that is the value
    /// of a field, a match arm or an `if` branch.
    fn is_value_position(&self, node: &SyntaxNode) -> bool {
        let Some(parent) = node.parent() else {
            return false;
        };
        !matches!(parent.kind(), SyntaxKind::Block) || {
            let is_last = node
                .next_sibling()
                .is_none_or(|s| !ast::Stmt::can_cast(s.kind()));
            is_last
                && parent.parent().is_some_and(|gp| {
                    matches!(
                        gp.kind(),
                        SyntaxKind::FieldDecl | SyntaxKind::MatchArm | SyntaxKind::IfExpr
                    )
                })
        }
    }

    // ------------------------------------------------------------ patterns

    /// Checks a pattern against `ty`; collects bindings. `None` when the
    /// pattern has an error (no exhaustiveness check then).
    fn pattern(
        &mut self,
        p: &ast::Pattern,
        ty: &Ty,
        binds: &mut Vec<(String, Ty, TextRange)>,
    ) -> Option<Pat> {
        let ty = self.resolve(ty);
        match p {
            ast::Pattern::WildcardPattern(_) => Some(Pat::Wild),
            ast::Pattern::BindingPattern(b) => {
                if let Some(name) = token(b.syntax(), SyntaxKind::Ident) {
                    binds.push((name.text().to_owned(), ty.clone(), name.text_range()));
                }
                Some(Pat::Wild)
            }
            ast::Pattern::IdentPattern(i) => {
                if let Some(name) = token(i.syntax(), SyntaxKind::Ident) {
                    binds.push((name.text().to_owned(), ty.clone(), name.text_range()));
                }
                Some(Pat::Wild)
            }
            ast::Pattern::LiteralPattern(l) => self.literal_pattern(l.syntax(), &ty),
            ast::Pattern::RangePattern(r) => {
                for lp in r
                    .syntax()
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::LiteralPattern)
                {
                    self.literal_pattern(&lp, &ty);
                }
                Some(Pat::Range)
            }
            ast::Pattern::TuplePattern(t) => {
                let items: Vec<ast::Pattern> = t
                    .syntax()
                    .children()
                    .filter_map(ast::Pattern::cast)
                    .collect();
                let tys: Vec<Ty> = match &ty {
                    Ty::Tuple(ts) => ts.to_vec(),
                    _ => vec![Ty::Error; items.len()],
                };
                let mut subs = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    let t = tys.get(i).cloned().unwrap_or(Ty::Error);
                    subs.push(self.pattern(item, &t, binds)?);
                }
                // A tuple is one constructor whose fields are its elements;
                // against anything but a tuple of that arity (an error
                // reported elsewhere) the pattern matches like `_`.
                match &ty {
                    Ty::Tuple(ts) if ts.len() == subs.len() => {
                        Some(Pat::Ctor(pattern::Ctor::Tuple(subs.len()), subs))
                    }
                    _ => Some(Pat::Wild),
                }
            }
            ast::Pattern::EnumCasePattern(e) => self.case_pattern(e.syntax(), &ty, binds),
        }
    }

    /// Checks a literal pattern (`true`, `nil`, a number, a unit or a string)
    /// against `ty`; `None` when the literal can't match a `ty` (e.g. `nil`
    /// against a non-optional type).
    fn literal_pattern(&mut self, node: &SyntaxNode, ty: &Ty) -> Option<Pat> {
        let tok = node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| !t.kind().is_trivia() && t.kind() != SyntaxKind::Minus)?;
        let b = &self.cx.b;
        match tok.kind() {
            SyntaxKind::TrueKw => Some(Pat::Ctor(pattern::Ctor::True, Vec::new())),
            SyntaxKind::FalseKw => Some(Pat::Ctor(pattern::Ctor::False, Vec::new())),
            SyntaxKind::NilKw => {
                if !matches!(ty, Ty::Optional(_) | Ty::Error) {
                    return None;
                }
                Some(Pat::Ctor(pattern::Ctor::Nil, Vec::new()))
            }
            SyntaxKind::IntLit
            | SyntaxKind::FloatLit
            | SyntaxKind::UnitLit
            | SyntaxKind::StringStart => {
                let lit_ty = match tok.kind() {
                    SyntaxKind::IntLit => builtin(b.int),
                    SyntaxKind::FloatLit => builtin(b.double),
                    SyntaxKind::StringStart => builtin(b.string),
                    _ => Ty::Error,
                };
                let target = self.deopt(ty);
                if !lit_ty.is_error()
                    && !target.contains_error()
                    && !self.assignable_quiet(&lit_ty, &target)
                {
                    self.err(
                        codes::TYPE_MISMATCH,
                        node.text_range(),
                        format!(
                            "expected `{}`, found `{}`",
                            target.render(self.cx.p),
                            lit_ty.render(self.cx.p)
                        ),
                    );
                    return None;
                }
                Some(Pat::Lit(node.text().to_string()))
            }
            _ => None,
        }
    }

    /// Checks an enum case pattern (`.name`, `.name(p, label: q)` or a
    /// qualified `Light.red`) against `ty`, adding its bindings to `binds`.
    ///
    /// Returns the case constructor with its sub-patterns, or `None` when the
    /// pattern doesn't name a case of the scrutinee's type (reported) or `ty`
    /// is `{error}` (sub-patterns are still checked so their names bind).
    fn case_pattern(
        &mut self,
        node: &SyntaxNode,
        ty: &Ty,
        binds: &mut Vec<(String, Ty, TextRange)>,
    ) -> Option<Pat> {
        let name = case_name(node)?;
        let idents: Vec<SyntaxToken> = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::Dot)
            .collect();
        // A qualified pattern: `Light.red`.
        let qualifier: Vec<&SyntaxToken> = idents
            .iter()
            .take_while(|t| t.text_range() != name.text_range())
            .filter(|t| t.kind() == SyntaxKind::Ident)
            .collect();
        let args: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PatternField || ast::Pattern::can_cast(c.kind()))
            .collect();
        let has_parens = has_token(node, SyntaxKind::LParen);
        if ty.contains_error() {
            for a in &args {
                if let Some(p) = ast::Pattern::cast(a.clone())
                    .or_else(|| a.children().find_map(ast::Pattern::cast))
                {
                    self.pattern(&p, &Ty::Error, binds);
                }
            }
            return None;
        }
        let mut scrutinee = self.resolve(ty);
        if !qualifier.is_empty() {
            let q: Vec<&str> = qualifier.iter().map(|t| t.text()).collect();
            let found = if q.len() == 1 {
                match self.cx.lookup(self.file(), q[0]) {
                    Found::Decl(d) => Some(d),
                    _ => None,
                }
            } else {
                self.cx.p.lookup_qualified(&q.join("."))
            };
            let d = found?;
            if let Some(last) = qualifier.last() {
                self.note_res(last.text_range(), self.cx.decl_resolution(d));
            }
            if self.deopt(&scrutinee).decl() != Some(d) && !matches!(scrutinee, Ty::Error) {
                // A pattern of another enum than the scrutinee (e.g.
                // `expect throws E.x`): take the named enum.
                if self.deopt(&scrutinee).decl().is_none()
                    || self
                        .deopt(&scrutinee)
                        .decl()
                        .is_some_and(|sd| !self.cx.is_enum(sd))
                {
                    scrutinee = Ty::named(d);
                }
            }
        }
        // Optional: `.some(p)`, `.none`, or a case of the wrapped enum.
        if let Ty::Optional(inner) = &scrutinee {
            match name.text() {
                "some" => {
                    let inner_pat = args.first().and_then(|a| {
                        ast::Pattern::cast(a.clone())
                            .or_else(|| a.children().find_map(ast::Pattern::cast))
                    });
                    let sub = match inner_pat {
                        Some(p) => self.pattern(&p, inner, binds)?,
                        None => Pat::Wild,
                    };
                    if let Some(o) = self.cx.b.optional {
                        if let Some(&m) = self.cx.p.decl(o).member_names.get("some") {
                            self.note_res(name.text_range(), self.cx.member_resolution(m));
                        }
                    }
                    return Some(Pat::Ctor(pattern::Ctor::Some, vec![sub]));
                }
                "none" => return Some(Pat::Ctor(pattern::Ctor::Nil, Vec::new())),
                _ => {
                    let inner = (**inner).clone();
                    let sub = self.case_pattern(node, &inner, binds)?;
                    return Some(Pat::Ctor(pattern::Ctor::Some, vec![sub]));
                }
            }
        }
        let decl = scrutinee.decl()?;
        let d = self.cx.p.decl(decl);
        let member = d
            .member_names
            .get(name.text())
            .copied()
            .filter(|m| self.cx.p.member(*m).kind == MemberKind::Case);
        let Some(member) = member else {
            if d.has_annotation("Generated")
                && Some(decl) != self.cx.b.page
                && Some(decl) != self.cx.b.theme_variant
            {
                return None;
            }
            let candidates: Vec<String> = self
                .cx
                .cases(decl)
                .iter()
                .map(|m| self.cx.p.member(*m).name.clone())
                .collect();
            let mut diag = Diagnostic::new(
                codes::UNKNOWN_IMPLICIT_MEMBER,
                self.span(name.text_range()),
                format!(
                    "`{}` has no case `{}`",
                    scrutinee.render(self.cx.p),
                    name.text()
                ),
            );
            if let Some(c) = closest(name.text(), candidates.iter().map(String::as_str)) {
                diag = diag.with_fix(
                    Fix::replace(
                        format!("Replace with `{c}`"),
                        self.span(name.text_range()),
                        c,
                    )
                    .maybe_incorrect(),
                );
            }
            self.report(diag);
            for a in &args {
                if let Some(p) = ast::Pattern::cast(a.clone())
                    .or_else(|| a.children().find_map(ast::Pattern::cast))
                {
                    self.pattern(&p, &Ty::Error, binds);
                }
            }
            return None;
        };
        self.note_res(name.text_range(), self.cx.member_resolution(member));
        let sig = self.cx.case_sig(member);
        let subst: Vec<(GenId, Ty)> = d
            .generics
            .iter()
            .copied()
            .zip(scrutinee.args().iter().cloned())
            .collect();
        let field_tys: Vec<Ty> = sig.params.iter().map(|p| apply(&p.ty, &subst)).collect();
        let mut subs = vec![Pat::Wild; field_tys.len()];
        if has_parens {
            let mut position = 0;
            let mut ok = true;
            for a in &args {
                let (index, p) = if a.kind() == SyntaxKind::PatternField {
                    let label = token(a, SyntaxKind::Ident);
                    let index = label.and_then(|l| {
                        sig.params
                            .iter()
                            .position(|p| p.label.as_deref() == Some(l.text()))
                    });
                    (index, a.children().find_map(ast::Pattern::cast))
                } else {
                    let i = position;
                    position += 1;
                    (Some(i), ast::Pattern::cast(a.clone()))
                };
                let Some(p) = p else { continue };
                match index.filter(|i| *i < field_tys.len()) {
                    Some(i) => match self.pattern(&p, &field_tys[i].clone(), binds) {
                        Some(sub) => subs[i] = sub,
                        None => ok = false,
                    },
                    None => {
                        self.pattern(&p, &Ty::Error, binds);
                        ok = false;
                    }
                }
            }
            if !ok {
                return None;
            }
        }
        Some(Pat::Ctor(pattern::Ctor::Case(member), subs))
    }

    // ------------------------------------------------------------ effects

    /// Checks a `try` (or `try … else { … }`) expression: errors thrown by
    /// the operand go to the `else` block or propagate, and a `try` over
    /// nothing that throws is K2202 (with a fix removing it).
    fn try_expr(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let operand = node.children().find_map(ast::Expr::cast);
        let else_block = node.children().find_map(ast::Block::cast);
        let try_kw = token(node, SyntaxKind::TryKw).map_or(node.text_range(), |t| t.text_range());
        self.fcx().tries.push(TryCx {
            range: node.text_range(),
            has_else: else_block.is_some(),
            saw_throwing: false,
            saw_unknown: false,
            reported: false,
        });
        let ty = operand
            .as_ref()
            .map_or(Ty::Error, |e| self.infer(e, expected));
        let t = self.fcx().tries.pop();
        if let Some(t) = t {
            if !t.saw_throwing && !t.saw_unknown && operand.is_some() && !ty.contains_error() {
                let mut d = Diagnostic::new(
                    codes::NEEDLESS_TRY,
                    self.span(try_kw),
                    "nothing in this expression can throw",
                );
                if else_block.is_none() {
                    let end = operand
                        .as_ref()
                        .map_or(try_kw.end(), |o| o.syntax().text_range().start());
                    d = d.with_fix(Fix::replace(
                        "Remove `try `",
                        self.span(TextRange::new(try_kw.start(), end)),
                        "",
                    ));
                }
                self.report(d);
            }
        }
        if let Some(block) = else_block {
            let stmts: Vec<ast::Stmt> = block.statements().collect();
            let before = self.flow.clone();
            let value = self.resolve(&ty);
            let exp = if value.contains_error() || matches!(value, Ty::Var(_)) {
                None
            } else {
                Some(value.clone())
            };
            self.value_stmts(&stmts, exp.as_ref(), exp.is_some());
            self.flow = before;
        }
        ty
    }

    /// Checks an `await` expression: K2205 outside an async function (with a
    /// fix adding `async` to the enclosing function), and the operand is
    /// checked with [`FnCx::awaits`] raised so its async calls are awaited.
    fn await_expr(&mut self, node: &SyntaxNode, expected: Option<&Ty>) -> Ty {
        let kw = token(node, SyntaxKind::AwaitKw).map_or(node.text_range(), |t| t.text_range());
        if !self.fcx_ref().is_async {
            let mut d = Diagnostic::new(
                codes::AWAIT_OUTSIDE_ASYNC,
                self.span(kw),
                "`await` is allowed only in an async function",
            );
            let points = self
                .fcx_ref()
                .decl_node
                .as_ref()
                .filter(|n| n.kind() == SyntaxKind::FuncDecl)
                .and_then(SigPoints::of);
            if let Some(points) = points {
                d = d
                    .with_label(self.span(points.name), "declared here without `async`")
                    .with_fix(
                        Fix::replace(
                            "Add `async`",
                            self.span(TextRange::empty(points.after_params)),
                            " async",
                        )
                        .maybe_incorrect(),
                    );
            }
            self.report(d);
        }
        self.fcx().awaits += 1;
        let ty = node
            .children()
            .find_map(ast::Expr::cast)
            .map_or(Ty::Error, |e| self.infer(&e, expected));
        self.fcx().awaits -= 1;
        // Awaiting a query runs it: `await notes.filter { … }` is a list.
        let query = self.cx.p.lookup_qualified("k.db.Query");
        match self.resolve(&ty) {
            Ty::Named(d, args) if query == Some(d) => {
                Ty::generic(self.cx.b.list.unwrap_or(d), args.to_vec())
            }
            _ => ty,
        }
    }

    /// Applies the effects of calling a function of the given kind at
    /// `call` (sema.md §10).
    fn call_effects(&mut self, call: &SyntaxNode, is_async: bool, throws: Option<Ty>) {
        let range = call.text_range();
        let parent_await = call.parent().filter(|p| p.kind() == SyntaxKind::AwaitExpr);
        if is_async && self.fcx_ref().awaits == 0 {
            let in_async = self.fcx_ref().is_async;
            let at = covering_expr_start(call, false);
            let fix = Fix::replace(
                "Insert `await `",
                Span::empty(self.span(range).file, at),
                "await ",
            );
            let d = Diagnostic::new(
                codes::MISSING_AWAIT,
                self.span(range),
                "this async call must be awaited",
            )
            .with_fix(if in_async { fix } else { fix.maybe_incorrect() });
            self.report(d);
        }
        let Some(err) = throws.map(|t| self.resolve(&t)) else {
            return;
        };
        let err = match err {
            Ty::Var(v) => {
                // An error-generic call whose block threw nothing.
                self.bind(v, Ty::Never, None);
                return;
            }
            Ty::Never => return,
            other => other,
        };
        if err.contains_error() {
            if let Some(t) = self.fcx().tries.last_mut() {
                t.saw_unknown = true;
            }
            if let Some(d) = self.fcx().dos.last_mut() {
                d.saw_unknown = true;
            }
            return;
        }
        let covered = !self.fcx_ref().tries.is_empty();
        if !covered {
            let span_range = parent_await.map_or(range, |p| p.text_range());
            let d = Diagnostic::new(
                codes::MISSING_TRY,
                self.span(span_range),
                "this call can throw and must be marked with `try`",
            )
            .with_fix({
                let fix = Fix::replace(
                    "Insert `try ` before the expression",
                    Span::empty(self.span(range).file, covering_expr_start(call, true)),
                    "try ",
                );
                // Where the error has nowhere to go, the inserted `try` would
                // only turn K2201 into K2203: the edit needs review.
                if self.error_can_propagate(&err) {
                    fix
                } else {
                    fix.maybe_incorrect()
                }
            });
            self.report(d);
            return;
        }
        let try_range = {
            let t = self.fcx().tries.last_mut();
            match t {
                Some(t) => {
                    t.saw_throwing = true;
                    t.range
                }
                None => range,
            }
        };
        self.throw_error(err, Vec::new(), try_range, true);
    }

    /// Notes a call whose callee couldn't be typed: the enclosing `try` and
    /// `do` treat it as possibly throwing, so they report nothing about it.
    fn unknown_call(&mut self) {
        if let Some(t) = self.fcx().tries.last_mut() {
            t.saw_unknown = true;
        }
        if let Some(d) = self.fcx().dos.last_mut() {
            d.saw_unknown = true;
        }
    }

    // ------------------------------------------------------------ operators

    /// Checks a prefix operator: `not`/`!` takes and gives `Bool`; `-` takes a
    /// number, a unit type or `Money` and gives the same type (K2013 for
    /// anything else). An expected optional type is unwrapped for the operand.
    fn prefix(&mut self, p: &ast::PrefixExpr, expected: Option<&Ty>) -> Ty {
        let op = p.op();
        let Some(operand) = p.operand() else {
            return Ty::Error;
        };
        let bool_ty = builtin(self.cx.b.bool_);
        match op.as_ref().map(SyntaxToken::kind) {
            Some(SyntaxKind::NotKw | SyntaxKind::Bang) => {
                self.check(&operand, &bool_ty);
                bool_ty
            }
            _ => {
                // `-` yields a number, which is wrapped into an optional
                // only after negation: the operand sees the payload type
                // (`let a: Int? = -(if c { 1 } else { 2 })`).
                let payload = expected.map(|e| match self.resolve(e) {
                    Ty::Optional(inner) => (*inner).clone(),
                    other => other,
                });
                let t = self.infer(&operand, payload.as_ref());
                let t = self.resolve(&t);
                if t.contains_error() {
                    return Ty::Error;
                }
                if self.is_numeric(&t) || self.is_unit(&t) || t.decl() == self.cx.b.money {
                    return t;
                }
                self.err(
                    codes::INVALID_OPERANDS,
                    p.syntax().text_range(),
                    format!("`-` is not defined for `{}`", t.render(self.cx.p)),
                );
                Ty::Error
            }
        }
    }

    /// Returns `true` if `t` is `Int`, `Double` or `Decimal`.
    fn is_numeric(&self, t: &Ty) -> bool {
        let b = &self.cx.b;
        let d = t.decl();
        d.is_some() && (d == b.int || d == b.double || d == b.decimal)
    }

    /// Returns `true` if `t` is a unit type (`Length`, `Duration`, `Angle`,
    /// `Percent`) or a length-like style token type (`Spacing`, `Radius`,
    /// `FontSize`).
    fn is_unit(&self, t: &Ty) -> bool {
        let b = &self.cx.b;
        let d = t.decl();
        d.is_some()
            && [
                b.length,
                b.duration,
                b.angle,
                b.percent,
                b.spacing,
                b.radius,
                b.font_size,
            ]
            .contains(&d)
    }

    /// Returns `true` if `e` is a numeric literal (integer or float), possibly
    /// negated.
    fn is_int_literal(e: &ast::Expr) -> bool {
        match e {
            ast::Expr::Literal(l) => l
                .token()
                .is_some_and(|t| matches!(t.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit)),
            ast::Expr::PrefixExpr(p) => p.operand().is_some_and(|o| Self::is_int_literal(&o)),
            _ => false,
        }
    }

    /// Checks a binary operator expression: logical operators on `Bool`,
    /// equality and comparison (giving `Bool`), arithmetic (see
    /// [`Checker::arith`]), ranges and `??`.
    fn binary(&mut self, b: &ast::BinaryExpr, expected: Option<&Ty>) -> Ty {
        let (Some(lhs), Some(rhs)) = (b.lhs(), b.rhs()) else {
            if let Some(l) = b.lhs() {
                self.infer(&l, None);
            }
            return Ty::Error;
        };
        let op = b.op().map(|t| t.kind());
        let bool_ty = builtin(self.cx.b.bool_);
        let range = b.syntax().text_range();
        match op {
            Some(
                SyntaxKind::AndKw | SyntaxKind::OrKw | SyntaxKind::AmpAmp | SyntaxKind::PipePipe,
            ) => {
                self.check(&lhs, &bool_ty);
                self.check(&rhs, &bool_ty);
                bool_ty
            }
            Some(SyntaxKind::EqEq | SyntaxKind::BangEq) => {
                let (l, r) = self.operands(&lhs, &rhs);
                if l.contains_error() || r.contains_error() {
                    return bool_ty;
                }
                // A `Secret<T>`, or a value revealed from one, is compared
                // with `matches`, in constant time (sema.md §9.7).
                if self.secret_operand(&lhs, &l) || self.secret_operand(&rhs, &r) {
                    self.err(
                        codes::TYPE_MISMATCH,
                        range,
                        "secrets have no `==`; compare with `matches(_:)`, which takes constant time",
                    );
                    return bool_ty;
                }
                // K2012: comparing a `Bool` with a literal.
                let lit = |e: &ast::Expr| match e {
                    ast::Expr::Literal(l) => l.token().and_then(|t| match t.kind() {
                        SyntaxKind::TrueKw => Some(true),
                        SyntaxKind::FalseKw => Some(false),
                        _ => None,
                    }),
                    _ => None,
                };
                let (lit_value, other, other_ty) = match (lit(&lhs), lit(&rhs)) {
                    (Some(v), None) => (Some(v), &rhs, r.clone()),
                    (None, Some(v)) => (Some(v), &lhs, l.clone()),
                    _ => (None, &lhs, l.clone()),
                };
                if let Some(v) = lit_value {
                    if self.is_bool(&other_ty) {
                        let negate = (op == Some(SyntaxKind::EqEq)) != v;
                        let text = self.text(other.syntax().text_range()).to_owned();
                        let replacement = if negate {
                            if matches!(
                                other,
                                ast::Expr::BinaryExpr(_)
                                    | ast::Expr::PrefixExpr(_)
                                    | ast::Expr::TryExpr(_)
                                    | ast::Expr::AwaitExpr(_)
                            ) {
                                format!("not ({text})")
                            } else {
                                format!("not {text}")
                            }
                        } else {
                            text
                        };
                        let d = Diagnostic::new(
                            codes::BOOL_LITERAL_COMPARISON,
                            self.span(range),
                            "comparing a `Bool` with a literal",
                        )
                        .with_fix(Fix::replace(
                            format!("Simplify to `{replacement}`"),
                            self.span(range),
                            replacement,
                        ));
                        self.report(d);
                        return bool_ty;
                    }
                }
                let is_nil = |e: &ast::Expr| matches!(e, ast::Expr::Literal(x) if x.token().is_some_and(|t| t.kind() == SyntaxKind::NilKw));
                if is_nil(&lhs) || is_nil(&rhs) {
                    return bool_ty;
                }
                let lo = self.deopt(&l);
                let ro = self.deopt(&r);
                let same = self.assignable_quiet(&r, &l)
                    || self.assignable_quiet(&l, &r)
                    || self.assignable_quiet(&lo, &ro);
                let equatable = self
                    .cx
                    .b
                    .equatable
                    .is_none_or(|e| self.cx.implements(&lo, e));
                if !same || !equatable {
                    self.err(
                        codes::INVALID_OPERANDS,
                        range,
                        format!(
                            "`{}` and `{}` can't be compared with `==`",
                            l.render(self.cx.p),
                            r.render(self.cx.p)
                        ),
                    );
                }
                bool_ty
            }
            Some(SyntaxKind::Lt | SyntaxKind::LtEq | SyntaxKind::Gt | SyntaxKind::GtEq) => {
                let (l, r) = self.operands(&lhs, &rhs);
                if l.contains_error() || r.contains_error() {
                    return bool_ty;
                }
                let same = self.assignable_quiet(&r, &l);
                let comparable = self
                    .cx
                    .b
                    .comparable
                    .is_none_or(|c| self.cx.implements(&l, c));
                if !same || !comparable {
                    self.err(
                        codes::INVALID_OPERANDS,
                        range,
                        format!(
                            "`{}` and `{}` can't be ordered",
                            l.render(self.cx.p),
                            r.render(self.cx.p)
                        ),
                    );
                }
                bool_ty
            }
            Some(SyntaxKind::QuestionQuestion) => {
                let l = self.infer(&lhs, expected.map(|e| Ty::optional(e.clone())).as_ref());
                let l = self.resolve(&l);
                if l.contains_error() {
                    self.infer(&rhs, None);
                    return Ty::Error;
                }
                let Ty::Optional(inner) = &l else {
                    if let Some(tok) = b.op() {
                        let start = tok.text_range().start();
                        let text = self.cx.p.file(self.file()).text;
                        let ws = text[..usize::from(start)].len()
                            - text[..usize::from(start)].trim_end().len();
                        let from = start - TextSize::from(u32::try_from(ws).unwrap_or(0));
                        let rhs_text = self.text(rhs.syntax().text_range());
                        let d = Diagnostic::new(
                            codes::NEEDLESS_OPTIONAL_OPERATOR,
                            self.span(tok.text_range()),
                            "the left side of `??` can't be `nil`",
                        )
                        .with_fix(Fix::replace(
                            format!("Remove `?? {rhs_text}`"),
                            self.span(TextRange::new(from, rhs.syntax().text_range().end())),
                            "",
                        ));
                        self.report(d);
                    }
                    self.infer(&rhs, Some(&l));
                    return l;
                };
                let inner = (**inner).clone();
                let r = self.infer(&rhs, Some(&inner));
                let r = self.resolve(&r);
                if let Ty::Optional(_) = r {
                    if !self.assignable(&r, &l, None) {
                        self.mismatch(&rhs, &r, &inner);
                    }
                    return l;
                }
                if !self.assignable(&r, &inner, Some(rhs.syntax().text_range())) {
                    self.mismatch(&rhs, &r, &inner);
                }
                inner
            }
            Some(SyntaxKind::DotDotLt | SyntaxKind::DotDotDot) => {
                let (l, r) = self.operands(&lhs, &rhs);
                let _ = self.assignable(&r, &l, None);
                let decl = if op == Some(SyntaxKind::DotDotLt) {
                    self.cx.b.half_open
                } else {
                    self.cx.b.closed
                };
                match decl {
                    Some(d) => Ty::generic(d, vec![l]),
                    None => Ty::Error,
                }
            }
            _ => {
                let (l, r) = self.operands(&lhs, &rhs);
                self.arith(op, &l, &r, range, &rhs, &lhs)
            }
        }
    }

    /// Infers both operands; a literal or implicit member takes the other
    /// side's type.
    fn operands(&mut self, lhs: &ast::Expr, rhs: &ast::Expr) -> (Ty, Ty) {
        let implicit = |e: &ast::Expr| {
            matches!(e, ast::Expr::ImplicitMemberExpr(_))
                || matches!(e, ast::Expr::CallExpr(c) if c.callee().is_some_and(|c| matches!(c, ast::Expr::ImplicitMemberExpr(_))))
        };
        let nil = |e: &ast::Expr| matches!(e, ast::Expr::Literal(l) if l.token().is_some_and(|t| t.kind() == SyntaxKind::NilKw));
        if implicit(lhs) || nil(lhs) || (Self::is_int_literal(lhs) && !Self::is_int_literal(rhs)) {
            let r = self.infer(rhs, None);
            let r = self.resolve(&r);
            let hint = if nil(lhs) {
                Ty::optional(r.clone())
            } else {
                r.clone()
            };
            let l = self.infer(lhs, Some(&hint));
            (self.resolve(&l), r)
        } else {
            let l = self.infer(lhs, None);
            let l = self.resolve(&l);
            let hint = if nil(rhs) {
                Ty::optional(l.clone())
            } else {
                l.clone()
            };
            let r = self.infer(rhs, Some(&hint));
            (l, self.resolve(&r))
        }
    }

    /// Types `l op r` for arithmetic operators.
    fn arith(
        &mut self,
        op: Option<SyntaxKind>,
        l: &Ty,
        r: &Ty,
        range: TextRange,
        _rhs: &ast::Expr,
        _lhs: &ast::Expr,
    ) -> Ty {
        let l = self.resolve(l);
        let r = self.resolve(r);
        if l.contains_error() || r.contains_error() {
            return Ty::Error;
        }
        let op = match op {
            Some(SyntaxKind::Plus | SyntaxKind::PlusEq) => '+',
            Some(SyntaxKind::Minus | SyntaxKind::MinusEq) => '-',
            Some(SyntaxKind::Star | SyntaxKind::StarEq) => '*',
            Some(SyntaxKind::Slash | SyntaxKind::SlashEq) => '/',
            Some(SyntaxKind::Percent | SyntaxKind::PercentEq) => '%',
            _ => return Ty::Error,
        };
        let b = &self.cx.b;
        let lengthy = |t: &Ty| {
            let d = t.decl();
            d.is_some() && [b.length, b.spacing, b.radius, b.font_size].contains(&d)
        };
        let invalid = |this: &mut Self| {
            this.err(
                codes::INVALID_OPERANDS,
                range,
                format!(
                    "`{op}` is not defined for `{}` and `{}`",
                    l.render(this.cx.p),
                    r.render(this.cx.p)
                ),
            );
            Ty::Error
        };
        if self.is_numeric(&l) && self.is_numeric(&r) {
            if l == r {
                return l;
            }
            return invalid(self);
        }
        if l.decl().is_some() && l.decl() == b.string && r == l && op == '+' {
            return l;
        }
        let date = self.cx.p.lookup_qualified("k.core.Date");
        if l.decl().is_some() && l.decl() == date {
            if (op == '+' || op == '-') && r.decl() == b.duration {
                return l;
            }
            if op == '-' && r == l {
                return builtin(b.duration);
            }
            return invalid(self);
        }
        let bytes = self.cx.p.lookup_qualified("k.core.Bytes");
        if op == '+'
            && r == l
            && (l.decl() == b.list || (bytes.is_some() && l.decl() == bytes))
            && l.decl().is_some()
        {
            return l;
        }
        let money = b.money;
        if l.decl().is_some() && l.decl() == money {
            if (op == '+' || op == '-') && r == l {
                return l;
            }
            // `Money * Int` scales by a quantity; a rate goes through
            // `multiplied(by:rounding:)` and there is no `/`
            // (docs/design/34-money.md).
            if op == '*' && r.decl().is_some() && r.decl() == b.int {
                return l;
            }
            return invalid(self);
        }
        let lu = self.is_unit(&l);
        let ru = self.is_unit(&r);
        if lu || ru {
            match op {
                '+' | '-' => {
                    if lu && ru {
                        if l == r || (lengthy(&l) && lengthy(&r)) {
                            return if l == r { l } else { builtin(b.length) };
                        }
                        let pct = |t: &Ty| t.decl().is_some() && t.decl() == b.percent;
                        if (pct(&l) && lengthy(&r)) || (lengthy(&l) && pct(&r)) {
                            return builtin(b.length);
                        }
                    }
                    self.err(
                        codes::MIXED_UNIT_DIMENSIONS,
                        range,
                        format!(
                            "`{}` and `{}` have different dimensions",
                            l.render(self.cx.p),
                            r.render(self.cx.p)
                        ),
                    );
                    return Ty::Error;
                }
                '*' => {
                    if lu && self.is_numeric(&r) {
                        return l;
                    }
                    if ru && self.is_numeric(&l) {
                        return r;
                    }
                    return invalid(self);
                }
                '/' => {
                    if lu && self.is_numeric(&r) {
                        return l;
                    }
                    if lu && ru && l == r {
                        return builtin(b.double);
                    }
                    return invalid(self);
                }
                _ => return invalid(self),
            }
        }
        invalid(self)
    }

    // ------------------------------------------------------------ members

    /// Checks a member expression `recv.name` (or `recv?.name`) read as a
    /// value, including the framework checks on reads of client state
    /// (K4013) and of `form.value` (K7001).
    ///
    /// The expected type is passed to the receiver only when the chain starts
    /// with an implicit member (`.idle.label`).
    fn member(&mut self, m: &ast::MemberExpr, expected: Option<&Ty>) -> Ty {
        let Some(recv) = m.receiver() else {
            return Ty::Error;
        };
        let hint = if expected.is_some() && self.implicit_chain(&recv) {
            expected.cloned()
        } else {
            None
        };
        let recv_ty = self.infer(&recv, hint.as_ref());
        let recv_ty = self.resolve(&recv_ty);
        let Some(name) = m.name() else {
            return Ty::Error;
        };
        let optional_chain = m.is_optional();
        let t = self
            .member_access(m.syntax(), &recv, &recv_ty, &name, optional_chain, false)
            .0;
        self.check_money_literal(m, &recv, &t);
        self.client_state_read(m, &name);
        self.form_value(&name);
        t
    }

    /// `.product(id: 1).tab(.reviews)`: a query `@Param` of a page, chained
    /// on a `Page` value (sema.md §12.7).
    fn page_modifier(
        &mut self,
        recv: &Ty,
        name: &SyntaxToken,
        node: &SyntaxNode,
        args: &[Arg],
    ) -> Option<Ty> {
        let page_ty = self.cx.b.page?;
        if self.resolve(recv).decl() != Some(page_ty) || matches!(recv, Ty::Meta(_)) {
            return None;
        }
        let field = self.cx.p.page_of.values().find_map(|page| {
            self.cx.p.decl(*page).members.iter().copied().find(|f| {
                let f = self.cx.p.member(*f);
                f.kind == MemberKind::Field
                    && f.has_annotation("Param")
                    && f.has_default
                    && f.name == name.text()
            })
        })?;
        self.note_res(name.text_range(), self.cx.member_resolution(field));
        let ty = self.cx.field_ty(field);
        match args.first().and_then(|a| a.expr.clone()) {
            Some(e) => {
                self.check(&e, &ty);
            }
            None => {
                let at = node.text_range().end();
                self.report(Diagnostic::new(
                    codes::MISSING_ARGUMENT,
                    Span::empty(self.span(node.text_range()).file, at),
                    format!("`.{}(…)` needs a value", name.text()),
                ));
            }
        }
        self.args_unchecked(args.get(1..).unwrap_or(&[]));
        Some(Ty::named(page_ty))
    }

    /// K4013: a server-rendered view reads a client-only service's state,
    /// which the server can't know.
    fn client_state_read(&mut self, m: &ast::MemberExpr, name: &SyntaxToken) {
        if !self.ssr {
            return;
        }
        let Some(Target::Member(field)) = self.targets.get(&name.text_range()).copied() else {
            return;
        };
        let member = self.cx.p.member(field);
        if member.kind != MemberKind::Field {
            return;
        }
        let owner = member.owner;
        // The stdlib's client-only services (`Theme`) carry what the server
        // needs to render (the theme cookie).
        if self.cx.p.decl(owner).named_kind() != Some(DeclarationKind::Service)
            || self.cx.p.is_stdlib(owner)
        {
            return;
        }
        let side = crate::di::side(self.cx, owner);
        if !side.client || side.server {
            return;
        }
        if !self
            .owner
            .decl
            .is_some_and(|d| crate::di::reaches_client(self.cx, d))
        {
            return;
        }
        let Some((app, at)) = crate::di::server_rendered_app(self.cx) else {
            return;
        };
        let app_file = self.cx.p.decl(app).file;
        let d = Diagnostic::new(
            codes::CLIENT_STATE_IN_SERVER_RENDER,
            self.span(m.syntax().text_range()),
            format!(
                "`{}` is client-only: the server renders this page without its state",
                self.cx.p.decl_name(owner)
            ),
        )
        .with_fix(
            Fix::replace(
                "Add `@Rendering(.client)` to the app",
                Span::empty(self.cx.p.file(app_file).id, at),
                "@Rendering(.client)\n",
            )
            .maybe_incorrect(),
        );
        self.report(d);
    }

    /// Returns `true` if `recv` is a member access or method call whose
    /// member's own type is optional (`u?.address` with `address: Address?`).
    ///
    /// The rest of an optional chain reads the value `?.` unwrapped, but
    /// not an optional the member itself has: `u?.address.city` needs a
    /// second `?.` exactly like `v.address.city` (K2017).
    fn member_itself_optional(&self, recv: &ast::Expr) -> bool {
        let (name, is_call) = match recv {
            ast::Expr::MemberExpr(m) => (m.name(), false),
            ast::Expr::CallExpr(c) => match c.callee() {
                Some(ast::Expr::MemberExpr(m)) => (m.name(), true),
                _ => return false,
            },
            _ => return false,
        };
        let Some(name) = name else { return false };
        let Some(Target::Member(m)) = self.targets.get(&name.text_range()) else {
            return false;
        };
        let member = self.cx.p.member(*m);
        let own = match (member.kind, is_call) {
            (MemberKind::Field | MemberKind::Computed, false) => self.cx.field_ty(*m),
            (MemberKind::Func | MemberKind::Rpc, true) => {
                self.cx.sig(SigKey::Member(*m)).ret.clone()
            }
            _ => return false,
        };
        matches!(own, Ty::Optional(_))
    }

    /// Resolves `recv.name`; returns the member's value type and, for
    /// methods and payload cases, the signature to call.
    #[allow(clippy::type_complexity)]
    fn member_access(
        &mut self,
        node: &SyntaxNode,
        recv: &ast::Expr,
        recv_ty: &Ty,
        name: &SyntaxToken,
        optional_chain: bool,
        called: bool,
    ) -> (
        Ty,
        Option<(Rc<Sig>, Vec<(GenId, Ty)>, Option<MemberId>)>,
        bool,
    ) {
        if recv_ty.is_error() || matches!(recv_ty, Ty::Meta(t) if t.is_error()) {
            return (Ty::Error, None, false);
        }
        let mut base = recv_ty.clone();
        let mut lifted = false;
        if optional_chain {
            match &base {
                Ty::Optional(inner) => {
                    base = (**inner).clone();
                    lifted = true;
                }
                _ => {
                    // K2018: `?.` on a value that can't be nil.
                    if let Some(tok) = token(node, SyntaxKind::QuestionDot) {
                        let d = Diagnostic::new(
                            codes::NEEDLESS_OPTIONAL_OPERATOR,
                            self.span(tok.text_range()),
                            "the receiver of `?.` can't be `nil`",
                        )
                        .with_fix(Fix::replace(
                            "Replace `?.` with `.`",
                            self.span(tok.text_range()),
                            ".",
                        ));
                        self.report(d);
                    }
                }
            }
        } else if let (Ty::Optional(inner), true) = (
            &base,
            self.chained.contains(&recv.syntax().text_range())
                && !self.member_itself_optional(recv),
        ) {
            // `a?.b.c`: the rest of an optional chain reads the unwrapped
            // value.
            base = (**inner).clone();
            lifted = true;
        } else if let Ty::Optional(inner) = &base {
            if !matches!(&**inner, Ty::Error) {
                // K2017: a member of an optional without `?.`.
                let probe =
                    self.cx
                        .lookup_member(self.file(), self.owner.decl, inner, name.text(), false);
                if !matches!(probe, MemberFound::Missing) {
                    if let Some(dot) = token(node, SyntaxKind::Dot) {
                        let d = Diagnostic::new(
                            codes::UNWRAP_REQUIRED,
                            self.span(recv.syntax().text_range()),
                            "the value may be `nil`: use `?.` or unwrap it first",
                        )
                        .with_fix(
                            Fix::replace("Use `?.`", self.span(dot.text_range()), "?.")
                                .maybe_incorrect(),
                        );
                        self.report(d);
                    }
                    base = (**inner).clone();
                    lifted = true;
                }
            }
        }
        let (statics, target) = match &base {
            Ty::Meta(inner) => (true, (**inner).clone()),
            other => (false, other.clone()),
        };
        let mut found =
            self.cx
                .lookup_member(self.file(), self.owner.decl, &target, name.text(), statics);
        if called {
            if let (MemberFound::Member(m, subst), Some(d)) = (&found, target.decl()) {
                let kind = self.cx.p.member(*m).kind;
                if !matches!(kind, MemberKind::Func | MemberKind::Case | MemberKind::Rpc) {
                    if let Some(f) = self
                        .cx
                        .p
                        .decl(d)
                        .funcs
                        .get(name.text())
                        .and_then(|f| f.first())
                    {
                        found = MemberFound::Member(*f, subst.clone());
                    }
                }
            }
        }
        if lifted {
            self.chained.insert(node.text_range());
            if let Some(parent) = node.parent().filter(|p| p.kind() == SyntaxKind::CallExpr) {
                self.chained.insert(parent.text_range());
            }
        }
        let wrap = |t: Ty| if lifted { Ty::optional(t) } else { t };
        if let MemberFound::Member(m, _) = &found {
            if self.is_session_user(*m) && self.guarded() {
                let t = self.cx.member_value_ty(*m, &[]);
                self.note_res(name.text_range(), self.cx.member_resolution(*m));
                return (wrap(self.deopt(&t)), None, lifted);
            }
        }
        match found {
            MemberFound::Member(member, subst) => {
                self.note_res(name.text_range(), self.cx.member_resolution(member));
                self.targets
                    .insert(name.text_range(), Target::Member(member));
                let mkind = self.cx.p.member(member).kind;
                match mkind {
                    MemberKind::Func | MemberKind::Rpc => {
                        let sig = self.cx.sig(SigKey::Member(member));
                        if called {
                            return (Ty::Error, Some((sig, subst, Some(member))), lifted);
                        }
                        let (params, ret, is_async, throws) = self.instantiate(&sig, &subst);
                        (wrap(Ty::func(params, is_async, throws, ret)), None, lifted)
                    }
                    MemberKind::Case if self.cx.p.member(member).node.is_none() && called => {
                        // A generated case (`Page.product`): its payload is
                        // checked by the routing layer.
                        (Ty::Error, None, false)
                    }
                    MemberKind::Case if self.cx.case_has_payload(member) => {
                        let sig = self.cx.case_sig(member);
                        let subst = if subst.is_empty() {
                            self.fresh_subst(target.decl())
                        } else {
                            subst
                        };
                        if called {
                            return (Ty::Error, Some((sig, subst, Some(member))), lifted);
                        }
                        let at =
                            Span::empty(self.span(node.text_range()).file, node.text_range().end());
                        self.report(Diagnostic::new(
                            codes::MISSING_ARGUMENT,
                            at,
                            format!("`.{}` needs its payload", name.text()),
                        ));
                        (wrap(apply(&sig.ret, &subst)), None, lifted)
                    }
                    MemberKind::Case => {
                        let subst = if subst.is_empty()
                            || subst.iter().any(|(_, t)| matches!(t, Ty::Var(_)))
                        {
                            let fresh = self.fresh_subst(target.decl());
                            if target
                                .args()
                                .iter()
                                .all(|a| matches!(self.resolve(a), Ty::Var(_)))
                                && !fresh.is_empty()
                            {
                                // `Fetch.idle` can't fix `T` without context.
                                fresh
                            } else {
                                subst
                            }
                        } else {
                            subst
                        };
                        (wrap(self.cx.member_value_ty(member, &subst)), None, lifted)
                    }
                    _ => {
                        let t = self.cx.member_value_ty(member, &subst);
                        let owner = self.cx.p.member(member).owner;
                        let component_like = matches!(
                            self.cx.p.decl(owner).named_kind(),
                            Some(
                                DeclarationKind::Component
                                    | DeclarationKind::Page
                                    | DeclarationKind::Layout
                            )
                        );
                        if called && component_like && mkind == MemberKind::Field {
                            // A chained prop or slot (`Card { … }.footer { … }`),
                            // checked by the framework layer.
                            self.quiet_args = true;
                            let sig = Sig {
                                generics: Vec::new(),
                                params: vec![SigParam {
                                    label: None,
                                    name: String::new(),
                                    ty: Ty::Error,
                                    has_default: true,
                                    name_range: name.text_range(),
                                }],
                                is_async: false,
                                throws: None,
                                ret: target.clone(),
                            };
                            return (Ty::Error, Some((Rc::new(sig), Vec::new(), None)), lifted);
                        }
                        if called
                            && self.cx.p.decl(owner).has_annotation("Builder")
                            && mkind == MemberKind::Field
                        {
                            // A chained optional field of a builder struct
                            // (`.query(q)`): a setter returning the value.
                            let sig = Sig {
                                generics: Vec::new(),
                                params: vec![SigParam {
                                    label: None,
                                    name: String::new(),
                                    ty: t,
                                    has_default: false,
                                    name_range: name.text_range(),
                                }],
                                is_async: false,
                                throws: None,
                                ret: target.clone(),
                            };
                            return (Ty::Error, Some((Rc::new(sig), Vec::new(), None)), lifted);
                        }
                        (wrap(t), None, lifted)
                    }
                }
            }
            MemberFound::Token(member) => {
                self.note_res(name.text_range(), self.cx.member_resolution(member));
                (wrap(self.cx.field_ty(member)), None, lifted)
            }
            MemberFound::NotVisible(member) => {
                let what = &self.cx.p.member(member).name;
                self.err(
                    codes::NOT_VISIBLE,
                    name.text_range(),
                    format!("`{what}` is not visible here"),
                );
                (Ty::Error, None, false)
            }
            MemberFound::Unsatisfied(member) => {
                self.note_res(name.text_range(), self.cx.member_resolution(member));
                self.err(
                    codes::UNSATISFIED_CONSTRAINT,
                    recv.syntax().text_range(),
                    format!(
                        "`{}` doesn't meet the constraints of `{}`",
                        target.render(self.cx.p),
                        name.text()
                    ),
                );
                (Ty::Error, None, false)
            }
            MemberFound::WrongKind | MemberFound::Missing => {
                if matches!(target, Ty::Var(_) | Ty::Error) {
                    return (Ty::Error, None, false);
                }
                // A job's generated `static let kind: JobKind<Payload>`
                // (30-background-jobs.md), typed by its `Job<…>` payload.
                if let Some(kind) = target
                    .decl()
                    .filter(|_| statics && name.text() == "kind")
                    .and_then(|d| crate::locks::job_payload(self.cx, d))
                    .and_then(|payload| {
                        let k = self.cx.p.lookup_qualified("k.server.JobKind")?;
                        Some(Ty::Named(k, vec![payload].into()))
                    })
                {
                    return (kind, None, false);
                }
                if target.decl().is_some_and(|d| self.cx.is_open(d, statics)) {
                    return (Ty::Error, None, false);
                }
                let candidates = self.member_candidates(&target, statics);
                let mut d = Diagnostic::new(
                    codes::UNKNOWN_MEMBER,
                    self.span(name.text_range()),
                    format!(
                        "`{}` has no member `{}`",
                        target.render(self.cx.p),
                        name.text()
                    ),
                );
                if let Some(c) = closest(name.text(), candidates.iter().map(String::as_str)) {
                    d = d.with_fix(
                        Fix::replace(
                            format!("Replace with `{c}`"),
                            self.span(name.text_range()),
                            c,
                        )
                        .maybe_incorrect(),
                    );
                }
                self.report(d);
                (Ty::Error, None, false)
            }
        }
    }

    /// Returns a substitution of `decl`'s generic parameters by fresh
    /// inference variables; empty for `None`.
    fn fresh_subst(&mut self, decl: Option<DeclId>) -> Vec<(GenId, Ty)> {
        let Some(d) = decl else { return Vec::new() };
        let gens = self.cx.p.decl(d).generics.clone();
        gens.into_iter().map(|g| (g, self.fresh())).collect()
    }

    /// Returns the names of `ty`'s static (with `statics`) or instance
    /// members, its extensions' included and initializers excluded; used for
    /// "did you mean" suggestions.
    fn member_candidates(&self, ty: &Ty, statics: bool) -> Vec<String> {
        let Some(d) = ty.decl() else {
            return Vec::new();
        };
        let mut out: Vec<String> = self
            .cx
            .p
            .decl(d)
            .members
            .iter()
            .map(|m| self.cx.p.member(*m))
            .filter(|m| m.is_static == statics && m.kind != MemberKind::Init)
            .map(|m| m.name.clone())
            .collect();
        for ext in self
            .cx
            .extensions_of
            .borrow()
            .get(&d)
            .cloned()
            .unwrap_or_default()
        {
            if self.cx.extension_visible(ext, self.file()) {
                out.extend(
                    self.cx
                        .p
                        .decl(ext)
                        .members
                        .iter()
                        .map(|m| self.cx.p.member(*m).name.clone()),
                );
            }
        }
        out
    }

    // ------------------------------------------------------------ calls

    /// Checks a call expression: works out what is called (an implicit member,
    /// a type's initializer, a function, a method, a payload case or a value
    /// of function type), selects the overload and checks the arguments
    /// against the signature (see [`Checker::apply_sig`]).
    fn call(&mut self, c: &ast::CallExpr, expected: Option<&Ty>) -> Ty {
        let node = c.syntax().clone();
        let args = self.collect_args(c);
        let Some(callee) = c.callee() else {
            self.args_unchecked(&args);
            return Ty::Error;
        };
        let expected = expected.cloned();
        // What is being called?
        match &callee {
            ast::Expr::ImplicitMemberExpr(m) => {
                let (ty, sig) = self.implicit_member(m.syntax(), expected.as_ref(), true);
                self.node_types
                    .push((callee.syntax().text_range(), ty.clone()));
                match sig {
                    Some((sig, subst)) => {
                        let ret = self.apply_sig(
                            &node,
                            &sig,
                            &subst,
                            &args,
                            expected.as_ref(),
                            CallKind::Plain,
                        );
                        self.check_status_literal(&node, m.syntax(), &args, &ty);
                        ret
                    }
                    None => {
                        self.args_unchecked(&args);
                        ty
                    }
                }
            }
            ast::Expr::MemberExpr(m) => {
                let Some(recv) = m.receiver() else {
                    self.args_unchecked(&args);
                    return Ty::Error;
                };
                let hint = if expected.is_some() && self.implicit_chain(&recv) {
                    expected.clone()
                } else {
                    None
                };
                let recv_ty = self.infer(&recv, hint.as_ref());
                let recv_ty = self.resolve(&recv_ty);
                let Some(name) = m.name() else {
                    self.args_unchecked(&args);
                    return Ty::Error;
                };
                if name.text() == "copy" {
                    if let Some(t) = self.copy_call(&recv_ty, &name, &node, &args) {
                        return t;
                    }
                }
                if let Some(t) = self.page_modifier(&recv_ty, &name, &node, &args) {
                    return t;
                }
                let (ty, sig, lifted) =
                    self.member_access(m.syntax(), &recv, &recv_ty, &name, m.is_optional(), true);
                match sig {
                    Some((sig, subst, member)) => {
                        let kind = CallKind::Plain;
                        let (sig, member) = match member {
                            Some(mm) => {
                                let chosen = self.select_overload(mm, &args, &subst);
                                if chosen == mm {
                                    (sig, member)
                                } else {
                                    self.note_res_force(
                                        name.text_range(),
                                        self.cx.member_resolution(chosen),
                                    );
                                    (self.cx.sig(SigKey::Member(chosen)), Some(chosen))
                                }
                            }
                            None => (sig, member),
                        };
                        // `Result.of`, generic statics: fresh type arguments.
                        let subst = if matches!(recv_ty, Ty::Meta(_)) {
                            subst
                                .into_iter()
                                .map(|(g, t)| {
                                    let t = self.resolve(&t);
                                    (g, t)
                                })
                                .collect()
                        } else {
                            subst
                        };
                        let quiet = std::mem::take(&mut self.quiet_args);
                        if quiet {
                            self.quiet += 1;
                            self.args_unchecked(&args);
                            self.quiet -= 1;
                            return sig.ret.clone();
                        }
                        let handle = member.is_some_and(|mm| {
                            self.cx.p.member(mm).name == "handle"
                                && self.cx.p.lookup_qualified("k.validation.Form")
                                    == Some(self.cx.p.member(mm).owner)
                        });
                        let self_recv = matches!(&recv, ast::Expr::NameExpr(r)
                            if r.syntax().first_token().is_some_and(|t| t.kind() == SyntaxKind::SelfKw));
                        if let (true, Some(mm)) = (self_recv, member) {
                            self.derived_call(mm, node.text_range());
                        }
                        self.handle_depth += u32::from(handle);
                        let query = member.is_some_and(|mm| {
                            self.cx.p.lookup_qualified("k.db.Query")
                                == Some(self.cx.p.member(mm).owner)
                        });
                        let update =
                            query && member.is_some_and(|mm| self.cx.p.member(mm).name == "update");
                        self.query_lambda_depth += u32::from(query);
                        self.query_update_depth += u32::from(update);
                        let ret =
                            self.apply_sig(&node, &sig, &subst, &args, expected.as_ref(), kind);
                        if let Some(mm) = member {
                            self.check_numeric_sum(mm, &sig, &recv_ty, &name, &node);
                            self.check_remote_client(mm, &ret, &name, &node);
                            self.check_set_cookie(mm, &args);
                        }
                        self.query_update_depth -= u32::from(update);
                        self.query_lambda_depth -= u32::from(query);
                        self.handle_depth -= u32::from(handle);
                        let user_method = member
                            .is_some_and(|mm| !self.cx.p.is_stdlib(self.cx.p.member(mm).owner));
                        self.untranslatable_call(&node, user_method);
                        if let Some(mm) = member {
                            let mname = &self.cx.p.member(mm).name;
                            let owner = self.cx.p.member(mm).owner;
                            let b = &self.cx.b;
                            let collection = [b.list, b.map, b.set].contains(&Some(owner));
                            if MUTATING_METHODS.contains(&mname.as_str())
                                && collection
                                && !self.derived_write(&recv, node.text_range())
                            {
                                self.check_mutable(&recv);
                            }
                            self.client_only_call(mm, &node);
                        }
                        if lifted { Ty::optional(ret) } else { ret }
                    }
                    None => {
                        let generated_copy = self.copy_call(&recv_ty, &name, &node, &args);
                        if let Some(t) = generated_copy {
                            return t;
                        }
                        self.call_value(&node, &callee, ty, &args, expected.as_ref())
                    }
                }
            }
            ast::Expr::NameExpr(n) => {
                let Some(name) = n.syntax().first_token() else {
                    self.args_unchecked(&args);
                    return Ty::Error;
                };
                // A local of function type, a member, or a declaration.
                // A field of function type is called like a function (a
                // scoped slot `row(item)`), unless a declaration of that
                // name is in scope (`chart()` next to `@Load let chart`).
                let field_of_self = self.find_local(name.text()).is_none()
                    && !matches!(self.cx.lookup(self.file(), name.text()), Found::Decl(_))
                    && self.owner.self_ty.clone().is_some_and(|self_ty| {
                        [false, true].iter().any(|statics| {
                            matches!(
                                self.cx.lookup_member(
                                    self.file(),
                                    self.owner.decl,
                                    &self_ty,
                                    name.text(),
                                    *statics,
                                ),
                                MemberFound::Member(m, _)
                                    if matches!(
                                        self.cx.p.member(m).kind,
                                        MemberKind::Field | MemberKind::Computed
                                    )
                            )
                        })
                    });
                if name.kind() != SyntaxKind::SelfKw
                    && self.find_local(name.text()).is_none()
                    && !field_of_self
                {
                    if let Some(self_ty) = self.owner.self_ty.clone() {
                        for statics in [false, true] {
                            if let MemberFound::Member(m, subst) = self.cx.lookup_member(
                                self.file(),
                                self.owner.decl,
                                &self_ty,
                                name.text(),
                                statics,
                            ) {
                                let kind = self.cx.p.member(m).kind;
                                if matches!(kind, MemberKind::Func | MemberKind::Rpc) {
                                    self.note_res(name.text_range(), self.cx.member_resolution(m));
                                    self.derived_call(m, node.text_range());
                                    let sig = self.cx.sig(SigKey::Member(m));
                                    return self.apply_sig(
                                        &node,
                                        &sig,
                                        &subst,
                                        &args,
                                        expected.as_ref(),
                                        CallKind::Plain,
                                    );
                                }
                                if kind == MemberKind::Case && self.cx.case_has_payload(m) {
                                    self.note_res(name.text_range(), self.cx.member_resolution(m));
                                    let sig = self.cx.case_sig(m);
                                    return self.apply_sig(
                                        &node,
                                        &sig,
                                        &subst,
                                        &args,
                                        expected.as_ref(),
                                        CallKind::Plain,
                                    );
                                }
                            }
                        }
                    }
                    match self.cx.lookup(self.file(), name.text()) {
                        Found::Decl(d) => {
                            let decl = self.cx.p.decl(d);
                            if decl.named_kind() == Some(DeclarationKind::Function) {
                                self.note_res(name.text_range(), self.cx.decl_resolution(d));
                                let sig = self.cx.sig(SigKey::Decl(d));
                                let ret = self.apply_sig(
                                    &node,
                                    &sig,
                                    &[],
                                    &args,
                                    expected.as_ref(),
                                    CallKind::Plain,
                                );
                                let user = !self.cx.p.is_stdlib(d);
                                self.untranslatable_call(&node, user);
                                return ret;
                            }
                            if decl.is_type() {
                                self.note_res(name.text_range(), self.cx.decl_resolution(d));
                                return self.construct(
                                    &node,
                                    d,
                                    name.text_range(),
                                    &args,
                                    expected.as_ref(),
                                );
                            }
                        }
                        Found::Missing if name.text() == "reload" && self.owner_has_load() => {
                            self.reload_ref(name.text_range());
                            self.args_unchecked(&args);
                            return builtin(self.cx.b.void);
                        }
                        Found::Missing => {
                            // A lowercase unknown callee in a view is an
                            // element; elsewhere K1201.
                            let ty = self.infer(&callee, None);
                            self.args_unchecked(&args);
                            self.unknown_call();
                            return ty;
                        }
                        _ => {}
                    }
                }
                let ty = self.infer(&callee, None);
                self.call_value(&node, &callee, ty, &args, expected.as_ref())
            }
            _ => {
                let ty = self.infer(&callee, None);
                self.call_value(&node, &callee, ty, &args, expected.as_ref())
            }
        }
    }

    /// Whether a postfix chain starts with an implicit member, memoized for
    /// every node of the chain so long chains stay linear.
    fn implicit_chain(&mut self, e: &ast::Expr) -> bool {
        if let Some(&known) = self.implicit_chains.get(&e.syntax().text_range()) {
            return known;
        }
        let result = starts_with_implicit(e);
        let mut current = Some(e.clone());
        while let Some(c) = current {
            if self
                .implicit_chains
                .insert(c.syntax().text_range(), result)
                .is_some()
            {
                break;
            }
            current = match &c {
                ast::Expr::MemberExpr(m) => m.receiver(),
                ast::Expr::CallExpr(call) => call.callee(),
                _ => None,
            };
        }
        result
    }

    /// Picks the overload of `member`'s function name whose labels and
    /// parameter types accept `args`.
    fn select_overload(
        &mut self,
        member: MemberId,
        args: &[Arg],
        subst: &[(GenId, Ty)],
    ) -> MemberId {
        let m = self.cx.p.member(member);
        let Some(all) = self.cx.p.decl(m.owner).funcs.get(&m.name).cloned() else {
            return member;
        };
        if all.len() < 2 {
            return member;
        }
        // Argument types, inferred quietly.
        self.quiet += 1;
        let record = self.record;
        self.record = false;
        let tys: Vec<Option<Ty>> = args
            .iter()
            .map(|a| {
                if a.is_lambda {
                    None
                } else {
                    a.expr.as_ref().map(|e| {
                        let t = self.infer(e, None);
                        self.resolve(&t)
                    })
                }
            })
            .collect();
        self.record = record;
        self.quiet -= 1;
        let trailing = args
            .last()
            .is_some_and(|a| a.is_lambda && a.label.is_none());
        for &candidate in &all {
            let sig = self.cx.sig(SigKey::Member(candidate));
            if trailing
                && !sig
                    .params
                    .last()
                    .is_some_and(|p| matches!(p.ty, Ty::Func(_)))
            {
                continue;
            }
            // Every label must name a parameter, a lambda's included
            // (`contains(where: { … })` is not `contains(_:)`); an unlabelled
            // value needs an unlabelled parameter. An unlabelled lambda is
            // matched by position (the trailing-lambda test above).
            let labels_ok = args.iter().all(|a| match &a.label {
                Some((l, _)) => sig.params.iter().any(|p| {
                    p.label.as_deref() == Some(l.as_str())
                        && (!a.is_lambda || matches!(p.ty, Ty::Func(_)))
                }),
                None => a.is_lambda || sig.params.iter().any(|p| p.label.is_none()),
            });
            if !labels_ok {
                continue;
            }
            let mut ok = true;
            for (a, t) in args.iter().zip(&tys) {
                let Some(t) = t else { continue };
                let param = match &a.label {
                    Some((l, _)) => sig
                        .params
                        .iter()
                        .find(|p| p.label.as_deref() == Some(l.as_str())),
                    None => sig.params.iter().find(|p| p.label.is_none()),
                };
                if let Some(p) = param {
                    let pt = apply(&p.ty, subst);
                    if !pt.has_params() && !self.assignable_quiet(t, &pt) {
                        ok = false;
                    }
                }
            }
            if ok {
                return candidate;
            }
        }
        member
    }

    /// Records the resolution of `range` like [`Checker::note_res`], replacing
    /// one already recorded for the same range.
    fn note_res_force(&self, range: TextRange, resolution: Resolution) {
        if self.record {
            let key = (self.cx.p.file(self.file()).id, range);
            self.cx.resolutions.borrow_mut().insert(key, resolution);
        }
    }

    /// Checks `list.sum()`, which only lists of `Int` or `Double` have:
    /// a list of `Money` is K2402 with a fix to `sum(in:_:)`
    /// (docs/design/34-money.md), any other element type K2009. `sum` is
    /// declared without a constraint in `List.k` because no interface
    /// describes "Int or Double"; this check is that constraint.
    fn check_numeric_sum(
        &mut self,
        member: MemberId,
        sig: &Sig,
        recv_ty: &Ty,
        name: &SyntaxToken,
        call: &SyntaxNode,
    ) {
        let b = &self.cx.b;
        let m = self.cx.p.member(member);
        if m.name != "sum" || !sig.params.is_empty() || b.list.is_none() || Some(m.owner) != b.list
        {
            return;
        }
        let list = match recv_ty {
            Ty::Optional(inner) => (**inner).clone(),
            other => other.clone(),
        };
        let Some(element) = list.args().first().cloned() else {
            return;
        };
        let element = self.resolve(&element);
        if element.contains_error() || matches!(element, Ty::Var(_) | Ty::Param(_)) {
            return;
        }
        let decl = element.decl();
        if decl.is_some() && (decl == b.int || decl == b.double) {
            return;
        }
        let range = TextRange::new(name.text_range().start(), call.text_range().end());
        if decl.is_some() && decl == b.money {
            let d = Diagnostic::new(
                codes::MONEY_SUM_WITHOUT_CURRENCY,
                self.span(range),
                "a sum over `Money` needs a currency: use `sum(in:_:)`, which is `Money.zero` for an empty list",
            )
            .with_fix(
                Fix::replace(
                    "Sum in a currency with `sum(in:_:)`",
                    self.span(range),
                    "sum(in: .usd) { amount => amount }",
                )
                .maybe_incorrect(),
            );
            self.report(d);
            return;
        }
        let rendered = element.render(self.cx.p);
        self.err(
            codes::UNSATISFIED_CONSTRAINT,
            range,
            format!("`sum()` adds `Int` or `Double` elements, and `{rendered}` is neither"),
        );
    }

    /// K5013: `HttpResponse.headers([...])` with a literal `set-cookie`
    /// key, in any case (27-endpoint-pages.md: cookies are set only through
    /// `Session`). A computed name panics at run time instead.
    fn check_set_cookie(&mut self, member: MemberId, args: &[Arg]) {
        let m = self.cx.p.member(member);
        if m.name != "headers" || self.cx.p.lookup_qualified("k.http.HttpResponse") != Some(m.owner)
        {
            return;
        }
        let Some(ast::Expr::MapExpr(map)) = args.first().and_then(|a| a.expr.as_ref()) else {
            return;
        };
        let keys: Vec<ast::Expr> = map
            .syntax()
            .children()
            .filter(|c| c.kind() == SyntaxKind::MapEntry)
            .filter_map(|e| e.children().find_map(ast::Expr::cast))
            .collect();
        for key in keys {
            let ast::Expr::StringLit(lit) = &key else {
                continue;
            };
            if !lit
                .plain_value()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("set-cookie"))
            {
                continue;
            }
            let d = Diagnostic::new(
                codes::SET_COOKIE_HEADER,
                self.span(key.syntax().text_range()),
                "a response can't set `set-cookie`: cookies are set only through `Session`",
            )
            .with_help(
                "sign the user in or out with `SessionManager`, which sets the session cookie",
            );
            self.report(d);
        }
    }

    /// K4026: `RemoteClient.at(url:token:)` returns a client of the
    /// protocol its result is used as
    /// (docs/design/40-workspaces-and-services.md), so that type must be
    /// known and be a protocol.
    fn check_remote_client(
        &mut self,
        member: MemberId,
        ret: &Ty,
        name: &SyntaxToken,
        call: &SyntaxNode,
    ) {
        let m = self.cx.p.member(member);
        if m.name != "at" || self.cx.p.lookup_qualified("k.rpc.RemoteClient") != Some(m.owner) {
            return;
        }
        let ret = self.resolve(ret);
        if ret.contains_error() {
            return;
        }
        let protocol = ret
            .decl()
            .is_some_and(|d| self.cx.p.decl(d).named_kind() == Some(DeclarationKind::Protocol));
        if protocol {
            return;
        }
        // An unknown protocol is K2002 at the call already.
        if ret.decl().is_none() {
            return;
        }
        let range = TextRange::new(name.text_range().start(), call.text_range().end());
        let message = format!(
            "`RemoteClient.at` builds a client of a protocol, and `{}` is not one",
            ret.render(self.cx.p)
        );
        self.err(codes::REMOTE_CLIENT_NOT_A_PROTOCOL, range, message);
    }

    /// K2401: `.other(503)` for an `HttpStatus` that has a named case, with a
    /// fix to the case (`.serviceUnavailable`).
    ///
    /// The case is found by [`Checker::status_case`] from the stdlib's doc
    /// comments.
    fn check_status_literal(
        &mut self,
        call: &SyntaxNode,
        member: &SyntaxNode,
        args: &[Arg],
        ty: &Ty,
    ) {
        let Some(status) = self.cx.b.http_status else {
            return;
        };
        if ty.decl() != Some(status)
            || token(member, SyntaxKind::Ident).is_none_or(|t| t.text() != "other")
        {
            return;
        }
        let [arg] = args else { return };
        let Some(ast::Expr::Literal(lit)) = &arg.expr else {
            return;
        };
        let Some(tok) = lit.token().filter(|t| t.kind() == SyntaxKind::IntLit) else {
            return;
        };
        let Ok(code) = tok.text().replace('_', "").parse::<u32>() else {
            return;
        };
        let Some(name) = self.status_case(status, code) else {
            return;
        };
        let d = Diagnostic::new(
            codes::NAMED_STATUS_AS_OTHER,
            self.span(tok.text_range()),
            format!("{code} has a named case: write `.{name}`"),
        )
        .with_fix(Fix::replace(
            format!("Replace with `.{name}`"),
            self.span(call.text_range()),
            format!(".{name}"),
        ));
        self.report(d);
    }

    /// The case of `HttpStatus` whose doc comment names `code` (`/// 503.`).
    fn status_case(&self, status: DeclId, code: u32) -> Option<String> {
        let decl = self.cx.p.decl(status);
        let text = self.cx.p.file(decl.file).text;
        for m in self.cx.cases(status) {
            let member = self.cx.p.member(m);
            let Some(node) = &member.node else { continue };
            let start = usize::from(crate::program::first_token_range(node).start());
            let before = &text[..start];
            let line = before.trim_end().rsplit('\n').next().unwrap_or("").trim();
            let doc = line.strip_prefix("///").map(str::trim).unwrap_or("");
            if doc.trim_end_matches('.').parse::<u32>().ok() == Some(code) {
                return Some(member.name.clone());
            }
        }
        None
    }

    /// `copy(...)` of a `Copyable` struct.
    fn copy_call(
        &mut self,
        recv_ty: &Ty,
        name: &SyntaxToken,
        node: &SyntaxNode,
        args: &[Arg],
    ) -> Option<Ty> {
        if name.text() != "copy" {
            return None;
        }
        let d = recv_ty.decl()?;
        let copyable = self.cx.b.copyable?;
        if !self.cx.declares(d, copyable) {
            return None;
        }
        let fields = self.cx.stored_fields(d);
        let decl = self.cx.p.decl(d);
        let subst: Vec<(GenId, Ty)> = decl
            .generics
            .iter()
            .copied()
            .zip(recv_ty.args().iter().cloned())
            .collect();
        let params: Vec<SigParam> = fields
            .iter()
            .map(|f| {
                let m = self.cx.p.member(*f);
                SigParam {
                    label: Some(m.name.clone()),
                    name: m.name.clone(),
                    ty: apply(&self.cx.field_ty(*f), &subst),
                    has_default: true,
                    name_range: m.name_range,
                }
            })
            .collect();
        let labels: Vec<String> = params
            .iter()
            .map(|p| format!("{}:", p.label.clone().unwrap_or_default()))
            .collect();
        self.note_res(
            name.text_range(),
            Resolution::Member {
                owner: self.cx.p.decl_name(d),
                name: format!("copy({})", labels.join("")),
            },
        );
        let sig = Sig {
            generics: Vec::new(),
            params,
            is_async: false,
            throws: None,
            ret: recv_ty.clone(),
        };
        Some(self.apply_sig(node, &sig, &[], args, None, CallKind::Plain))
    }

    /// Calls a value of function type.
    fn call_value(
        &mut self,
        node: &SyntaxNode,
        callee: &ast::Expr,
        ty: Ty,
        args: &[Arg],
        expected: Option<&Ty>,
    ) -> Ty {
        let ty = self.resolve(&ty);
        match &ty {
            Ty::Func(f) => {
                let sig = Sig {
                    generics: Vec::new(),
                    params: f
                        .params
                        .iter()
                        .map(|p| SigParam {
                            label: None,
                            name: String::new(),
                            ty: p.clone(),
                            has_default: false,
                            name_range: TextRange::empty(0.into()),
                        })
                        .collect(),
                    is_async: f.is_async,
                    throws: f.throws.clone(),
                    ret: f.ret.clone(),
                };
                self.apply_sig(node, &sig, &[], args, expected, CallKind::Plain)
            }
            Ty::Meta(inner) => {
                if let Some(d) = inner.decl() {
                    let range = callee.syntax().text_range();
                    return self.construct(node, d, range, args, expected);
                }
                self.args_unchecked(args);
                Ty::Error
            }
            Ty::Error | Ty::Var(_) => {
                // The callee is unknown, so nothing is expected of the
                // arguments; but they are expressions of their own, whose
                // errors (an unresolved `Later`) are independent of the
                // callee's and are reported now, not after a fix resolves
                // it (sema.md §7.3).
                self.args_unchecked(args);
                self.unknown_call();
                Ty::Error
            }
            _ => {
                self.err(
                    codes::NOT_CALLABLE,
                    callee.syntax().text_range(),
                    format!("a value of `{}` can't be called", ty.render(self.cx.p)),
                );
                self.args_unchecked(args);
                Ty::Error
            }
        }
    }

    /// Constructs a value of declaration `d`.
    fn construct(
        &mut self,
        node: &SyntaxNode,
        d: DeclId,
        name_range: TextRange,
        args: &[Arg],
        expected: Option<&Ty>,
    ) -> Ty {
        let decl = self.cx.p.decl(d);
        if matches!(
            decl.named_kind(),
            Some(DeclarationKind::Component | DeclarationKind::Page | DeclarationKind::Layout)
        ) {
            // Component calls are checked by the framework layer.
            self.quiet += 1;
            self.args_unchecked(args);
            self.quiet -= 1;
            return Ty::named(d);
        }
        if decl.has_annotation("Builder")
            && args
                .last()
                .is_some_and(|a| a.is_lambda && a.label.is_none())
        {
            // A builder's trailing block holds its content (`@Builder`).
            let (body, rest) = args.split_at(args.len() - 1);
            self.quiet += 1;
            self.args_unchecked(rest);
            self.quiet -= 1;
            return self.construct(node, d, name_range, body, expected);
        }
        let generics = decl.generics.clone();
        // Type arguments from the expected type, or fresh variables.
        let subst: Vec<(GenId, Ty)> = match expected.map(|e| self.deopt(e)) {
            Some(Ty::Named(ed, eargs)) if ed == d => generics
                .iter()
                .copied()
                .zip(eargs.iter().cloned())
                .collect(),
            _ => generics.iter().map(|g| (*g, self.fresh())).collect(),
        };
        let self_ty = Ty::generic(d, subst.iter().map(|(_, t)| t.clone()).collect());
        let inits = self.cx.inits(d);
        if decl.has_annotation("Native") && inits.is_empty()
            || matches!(
                decl.named_kind(),
                Some(DeclarationKind::Interface | DeclarationKind::Enum)
            )
        {
            self.args_unchecked(args);
            // `HttpStatus(code: 418)`: an enum's `@Literal` initializer.
            self.check_literal_call(d, name_range, args);
            return if self.cx.is_interface(d) {
                Ty::Error
            } else {
                self_ty
            };
        }
        if !inits.is_empty() {
            // Pick the initializer whose labels match best.
            let labels: Vec<Option<&str>> = args
                .iter()
                .filter(|a| !a.is_lambda)
                .map(|a| a.label.as_ref().map(|l| l.0.as_str()))
                .collect();
            let mut best = inits[0];
            let mut best_score = i64::MIN;
            for &i in &inits {
                let sig = self.cx.sig(SigKey::Member(i));
                let score: i64 = labels
                    .iter()
                    .map(|l| {
                        if sig.params.iter().any(|p| p.label.as_deref() == *l) {
                            2
                        } else {
                            -3
                        }
                    })
                    .sum::<i64>()
                    - i64::try_from(
                        sig.params
                            .iter()
                            .filter(|p| !p.has_default && !labels.contains(&p.label.as_deref()))
                            .count(),
                    )
                    .unwrap_or(0);
                if score > best_score {
                    best_score = score;
                    best = i;
                }
            }
            let member = self.cx.p.member(best);
            if member.visibility == Visibility::Private && self.owner.decl != Some(d) {
                self.err(
                    codes::NOT_VISIBLE,
                    name_range,
                    format!("the initializer of `{}` is private", decl.name),
                );
                self.args_unchecked(args);
                return self_ty;
            }
            let sig = self.cx.sig(SigKey::Member(best));
            let mut sig = (*sig).clone();
            sig.ret = self_ty.clone();
            self.apply_sig(node, &sig, &subst, args, None, CallKind::Plain);
            self.check_literal_call(d, name_range, args);
            return self.resolve(&self_ty);
        }
        if decl.named_kind() == Some(DeclarationKind::Config) && !self.in_test_code() {
            self.err(
                codes::CONFIG_CONSTRUCTION,
                node.text_range(),
                "a config's values come from the environment; only tests construct one",
            );
        }
        let memberwise = matches!(
            decl.named_kind(),
            Some(DeclarationKind::Struct | DeclarationKind::Entity | DeclarationKind::Config)
        );
        let params: Vec<SigParam> = if memberwise {
            self.cx
                .stored_fields(d)
                .iter()
                .map(|f| {
                    let m = self.cx.p.member(*f);
                    SigParam {
                        label: Some(m.name.clone()),
                        name: m.name.clone(),
                        ty: apply(&self.cx.field_ty(*f), &subst),
                        has_default: m.has_default || m.has_annotation("Inject"),
                        name_range: m.name_range,
                    }
                })
                .collect()
        } else if matches!(decl.named_kind(), Some(DeclarationKind::Class)) {
            Vec::new()
        } else {
            self.args_unchecked(args);
            return self_ty;
        };
        let sig = Sig {
            generics: Vec::new(),
            params,
            is_async: false,
            throws: None,
            ret: self_ty.clone(),
        };
        self.apply_sig(
            node,
            &sig,
            &[],
            args,
            None,
            if memberwise {
                CallKind::Memberwise
            } else {
                CallKind::Plain
            },
        );
        self.resolve(&self_ty)
    }

    /// Collects the arguments of a call in source order, the trailing lambda
    /// last.
    fn collect_args(&self, c: &ast::CallExpr) -> Vec<Arg> {
        let mut out = Vec::new();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                let label = a.label().map(|l| (l.text().to_owned(), l.text_range()));
                let expr = a.value();
                let is_lambda = matches!(expr, Some(ast::Expr::LambdaExpr(_)));
                out.push(Arg {
                    label,
                    expr,
                    range: a.syntax().text_range(),
                    is_lambda,
                });
            }
        }
        if let Some(l) = c.trailing_lambda() {
            out.push(Arg {
                label: None,
                range: l.syntax().text_range(),
                expr: Some(ast::Expr::LambdaExpr(l)),
                is_lambda: true,
            });
        }
        out
    }

    /// Infers the arguments of a call that couldn't be resolved, without
    /// expected types and without reporting implicit members (K2007), so the
    /// names inside them still resolve.
    fn args_unchecked(&mut self, args: &[Arg]) {
        self.no_implicit += 1;
        self.args_unchecked_inner(args);
        self.no_implicit -= 1;
    }

    /// The work of [`Checker::args_unchecked`]: lambda arguments are checked
    /// with `{error}` parameter types, other arguments inferred.
    fn args_unchecked_inner(&mut self, args: &[Arg]) {
        for a in args {
            if let Some(e) = &a.expr {
                if a.is_lambda {
                    if let ast::Expr::LambdaExpr(l) = e {
                        let params: Vec<(String, TextRange, Ty)> = l
                            .param_list()
                            .map(|list| {
                                list.syntax()
                                    .children()
                                    .filter(|c| c.kind() == SyntaxKind::LambdaParam)
                                    .filter_map(|p| token(&p, SyntaxKind::Ident))
                                    .map(|n| (n.text().to_owned(), n.text_range(), Ty::Error))
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.lambda_body(l, params, None, Some(Ty::Error), true, None);
                        continue;
                    }
                }
                self.infer(e, None);
            }
        }
    }

    /// Matches arguments to a signature, checks them, applies effects and
    /// returns the call's result type.
    fn apply_sig(
        &mut self,
        node: &SyntaxNode,
        sig: &Sig,
        subst: &[(GenId, Ty)],
        args: &[Arg],
        expected: Option<&Ty>,
        kind: CallKind,
    ) -> Ty {
        let (params, ret, is_async, throws, full) = self.instantiate_full(sig, subst);
        // The expected type fixes the result's variables first.
        if let Some(e) = expected {
            let r = self.resolve(&ret);
            if r.has_vars_in(&self.vars) {
                let _ = self.assignable_quiet(&r, e);
            }
        }
        let n = params.len();
        let mut unknown_label = false;
        let mut assigned: Vec<Option<usize>> = vec![None; n];
        let mut positional = 0;
        let mut trailing: Option<usize> = None;
        let has_trailing = node.children().any(|c| c.kind() == SyntaxKind::LambdaExpr);
        for (ai, a) in args.iter().enumerate() {
            let is_trailing = has_trailing
                && ai + 1 == args.len()
                && a.is_lambda
                && a.label.is_none()
                && node
                    .children()
                    .last()
                    .is_some_and(|l| l.text_range() == a.range);
            if is_trailing {
                trailing = Some(ai);
                continue;
            }
            match &a.label {
                Some((label, lrange)) => {
                    match (0..n).find(|&i| {
                        assigned[i].is_none()
                            && sig.params[i].label.as_deref() == Some(label.as_str())
                    }) {
                        Some(i) => assigned[i] = Some(ai),
                        None => {
                            unknown_label = true;
                            let candidates: Vec<&str> = sig
                                .params
                                .iter()
                                .filter_map(|p| p.label.as_deref())
                                .collect();
                            let mut d = Diagnostic::new(
                                codes::UNKNOWN_ARGUMENT_LABEL,
                                self.span(*lrange),
                                format!("no parameter is labelled `{label}`"),
                            );
                            if let Some(c) = closest(label, candidates.into_iter()) {
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
                                self.infer(e, None);
                            }
                        }
                    }
                }
                None => {
                    while positional < n
                        && (assigned[positional].is_some()
                            || sig.params[positional].label.is_some())
                    {
                        positional += 1;
                    }
                    if positional < n {
                        assigned[positional] = Some(ai);
                        positional += 1;
                    } else if let Some(e) = &a.expr {
                        self.infer(e, None);
                    }
                }
            }
        }
        if let Some(ai) = trailing {
            match (0..n).rev().find(|&i| assigned[i].is_none()) {
                Some(i) => assigned[i] = Some(ai),
                None => {
                    if let Some(e) = &args[ai].expr {
                        self.infer(e, None);
                    }
                }
            }
        }
        // Check non-lambda arguments first, then lambdas.
        for pass in 0..2 {
            for i in 0..n {
                let Some(ai) = assigned[i] else { continue };
                let a = &args[ai];
                if (pass == 0) == a.is_lambda {
                    continue;
                }
                if let Some(e) = &a.expr {
                    let pty = params[i].clone();
                    self.check(e, &pty);
                }
            }
        }
        // A `Void` parameter takes no argument (`closed.emit()` for an
        // `Event<Void>`).
        let void_param: Vec<bool> = params.iter().map(|t| self.is_void(t)).collect();
        let missing: Vec<&SigParam> = (0..n)
            .filter(|&i| assigned[i].is_none() && !sig.params[i].has_default && !void_param[i])
            .map(|i| &sig.params[i])
            .collect();
        if !missing.is_empty() && !unknown_label {
            let at = node
                .children()
                .find(|c| c.kind() == SyntaxKind::ArgList)
                .and_then(|l| token(&l, SyntaxKind::RParen))
                .map_or_else(
                    || {
                        node.children()
                            .find_map(ast::Expr::cast)
                            .map_or(node.text_range().end(), |c| c.syntax().text_range().end())
                    },
                    |t| t.text_range().start(),
                );
            let names: Vec<String> = missing
                .iter()
                .map(|p| format!("`{}`", p.label.clone().unwrap_or_else(|| p.name.clone())))
                .collect();
            let code = if kind == CallKind::Memberwise {
                codes::MISSING_MEMBERWISE_FIELD
            } else {
                codes::MISSING_ARGUMENT
            };
            let any_error = missing.iter().any(|p| p.ty.contains_error());
            if !any_error {
                let fix = self.missing_args_fix(node, at, &missing);
                self.report(
                    Diagnostic::new(
                        code,
                        Span::empty(self.span(node.text_range()).file, at),
                        format!("missing argument(s) {}", names.join(", ")),
                    )
                    .with_fix(fix),
                );
            }
        }
        // Constraints of the function's generics, at the argument they were
        // inferred from.
        for g in &sig.generics {
            let Some(var) = full.iter().find(|(p, _)| p == g).map(|(_, t)| t.clone()) else {
                continue;
            };
            let value = self.resolve(&var);
            for c in self.cx.constraints(*g).iter() {
                let Some(iface) = c.decl() else { continue };
                if value.contains_error() || matches!(value, Ty::Var(_)) {
                    continue;
                }
                if !self.cx.implements(&value, iface) {
                    let origin = if let Ty::Var(v) = var {
                        self.origins.get(v as usize).copied().flatten()
                    } else {
                        None
                    };
                    let at = origin.unwrap_or(node.text_range());
                    self.err(
                        codes::UNSATISFIED_CONSTRAINT,
                        at,
                        format!(
                            "`{}` doesn't implement `{}`",
                            value.render(self.cx.p),
                            self.cx.p.decl_name(iface)
                        ),
                    );
                }
            }
        }
        self.call_effects(node, is_async, throws);
        self.resolve(&ret)
    }

    /// The context.
    pub(crate) fn cx_ref(&self) -> &'c Cx<'p, 'a> {
        self.cx
    }
}

/// How a call is checked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CallKind {
    /// A function, method, case or initializer.
    Plain,
    /// A memberwise initializer (missing fields are K2008).
    Memberwise,
}

/// Returns `true` if a postfix chain starts with an implicit member
/// (`.white.opacity(0.5)`), whose type comes from the chain's context.
fn starts_with_implicit(e: &ast::Expr) -> bool {
    let mut current = e.clone();
    loop {
        let next = match &current {
            ast::Expr::ImplicitMemberExpr(_) => return true,
            ast::Expr::MemberExpr(m) => m.receiver(),
            ast::Expr::CallExpr(c) => c.callee(),
            _ => return false,
        };
        match next {
            Some(n) => current = n,
            None => return false,
        }
    }
}

/// The parameter names of a lambda, typed `{error}`.
fn lambda_param_names(l: &ast::LambdaExpr) -> Vec<(String, TextRange, Ty)> {
    l.param_list()
        .map(|list| {
            list.syntax()
                .children()
                .filter(|c| c.kind() == SyntaxKind::LambdaParam)
                .filter_map(|p| token(&p, SyntaxKind::Ident))
                .map(|n| (n.text().to_owned(), n.text_range(), Ty::Error))
                .collect()
        })
        .unwrap_or_default()
}

/// The case name of an enum case pattern: the last identifier before `(`.
pub(crate) fn case_name(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .take_while(|t| t.kind() != SyntaxKind::LParen)
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident || t.kind().is_keyword())
        .last()
}

/// Unbound inference variables in a type.
trait VarsIn {
    /// Returns `true` if the type mentions an inference variable that is
    /// unbound in `vars` (the checker's variable table).
    fn has_vars_in(&self, vars: &[Option<Ty>]) -> bool;
}

impl VarsIn for Ty {
    fn has_vars_in(&self, vars: &[Option<Ty>]) -> bool {
        match self {
            Ty::Var(v) => vars.get(*v as usize).is_some_and(Option::is_none),
            Ty::Named(_, a) | Ty::Tuple(a) => a.iter().any(|t| t.has_vars_in(vars)),
            Ty::Optional(i) | Ty::Meta(i) => i.has_vars_in(vars),
            Ty::Func(f) => f.params.iter().any(|t| t.has_vars_in(vars)) || f.ret.has_vars_in(vars),
            _ => false,
        }
    }
}

/// Keeps the `FnTy` and `DeclKind` imports referenced; it is never called.
#[allow(dead_code)]
fn unused(_: FnTy, _: DeclKind) {}

/// An empty range at `at`.
fn at_range(at: TextSize) -> TextRange {
    TextRange::empty(at)
}

/// Returns `true` if a `requires` clause that is a direct child of `node`
/// (a page, protocol or protocol rpc) names the `Authenticated` guard.
///
/// Guards are matched by name, as the server code generator lowers them, so
/// the narrowing holds exactly where the server checks for a session.
fn requires_authenticated(node: &SyntaxNode) -> bool {
    node.children()
        .filter(|c| c.kind() == SyntaxKind::RequiresClause)
        .flat_map(|c| c.children().filter(|g| g.kind() == SyntaxKind::Guard))
        .any(|g| {
            g.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind().is_name_like())
                .is_some_and(|t| t.text() == "Authenticated")
        })
}
