//! User activation (`docs/design/45-web-platform.md`, "User activation"):
//! a member marked `@UserAction` may be called only from the synchronous
//! part of a user's action, and calling it anywhere else is **K4030**.
//!
//! A project pass after the bodies are checked, over their syntax and the
//! resolutions the body check recorded:
//!
//! 1. Every body becomes a **context** with a small event tree: the calls
//!    of functions in source evaluation order, the `await`s between them,
//!    and branches and loops. A function's body is a context of its own,
//!    as is every lambda. A lambda passed directly to `.on(…)` of an
//!    activation-triggering event (`click`, `submit`, a component event
//!    declared `@UserAction @Emits`) or to `form.handle(…)` is a user's
//!    action; one passed anywhere else, and an effect, `init()`, a field
//!    initializer and a view, are not.
//! 2. **Inference:** a function that calls a user-action-only function
//!    before its first `await` is user-action-only itself, to a fixed
//!    point. The `@UserAction` members are the base, and so is `emit()` of
//!    an event declared `@UserAction`.
//! 3. **Reports**, each K4030 at the call or reference: a user-action-only
//!    call in a context that isn't a user's action; one after an `await`
//!    (a loop body counts twice, so an `await` later in the body precedes
//!    the next iteration's call); a second consuming call
//!    (`@UserAction(consuming: true)`) on one path; a user-action-only
//!    function used as a value other than directly in `.on(…)` or
//!    `form.handle(…)`; and a user-action-only function implementing an
//!    interface member not declared `@UserAction`.
//!
//! Test bodies are users' actions (the test is the user), so test suites
//! aren't walked.

use std::collections::{HashMap, HashSet};

use k_diagnostics::{Diagnostic, FileId, Fix, Span, TextRange, codes};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode};

use crate::cx::Cx;
use crate::lookup::Resolution;
use crate::program::{DeclId, FileIx, MemberId, MemberKind};
use crate::stdlib::DeclarationKind as K;

/// The element events the HTML standard counts as activation-triggering.
const ACTIVATION_EVENTS: &[&str] = &["click", "submit", "keyDown", "pointerDown", "pointerUp"];

/// Something a body may call.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Callee {
    /// A member function (or an interface requirement).
    Member(MemberId),
    /// A top-level function.
    Decl(DeclId),
    /// `emit()` of a component event field declared `@UserAction`.
    Event(MemberId),
}

/// What happens in a body, in evaluation order.
#[derive(Debug)]
enum Ev {
    /// A call of a callee, after its receiver and arguments.
    Call(Callee, TextRange),
    /// The suspension of an `await` (`for await` included).
    Await(TextRange),
    /// One after the other.
    Seq(Vec<Ev>),
    /// One of the branches.
    Alt(Vec<Ev>),
    /// Zero or more times.
    Loop(Box<Ev>),
}

/// What kind of context a body is.
#[derive(Clone, Debug)]
enum Kind {
    /// A function's body: user-action-only when it calls one first.
    Function(Callee),
    /// A user's action: a handler of an activation-triggering event.
    Action,
    /// Never a user's action; the text says what it is ("an `@Effect`").
    Not(String),
}

/// One body.
#[derive(Debug)]
struct Context {
    /// The file it is in.
    file: FileIx,
    /// Whether it is a user's action.
    kind: Kind,
    /// What happens in it.
    body: Ev,
}

/// Where a function is used as a value.
#[derive(Clone, Debug)]
enum Site {
    /// Directly in `.on(…)` of an activation-triggering event, or in
    /// `form.handle(…)`.
    Action,
    /// Directly in `.on(…)` of another event.
    Handler(String),
    /// Anywhere else.
    Value,
}

/// A function used as a value.
#[derive(Debug)]
struct Use {
    /// The file it is in.
    file: FileIx,
    /// The function.
    callee: Callee,
    /// The reference.
    range: TextRange,
    /// Where the value goes.
    site: Site,
}

/// The pass's lookup tables.
struct Tables<'r> {
    /// Resolutions by file and range.
    res: HashMap<(FileId, TextRange), &'r Resolution>,
    /// The callees by their resolution.
    callees: HashMap<Resolution, Callee>,
    /// Components by qualified name.
    components: HashMap<String, DeclId>,
    /// `k.core.Event`'s `emit(_:)` and `k.validation.Form`'s `handle(_:)`.
    emit: Option<Resolution>,
    /// How `k.validation.Form` is named in resolutions.
    handle_owner: Option<String>,
}

/// The state while one context is walked.
struct Walk<'t, 'r> {
    /// The lookup tables.
    t: &'t Tables<'r>,
    /// The file walked.
    file: FileIx,
    /// Its host id, which resolutions are keyed by.
    id: FileId,
    /// The reason of the enclosing context when it isn't a user's action
    /// (a view's content lambdas share it).
    outer: Option<String>,
    /// The contexts found so far.
    contexts: Vec<Context>,
    /// The function values found so far.
    uses: Vec<Use>,
}

impl Walk<'_, '_> {
    /// What the name at `range` resolves to.
    fn resolution(&self, range: TextRange) -> Option<&Resolution> {
        self.t.res.get(&(self.id, range)).copied()
    }

    /// The callee the name at `range` names, if any.
    fn callee_at(&self, range: TextRange) -> Option<Callee> {
        self.resolution(range)
            .and_then(|r| self.t.callees.get(r))
            .copied()
    }

    /// The events of `n` and what it contains.
    fn walk(&mut self, n: &SyntaxNode) -> Ev {
        let mut out = Vec::new();
        self.node(n, &mut out);
        Ev::Seq(out)
    }

    /// The events of `n`'s children, in order, into `out`.
    fn children(&mut self, n: &SyntaxNode, out: &mut Vec<Ev>) {
        for c in n.children() {
            self.node(&c, out);
        }
    }

    /// The events of `n`, into `out`.
    fn node(&mut self, n: &SyntaxNode, out: &mut Vec<Ev>) {
        match n.kind() {
            SyntaxKind::LambdaExpr => self.lambda(n),
            // A nested declaration is a context of its own.
            SyntaxKind::FuncDecl | SyntaxKind::InitDecl | SyntaxKind::StyleBlock => {}
            SyntaxKind::Annotation => {}
            SyntaxKind::AwaitExpr => {
                self.children(n, out);
                out.push(Ev::Await(await_range(n)));
            }
            SyntaxKind::CallExpr => self.call(n, out),
            SyntaxKind::NameExpr => {
                if let Some(tok) = n.first_token() {
                    self.reference(n, tok.text_range());
                }
            }
            SyntaxKind::MemberExpr => {
                let m = ast::MemberExpr::cast(n.clone());
                if let Some(r) = m.as_ref().and_then(ast::MemberExpr::receiver) {
                    self.node(r.syntax(), out);
                }
                if let Some(name) = m.and_then(|m| m.name()) {
                    self.reference(n, name.text_range());
                }
            }
            SyntaxKind::IfExpr => {
                let mut branches = Vec::new();
                let mut seen_then = false;
                for c in n.children() {
                    if c.kind() == SyntaxKind::Block
                        || (seen_then && c.kind() == SyntaxKind::IfExpr)
                    {
                        seen_then = true;
                        branches.push(self.walk(&c));
                    } else {
                        self.node(&c, out);
                    }
                }
                if branches.len() < 2 {
                    branches.push(Ev::Seq(Vec::new()));
                }
                out.push(Ev::Alt(branches));
            }
            SyntaxKind::MatchExpr => {
                let mut arms = Vec::new();
                for c in n.children() {
                    if c.kind() == SyntaxKind::MatchArm {
                        arms.push(self.walk(&c));
                    } else {
                        self.node(&c, out);
                    }
                }
                out.push(Ev::Alt(arms));
            }
            SyntaxKind::DoStmt => {
                let mut catches = vec![Ev::Seq(Vec::new())];
                for c in n.children() {
                    if c.kind() == SyntaxKind::CatchClause {
                        catches.push(self.walk(&c));
                    } else {
                        self.node(&c, out);
                    }
                }
                out.push(Ev::Alt(catches));
            }
            SyntaxKind::GuardStmt => {
                let mut otherwise = Ev::Seq(Vec::new());
                for c in n.children() {
                    if c.kind() == SyntaxKind::Block {
                        otherwise = self.walk(&c);
                    } else {
                        self.node(&c, out);
                    }
                }
                out.push(Ev::Alt(vec![Ev::Seq(Vec::new()), otherwise]));
            }
            SyntaxKind::WhileStmt => {
                // The conditions run before every iteration.
                let mut body = Vec::new();
                self.children(n, &mut body);
                out.push(Ev::Loop(Box::new(Ev::Seq(body))));
            }
            SyntaxKind::ForStmt => {
                let awaits = n
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::AwaitKw);
                let mut body = Vec::new();
                if let Some(a) = &awaits {
                    body.push(Ev::Await(a.text_range()));
                }
                for c in n.children() {
                    if c.kind() == SyntaxKind::Block {
                        body.push(self.walk(&c));
                    } else {
                        self.node(&c, out);
                    }
                }
                out.push(Ev::Loop(Box::new(Ev::Seq(body))));
            }
            _ => self.children(n, out),
        }
    }

    /// A call: its receiver, its arguments and trailing lambda, then the
    /// call itself.
    fn call(&mut self, n: &SyntaxNode, out: &mut Vec<Ev>) {
        let Some(call) = ast::CallExpr::cast(n.clone()) else {
            return;
        };
        let mut target = None;
        match call.callee() {
            Some(ast::Expr::NameExpr(name)) => {
                if let Some(tok) = name.syntax().first_token() {
                    target = self.callee_at(tok.text_range());
                }
            }
            Some(ast::Expr::MemberExpr(m)) => {
                if let Some(r) = m.receiver() {
                    self.node(r.syntax(), out);
                }
                if let Some(name) = m.name() {
                    target = self.callee_at(name.text_range());
                    // `pressed.emit()` of an event declared `@UserAction`.
                    let is_emit = self
                        .t
                        .emit
                        .as_ref()
                        .is_some_and(|e| self.resolution(name.text_range()) == Some(e));
                    if is_emit {
                        let field = m.receiver().and_then(|r| match r {
                            ast::Expr::NameExpr(n) => n.syntax().first_token(),
                            ast::Expr::MemberExpr(m) => m.name(),
                            _ => None,
                        });
                        if let Some(Callee::Event(e)) =
                            field.and_then(|f| self.callee_at(f.text_range()))
                        {
                            target = Some(Callee::Event(e));
                        }
                    }
                }
            }
            Some(other) => self.node(other.syntax(), out),
            None => {}
        }
        for c in n.children().skip(1) {
            self.node(&c, out);
        }
        if let Some(t) = target {
            out.push(Ev::Call(t, n.text_range()));
        }
    }

    /// A function named at `name` by `expr` without calling it.
    fn reference(&mut self, expr: &SyntaxNode, name: TextRange) {
        let Some(callee) = self.callee_at(name) else {
            return;
        };
        if matches!(callee, Callee::Event(_)) {
            return;
        }
        let site = self.site(expr);
        self.uses.push(Use {
            file: self.file,
            callee,
            range: expr.text_range(),
            site,
        });
    }

    /// Where the value `expr` goes: directly into `.on(…)` or
    /// `form.handle(…)`, or elsewhere.
    fn site(&self, expr: &SyntaxNode) -> Site {
        let Some(arg) = expr.parent().and_then(ast::Arg::cast) else {
            return Site::Value;
        };
        let Some(call) = arg
            .syntax()
            .parent()
            .and_then(|l| l.parent())
            .and_then(ast::CallExpr::cast)
        else {
            return Site::Value;
        };
        let Some(ast::Expr::MemberExpr(m)) = call.callee() else {
            return Site::Value;
        };
        let Some(name) = m.name() else {
            return Site::Value;
        };
        match name.text() {
            "on" => {
                let Some(label) = arg.label() else {
                    return Site::Value;
                };
                match self.resolution(label.text_range()) {
                    Some(Resolution::Event { owner, name }) => {
                        if self.activation_event(owner, name) {
                            Site::Action
                        } else {
                            Site::Handler(name.clone())
                        }
                    }
                    _ => Site::Value,
                }
            }
            "handle" => match self.resolution(name.text_range()) {
                Some(Resolution::Member { owner, .. })
                    if self.t.handle_owner.as_ref() == Some(owner) =>
                {
                    Site::Action
                }
                _ => Site::Value,
            },
            _ => Site::Value,
        }
    }

    /// `true` when event `name` of `owner` (an element's name or a
    /// component's qualified name) is a user's action.
    fn activation_event(&self, owner: &str, name: &str) -> bool {
        match self.t.components.get(owner) {
            Some(_) => {
                let field = Resolution::Member {
                    owner: owner.to_owned(),
                    name: name.to_owned(),
                };
                matches!(self.t.callees.get(&field), Some(Callee::Event(_)))
            }
            None => ACTIVATION_EVENTS.contains(&name),
        }
    }

    /// A lambda: a context of its own, classified by where it goes.
    fn lambda(&mut self, n: &SyntaxNode) {
        let kind = match self.site(n) {
            Site::Action => Kind::Action,
            Site::Handler(event) => {
                let article = if event.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                Kind::Not(format!("{article} `{event}` handler"))
            }
            Site::Value => match &self.outer {
                Some(reason) => Kind::Not(reason.clone()),
                None if timer_callback(n) => Kind::Not("a timer callback".into()),
                None => Kind::Not("a function passed as a value".into()),
            },
        };
        let saved = std::mem::replace(
            &mut self.outer,
            match &kind {
                Kind::Not(r) if r == "a view" => Some(r.clone()),
                _ => None,
            },
        );
        let mut body = Vec::new();
        for c in n.children() {
            if c.kind() != SyntaxKind::LambdaParamList {
                self.node(&c, &mut body);
            }
        }
        self.outer = saved;
        self.contexts.push(Context {
            file: self.file,
            kind,
            body: Ev::Seq(body),
        });
    }

    /// Adds the context of `body` (a function body, an initializer, a view).
    fn context(&mut self, kind: Kind, body: &SyntaxNode) {
        let outer = match &kind {
            Kind::Not(r) if r == "a view" => Some(r.clone()),
            _ => None,
        };
        let saved = std::mem::replace(&mut self.outer, outer);
        let ev = self.walk(body);
        self.outer = saved;
        self.contexts.push(Context {
            file: self.file,
            kind,
            body: ev,
        });
    }
}

/// The range of `await` in an `await` expression.
fn await_range(n: &SyntaxNode) -> TextRange {
    n.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::AwaitKw)
        .map_or_else(|| n.text_range(), |t| t.text_range())
}

/// `true` for a lambda given to `clock.after(…)` or `clock.every(…)`.
fn timer_callback(lambda: &SyntaxNode) -> bool {
    let call = lambda
        .parent()
        .filter(|p| p.kind() == SyntaxKind::Arg)
        .and_then(|a| a.parent())
        .and_then(|l| l.parent())
        .or_else(|| lambda.parent())
        .and_then(ast::CallExpr::cast);
    call.and_then(|c| c.callee()).is_some_and(|c| match c {
        ast::Expr::MemberExpr(m) => m
            .name()
            .is_some_and(|n| matches!(n.text(), "after" | "every")),
        _ => false,
    })
}

/// The walk's result for one context.
#[derive(Default)]
struct Outcome {
    /// User-action-only calls before any `await`, first first.
    early: Vec<(Callee, TextRange)>,
    /// User-action-only calls after an `await`, with the `await`.
    late: Vec<(Callee, TextRange, TextRange)>,
    /// Consuming calls that follow another on the same path, with it.
    twice: Vec<(Callee, TextRange, TextRange)>,
}

/// The state along one path.
#[derive(Clone, Copy, Default)]
struct State {
    /// The first `await` on the path.
    awaited: Option<TextRange>,
    /// The first consuming call on the path, and whether there were more.
    consumed: Option<TextRange>,
}

/// What the pass knows about callees.
struct Facts {
    /// The user-action-only callees: the call that makes each so (`None`
    /// for an annotated member or event).
    ua: HashMap<Callee, Option<(Callee, TextRange, FileIx)>>,
    /// The callees that consume the user's action, directly or through a
    /// call.
    consuming: HashSet<Callee>,
}

impl Facts {
    /// Follows `ev` from state `st`, recording the user-action-only calls
    /// in `out`, and returns the state after it.
    fn eval(&self, ev: &Ev, st: State, out: &mut Outcome) -> State {
        match ev {
            Ev::Call(c, range) => {
                if !self.ua.contains_key(c) {
                    return st;
                }
                let mut st = st;
                match st.awaited {
                    Some(a) => out.late.push((*c, *range, a)),
                    None => out.early.push((*c, *range)),
                }
                if self.consuming.contains(c) {
                    match st.consumed {
                        Some(first) => out.twice.push((*c, *range, first)),
                        None => st.consumed = Some(*range),
                    }
                }
                st
            }
            Ev::Await(r) => State {
                awaited: st.awaited.or(Some(*r)),
                ..st
            },
            Ev::Seq(list) => list.iter().fold(st, |s, e| self.eval(e, s, out)),
            Ev::Alt(branches) => {
                let mut merged = st;
                for b in branches {
                    let s = self.eval(b, st, out);
                    merged.awaited = merged.awaited.or(s.awaited);
                    merged.consumed = merged.consumed.or(s.consumed);
                }
                merged
            }
            Ev::Loop(body) => {
                let once = self.eval(body, st, out);
                let twice = self.eval(body, once, out);
                State {
                    awaited: st.awaited.or(twice.awaited),
                    consumed: st.consumed.or(twice.consumed),
                }
            }
        }
    }
}

/// Runs the pass over the project (K4030).
pub(crate) fn check(cx: &Cx<'_, '_>) {
    let p = cx.p;
    let live = cx.resolutions.borrow();
    let chunks = cx.body_resolutions.borrow();
    let mut res: HashMap<(FileId, TextRange), &Resolution> = HashMap::new();
    for (k, r) in live.iter().chain(chunks.iter().flat_map(|c| c.iter())) {
        if matches!(
            r,
            Resolution::Member { .. } | Resolution::Declaration { .. } | Resolution::Event { .. }
        ) {
            res.entry(*k).or_insert(r);
        }
    }
    // The callees: user functions, and every annotated member.
    let mut callees: HashMap<Resolution, Callee> = HashMap::new();
    let mut components: HashMap<String, DeclId> = HashMap::new();
    let mut facts = Facts {
        ua: HashMap::new(),
        consuming: HashSet::new(),
    };
    for (i, d) in p.decls.iter().enumerate() {
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let stdlib = p.file(d.file).stdlib;
        match d.named_kind() {
            Some(K::Function) if !stdlib => {
                callees.insert(cx.decl_resolution(id), Callee::Decl(id));
            }
            Some(K::Component) => {
                components.insert(p.decl_name(id), id);
            }
            _ => {}
        }
        for &m in &d.members {
            let member = p.member(m);
            let annotated = member.has_annotation("UserAction");
            let callee = match member.kind {
                MemberKind::Func if annotated || !stdlib => Callee::Member(m),
                MemberKind::Init if !stdlib => Callee::Member(m),
                MemberKind::Field if annotated => Callee::Event(m),
                _ => continue,
            };
            callees.insert(cx.member_resolution(m), callee);
            if annotated {
                facts.ua.insert(callee, None);
                if member.node.as_ref().is_some_and(consuming) {
                    facts.consuming.insert(callee);
                }
            }
        }
    }
    let tables = Tables {
        res,
        callees,
        components,
        emit: p
            .lookup_qualified("k.core.Event")
            .map(|e| Resolution::Member {
                owner: p.decl_name(e),
                name: "emit(_:)".into(),
            }),
        handle_owner: p
            .lookup_qualified("k.validation.Form")
            .map(|f| p.decl_name(f)),
    };
    let (contexts, uses) = collect(cx, &tables);
    drop(live);
    drop(chunks);
    infer(&contexts, &mut facts);
    report(cx, &contexts, &uses, &facts);
    interfaces(cx, &facts);
}

/// `true` for a member annotated `@UserAction(consuming: true)`.
fn consuming(node: &SyntaxNode) -> bool {
    node.children()
        .filter_map(ast::Annotation::cast)
        .filter(|a| a.name().is_some_and(|n| n.text() == "UserAction"))
        .filter_map(|a| a.arg_list())
        .flat_map(|l| l.args())
        .any(|arg| {
            arg.label().is_some_and(|l| l.text() == "consuming")
                && arg
                    .value()
                    .is_some_and(|v| v.syntax().text().to_string().trim() == "true")
        })
}

/// Walks every body of the project's code outside test suites.
fn collect(cx: &Cx<'_, '_>, t: &Tables<'_>) -> (Vec<Context>, Vec<Use>) {
    let p = cx.p;
    let mut contexts = Vec::new();
    let mut uses = Vec::new();
    for (ix, file) in p.files.iter().enumerate() {
        if file.stdlib {
            continue;
        }
        let fix = FileIx(u32::try_from(ix).unwrap_or(u32::MAX));
        let mut w = Walk {
            t,
            file: fix,
            id: file.id,
            outer: None,
            contexts: Vec::new(),
            uses: Vec::new(),
        };
        for &d in &file.decls {
            let decl = p.decl(d);
            match decl.named_kind() {
                Some(K::TestSuite) => continue,
                Some(K::Function) => {
                    if let Some(body) = decl.node.children().find(|c| c.kind() == SyntaxKind::Block)
                    {
                        w.context(Kind::Function(Callee::Decl(d)), &body);
                    }
                    continue;
                }
                Some(K::Constant) => {
                    w.context(Kind::Not("a constant's initializer".into()), &decl.node);
                    continue;
                }
                _ => {}
            }
            for &m in &decl.members {
                let member = p.member(m);
                let Some(node) = &member.node else { continue };
                match member.kind {
                    MemberKind::Func | MemberKind::Init => {
                        let Some(body) = node.children().find(|c| c.kind() == SyntaxKind::Block)
                        else {
                            continue;
                        };
                        let kind = if member.kind == MemberKind::Init {
                            Kind::Not("`init()`".into())
                        } else if member.has_annotation("Effect") {
                            Kind::Not("an `@Effect`".into())
                        } else {
                            Kind::Function(Callee::Member(m))
                        };
                        w.context(kind, &body);
                    }
                    MemberKind::Field | MemberKind::Computed => {
                        let reason = if member.has_annotation("Load") {
                            "a `@Load` initializer"
                        } else if member.has_annotation("Derived") {
                            "a `@Derived` value"
                        } else if member.kind == MemberKind::Computed {
                            "a computed property"
                        } else {
                            "a field initializer"
                        };
                        w.context(Kind::Not(reason.into()), node);
                    }
                    _ => {}
                }
            }
            // The view, `loading`, `failed` and `metadata` blocks.
            if let Some(list) = decl
                .node
                .children()
                .find(|c| c.kind() == SyntaxKind::MemberList)
            {
                for block in list.children().filter(|c| {
                    matches!(
                        c.kind(),
                        SyntaxKind::ViewBlock
                            | SyntaxKind::LoadingBlock
                            | SyntaxKind::FailedBlock
                            | SyntaxKind::MetadataBlock
                    )
                }) {
                    w.context(Kind::Not("a view".into()), &block);
                }
            }
        }
        contexts.append(&mut w.contexts);
        uses.append(&mut w.uses);
    }
    (contexts, uses)
}

/// Infers the user-action-only functions and the consuming ones, to a
/// fixed point.
fn infer(contexts: &[Context], facts: &mut Facts) {
    loop {
        let mut changed = false;
        for c in contexts {
            let Kind::Function(f) = c.kind else { continue };
            let mut out = Outcome::default();
            let end = facts.eval(&c.body, State::default(), &mut out);
            if let Some(&(callee, range)) = out.early.first()
                && let std::collections::hash_map::Entry::Vacant(e) = facts.ua.entry(f)
            {
                e.insert(Some((callee, range, c.file)));
                changed = true;
            }
            if end.consumed.is_some() && facts.consuming.insert(f) {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// How a callee is written in a message: `Files.open(accepting:)`.
fn display(cx: &Cx<'_, '_>, c: Callee) -> String {
    let p = cx.p;
    match c {
        Callee::Decl(d) => format!("{}()", p.decl(d).name),
        Callee::Member(m) | Callee::Event(m) => {
            let owner = &p.decl(cx.member_owner_type(m)).name;
            let name = match cx.member_resolution(m) {
                Resolution::Member { name, .. } => name,
                _ => p.member(m).name.clone(),
            };
            format!("{owner}.{name}")
        }
    }
}

/// The calls from `c` down to the `@UserAction` member, rendered.
fn path(cx: &Cx<'_, '_>, facts: &Facts, c: Callee) -> Vec<String> {
    let mut out = vec![format!("`{}`", display(cx, c))];
    let mut seen = HashSet::from([c]);
    let mut at = c;
    while let Some(Some((next, _, _))) = facts.ua.get(&at) {
        if !seen.insert(*next) {
            break;
        }
        out.push(format!("`{}`", display(cx, *next)));
        at = *next;
    }
    out
}

/// A K4030 diagnostic at `range` about `callee`, with the path as a note
/// when it goes through other functions.
fn diagnostic(
    cx: &Cx<'_, '_>,
    facts: &Facts,
    file: FileIx,
    range: TextRange,
    callee: Callee,
    message: String,
) -> Diagnostic {
    let path = path(cx, facts, callee);
    let d = Diagnostic::new(codes::USER_ACTION_REQUIRED, cx.p.span(file, range), message);
    if path.len() > 1 {
        d.with_note(format!(
            "the path to the user's action: {}",
            path.join(" → ")
        ))
    } else {
        d
    }
}

/// Reports the calls and uses that break the rule.
fn report(cx: &Cx<'_, '_>, contexts: &[Context], uses: &[Use], facts: &Facts) {
    let mut reported: HashSet<(FileIx, TextRange)> = HashSet::new();
    for c in contexts {
        let mut out = Outcome::default();
        facts.eval(&c.body, State::default(), &mut out);
        match &c.kind {
            Kind::Not(reason) => {
                let all = out
                    .early
                    .iter()
                    .copied()
                    .chain(out.late.iter().map(|(c, r, _)| (*c, *r)));
                for (callee, range) in all {
                    if !reported.insert((c.file, range)) {
                        continue;
                    }
                    let d = diagnostic(
                        cx,
                        facts,
                        c.file,
                        range,
                        callee,
                        format!(
                            "`{}` needs a user's action, such as a click, but {reason} isn't one",
                            display(cx, callee)
                        ),
                    )
                    .with_help(
                        "call it from a click, submit or key handler, before the handler's first `await`",
                    );
                    cx.report(c.file, d);
                }
            }
            Kind::Function(_) | Kind::Action => {
                // A second consuming call is reported as one, even after an
                // `await`.
                let twice: HashSet<TextRange> = out.twice.iter().map(|(_, r, _)| *r).collect();
                for &(callee, range, awaited) in &out.late {
                    if twice.contains(&range) || !reported.insert((c.file, range)) {
                        continue;
                    }
                    let d = diagnostic(
                        cx,
                        facts,
                        c.file,
                        range,
                        callee,
                        format!(
                            "`{}` needs a user's action, which may have expired by the time this `await` finishes",
                            display(cx, callee)
                        ),
                    )
                    .with_label(cx.p.span(c.file, awaited), "the user's action can expire while this waits")
                    .with_help(
                        "call it before the first `await`; load what it needs before the click, in `@Load`",
                    );
                    cx.report(c.file, d);
                }
                for &(callee, range, first) in &out.twice {
                    if !reported.insert((c.file, range)) {
                        continue;
                    }
                    let d = diagnostic(
                        cx,
                        facts,
                        c.file,
                        range,
                        callee,
                        format!(
                            "`{}` uses up the user's action, and an earlier call already did; a second one needs another click",
                            display(cx, callee)
                        ),
                    )
                    .with_label(
                        cx.p.span(c.file, first),
                        if first == range {
                            "this call uses up the user's action, and the loop makes it again"
                        } else {
                            "this call uses up the user's action"
                        },
                    );
                    cx.report(c.file, d);
                }
            }
        }
    }
    for u in uses {
        if !facts.ua.contains_key(&u.callee) || !reported.insert((u.file, u.range)) {
            continue;
        }
        let name = display(cx, u.callee);
        let message = match &u.site {
            Site::Action => continue,
            Site::Handler(event) => format!(
                "`{name}` needs a user's action, and `{event}` isn't one: only click, submit and key events and component events declared `@UserAction` are"
            ),
            Site::Value => format!(
                "`{name}` needs a user's action, so it can only be passed directly to `.on(…)` or `form.handle(…)`, where the compiler sees it is called"
            ),
        };
        let d = diagnostic(cx, facts, u.file, u.range, u.callee, message);
        cx.report(u.file, d);
    }
}

/// A user-action-only function that implements a member of one of the
/// project's interfaces needs that member declared `@UserAction`, so calls
/// through the interface are checked too.
fn interfaces(cx: &Cx<'_, '_>, facts: &Facts) {
    let p = cx.p;
    for (i, d) in p.decls.iter().enumerate() {
        if p.file(d.file).stdlib {
            continue;
        }
        let id = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        let list = cx.implements_list(id);
        for (ty, _) in list.iter() {
            let Some(iface) = ty.decl() else { continue };
            let idecl = p.decl(iface);
            if p.file(idecl.file).stdlib || idecl.named_kind() != Some(K::Interface) {
                continue;
            }
            for &m in &d.members {
                if !facts.ua.contains_key(&Callee::Member(m)) {
                    continue;
                }
                let want = match cx.member_resolution(m) {
                    Resolution::Member { name, .. } => name,
                    _ => continue,
                };
                let required = idecl.members.iter().copied().find(|r| {
                    matches!(cx.member_resolution(*r), Resolution::Member { name, .. } if name == want)
                });
                let Some(r) = required else { continue };
                let req = p.member(r);
                if req.has_annotation("UserAction") {
                    continue;
                }
                let member = p.member(m);
                let mut diag = diagnostic(
                    cx,
                    facts,
                    d.file,
                    member.name_range,
                    Callee::Member(m),
                    format!(
                        "`{}` needs a user's action, so `{}` must be declared `@UserAction`, or calls through `{}` go unchecked",
                        display(cx, Callee::Member(m)),
                        display(cx, Callee::Member(r)),
                        idecl.name
                    ),
                )
                .with_label(
                    p.span(idecl.file, req.name_range),
                    "declared here without `@UserAction`",
                );
                if let Some(node) = &req.node {
                    let start = crate::program::first_token_range(node).start();
                    let text = p.file(idecl.file).text;
                    let line_start = text[..usize::from(start)].rfind('\n').map_or(0, |i| i + 1);
                    let indent = &text[line_start..usize::from(start)];
                    if indent.chars().all(|c| c == ' ' || c == '\t') {
                        diag = diag.with_fix(
                            Fix::replace(
                                "Declare the requirement `@UserAction`",
                                Span::empty(p.span(idecl.file, req.name_range).file, start),
                                format!("@UserAction\n{indent}"),
                            )
                            .maybe_incorrect(),
                        );
                    }
                }
                cx.report(d.file, diag);
            }
        }
    }
}
