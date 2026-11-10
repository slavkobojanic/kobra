//! Lowering of function bodies: statements and expressions to the JS IR
//! (`docs/compiler/codegen.md` §6–§9).
//!
//! A [`Body`] lowers one function-like body. Expressions lower to
//! [`js::Expr`]; constructs that need statements (an `if` or `match` used
//! as a value with statements in its arms, `try … else`) emit them into the
//! current statement buffer and yield a temporary, so no IIFEs are
//! generated. Lazily evaluated operands (`and`, `or`, `??`, conditional
//! arms) are lowered into their own buffer and guarded when they need
//! statements.

use std::collections::{BTreeSet, HashMap, HashSet};

use k_diagnostics::{FileId, TextRange, TextSize};
use k_sema::Resolution;
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, GenId, MemberId, MemberKind, Resolved, Signature, Type};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::cx::{Gen, Target, has_token, mangle, token};
use crate::js::{self, Expr, Param, Prop, Stmt, TemplatePart, bin, call, id, member, rt, un};

/// How `self` and the owner's members are reached in a body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SelfMode {
    /// No owner (a top-level function).
    None,
    /// A struct or enum method: `self` is the first parameter.
    Value,
    /// A class or service method: `this`.
    This,
    /// A struct initializer: fields are locals, the value is built at the
    /// end.
    StructInit,
    /// A component or page: state, props and functions are locals of the
    /// factory (the component lowering supplies how each member reads).
    Component,
}

/// How a component member is read and written inside a component body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MemberAccess {
    /// A plain local (`const x`, a function).
    Local(String),
    /// A signal: `x.get()` / `x.set(v)`.
    Signal(String),
    /// A computed value: `x.get()`.
    Computed(String),
    /// A prop getter: `props.x()`.
    Prop(String),
    /// A binding pair: `props.x.get()` / `props.x.set(v)`.
    Binding(String),
    /// A path store: `x.get([])`, `x.set(path, v)`.
    Store(String),
    /// An expression evaluated as is (a container lookup, `props.content`).
    Raw(Expr),
}

/// Where the value of a lowered statement-like expression goes.
#[derive(Clone, Debug)]
enum Dest {
    /// `return value;`.
    Return,
    /// Assign to the named temporary.
    Assign(String),
    /// Discard (statement position).
    Discard,
}

/// The lowering state of one function-like body.
pub(crate) struct Body<'g, 'a> {
    /// The generator context.
    pub g: &'g Gen<'a>,
    /// The file the body is in: types and resolutions are looked up by range
    /// in it.
    pub file: FileId,
    /// The bundle the body is lowered for.
    pub target: Target,
    /// How `self` and the owner's members are reached.
    pub self_mode: SelfMode,
    /// The declaration whose member is being lowered, `None` for a top-level
    /// function.
    pub owner: Option<DeclId>,
    /// `true` when the body has a cancellation context `$c`.
    pub has_c: bool,
    /// Witness parameters by generic parameter.
    pub witnesses: HashMap<GenId, String>,
    /// Component members by member id (components only).
    pub access: HashMap<MemberId, MemberAccess>,
    /// The visible locals, innermost scope last: each maps the offset of a
    /// local's definition to its K name and its JavaScript name.
    scopes: Vec<HashMap<u32, (String, String)>>,
    /// The JavaScript names taken in each enclosing function scope, so a new
    /// local or temporary never shadows one that is still visible.
    names_in_use: Vec<HashSet<String>>,
    /// The catch variables of the handlers being lowered, innermost last:
    /// a K local declared inside a handler never takes one of these names
    /// (`catch (e) { const e = e; }` is a SyntaxError).
    catch_vars: Vec<String>,
    /// The counter behind [`Body::fresh`]'s temporary names.
    temp: usize,
    /// The collection locals known to be uniquely owned, which may be mutated
    /// in place instead of copied (codegen.md §6.3).
    owned: HashSet<String>,
    /// The symbols of declarations the body references, for imports and
    /// reachability.
    pub uses: BTreeSet<(DeclId, String)>,
    /// The statement buffer that lowering emits into ([`Body::capture`] swaps
    /// it).
    pub stmts: Vec<Stmt>,
    /// Locals that are `var`s holding collections (copy-on-write).
    collection_vars: HashSet<String>,
    /// Struct-init field locals.
    init_fields: Vec<(String, String)>,
    /// The labels of the enclosing loops, innermost last, which `break` and
    /// `continue` target; `None` for an unlabelled loop.
    labels: Vec<Option<String>>,
    /// `true` while lowering the server renderer of a component (no
    /// signals: state reads are plain locals).
    pub ssr: bool,
    /// Module-level statements the body needs (templates).
    pub prelude: Vec<Stmt>,
    /// On-demand pieces of the declaration being lowered.
    pub on_demand: Vec<crate::decl::OnDemand>,
    /// The next lambda is an event handler (no `$c`).
    force_sync: bool,
    /// `true` once the body of a component's client renderer refers to the
    /// component's lifetime `$life`, so the renderer declares it.
    pub(crate) uses_life: bool,
    /// The lambdas lowered as call arguments are bound to their owner
    /// ([`Body::owned_lambda`]): the arguments of `form.handle(_:)`, which
    /// calls its action with no cancellation context.
    own_args: bool,
    /// The function values lowered as call arguments are bound to the
    /// component's lifetime ([`Body::bound_arg`]): the arguments of a
    /// service's method, which the service may keep or call later.
    bind_args: bool,
    /// Locals that are read through a signal accessor named after them
    /// (`row` reads as `row$()`): the item of a keyed list row, so every
    /// binding of the row tracks the item and an item that changes under
    /// the same key re-renders in place (runtime.md §5.3).
    pub(crate) signal_locals: HashSet<String>,
    /// Expressions already evaluated into a temporary, by range: the
    /// operands of a test's `expect` (`test_expect`).
    overrides: HashMap<TextRange, Expr>,
    /// The nil tests of the `?.` links of the optional chains being lowered,
    /// each with the range of its chain's outermost link, which wraps the
    /// whole chain in them ([`Body::chain`]).
    opt_guards: Vec<(TextRange, Expr)>,
    /// The outermost links of the chains with a JavaScript `?.` in them,
    /// which may yield `undefined`.
    opt_undefined: HashSet<TextRange>,
    /// The error-type variables of the enclosing `do`s and `expect throws`
    /// whose body throws more than one error type, innermost last: every
    /// `try` and `throw` in their bodies sets them to the error type's tag
    /// ([`Body::error_tag`]) before it can throw, so a `catch` tells
    /// `A.x` from `B.y` at the same case position.
    err_tags: Vec<String>,
    /// The tags of the `try`s being lowered, innermost last: a `try` in
    /// another's operand sets the variables back to the outer one's tag
    /// once it returns, as the outer call runs after it.
    try_tags: Vec<f64>,
    /// The result type of the function or lambda being lowered, which a
    /// returned value is coerced to ([`Body::coerce_to_return`]); `None`
    /// where it isn't known (nothing is converted).
    pub(crate) ret: Option<Type>,
    /// The error witnesses (`$e_E`) of the function's generic parameters
    /// constrained to `Error`: the text function of the type `E` stands
    /// for, `null` when it is `Error` itself (codegen.md §7).
    pub(crate) error_witnesses: HashMap<GenId, String>,
}

impl<'g, 'a> Body<'g, 'a> {
    /// Creates the state for a body of `file`.
    pub(crate) fn new(g: &'g Gen<'a>, file: FileId, target: Target) -> Body<'g, 'a> {
        Body {
            g,
            file,
            target,
            self_mode: SelfMode::None,
            owner: None,
            has_c: false,
            witnesses: HashMap::new(),
            access: HashMap::new(),
            scopes: vec![HashMap::new()],
            names_in_use: vec![HashSet::new()],
            catch_vars: Vec::new(),
            temp: 0,
            owned: HashSet::new(),
            uses: BTreeSet::new(),
            stmts: Vec::new(),
            collection_vars: HashSet::new(),
            init_fields: Vec::new(),
            labels: Vec::new(),
            ssr: false,
            prelude: Vec::new(),
            on_demand: Vec::new(),
            force_sync: false,
            uses_life: false,
            own_args: false,
            bind_args: false,
            signal_locals: HashSet::new(),
            overrides: HashMap::new(),
            opt_guards: Vec::new(),
            opt_undefined: HashSet::new(),
            err_tags: Vec::new(),
            try_tags: Vec::new(),
            ret: None,
            error_witnesses: HashMap::new(),
        }
    }

    // ------------------------------------------------------------ helpers

    /// The recorded type of `node`.
    pub(crate) fn ty(&self, node: &SyntaxNode) -> Type {
        self.g.type_at(self.file, node.text_range())
    }

    /// The recorded type at exactly `range` of this body's file.
    fn ty_range(&self, range: TextRange) -> Type {
        self.g.type_at(self.file, range)
    }

    /// The name resolution recorded at exactly `range` of this body's file.
    fn resolution(&self, range: TextRange) -> Option<Resolution> {
        self.g.resolution(self.file, range)
    }

    /// The declaration or member that the name at `range` resolves to.
    fn resolved(&self, range: TextRange) -> Option<Resolved> {
        self.g.resolved(self.file, range)
    }

    /// Records a reference to a declaration's symbol and returns it.
    pub(crate) fn use_decl(&mut self, decl: DeclId, symbol: String) -> Expr {
        self.uses.insert((decl, symbol.clone()));
        id(symbol)
    }

    /// The JS name of a declaration, recording the use.
    pub(crate) fn decl_ref(&mut self, decl: DeclId) -> Expr {
        let n = self.g.name(decl);
        self.use_decl(decl, n)
    }

    /// The module function of a member (`Point$moved`), recording the use.
    pub(crate) fn fn_ref(&mut self, m: MemberId) -> Expr {
        let n = self.g.fn_name(m);
        let owner = self.g.view.member(m).owner;
        self.use_decl(owner, n)
    }

    /// A fresh temporary name.
    pub(crate) fn fresh(&mut self, base: &str) -> String {
        loop {
            self.temp += 1;
            let n = format!("${base}{}", self.temp);
            if !self.name_taken(&n) {
                self.names_in_use.last_mut().map(|s| s.insert(n.clone()));
                return n;
            }
        }
    }

    /// `true` if `n` is in use in any enclosing function scope.
    fn name_taken(&self, n: &str) -> bool {
        self.names_in_use.iter().any(|s| s.contains(n))
    }

    /// The name of a `catch` variable: `e`, unless a local uses it. The
    /// name stays reserved against K locals until [`Body::end_catch`], so
    /// a binding in the handler (`catch let e`) is renamed instead.
    pub(crate) fn catch_name(&mut self) -> String {
        let n = if self.name_taken("e") {
            self.fresh("e")
        } else {
            "e".to_owned()
        };
        self.catch_vars.push(n.clone());
        n
    }

    /// Ends the handler whose catch variable [`Body::catch_name`] returned
    /// last.
    pub(crate) fn end_catch(&mut self) {
        self.catch_vars.pop();
    }

    /// Opens a function scope (names may be reused inside it).
    pub(crate) fn push_fn_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.names_in_use.push(HashSet::new());
    }

    /// Closes a function scope.
    pub(crate) fn pop_fn_scope(&mut self) {
        self.scopes.pop();
        self.names_in_use.pop();
    }

    /// Opens a block scope inside the current function scope.
    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Closes the innermost block scope; the JavaScript names it used stay
    /// taken until the function scope closes.
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Declares a local bound at `token`; returns its JS name.
    pub(crate) fn bind(&mut self, token: &SyntaxToken) -> String {
        self.bind_name(token.text(), token.text_range().start())
    }

    /// Declares a local named `name` defined at `at`.
    pub(crate) fn bind_name(&mut self, name: &str, at: TextSize) -> String {
        let base = mangle(name);
        let mut js = base.clone();
        let mut i = 1;
        while self.name_taken(&js) || self.catch_vars.contains(&js) {
            js = format!("{base}${i}");
            i += 1;
        }
        if let Some(s) = self.names_in_use.last_mut() {
            s.insert(js.clone());
        }
        if let Some(s) = self.scopes.last_mut() {
            s.insert(u32::from(at), (name.to_owned(), js.clone()));
        }
        js
    }

    /// The JS name of the innermost visible local named `name` (shorthand
    /// conditions `if let x`).
    pub(crate) fn local_by_name(&self, name: &str) -> Option<String> {
        for s in self.scopes.iter().rev() {
            let mut best: Option<(u32, &String)> = None;
            for (at, (k, js)) in s {
                if k == name && best.is_none_or(|(b, _)| *at > b) {
                    best = Some((*at, js));
                }
            }
            if let Some((_, js)) = best {
                return Some(js.clone());
            }
        }
        None
    }

    /// The JavaScript name of the local `name` defined at `definition`,
    /// searching the innermost scope first; the mangled K name when no scope
    /// records it (a binding made outside this body).
    fn local_name(&self, name: &str, definition: TextRange) -> String {
        let at = u32::from(definition.start());
        for s in self.scopes.iter().rev() {
            if let Some((_, n)) = s.get(&at) {
                return n.clone();
            }
        }
        mangle(name)
    }

    /// The JS name of the local defined at `definition`.
    pub(crate) fn local_js(&self, name: &str, definition: TextRange) -> String {
        self.local_name(name, definition)
    }

    /// `true` when the local collection `js` is uniquely owned.
    pub(crate) fn is_owned(&self, js: &str) -> bool {
        self.owned.contains(js)
    }

    /// Marks the local collection `js` as uniquely owned.
    pub(crate) fn set_owned(&mut self, js: &str) {
        self.owned.insert(js.to_owned());
    }

    /// Appends `s` to the current statement buffer.
    fn emit(&mut self, s: Stmt) {
        self.stmts.push(s);
    }

    /// Runs `f` with a fresh statement buffer and returns what it emitted.
    pub(crate) fn capture<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (Vec<Stmt>, R) {
        let saved = std::mem::take(&mut self.stmts);
        let r = f(self);
        let out = std::mem::replace(&mut self.stmts, saved);
        (out, r)
    }

    /// The expression for `self`.
    pub(crate) fn self_expr(&self) -> Expr {
        match self.self_mode {
            SelfMode::This | SelfMode::Component => Expr::This,
            _ => id("self"),
        }
    }

    /// `$c`, or `null` outside a cancellable body.
    pub(crate) fn c_expr(&self) -> Expr {
        if self.has_c { id("$c") } else { Expr::Null }
    }

    /// `await x` as a body prints it (codegen.md §8): guarded by `$c`
    /// (`$k.guard`, unless `x` is a runtime primitive that honours `$c`
    /// itself), and in a body with a cancellation context checked again
    /// when the body resumes (`$k.live`), which stops the body when the
    /// run was cancelled while `x` settled.
    pub(crate) fn awaited(&self, x: Expr) -> Expr {
        let a = if is_guarded_primitive(&x) {
            Expr::Await(Box::new(x))
        } else {
            Expr::Await(Box::new(rt("guard", vec![self.c_expr(), x])))
        };
        if self.has_c {
            rt("live", vec![id("$c"), a])
        } else {
            a
        }
    }

    /// The check that starts a `catch` after its panic rethrow, and each
    /// iteration of a `for await`, in a body with a cancellation context:
    /// `$k.live($c);`, which stops the body when the run was cancelled
    /// while the promise it resumed from settled. Empty without a context.
    pub(crate) fn live_check(&self) -> Vec<Stmt> {
        if self.has_c {
            vec![Stmt::Expr(rt("live", vec![id("$c")]))]
        } else {
            Vec::new()
        }
    }

    // ------------------------------------------------------- statements

    /// Lowers a block's statements into the current buffer, in a scope.
    pub(crate) fn block(&mut self, block: &ast::Block) {
        self.push_scope();
        for s in block.statements() {
            self.stmt(&s);
        }
        self.pop_scope();
    }

    /// Lowers a block into its own statement list.
    pub(crate) fn block_stmts(&mut self, block: &ast::Block) -> Vec<Stmt> {
        let (stmts, ()) = self.capture(|b| b.block(block));
        stmts
    }

    /// Lowers a block whose last expression is its value, into `dest`.
    fn value_block(&mut self, node: &SyntaxNode, dest: &Dest) {
        self.push_scope();
        let stmts: Vec<ast::Stmt> = match ast::Block::cast(node.clone()) {
            Some(b) => b.statements().collect(),
            None => Vec::new(),
        };
        let n = stmts.len();
        for (i, s) in stmts.iter().enumerate() {
            if i + 1 == n {
                match s {
                    // A branch's value takes the type of the expression
                    // it is a branch of (`A.x` as an `Error`).
                    ast::Stmt::Expr(e)
                        if matches!(dest, Dest::Assign(_)) && !self.is_statementy(e) =>
                    {
                        let to = self.branch_type(node);
                        self.mapped(e.syntax(), |b| {
                            let x = b.expr(e);
                            let x = match &to {
                                Some(to) => {
                                    let from = b.ty(e.syntax());
                                    b.coerce(x, &from, to)
                                }
                                None => x,
                            };
                            if let Dest::Assign(t) = dest {
                                b.emit(Stmt::Expr(js::assign(id(t), x)));
                            }
                        });
                    }
                    ast::Stmt::Expr(e) => {
                        self.mapped(e.syntax(), |b| b.expr_into(e, dest));
                    }
                    other => {
                        self.stmt(other);
                    }
                }
            } else {
                self.stmt(s);
            }
        }
        if n == 0 {
            if let Dest::Return = dest {
                self.emit(Stmt::Return(None));
            }
        }
        self.pop_scope();
    }

    /// The type of the `if`, `match` or `try … else` whose branch is the
    /// block `block`, which the branch's value is converted to; `None`
    /// for any other block.
    fn branch_type(&self, block: &SyntaxNode) -> Option<Type> {
        let parent = block.parent()?;
        let owner = match parent.kind() {
            SyntaxKind::IfExpr | SyntaxKind::TryExpr => parent,
            SyntaxKind::MatchArm => parent
                .ancestors()
                .find(|a| a.kind() == SyntaxKind::MatchExpr)?,
            _ => return None,
        };
        Some(self.ty(&owner))
    }

    /// The K location of `node` for source maps: where its first
    /// non-trivia token starts.
    pub(crate) fn loc(&self, node: &SyntaxNode) -> Option<js::Loc> {
        let start = node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| !t.kind().is_trivia())
            .map_or_else(|| node.text_range().start(), |t| t.text_range().start());
        Some(js::Loc {
            file: self.file.raw(),
            offset: u32::from(start),
        })
    }

    /// `x`, mapped to `node` in the source map ([`Expr::At`]).
    pub(crate) fn at(&self, node: &SyntaxNode, x: Expr) -> Expr {
        js::at(self.loc(node), x)
    }

    /// Lowers one statement; every JavaScript statement it emits maps to it
    /// in the source map ([`Stmt::At`]).
    pub(crate) fn stmt(&mut self, s: &ast::Stmt) {
        let node = s.syntax().clone();
        self.mapped(&node, |b| b.stmt_unmapped(s));
    }

    /// Runs `f`, mapping every statement it emits to `node`.
    pub(crate) fn mapped<R>(&mut self, node: &SyntaxNode, f: impl FnOnce(&mut Self) -> R) -> R {
        let start = self.stmts.len();
        let r = f(self);
        if let Some(loc) = self.loc(node) {
            for st in self.stmts.iter_mut().skip(start) {
                if !matches!(st, Stmt::At(..)) {
                    let inner = std::mem::replace(st, Stmt::Break(None));
                    *st = Stmt::At(loc, Box::new(inner));
                }
            }
        }
        r
    }

    /// [`Body::stmt`] without the source-map wrapping.
    #[allow(clippy::too_many_lines)]
    fn stmt_unmapped(&mut self, s: &ast::Stmt) {
        match s {
            ast::Stmt::Let(l) => self.let_stmt(l.syntax()),
            ast::Stmt::Assign(a) => self.assign_stmt(a.syntax()),
            ast::Stmt::Return(r) => {
                let value = r.syntax().children().find_map(ast::Expr::cast);
                match value {
                    Some(v) => {
                        if self.is_statementy(&v) {
                            self.expr_into(&v, &Dest::Return);
                        } else {
                            let e = self.expr(&v);
                            let e = self.coerce_to_return(e, &v);
                            self.emit(Stmt::Return(Some(e)));
                        }
                    }
                    None => {
                        if self.self_mode == SelfMode::StructInit {
                            let obj = self.init_object();
                            self.emit(Stmt::Return(Some(obj)));
                        } else {
                            self.emit(Stmt::Return(None));
                        }
                    }
                }
            }
            ast::Stmt::Throw(t) => {
                if let Some(v) = t.syntax().children().find_map(ast::Expr::cast) {
                    let tag = self.error_tag(Some(&self.ty(v.syntax())));
                    let e = self.tagged(tag, |b| b.expr(&v));
                    self.emit(Stmt::Throw(e));
                }
            }
            ast::Stmt::Break(_) => {
                let label = self.labels.last().cloned().flatten();
                self.emit(Stmt::Break(label));
            }
            ast::Stmt::Continue(_) => {
                let label = self.labels.last().cloned().flatten();
                self.emit(Stmt::Continue(label));
            }
            ast::Stmt::Guard(g) => self.guard_stmt(g.syntax()),
            ast::Stmt::While(w) => self.while_stmt(w.syntax()),
            ast::Stmt::For(f) => self.for_stmt(f.syntax()),
            ast::Stmt::Do(d) => self.do_stmt(d.syntax()),
            ast::Stmt::Expect(e) if self.g.options.entry == crate::Entry::Tests => {
                if let Some(v) = e.syntax().children().find_map(ast::Expr::cast) {
                    self.test_expect(&v);
                }
            }
            ast::Stmt::ExpectThrows(e) if self.g.options.entry == crate::Entry::Tests => {
                self.test_expect_throws(e.syntax());
            }
            ast::Stmt::Expect(e) => {
                if let Some(v) = e.syntax().children().find_map(ast::Expr::cast) {
                    let c = self.expr(&v);
                    self.emit(Stmt::If(
                        un("!", c),
                        vec![Stmt::Expr(rt("panic", vec![js::str("expectation failed")]))],
                        None,
                    ));
                }
            }
            ast::Stmt::ExpectThrows(e) => {
                let (body, ()) = self.capture(|b| {
                    if let Some(block) = e.syntax().children().find_map(ast::Block::cast) {
                        b.block(&block);
                    }
                    if let Some(v) = e.syntax().children().find_map(ast::Expr::cast) {
                        let x = b.expr(&v);
                        b.emit(Stmt::Expr(x));
                    }
                });
                let flag = self.fresh("threw");
                self.emit(Stmt::Let(flag.clone(), Some(Expr::Bool(false))));
                self.emit(Stmt::Try(
                    body,
                    "e".into(),
                    vec![
                        rethrow_panics(),
                        Stmt::Expr(js::assign(id(&flag), Expr::Bool(true))),
                    ],
                ));
                self.emit(Stmt::If(
                    un("!", id(&flag)),
                    vec![Stmt::Expr(rt("panic", vec![js::str("expected an error")]))],
                    None,
                ));
            }
            ast::Stmt::Expr(e) => self.expr_stmt(e),
        }
    }

    /// Makes `expr` of the node at `range` read as `value` until
    /// [`Body::clear_overrides`]: an expression evaluated once, earlier.
    pub(crate) fn set_override(&mut self, range: TextRange, value: Expr) {
        self.overrides.insert(range, value);
    }

    /// Ends every [`Body::set_override`].
    pub(crate) fn clear_overrides(&mut self) {
        self.overrides.clear();
    }

    /// `expect cond` in a test bundle: the operands of a comparison (or
    /// of `not`) are evaluated once into temporaries, in order, so a
    /// failure can show their values; then `$k.$test.fail` with the
    /// condition's source range and text when it is false
    /// (21-testing.md: "the compiler shows the value of each
    /// sub-expression").
    fn test_expect(&mut self, cond: &ast::Expr) {
        let mut operands: Vec<ast::Expr> = Vec::new();
        match cond {
            ast::Expr::BinaryExpr(bx)
                if bx
                    .op()
                    .is_some_and(|t| matches!(t.text(), "==" | "!=" | "<" | "<=" | ">" | ">=")) =>
            {
                operands.extend(bx.lhs());
                operands.extend(bx.rhs());
            }
            ast::Expr::PrefixExpr(px) if px.op().is_some_and(|t| t.text() == "not") => {
                operands.extend(px.operand());
            }
            _ => {}
        }
        let mut values = Vec::new();
        for o in operands {
            if matches!(o, ast::Expr::Literal(_) | ast::Expr::StringLit(_)) {
                continue;
            }
            let ty = self.ty(o.syntax());
            let x = self.expr(&o);
            let t = self.fresh("e");
            self.emit(Stmt::Const(t.clone(), x));
            self.overrides.insert(o.syntax().text_range(), id(&t));
            let shown = if self.g.is_string(&ty) {
                crate::suite::kit_call("quote", vec![id(&t)])
            } else {
                let f = self.text_fn(&ty);
                call(f, vec![id(&t)])
            };
            values.push(Expr::Array(vec![
                js::str(squash(&o.syntax().text().to_string())),
                js::arrow(vec![], shown),
            ]));
        }
        let c = self.expr(cond);
        self.overrides.clear();
        let range = cond.syntax().text_range();
        let info = Expr::Object(
            vec![
                Prop::KeyValue(
                    "start".into(),
                    Expr::Num(f64::from(u32::from(range.start()))),
                ),
                Prop::KeyValue("end".into(), Expr::Num(f64::from(u32::from(range.end())))),
                Prop::KeyValue(
                    "source".into(),
                    js::str(squash(&cond.syntax().text().to_string())),
                ),
                Prop::KeyValue("values".into(), Expr::Array(values)),
            ],
            false,
        );
        self.emit(Stmt::If(
            un("!", c),
            vec![Stmt::Expr(crate::suite::kit_call("fail", vec![info]))],
            None,
        ));
    }

    /// `expect throws Pattern { … }` in a test bundle: the block must throw
    /// an error that matches the pattern, as a `catch` clause would; a
    /// panic passes through. Otherwise `$k.$test.failThrows` with the
    /// pattern's range and what was thrown instead, if anything.
    fn test_expect_throws(&mut self, node: &SyntaxNode) {
        let block = node.children().find_map(ast::Block::cast);
        let pattern = node.children().find_map(ast::Pattern::cast);
        // As in `do` (`do_stmt`): with two error types, the pattern's
        // enum is checked too.
        let types = self.thrown_decls(&block);
        let et = (types.len() > 1).then(|| self.fresh("et"));
        if let Some(v) = &et {
            self.emit(Stmt::Let(v.clone(), None));
            self.err_tags.push(v.clone());
        }
        let body = match &block {
            Some(b) => self.block_stmts(b),
            None => Vec::new(),
        };
        if et.is_some() {
            self.err_tags.pop();
        }
        let owner = match (&et, &pattern) {
            (Some(_), Some(p)) => self.pattern_case_owner(p, &types),
            _ => None,
        };
        let err_ty = owner.map_or_else(|| self.thrown_type(&block), |d| Type::Named(d, Vec::new()));
        let err = self.catch_name();
        let ok = self.fresh("matched");
        let got = self.fresh("got");
        self.emit(Stmt::Let(ok.clone(), Some(Expr::Bool(false))));
        self.emit(Stmt::Let(got.clone(), Some(Expr::Null)));
        self.push_scope();
        let (test, binds) = match &pattern {
            Some(p) => self.pattern_test(p, &id(&err), &err_ty),
            None => (None, Vec::new()),
        };
        let test = match (&et, owner, test) {
            (Some(v), Some(d), Some(t)) => {
                Some(bin("&&", bin("===", id(v), Expr::Num(f64::from(d.0))), t))
            }
            (_, _, t) => t,
        };
        self.pop_scope();
        let mut handler = vec![rethrow(&err)];
        handler.extend(binds.into_iter().map(|(n, e)| Stmt::Const(n, e)));
        let shown = match &et {
            // What was thrown, by its own type's text.
            Some(v) => js::arrow(vec![], self.tagged_error_text(&err, v, &types, None)),
            None => {
                let f = self.text_fn(&err_ty);
                js::arrow(vec![], call(f, vec![id(&err)]))
            }
        };
        let matched = Stmt::Expr(js::assign(id(&ok), Expr::Bool(true)));
        let missed = Stmt::Expr(js::assign(id(&got), shown));
        match test {
            Some(t) => handler.push(Stmt::If(t, vec![matched], Some(vec![missed]))),
            None => handler.push(matched),
        }
        self.end_catch();
        self.emit(Stmt::Try(body, err, handler));
        let range = pattern
            .as_ref()
            .map_or_else(|| node.text_range(), |p| p.syntax().text_range());
        let text = pattern
            .as_ref()
            .map(|p| squash(&p.syntax().text().to_string()))
            .unwrap_or_default();
        let info = Expr::Object(
            vec![
                Prop::KeyValue(
                    "start".into(),
                    Expr::Num(f64::from(u32::from(range.start()))),
                ),
                Prop::KeyValue("end".into(), Expr::Num(f64::from(u32::from(range.end())))),
                Prop::KeyValue("pattern".into(), js::str(text)),
                Prop::KeyValue("got".into(), id(&got)),
            ],
            false,
        );
        self.emit(Stmt::If(
            un("!", id(&ok)),
            vec![Stmt::Expr(crate::suite::kit_call("failThrows", vec![info]))],
            None,
        ));
    }

    /// Lowers an expression used as a statement: a statement-like expression
    /// (`if`, `match`, …) is lowered straight into statements, anything else
    /// becomes an expression statement.
    fn expr_stmt(&mut self, e: &ast::Expr) {
        self.kill_aliases(e.syntax());
        if self.is_statementy(e) {
            self.expr_into(e, &Dest::Discard);
            return;
        }
        let x = self.expr(e);
        if !matches!(
            x,
            Expr::Ident(_) | Expr::Null | Expr::Undefined | Expr::Num(_) | Expr::Str(_)
        ) {
            self.emit(Stmt::Expr(x));
        }
    }

    /// `true` for `if`, `match` and `do`-like expressions that are best
    /// lowered as statements into a destination.
    fn is_statementy(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::MatchExpr(_) => true,
            ast::Expr::IfExpr(i) => !self.if_is_simple(i.syntax()),
            ast::Expr::TryExpr(t) => has_token(t.syntax(), SyntaxKind::ElseKw),
            ast::Expr::ParenExpr(p) => p
                .syntax()
                .children()
                .find_map(ast::Expr::cast)
                .is_some_and(|i| self.is_statementy(&i)),
            _ => false,
        }
    }

    /// Lowers `e` so its value goes to `dest`.
    fn expr_into(&mut self, e: &ast::Expr, dest: &Dest) {
        match e {
            ast::Expr::MatchExpr(m) => self.match_into(m.syntax(), dest),
            ast::Expr::IfExpr(i) if !self.if_is_simple(i.syntax()) => {
                self.if_into(i.syntax(), dest)
            }
            ast::Expr::TryExpr(t) if has_token(t.syntax(), SyntaxKind::ElseKw) => {
                self.try_else_into(t.syntax(), dest);
            }
            ast::Expr::ParenExpr(p) => {
                if let Some(inner) = p.syntax().children().find_map(ast::Expr::cast) {
                    self.expr_into(&inner, dest);
                }
            }
            _ => {
                let x = self.expr(e);
                match dest {
                    Dest::Return => {
                        let x = self.coerce_to_return(x, e);
                        if self.self_mode == SelfMode::StructInit {
                            self.emit(Stmt::Return(Some(x)));
                        } else if self.ty(e.syntax()).decl() == self.g.b.void
                            || matches!(self.ty(e.syntax()), Type::Never)
                        {
                            if !matches!(x, Expr::Ident(_) | Expr::Undefined) {
                                self.emit(Stmt::Expr(x));
                            }
                            if !matches!(self.ty(e.syntax()), Type::Never) {
                                self.emit(Stmt::Return(None));
                            }
                        } else {
                            self.emit(Stmt::Return(Some(x)));
                        }
                    }
                    Dest::Assign(t) => self.emit(Stmt::Expr(js::assign(id(t), x))),
                    Dest::Discard => {
                        if !matches!(
                            x,
                            Expr::Ident(_)
                                | Expr::Null
                                | Expr::Undefined
                                | Expr::Num(_)
                                | Expr::Str(_)
                        ) {
                            self.emit(Stmt::Expr(x));
                        }
                    }
                }
            }
        }
    }

    /// Lowers `e` as the returned value of a body.
    pub(crate) fn return_value(&mut self, e: &ast::Expr) {
        if self.is_statementy(e) {
            self.expr_into(e, &Dest::Return);
        } else {
            let x = self.expr(e);
            let x = self.coerce_to_return(x, e);
            self.emit(Stmt::Return(Some(x)));
        }
    }

    /// Converts `x`, the lowered value of `e`, to the result type of the
    /// function or lambda it is returned from ([`Body::ret`]), as an
    /// argument is converted to its parameter's type: a concrete error
    /// returned as an `Error` is boxed, a struct returned as a project
    /// interface gets its adapter.
    fn coerce_to_return(&mut self, x: Expr, e: &ast::Expr) -> Expr {
        let Some(to) = self.ret.clone() else {
            return x;
        };
        let from = self.ty(e.syntax());
        self.coerce(x, &from, &to)
    }

    /// Lowers `let` and `var` bindings, including destructuring patterns; a
    /// `var` holding a collection is tracked for copy-on-write.
    fn let_stmt(&mut self, node: &SyntaxNode) {
        let is_var = has_token(node, SyntaxKind::VarKw);
        let value = node.children().find_map(ast::Expr::cast);
        let pattern = node.children().find_map(ast::Pattern::cast);
        let name_tok = token(node, SyntaxKind::Ident);
        if let Some(p) = pattern {
            // `let (a, b) = pair`
            let v = match &value {
                Some(v) => self.expr(v),
                None => Expr::Null,
            };
            let t = self.fresh("t");
            self.emit(Stmt::Const(t.clone(), v));
            let vty = value
                .as_ref()
                .map(|v| self.ty(v.syntax()))
                .unwrap_or(Type::Error);
            let (_, binds) = self.pattern_test(&p, &id(&t), &vty);
            for (n, e) in binds {
                self.emit(Stmt::Const(n, e));
            }
            return;
        }
        let Some(name) = name_tok else {
            return;
        };
        let declared = self.ty_range(name.text_range());
        let init = value.as_ref().map(|v| {
            if self.is_statementy(v) {
                let t = self.fresh("v");
                self.emit(Stmt::Let(t.clone(), None));
                self.expr_into(v, &Dest::Assign(t.clone()));
                id(t)
            } else {
                let x = self.expr(v);
                let from = self.ty(v.syntax());
                self.coerce(x, &from, &declared)
            }
        });
        let fresh = value.as_ref().is_some_and(|v| self.is_fresh(v));
        if let Some(v) = &value {
            self.kill_aliases(v.syntax());
        }
        let js = self.bind(&name);
        if is_var && self.is_collection(&declared) {
            self.collection_vars.insert(js.clone());
            if fresh {
                self.owned.insert(js.clone());
            } else {
                self.owned.remove(&js);
            }
        }
        if is_var {
            self.emit(Stmt::Let(js, init));
        } else {
            self.emit(Stmt::Const(js, init.unwrap_or(Expr::Null)));
        }
    }

    /// `true` for `List`, `Map` and `Set` types, whose locals are
    /// copy-on-write.
    fn is_collection(&self, t: &Type) -> bool {
        [self.g.b.list, self.g.b.map, self.g.b.set]
            .iter()
            .any(|d| self.g.is(t, *d))
    }

    /// `true` for expressions that produce a fresh, unaliased value.
    fn is_fresh(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::ListExpr(_) | ast::Expr::MapExpr(_) => true,
            ast::Expr::CallExpr(_) => true,
            ast::Expr::BinaryExpr(_) => true,
            ast::Expr::ParenExpr(p) => p
                .syntax()
                .children()
                .find_map(ast::Expr::cast)
                .is_some_and(|i| self.is_fresh(&i)),
            _ => false,
        }
    }

    /// Marks locals read as values in `e` as no longer owned.
    fn kill_aliases(&mut self, node: &SyntaxNode) {
        if self.owned.is_empty() {
            return;
        }
        for n in node.descendants() {
            if n.kind() != SyntaxKind::NameExpr {
                continue;
            }
            let captured = n.ancestors().any(|a| {
                a.kind() == SyntaxKind::LambdaExpr
                    && a.text_range().start() >= node.text_range().start()
            });
            let receiver = !captured
                && n.parent().is_some_and(|p| {
                    (p.kind() == SyntaxKind::MemberExpr || p.kind() == SyntaxKind::IndexExpr)
                        && p.first_child().as_ref() == Some(&n)
                });
            if receiver {
                continue;
            }
            if let Some(Resolution::Local { name, definition }) = self.resolution(n.text_range()) {
                let js = self.local_name(&name, definition.range);
                self.owned.remove(&js);
            }
        }
    }

    /// Lowers `guard cond else { … }`: the `else` block runs when a condition
    /// fails, and the `let` bindings of the conditions stay visible after the
    /// statement.
    fn guard_stmt(&mut self, node: &SyntaxNode) {
        let else_block = node.children().filter_map(ast::Block::cast).last();
        let conds: Vec<SyntaxNode> = node
            .children()
            .take_while(|c| c.kind() != SyntaxKind::Block)
            .filter(|c| c.kind() == SyntaxKind::LetCondition || ast::Expr::can_cast(c.kind()))
            .collect();
        let otherwise = match &else_block {
            Some(b) => self.block_stmts(b),
            None => Vec::new(),
        };
        for c in conds {
            if c.kind() == SyntaxKind::LetCondition {
                let (name, value) = let_condition(&c);
                let v = match (&value, &name) {
                    (Some(value), _) => self.expr(value),
                    (None, Some(n)) => self.shorthand_value(n),
                    (None, None) => continue,
                };
                match name {
                    Some(n) => {
                        let js = self.bind(&n);
                        self.emit(Stmt::Const(js.clone(), v));
                        self.emit(Stmt::If(
                            bin("===", id(&js), Expr::Null),
                            otherwise.clone(),
                            None,
                        ));
                    }
                    None => {
                        self.emit(Stmt::If(bin("===", v, Expr::Null), otherwise.clone(), None));
                    }
                }
            } else if let Some(e) = ast::Expr::cast(c) {
                let x = self.expr(&e);
                self.emit(Stmt::If(un("!", x), otherwise.clone(), None));
            }
        }
    }

    /// Lowers a `while` loop. A condition that needs statements is evaluated at
    /// the top of `while (true)` and breaks out when false.
    fn while_stmt(&mut self, node: &SyntaxNode) {
        let cond = node.children().find_map(ast::Expr::cast);
        let body = node.children().find_map(ast::Block::cast);
        if let Some(b) = &body {
            self.kill_aliases(b.syntax());
        }
        let (pre, c) = self.capture(|b| match &cond {
            Some(c) => b.expr(c),
            None => Expr::Bool(true),
        });
        self.labels.push(None);
        let stmts = match &body {
            Some(b) => self.block_stmts(b),
            None => Vec::new(),
        };
        self.labels.pop();
        if pre.is_empty() {
            self.emit(Stmt::While(c, stmts));
        } else {
            let mut inner = pre;
            inner.push(Stmt::If(un("!", c), vec![Stmt::Break(None)], None));
            inner.extend(stmts);
            self.emit(Stmt::While(Expr::Bool(true), inner));
        }
    }

    /// Lowers a `for … in` loop: a counted `for (let i = lo; i < hi; i += 1)`
    /// over an `Int` range literal, otherwise `for … of` over the iterable
    /// (ranges through `$k.rangeValues`), destructuring a pattern into
    /// `const`s at the top of the body; `for await` keeps its `await`.
    fn for_stmt(&mut self, node: &SyntaxNode) {
        let pattern = node.children().find_map(ast::Pattern::cast);
        let iter = node.children().find_map(ast::Expr::cast);
        let body = node.children().find_map(ast::Block::cast);
        let is_await = has_token(node, SyntaxKind::AwaitKw);
        let Some(iter) = iter else { return };
        if let Some(b) = &body {
            self.kill_aliases(b.syntax());
        }
        self.kill_aliases(iter.syntax());
        self.push_scope();
        // Counted loops over Int ranges.
        if let ast::Expr::BinaryExpr(be) = &iter {
            let op = be.op().map(|t| t.text().to_owned()).unwrap_or_default();
            if (op == "..<" || op == "...")
                && self.g.is_int(
                    &self.ty(&be
                        .lhs()
                        .map_or(iter.syntax().clone(), |l| l.syntax().clone())),
                )
            {
                if let (Some(lo), Some(hi)) = (be.lhs(), be.rhs()) {
                    let lo = self.expr(&lo);
                    let hi = self.expr(&hi);
                    let end = if matches!(hi, Expr::Num(_) | Expr::Ident(_)) {
                        hi
                    } else {
                        let t = self.fresh("end");
                        self.emit(Stmt::Const(t.clone(), hi));
                        id(t)
                    };
                    let var = match &pattern {
                        Some(ast::Pattern::IdentPattern(p)) => match p.syntax().first_token() {
                            Some(t) => self.bind(&t),
                            None => self.fresh("i"),
                        },
                        _ => self.fresh("i"),
                    };
                    let cmp = if op == "..<" { "<" } else { "<=" };
                    self.labels.push(None);
                    let stmts = match &body {
                        Some(b) => self.block_stmts(b),
                        None => Vec::new(),
                    };
                    self.labels.pop();
                    self.emit(Stmt::For(
                        Box::new(Stmt::Let(var.clone(), Some(lo))),
                        bin(cmp, id(&var), end),
                        Expr::Assign("+=", Box::new(id(&var)), Box::new(Expr::Num(1.0))),
                        stmts,
                    ));
                    self.pop_scope();
                    return;
                }
            }
        }
        let ity = self.ty(iter.syntax());
        let mut it = self.expr(&iter);
        if self.g.is(&ity, self.g.b.half_open) || self.g.is(&ity, self.g.b.closed) {
            it = rt("rangeValues", vec![it]);
        } else if self.g.is(&ity, self.g.b.map) {
            // entries are [k, v] pairs
        }
        let elem_ty = self.element_type(&ity);
        let var = self.fresh("x");
        let mut prefix = Vec::new();
        if let Some(p) = &pattern {
            match p {
                ast::Pattern::IdentPattern(ip) => {
                    if let Some(t) = ip.syntax().first_token() {
                        let js = self.bind(&t);
                        prefix.push(Stmt::Const(js, id(&var)));
                    }
                }
                other => {
                    let (_, binds) = self.pattern_test(other, &id(&var), &elem_ty);
                    for (n, e) in binds {
                        prefix.push(Stmt::Const(n, e));
                    }
                }
            }
        }
        self.labels.push(None);
        let mut stmts = prefix;
        if let Some(b) = &body {
            stmts.extend(self.block_stmts(b));
        }
        self.labels.pop();
        // Simplify `const x = $x1;` into the loop variable.
        if let Some(Stmt::Const(n, Expr::Ident(v))) = stmts.first() {
            if *v == var {
                let n = n.clone();
                stmts.remove(0);
                if is_await {
                    let mut checked = self.live_check();
                    checked.extend(stmts);
                    stmts = checked;
                }
                self.emit(Stmt::ForOf(n, it, stmts, is_await));
                self.pop_scope();
                return;
            }
        }
        if is_await {
            let mut checked = self.live_check();
            checked.extend(stmts);
            stmts = checked;
        }
        self.emit(Stmt::ForOf(var, it, stmts, is_await));
        self.pop_scope();
    }

    /// The element type of an iterable type.
    pub(crate) fn element_type(&self, t: &Type) -> Type {
        if self.g.is(t, self.g.b.map) {
            return Type::Tuple(t.args().to_vec());
        }
        t.args().first().cloned().unwrap_or(Type::Error)
    }

    /// Lowers `do { … } catch …`: a `try` whose handler rethrows panics, then
    /// tests each `catch` clause's pattern in order. Guarded clauses use a
    /// `$caught` flag so a failed guard falls through to the next clause; an
    /// error no clause matches is rethrown.
    #[allow(clippy::too_many_lines)]
    fn do_stmt(&mut self, node: &SyntaxNode) {
        let body = node.children().find_map(ast::Block::cast);
        let clauses: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::CatchClause)
            .collect();
        // A body that throws errors of two or more types records which
        // type each throw had, as the cases' tags overlap.
        let types = if clauses.is_empty() {
            Vec::new()
        } else {
            self.thrown_decls(&body)
        };
        // `catch .email` after a repository write: the entity's
        // `UniqueViolation` case, by tag (crate::db), a type of its own.
        let unique = body
            .as_ref()
            .and_then(|b| crate::db::unique_fields_of_try(self, b));
        let kinds = types.len() + usize::from(unique.is_some());
        let et = (kinds > 1).then(|| self.fresh("et"));
        if let Some(v) = &et {
            self.emit(Stmt::Let(v.clone(), None));
            self.err_tags.push(v.clone());
        }
        let try_body = match &body {
            Some(b) => self.block_stmts(b),
            None => Vec::new(),
        };
        if et.is_some() {
            self.err_tags.pop();
        }
        if clauses.is_empty() {
            self.emit(Stmt::Block(try_body));
            return;
        }
        let err = self.catch_name();
        let mut handler = vec![Stmt::If(
            rt("isPanic", vec![id(&err)]),
            vec![Stmt::Throw(id(&err))],
            None,
        )];
        handler.extend(self.live_check());
        // Each clause: if (pattern) { body } else next; a final rethrow if
        // not exhaustive.
        let mut chain: Vec<(Option<Expr>, Vec<Stmt>, Vec<Stmt>)> = Vec::new();
        let mut exhaustive = false;
        // The thrown error's type: the error type of the try body is not
        // recorded, so patterns are matched structurally on tags.
        let err_ty = self.thrown_type(&body);
        for c in &clauses {
            let patterns: Vec<ast::Pattern> = c.children().filter_map(ast::Pattern::cast).collect();
            let guard = c
                .children()
                .find(|n| n.kind() == SyntaxKind::MatchGuard)
                .and_then(|g| g.children().find_map(ast::Expr::cast));
            let block = c.children().find_map(ast::Block::cast);
            self.push_scope();
            let mut tests = Vec::new();
            let mut binds = Vec::new();
            let mut catch_all = patterns.is_empty();
            for p in &patterns {
                if let Some(tag) = unique
                    .as_ref()
                    .and_then(|u| crate::db::unique_case_tag(p, u))
                {
                    let t = bin("===", member(id(&err), "$"), Expr::Num(tag as f64));
                    // The `UniqueViolation` type has no declaration: `-1`.
                    tests.push(Some(match &et {
                        Some(v) => bin("&&", bin("===", id(v), Expr::Num(-1.0)), t),
                        None => t,
                    }));
                    continue;
                }
                let owner = et.as_ref().and_then(|_| self.pattern_case_owner(p, &types));
                let ty = owner.map_or_else(|| err_ty.clone(), |d| Type::Named(d, Vec::new()));
                // `catch let error` over two or more types binds an
                // `Error`: the thrown value boxed with its text.
                let value = match &et {
                    Some(v)
                        if patterns.len() == 1 && matches!(p, ast::Pattern::BindingPattern(_)) =>
                    {
                        let text = self.tagged_error_text(&err, v, &types, unique.as_deref());
                        let boxed = error_box(id(&err), text);
                        // An `Error` the body threw is a box already.
                        match self.g.b.error.filter(|e| types.contains(e)) {
                            Some(d) => Expr::Cond(
                                Box::new(bin("===", id(v), Expr::Num(f64::from(d.0)))),
                                Box::new(id(&err)),
                                Box::new(boxed),
                            ),
                            None => boxed,
                        }
                    }
                    _ => id(&err),
                };
                let (t, b) = self.pattern_test(p, &value, &ty);
                if t.is_none() {
                    catch_all = true;
                }
                let t = match (&et, owner, t) {
                    (Some(v), Some(d), Some(t)) => {
                        Some(bin("&&", bin("===", id(v), Expr::Num(f64::from(d.0))), t))
                    }
                    (_, _, t) => t,
                };
                tests.push(t);
                binds = b;
            }
            let test = if catch_all {
                None
            } else {
                tests.into_iter().flatten().reduce(|a, b| bin("||", a, b))
            };
            let mut pre: Vec<Stmt> = binds.into_iter().map(|(n, e)| Stmt::Const(n, e)).collect();
            let (guard_pre, guard_e) = match &guard {
                Some(g) => {
                    let (s, e) = self.capture(|b| b.expr(g));
                    (s, Some(e))
                }
                None => (Vec::new(), None),
            };
            let stmts = match &block {
                Some(b) => self.block_stmts(b),
                None => Vec::new(),
            };
            self.pop_scope();
            if let Some(ge) = guard_e {
                pre.extend(guard_pre);
                let combined_test = test;
                chain.push((combined_test, pre, vec![Stmt::If(ge, stmts, None)]));
                continue;
            }
            if catch_all {
                exhaustive = true;
            }
            let mut all = pre;
            all.extend(stmts);
            chain.push((test, Vec::new(), all));
            if catch_all {
                break;
            }
        }
        // Lower the chain: guarded clauses fall through to the next one.
        let done = self.fresh("caught");
        let has_guards = chain
            .iter()
            .any(|(_, pre, body)| !pre.is_empty() && matches!(body.as_slice(), [Stmt::If(..)]));
        if has_guards {
            handler.push(Stmt::Let(done.clone(), Some(Expr::Bool(false))));
            for (test, pre, body) in chain {
                let mut inner = pre;
                let mut body = body;
                // Mark handled inside the guard / body.
                mark_handled(&mut body, &done);
                inner.extend(body);
                let guarded = Stmt::If(un("!", id(&done)), vec![], None);
                let _ = guarded;
                match test {
                    Some(t) => {
                        handler.push(Stmt::If(bin("&&", un("!", id(&done)), t), inner, None))
                    }
                    None => handler.push(Stmt::If(un("!", id(&done)), inner, None)),
                }
            }
            if !exhaustive {
                handler.push(Stmt::If(
                    un("!", id(&done)),
                    vec![Stmt::Throw(id(&err))],
                    None,
                ));
            }
        } else {
            let mut otherwise: Option<Vec<Stmt>> = if exhaustive {
                None
            } else {
                Some(vec![Stmt::Throw(id(&err))])
            };
            for (test, _, body) in chain.into_iter().rev() {
                match test {
                    Some(t) => {
                        otherwise = Some(vec![Stmt::If(t, body, otherwise)]);
                    }
                    None => {
                        otherwise = Some(body);
                    }
                }
            }
            handler.extend(otherwise.unwrap_or_default());
        }
        self.end_catch();
        self.emit(Stmt::Try(try_body, err, handler));
    }

    /// The tag of error type `t` that [`Body::err_tags`] are set to: its
    /// declaration's index, or `-1` for a type without one (an entity's
    /// `UniqueViolation`); `None` outside a `do` that needs one.
    fn error_tag(&self, t: Option<&Type>) -> Option<f64> {
        if self.err_tags.is_empty() {
            return None;
        }
        Some(t.and_then(Type::decl).map_or(-1.0, |d| f64::from(d.0)))
    }

    /// The text of the error caught in `err` inside a `do` over several
    /// error types, by the type `et` says was thrown: each type's
    /// interpolation text, the cases of an entity's `UniqueViolation`
    /// (`unique`, tag `-1`) by name, and an `Error` box's own text.
    fn tagged_error_text(
        &mut self,
        err: &str,
        et: &str,
        types: &[DeclId],
        unique: Option<&[String]>,
    ) -> Expr {
        let mut texts: Vec<(f64, Expr)> = Vec::new();
        for &d in types {
            let t = Type::Named(d, Vec::new());
            texts.push((f64::from(d.0), self.text_of(id(err), &t)));
        }
        if let Some(fields) = unique {
            let names = fields.iter().map(|f| js::str(f.clone())).collect();
            texts.push((
                -1.0,
                Expr::Index(Box::new(Expr::Array(names)), Box::new(member(id(err), "$"))),
            ));
        }
        let last = texts.pop().map_or(Expr::Null, |(_, t)| t);
        texts.into_iter().rev().fold(last, |acc, (tag, t)| {
            Expr::Cond(
                Box::new(bin("===", id(et), Expr::Num(tag))),
                Box::new(t),
                Box::new(acc),
            )
        })
    }

    /// `x`, of the concrete error type `from`, as an `Error`: boxed with
    /// its text (codegen.md §7). A value of a generic parameter constrained
    /// to `Error` is boxed by its error witness, and stays as it is when
    /// the parameter stands for `Error` (a box already). `x` is read more
    /// than once, so anything but a name or a member of one (`B.z`) goes
    /// through an arrow's parameter
    /// (`(($b) => ({ error: $b, text: A$$text($b) }))(A.y(3))`), which
    /// works wherever the conversion is: a body, a default argument or a
    /// field's default.
    fn boxed_error(&mut self, x: Expr, from: &Type) -> Expr {
        let witness = match from {
            Type::Param(g) => match self.error_witnesses.get(g) {
                Some(w) => Some(w.clone()),
                None => return x,
            },
            _ => None,
        };
        let pure = match &x {
            Expr::Ident(_) => true,
            Expr::Member(o, _) => matches!(**o, Expr::Ident(_)),
            _ => false,
        };
        let (v, bound) = if pure { (x, None) } else { (id("$b"), Some(x)) };
        let boxed = match witness {
            Some(w) => Expr::Cond(
                Box::new(id(&w)),
                Box::new(error_box(v.clone(), call(id(&w), vec![v.clone()]))),
                Box::new(v),
            ),
            None => {
                let text = self.text_of(v.clone(), from);
                error_box(v, text)
            }
        };
        match bound {
            Some(x) => call(js::arrow(vec![Param::new("$b")], boxed), vec![x]),
            None => boxed,
        }
    }

    /// Lowers a `try` operand or a thrown value with `lower`, preceded by
    /// setting the error-type variables to `tag` (`($et = 7, f())`); a
    /// `try` inside another's operand then sets them back to the outer
    /// one's tag (`($et = 7, $t = g(), $et = 3, $t)`).
    fn tagged(&mut self, tag: Option<f64>, lower: impl FnOnce(&mut Self) -> Expr) -> Expr {
        let Some(tag) = tag else {
            return lower(self);
        };
        let outer = self.try_tags.last().copied();
        self.try_tags.push(tag);
        let x = lower(self);
        self.try_tags.pop();
        let set = |b: &Self, tag: f64| {
            b.err_tags
                .iter()
                .rev()
                .fold(Expr::Num(tag), |acc, v| js::assign(id(v), acc))
        };
        match outer {
            Some(o) => {
                let t = self.fresh("t");
                self.emit(Stmt::Let(t.clone(), None));
                let back = bin(",", set(self, o), id(&t));
                bin(",", set(self, tag), bin(",", js::assign(id(&t), x), back))
            }
            None => bin(",", set(self, tag), x),
        }
    }

    /// The error types the `try`s and `throw`s of `body` throw, outside its
    /// lambdas, each once.
    fn thrown_decls(&self, body: &Option<ast::Block>) -> Vec<DeclId> {
        let Some(b) = body else { return Vec::new() };
        let mut out = Vec::new();
        let mut walk = b.syntax().preorder();
        while let Some(ev) = walk.next() {
            let rowan::WalkEvent::Enter(n) = ev else {
                continue;
            };
            let t = match n.kind() {
                SyntaxKind::LambdaExpr => {
                    walk.skip_subtree();
                    continue;
                }
                SyntaxKind::ThrowStmt => n
                    .children()
                    .find_map(ast::Expr::cast)
                    .map(|e| self.ty(e.syntax())),
                SyntaxKind::TryExpr if !has_token(&n, SyntaxKind::ElseKw) => n
                    .children()
                    .find_map(ast::Expr::cast)
                    .and_then(|i| self.throws_of_call(&crate::db::strip_parens(&strip_await(&i)))),
                _ => None,
            };
            if let Some(d) = t.as_ref().and_then(Type::decl) {
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
        out
    }

    /// The error enum whose case `p` names (`.cancelled`, `E.x(let y)`),
    /// among `types` when the name isn't resolved; `None` for a pattern
    /// that names no case.
    fn pattern_case_owner(&self, p: &ast::Pattern, types: &[DeclId]) -> Option<DeclId> {
        let node = match p {
            ast::Pattern::BindingPattern(b) => {
                let inner = b.syntax().children().find_map(ast::Pattern::cast)?;
                return self.pattern_case_owner(&inner, types);
            }
            ast::Pattern::EnumCasePattern(ep) => ep.syntax().clone(),
            ast::Pattern::IdentPattern(ip) => ip.syntax().clone(),
            _ => return None,
        };
        let name = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind().is_name_like() && t.kind() != SyntaxKind::LetKw)
            .last()?;
        if let Some(Resolved::Member(m)) = self.resolved(name.text_range()) {
            let mi = self.g.view.member(m);
            if mi.kind == MemberKind::Case {
                return Some(mi.owner);
            }
        }
        if !matches!(p, ast::Pattern::EnumCasePattern(_)) {
            return None;
        }
        types.iter().copied().find(|&d| {
            self.g
                .find_member(d, name.text())
                .is_some_and(|m| self.g.view.member(m).kind == MemberKind::Case)
        })
    }

    /// The error type thrown in a `do` body: the throws type of the first
    /// `try`'s callee.
    fn thrown_type(&self, body: &Option<ast::Block>) -> Type {
        let Some(b) = body else { return Type::Error };
        for n in b.syntax().descendants() {
            if n.kind() == SyntaxKind::ThrowStmt {
                if let Some(e) = n.children().find_map(ast::Expr::cast) {
                    let t = self.ty(e.syntax());
                    if t.decl().is_some() {
                        return t;
                    }
                }
            }
            if n.kind() != SyntaxKind::TryExpr {
                continue;
            }
            let Some(inner) = n.children().find_map(ast::Expr::cast) else {
                continue;
            };
            let inner = strip_await(&inner);
            if let Some(t) = self.throws_of_call(&inner) {
                return t;
            }
        }
        Type::Error
    }

    /// The declared error type of the function or member called by `e` (or of
    /// the function value's type), `None` when `e` is not a call or the
    /// callee doesn't throw.
    fn throws_of_call(&self, e: &ast::Expr) -> Option<Type> {
        let ast::Expr::CallExpr(c) = e else {
            return None;
        };
        let callee = c.callee()?;
        let range = match &callee {
            ast::Expr::NameExpr(n) => n.syntax().text_range(),
            ast::Expr::MemberExpr(m) => m.name()?.text_range(),
            ast::Expr::ImplicitMemberExpr(m) => m.name()?.text_range(),
            _ => {
                if let Type::Func(f) = self.ty(callee.syntax()) {
                    return f.throws.clone();
                }
                return None;
            }
        };
        match self.resolved(range) {
            Some(Resolved::Decl(d)) => self.g.view.decl(d).signature.as_ref()?.throws.clone(),
            Some(Resolved::Member(m)) => self.g.view.member(m).signature.as_ref()?.throws.clone(),
            None => {
                if let Type::Func(f) = self.ty(callee.syntax()) {
                    return f.throws.clone();
                }
                None
            }
        }
    }

    // ---------------------------------------------------------- patterns

    /// Lowers a pattern against `value` of type `ty`: the test (`None` when
    /// it always matches) and the bindings it introduces.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn pattern_test(
        &mut self,
        p: &ast::Pattern,
        value: &Expr,
        ty: &Type,
    ) -> (Option<Expr>, Vec<(String, Expr)>) {
        match p {
            ast::Pattern::WildcardPattern(_) => (None, Vec::new()),
            ast::Pattern::BindingPattern(b) => {
                // `let x` (possibly with a nested pattern)
                if let Some(inner) = b.syntax().children().find_map(ast::Pattern::cast) {
                    return self.pattern_test(&inner, value, ty);
                }
                match b
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|t| t.kind() == SyntaxKind::Ident)
                    .last()
                {
                    Some(t) => {
                        let n = self.bind(&t);
                        (None, vec![(n, value.clone())])
                    }
                    None => (None, Vec::new()),
                }
            }
            ast::Pattern::IdentPattern(ip) => {
                let Some(t) = ip.syntax().first_token() else {
                    return (None, Vec::new());
                };
                // A bare name: a binding (in `for`/`let` patterns) or an
                // enum case without a dot.
                if let Some(Resolved::Member(m)) = self.resolved(t.text_range()) {
                    if self.g.view.member(m).kind == MemberKind::Case {
                        let tag = self.g.case_tag(m);
                        return (
                            Some(bin(
                                "===",
                                member(value.clone(), "$"),
                                Expr::Num(tag as f64),
                            )),
                            Vec::new(),
                        );
                    }
                }
                let n = self.bind(&t);
                (None, vec![(n, value.clone())])
            }
            ast::Pattern::LiteralPattern(lp) => {
                let lit = self.literal_pattern_value(lp.syntax(), ty);
                let test = if self.g.is_identity_eq(ty) || matches!(lit, Expr::Null) {
                    bin("===", value.clone(), lit)
                } else {
                    rt("equals", vec![value.clone(), lit])
                };
                (Some(test), Vec::new())
            }
            ast::Pattern::RangePattern(rp) => {
                let bounds: Vec<SyntaxNode> = rp
                    .syntax()
                    .children()
                    .filter(|n| n.kind() == SyntaxKind::LiteralPattern)
                    .collect();
                let closed = has_token(rp.syntax(), SyntaxKind::DotDotDot);
                let mut test: Option<Expr> = None;
                if let Some(lo) = bounds.first() {
                    let v = self.literal_pattern_value(lo, ty);
                    test = Some(bin(">=", value.clone(), v));
                }
                if let Some(hi) = bounds.get(1) {
                    let v = self.literal_pattern_value(hi, ty);
                    let t = bin(if closed { "<=" } else { "<" }, value.clone(), v);
                    test = Some(match test {
                        Some(a) => bin("&&", a, t),
                        None => t,
                    });
                }
                (test, Vec::new())
            }
            ast::Pattern::EnumCasePattern(ep) => self.case_pattern(ep.syntax(), value, ty),
            ast::Pattern::TuplePattern(tp) => {
                let items: Vec<ast::Pattern> = tp
                    .syntax()
                    .children()
                    .filter_map(ast::Pattern::cast)
                    .collect();
                let tys: Vec<Type> = match ty {
                    Type::Tuple(t) => t.clone(),
                    _ => Vec::new(),
                };
                let mut test = None;
                let mut binds = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let v = Expr::Index(Box::new(value.clone()), Box::new(Expr::Num(i as f64)));
                    let t = tys.get(i).cloned().unwrap_or(Type::Error);
                    let (pt, pb) = self.pattern_test(item, &v, &t);
                    if let Some(pt) = pt {
                        test = Some(match test {
                            Some(a) => bin("&&", a, pt),
                            None => pt,
                        });
                    }
                    binds.extend(pb);
                }
                (test, binds)
            }
        }
    }

    /// Lowers an enum case pattern (`.outOfStock(let n)`, `Error.atLimit`,
    /// `.some(x)`, `.none`) matched against `value` of type `ty`.
    ///
    /// Returns the test (`None` when the pattern always matches) and the
    /// bindings it introduces, as `(name, value)` pairs.
    fn case_pattern(
        &mut self,
        node: &SyntaxNode,
        value: &Expr,
        ty: &Type,
    ) -> (Option<Expr>, Vec<(String, Expr)>) {
        // The case is the last name of the pattern's own tokens: `.atLimit`,
        // or `CounterError.atLimit` qualified by its type (the payload's
        // patterns are child nodes).
        let name = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind().is_name_like() && t.kind() != SyntaxKind::LetKw)
            .last();
        let Some(name) = name else {
            return (None, Vec::new());
        };
        // `.some(x)` / `.none` on optionals.
        let base = ty.unwrap_optional();
        if matches!(ty, Type::Optional(_)) && (name.text() == "some" || name.text() == "none") {
            if name.text() == "none" {
                return (Some(bin("===", value.clone(), Expr::Null)), Vec::new());
            }
            let mut binds = Vec::new();
            let mut test = Some(bin("!==", value.clone(), Expr::Null));
            if let Some(inner) = node.children().find_map(ast::Pattern::cast) {
                let (t, b) = self.pattern_test(&inner, value, base);
                if let Some(t) = t {
                    test = Some(bin("&&", test.unwrap_or(Expr::Bool(true)), t));
                }
                binds = b;
            }
            return (test, binds);
        }
        let case = match self.resolved(name.text_range()) {
            Some(Resolved::Member(m)) if self.g.view.member(m).kind == MemberKind::Case => Some(m),
            _ => base.decl().and_then(|d| self.g.find_member(d, name.text())),
        };
        let Some(case) = case else {
            return (None, Vec::new());
        };
        let tag = self.g.case_tag(case);
        let owner = self.g.view.member(case).owner;
        let mut test = Some(if self.g.is_simple_enum(&Type::Named(owner, Vec::new())) {
            let r = self.decl_ref(owner);
            bin(
                "===",
                value.clone(),
                member(r, self.g.view.member(case).name.clone()),
            )
        } else {
            bin("===", member(value.clone(), "$"), Expr::Num(tag as f64))
        });
        if matches!(ty, Type::Optional(_)) {
            test = Some(bin(
                "&&",
                bin("!==", value.clone(), Expr::Null),
                test.unwrap_or(Expr::Bool(true)),
            ));
        }
        let fields = self.g.case_fields(case);
        let ptypes: Vec<Type> = self
            .g
            .view
            .member(case)
            .signature
            .as_ref()
            .map(|s| {
                s.params
                    .iter()
                    .map(|p| self.subst_owner(&p.ty, base))
                    .collect()
            })
            .unwrap_or_default();
        let mut binds = Vec::new();
        let subs: Vec<SyntaxNode> = node
            .children()
            .filter(|n| ast::Pattern::can_cast(n.kind()) || n.kind() == SyntaxKind::PatternField)
            .collect();
        let mut positional = 0usize;
        for sub in subs {
            let (field_ix, pat) = if sub.kind() == SyntaxKind::PatternField {
                let label = sub
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind().is_name_like())
                    .map(|t| t.text().to_owned())
                    .unwrap_or_default();
                let ix = fields
                    .iter()
                    .position(|f| *f == label)
                    .unwrap_or(positional);
                (ix, sub.children().find_map(ast::Pattern::cast))
            } else {
                (positional, ast::Pattern::cast(sub))
            };
            positional = field_ix + 1;
            let Some(pat) = pat else { continue };
            let Some(field) = fields.get(field_ix) else {
                continue;
            };
            let v = member(value.clone(), field.clone());
            let pt = ptypes.get(field_ix).cloned().unwrap_or(Type::Error);
            let (t, b) = self.pattern_test(&pat, &v, &pt);
            if let Some(t) = t {
                test = Some(bin("&&", test.unwrap_or(Expr::Bool(true)), t));
            }
            binds.extend(b);
        }
        (test, binds)
    }

    /// Substitutes the owner's generic arguments of `owner_ty` into `t`.
    pub(crate) fn subst_owner(&self, t: &Type, owner_ty: &Type) -> Type {
        let Some(d) = owner_ty.decl() else {
            return t.clone();
        };
        let generics = &self.g.view.decl(d).generics;
        let args = owner_ty.args();
        subst(t, &|g| {
            generics
                .iter()
                .position(|x| *x == g)
                .and_then(|i| args.get(i).cloned())
        })
    }

    /// The value a literal pattern compares against: its literal or string,
    /// negated when the pattern has a leading `-`.
    fn literal_pattern_value(&mut self, node: &SyntaxNode, ty: &Type) -> Expr {
        // A literal pattern holds a literal or a string, possibly negated.
        let neg = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::Minus);
        let inner = node.children().find_map(ast::Expr::cast);
        let v = match inner {
            Some(e) => self.expr(&e),
            None => match node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| !t.kind().is_trivia() && t.kind() != SyntaxKind::Minus)
            {
                Some(t) => self.literal_token(&t, ty),
                None => Expr::Null,
            },
        };
        if neg {
            match v {
                Expr::Num(n) => Expr::Num(-n),
                other => un("-", other),
            }
        } else {
            v
        }
    }

    // ------------------------------------------------------------- match

    /// Lowers a `match` whose value goes to `dest`: a `switch` on the case tag
    /// for an enum with payloads matched without guards or nested patterns,
    /// otherwise an `if` chain of pattern tests (guarded arms in a labelled
    /// block, so a failed guard falls through). Falling off the end of an
    /// exhaustive match reaches `$k.unreachable()`.
    #[allow(clippy::too_many_lines)]
    fn match_into(&mut self, node: &SyntaxNode, dest: &Dest) {
        let scrutinee = node.children().find_map(ast::Expr::cast);
        let arms: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MatchArm)
            .collect();
        let Some(scrutinee) = scrutinee else { return };
        let sty = self.ty(scrutinee.syntax());
        let mut sv = self.expr(&scrutinee);
        if !matches!(sv, Expr::Ident(_)) {
            let t = self.fresh("m");
            self.emit(Stmt::Const(t.clone(), sv));
            sv = id(t);
        }
        // An enum with payloads and no guards or nested patterns: switch on
        // the tag.
        let is_enum = self.g.kind_of(&sty) == Some(DeclarationKind::Enum)
            && !matches!(sty, Type::Optional(_));
        let switchable = is_enum
            && !self.g.is_simple_enum(&sty)
            && arms.iter().all(|a| {
                !a.children().any(|n| n.kind() == SyntaxKind::MatchGuard)
                    && a.children()
                        .filter_map(ast::Pattern::cast)
                        .all(|p| match p {
                            ast::Pattern::EnumCasePattern(ep) => ep.syntax().children().all(|c| {
                                match ast::Pattern::cast(c.clone()) {
                                    Some(
                                        ast::Pattern::BindingPattern(_)
                                        | ast::Pattern::WildcardPattern(_),
                                    ) => true,
                                    Some(_) => false,
                                    None => {
                                        c.kind() == SyntaxKind::PatternField
                                            && c.children().filter_map(ast::Pattern::cast).all(
                                                |p| {
                                                    matches!(
                                                        p,
                                                        ast::Pattern::BindingPattern(_)
                                                            | ast::Pattern::WildcardPattern(_)
                                                    )
                                                },
                                            )
                                    }
                                }
                            }),
                            ast::Pattern::WildcardPattern(_) => true,
                            _ => false,
                        })
            });
        if switchable {
            let mut cases = Vec::new();
            let mut has_default = false;
            for arm in &arms {
                let pats: Vec<ast::Pattern> =
                    arm.children().filter_map(ast::Pattern::cast).collect();
                self.push_scope();
                let mut tests = Vec::new();
                let mut binds = Vec::new();
                let mut wildcard = false;
                for p in &pats {
                    if let ast::Pattern::WildcardPattern(_) = p {
                        wildcard = true;
                        continue;
                    }
                    if let ast::Pattern::EnumCasePattern(ep) = p {
                        let (_, b) = self.case_pattern(ep.syntax(), &sv, &sty);
                        binds = b;
                        if let Some(case) = self.case_of_pattern(ep.syntax(), &sty) {
                            tests.push(Expr::Num(self.g.case_tag(case) as f64));
                        }
                    }
                }
                let block = arm.children().find_map(ast::Block::cast);
                let (mut body, ()) = self.capture(|b| {
                    for (n, e) in binds {
                        b.emit(Stmt::Const(n, e));
                    }
                    if let Some(block) = &block {
                        b.value_block(block.syntax(), dest);
                    }
                });
                self.pop_scope();
                if !ends_flow(&body) {
                    body.push(Stmt::Break(None));
                }
                // `_` and a `default` arm (no patterns) are the `default:`
                // clause; the exhaustive match then needs no other.
                if wildcard || pats.is_empty() {
                    has_default = true;
                    cases.push(js::Case {
                        tests: Vec::new(),
                        body,
                    });
                } else {
                    cases.push(js::Case { tests, body });
                }
            }
            if !has_default {
                let unreachable = rt("unreachable", vec![]);
                let body = match dest {
                    Dest::Return => vec![Stmt::Return(Some(unreachable))],
                    _ => vec![Stmt::Expr(unreachable)],
                };
                cases.push(js::Case {
                    tests: Vec::new(),
                    body,
                });
            }
            self.emit(Stmt::Switch(member(sv, "$"), cases));
            return;
        }
        // General form: an if chain.
        let mut chain: Vec<MatchArmCode> = Vec::new();
        for arm in &arms {
            let pats: Vec<ast::Pattern> = arm.children().filter_map(ast::Pattern::cast).collect();
            let guard = arm
                .children()
                .find(|n| n.kind() == SyntaxKind::MatchGuard)
                .and_then(|g| g.children().find_map(ast::Expr::cast));
            self.push_scope();
            let mut tests = Vec::new();
            let mut binds = Vec::new();
            let mut always = pats.is_empty();
            for p in &pats {
                let (t, b) = self.pattern_test(p, &sv, &sty);
                match t {
                    Some(t) => tests.push(t),
                    None => always = true,
                }
                if !b.is_empty() {
                    binds = b;
                }
            }
            let test = if always {
                None
            } else {
                tests.into_iter().reduce(|a, b| bin("||", a, b))
            };
            let bind_stmts: Vec<Stmt> = binds.into_iter().map(|(n, e)| Stmt::Const(n, e)).collect();
            let guard_l = guard.map(|g| self.capture(|b| b.expr(&g)));
            let block = arm.children().find_map(ast::Block::cast);
            let (body, ()) = self.capture(|b| {
                if let Some(block) = &block {
                    b.value_block(block.syntax(), dest);
                }
            });
            self.pop_scope();
            chain.push((test, bind_stmts, guard_l, body));
        }
        let has_guard = chain.iter().any(|c| c.2.is_some());
        if has_guard {
            // Guards fall through: lower as a labelled block of ifs.
            let label = self.fresh("match");
            let mut inner = Vec::new();
            let mut exhaustive = false;
            for (test, binds, guard, body) in chain {
                let mut arm = binds;
                let mut body = body;
                if !ends_flow(&body) {
                    body.push(Stmt::Break(Some(label.clone())));
                }
                match guard {
                    Some((pre, g)) => {
                        arm.extend(pre);
                        arm.push(Stmt::If(g, body, None));
                    }
                    None => arm.extend(body),
                }
                match test {
                    Some(t) => inner.push(Stmt::If(t, arm, None)),
                    None => {
                        inner.push(Stmt::Block(arm));
                        exhaustive = true;
                    }
                }
            }
            if !exhaustive {
                inner.push(Stmt::Expr(rt("unreachable", vec![])));
            }
            self.emit(Stmt::Labeled(label, Box::new(Stmt::Block(inner))));
            return;
        }
        let mut otherwise: Option<Vec<Stmt>> = None;
        let mut first = true;
        for (test, binds, _, body) in chain.into_iter().rev() {
            let mut arm = binds;
            arm.extend(body);
            match test {
                Some(t) => {
                    let rest = otherwise.take().or_else(|| {
                        if first {
                            Some(vec![Stmt::Expr(rt("unreachable", vec![]))])
                        } else {
                            None
                        }
                    });
                    otherwise = Some(vec![Stmt::If(t, arm, rest)]);
                }
                None => otherwise = Some(arm),
            }
            first = false;
        }
        if let Some(stmts) = otherwise {
            self.stmts.extend(stmts);
        }
    }

    /// The enum case a case pattern names: the recorded resolution of its
    /// first name, or the member of that name on `ty` (optionals unwrapped).
    fn case_of_pattern(&self, node: &SyntaxNode, ty: &Type) -> Option<MemberId> {
        let name = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())?;
        match self.resolved(name.text_range()) {
            Some(Resolved::Member(m)) if self.g.view.member(m).kind == MemberKind::Case => Some(m),
            _ => ty
                .unwrap_optional()
                .decl()
                .and_then(|d| self.g.find_member(d, name.text())),
        }
    }

    // ---------------------------------------------------------------- if

    /// `true` if an `if` expression can be a conditional expression
    /// (`c ? a : b`): it has an `else`, no `let` conditions, and every branch
    /// is a single expression that isn't itself statement-like.
    fn if_is_simple(&self, node: &SyntaxNode) -> bool {
        if node
            .children()
            .any(|c| c.kind() == SyntaxKind::LetCondition)
        {
            return false;
        }
        let blocks: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::Block)
            .collect();
        let else_if = node
            .children()
            .skip_while(|c| c.kind() != SyntaxKind::Block)
            .find(|c| c.kind() == SyntaxKind::IfExpr);
        if blocks.len() == 1 && else_if.is_none() {
            return false;
        }
        let simple_block = |b: &SyntaxNode| {
            let stmts: Vec<ast::Stmt> = ast::Block::cast(b.clone())
                .map(|b| b.statements().collect())
                .unwrap_or_default();
            stmts.len() == 1 && matches!(&stmts[0], ast::Stmt::Expr(e) if !self.is_statementy(e))
        };
        blocks.iter().all(simple_block) && else_if.as_ref().is_none_or(|e| self.if_is_simple(e))
    }

    /// Lowers an `if` statement or expression whose value goes to `dest`,
    /// including `if let` bindings and `else if` chains.
    fn if_into(&mut self, node: &SyntaxNode, dest: &Dest) {
        let (test, binds) = self.conditions(node);
        let blocks: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::Block)
            .collect();
        let else_if = node
            .children()
            .skip_while(|c| c.kind() != SyntaxKind::Block)
            .find(|c| c.kind() == SyntaxKind::IfExpr);
        let (then, ()) = self.capture(|b| {
            b.push_scope();
            for s in binds {
                b.emit(s);
            }
            if let Some(t) = blocks.first() {
                b.value_block(t, dest);
            }
            b.pop_scope();
        });
        let otherwise = if let Some(ei) = else_if {
            let (s, ()) = self.capture(|b| b.if_into(&ei, dest));
            Some(s)
        } else if let Some(e) = blocks.get(1) {
            let (s, ()) = self.capture(|b| b.value_block(e, dest));
            Some(s)
        } else {
            None
        };
        self.emit(Stmt::If(test, then, otherwise));
    }

    /// Lowers the conditions of an `if`: the test, and the statements that
    /// bind its `let` conditions (emitted before the test, with the test
    /// reading them).
    fn conditions(&mut self, node: &SyntaxNode) -> (Expr, Vec<Stmt>) {
        let conds: Vec<SyntaxNode> = node
            .children()
            .take_while(|c| c.kind() != SyntaxKind::Block)
            .filter(|c| c.kind() == SyntaxKind::LetCondition || ast::Expr::can_cast(c.kind()))
            .collect();
        let mut test: Option<Expr> = None;
        for c in conds {
            if c.kind() == SyntaxKind::LetCondition {
                let (name, value) = let_condition(&c);
                let v = match (&value, &name) {
                    (Some(value), _) => self.expr(value),
                    (None, Some(n)) => self.shorthand_value(n),
                    (None, None) => continue,
                };
                let t = self.fresh("o");
                self.emit(Stmt::Let(t.clone(), None));
                let check = bin(
                    "!==",
                    Expr::Assign("=", Box::new(id(&t)), Box::new(v)),
                    Expr::Null,
                );
                test = Some(match test {
                    Some(a) => bin("&&", a, check),
                    None => check,
                });
                if let Some(n) = name {
                    // The binding reads the temporary, which the test set.
                    if let Some(s) = self.scopes.last_mut() {
                        s.insert(u32::from(n.text_range().start()), (n.text().to_owned(), t));
                    }
                }
            } else if let Some(e) = ast::Expr::cast(c) {
                let x = self.expr(&e);
                test = Some(match test {
                    Some(a) => bin("&&", a, x),
                    None => x,
                });
            }
        }
        (test.unwrap_or(Expr::Bool(true)), Vec::new())
    }

    /// Lowers `try expr else { fallback }` into `try { … } catch (e) { … }`:
    /// the handler rethrows panics, then evaluates the fallback into `dest`.
    fn try_else_into(&mut self, node: &SyntaxNode, dest: &Dest) {
        let inner = node.children().find_map(ast::Expr::cast);
        let fallback = node.children().find_map(ast::Block::cast);
        let err = self.catch_name();
        let (body, ()) = self.capture(|b| {
            if let Some(i) = &inner {
                b.expr_into(i, dest);
            }
        });
        let (handler, ()) = self.capture(|b| {
            b.emit(rethrow(&err));
            for s in b.live_check() {
                b.emit(s);
            }
            if let Some(f) = &fallback {
                b.value_block(f.syntax(), dest);
            }
        });
        self.end_catch();
        self.emit(Stmt::Try(body, err, handler));
    }

    // -------------------------------------------------------- assignment

    /// Lowers an assignment (`=` or a compound `+=`, `-=`, `*=`, `/=`, `%=`)
    /// to a local, a field path, a component member (a signal, binding or
    /// store write) or an element, copying shared collections and structs
    /// along the path as value semantics require.
    #[allow(clippy::too_many_lines)]
    fn assign_stmt(&mut self, node: &SyntaxNode) {
        let mut exprs = node.children().filter_map(ast::Expr::cast);
        let (Some(place), Some(value)) = (exprs.next(), exprs.next()) else {
            return;
        };
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
            })
            .map(|t| t.text().to_owned())
            .unwrap_or_else(|| "=".to_owned());
        self.kill_aliases(value.syntax());
        let pty = self.ty(place.syntax());
        let rhs = if self.is_statementy(&value) {
            let t = self.fresh("v");
            self.emit(Stmt::Let(t.clone(), None));
            self.expr_into(&value, &Dest::Assign(t.clone()));
            id(t)
        } else {
            let v = self.expr(&value);
            let vt = self.ty(value.syntax());
            self.coerce(v, &vt, &pty)
        };
        let new_value = if op == "=" {
            rhs
        } else {
            let bop = &op[..op.len() - 1];
            let current = self.expr(&place);
            let vt = self.ty(value.syntax());
            self.arith(bop, current, rhs, &pty, &vt, &pty)
        };
        // A fresh value assigned to a collection var makes it owned again.
        if let ast::Expr::NameExpr(n) = &place {
            if let Some(Resolution::Local { name, definition }) =
                self.resolution(n.syntax().text_range())
            {
                let js = self.local_name(&name, definition.range);
                if self.collection_vars.contains(&js) {
                    if self.is_fresh(&value) && op == "=" {
                        self.owned.insert(js.clone());
                    } else {
                        self.owned.remove(&js);
                    }
                }
            }
        }
        self.store(&place, new_value);
    }

    /// Stores `value` into `place`, rebuilding struct levels on the way
    /// (path copy, codegen.md §6.3).
    pub(crate) fn store(&mut self, place: &ast::Expr, value: Expr) {
        match place {
            ast::Expr::ParenExpr(p) => {
                if let Some(i) = p.syntax().children().find_map(ast::Expr::cast) {
                    self.store(&i, value);
                }
            }
            ast::Expr::NameExpr(n) => {
                let Some(tok) = n.name() else { return };
                if tok.kind() == SyntaxKind::SelfKw {
                    return;
                }
                match self.resolution(tok.text_range()) {
                    Some(
                        Resolution::Local { name, definition }
                        | Resolution::Parameter { name, definition },
                    ) => {
                        let js = self.local_name(&name, definition.range);
                        self.emit(Stmt::Expr(js::assign(id(js), value)));
                    }
                    Some(r) => {
                        if let Some(Resolved::Member(m)) = self.g.view.resolve(&r) {
                            self.store_member(m, None, value);
                        }
                    }
                    None => {}
                }
            }
            ast::Expr::MemberExpr(m) => {
                let Some(recv) = m.receiver() else { return };
                let Some(name) = m.name() else { return };
                let rty = self.ty(recv.syntax());
                let resolved = self.resolved(name.text_range());
                if let Some(Resolved::Member(mid)) = resolved {
                    if self.is_component_member(mid)
                        && matches!(recv, ast::Expr::NameExpr(ref n) if n.name().is_some_and(|t| t.kind() == SyntaxKind::SelfKw))
                    {
                        self.store_member(mid, None, value);
                        return;
                    }
                }
                let field = name.text().to_owned();
                if self.is_form_value(&recv, &rty) {
                    let r = self.expr(&recv);
                    self.emit(Stmt::Expr(call(
                        member(call(member(r, "field"), vec![js::str(field)]), "set"),
                        vec![value],
                    )));
                    return;
                }
                if let Some((st, path)) = self.store_path(place) {
                    if !self.ssr {
                        self.emit(Stmt::Expr(call(
                            member(id(st), "set"),
                            vec![Expr::Array(path.into_iter().map(js::str).collect()), value],
                        )));
                        return;
                    }
                }
                if self.self_mode == SelfMode::StructInit && is_self(&recv) {
                    self.emit(Stmt::Expr(js::assign(id(mangle(&field)), value)));
                    return;
                }
                if self.g.is_value_type(rty.unwrap_optional()) {
                    // Rebuild the struct and store it into the receiver.
                    let current = self.expr(&recv);
                    let rebuilt = Expr::Object(
                        vec![Prop::Spread(current), Prop::KeyValue(field, value)],
                        false,
                    );
                    self.store(&recv, rebuilt);
                } else {
                    // A class or service: assign in place.
                    if let Some(Resolved::Member(mid)) = resolved {
                        if self.store_reactive(mid, &recv, value.clone()) {
                            return;
                        }
                    }
                    let r = self.expr(&recv);
                    self.emit(Stmt::Expr(js::assign(member(r, field), value)));
                }
            }
            ast::Expr::IndexExpr(i) => {
                let mut exprs = i.syntax().children().filter_map(ast::Expr::cast);
                let (Some(base), Some(index)) = (exprs.next(), exprs.next()) else {
                    return;
                };
                let bty = self.ty(base.syntax());
                let b = self.expr(&base);
                let ix = self.expr(&index);
                if self.g.is(&bty, self.g.b.map) {
                    let copy = rt("copy", vec![b]);
                    let updated = if matches!(value, Expr::Null) {
                        rt("mapRemoved", vec![copy, ix])
                    } else {
                        rt("mapSet", vec![copy, ix, value])
                    };
                    self.store_collection(&base, updated);
                } else {
                    let updated = rt("setAt", vec![rt("copy", vec![b]), ix, value]);
                    self.store_collection(&base, updated);
                }
            }
            _ => {}
        }
    }

    /// Stores a collection value produced from a copy of `place`'s value.
    fn store_collection(&mut self, place: &ast::Expr, value: Expr) {
        self.store(place, value);
    }

    /// `true` if `m` is a member of the component being lowered, read through
    /// [`Body::access`].
    fn is_component_member(&self, m: MemberId) -> bool {
        self.access.contains_key(&m)
    }

    /// Stores into a member reached without an explicit receiver.
    fn store_member(&mut self, m: MemberId, _recv: Option<Expr>, value: Expr) {
        if let Some(acc) = self.access.get(&m).cloned() {
            let s = match acc {
                MemberAccess::Signal(n) | MemberAccess::Binding(n) => {
                    let target = if matches!(self.access.get(&m), Some(MemberAccess::Binding(_))) {
                        member(member(id("props"), n), "set")
                    } else {
                        member(id(n), "set")
                    };
                    if self.ssr {
                        return;
                    }
                    Stmt::Expr(call(target, vec![value]))
                }
                MemberAccess::Store(n) => {
                    if self.ssr {
                        return;
                    }
                    Stmt::Expr(call(
                        member(id(n), "set"),
                        vec![Expr::Array(Vec::new()), value],
                    ))
                }
                MemberAccess::Local(n) => Stmt::Expr(js::assign(id(n), value)),
                MemberAccess::Raw(e) => Stmt::Expr(js::assign(e, value)),
                MemberAccess::Prop(_) | MemberAccess::Computed(_) => return,
            };
            self.emit(s);
            return;
        }
        let info = self.g.view.member(m);
        if info.is_static {
            let target = self.static_ref(m);
            self.emit(Stmt::Expr(js::assign(target, value)));
            return;
        }
        match self.self_mode {
            SelfMode::StructInit => {
                self.emit(Stmt::Expr(js::assign(id(mangle(&info.name)), value)));
            }
            SelfMode::This | SelfMode::Component => {
                if self.store_reactive_this(m, value.clone()) {
                    return;
                }
                self.emit(Stmt::Expr(js::assign(
                    member(Expr::This, info.name.clone()),
                    value,
                )));
            }
            _ => {
                self.emit(Stmt::Expr(js::assign(
                    member(id("self"), info.name.clone()),
                    value,
                )));
            }
        }
    }

    /// Writes a `@State` field of a service through `this`.
    fn store_reactive_this(&mut self, m: MemberId, value: Expr) -> bool {
        let info = self.g.view.member(m);
        if info.has_annotation("State")
            && self.is_service_like(info.owner)
            && self.target == Target::Client
        {
            self.emit(Stmt::Expr(call(
                member(member(Expr::This, info.name.clone()), "set"),
                vec![value],
            )));
            return true;
        }
        false
    }

    /// Writes a `@State` field of a service through a receiver.
    fn store_reactive(&mut self, m: MemberId, recv: &ast::Expr, value: Expr) -> bool {
        let info = self.g.view.member(m);
        if info.has_annotation("State")
            && self.is_service_like(info.owner)
            && self.target == Target::Client
        {
            let r = self.expr(recv);
            self.emit(Stmt::Expr(call(
                member(member(r, info.name.clone()), "set"),
                vec![value],
            )));
            return true;
        }
        false
    }

    /// `true` for `k.validation.Form<T>`.
    pub(crate) fn is_form(&self, t: &Type) -> bool {
        t.decl()
            .is_some_and(|d| self.g.view.qualified_name(d) == "k.validation.Form")
    }

    /// `true` if `recv` is a form: of type `Form<T>`, or a `@Form var` of
    /// the component (whose recorded type is the struct `T`, but whose
    /// value is the form; `credentials.email` is the field's text).
    fn is_form_value(&self, recv: &ast::Expr, rty: &Type) -> bool {
        if self.is_form(rty.unwrap_optional()) {
            return true;
        }
        let ast::Expr::NameExpr(n) = recv else {
            return false;
        };
        let Some(tok) = n.name() else { return false };
        matches!(self.resolved(tok.text_range()), Some(Resolved::Member(m))
            if self.g.view.member(m).has_annotation("Form"))
    }

    /// `true` if `d` is a `service`, whose members are reached on the
    /// instance from the container.
    pub(crate) fn is_service_like(&self, d: DeclId) -> bool {
        matches!(
            self.g.view.decl(d).named_kind(),
            Some(DeclarationKind::Service)
        )
    }

    /// The fields of a struct initializer, as `{ a, b }`.
    pub(crate) fn init_object(&self) -> Expr {
        Expr::Object(
            self.init_fields
                .iter()
                .map(|(k, v)| Prop::KeyValue(k.clone(), id(v)))
                .collect(),
            false,
        )
    }

    /// Prepares a struct initializer: one local per stored field.
    pub(crate) fn begin_struct_init(&mut self, decl: DeclId) {
        self.self_mode = SelfMode::StructInit;
        self.owner = Some(decl);
        for f in self.g.stored_fields(decl) {
            let name = self.g.view.member(f).name.clone();
            let js = mangle(&name);
            self.names_in_use.last_mut().map(|s| s.insert(js.clone()));
            self.init_fields.push((name, js));
        }
    }

    // ----------------------------------------------------------- coercion

    /// Converts a value of type `from` to `to` where the representation
    /// changes: a struct or enum value becomes an interface existential.
    pub(crate) fn coerce(&mut self, e: Expr, from: &Type, to: &Type) -> Expr {
        let target = to.unwrap_optional();
        let source = from.unwrap_optional();
        if self.g.is(target, self.g.b.error)
            && !self.g.is(source, self.g.b.error)
            && (source.decl().is_some() || matches!(source, Type::Param(_)))
        {
            // A concrete error passed on as an `Error` keeps its text.
            if matches!(from, Type::Optional(_)) {
                let v = self.boxed_error(id("v"), source);
                return rt("mapOpt", vec![e, js::arrow(vec![Param::new("v")], v)]);
            }
            return self.boxed_error(e, source);
        }
        if target.decl().is_some()
            && target.decl() == self.g.b.codable
            && source.decl() != self.g.b.codable
            && !matches!(source, Type::Error)
        {
            // A `Codable` existential (`HttpBody.json(event)`) carries the
            // value with its codec, so it can be encoded later.
            let c = crate::codec::codec_expr(self, source);
            let boxed = |v: Expr| {
                Expr::Object(
                    vec![
                        Prop::KeyValue("codec".into(), c.clone()),
                        Prop::KeyValue("value".into(), v),
                    ],
                    false,
                )
            };
            if matches!(from, Type::Optional(_)) {
                return rt(
                    "mapOpt",
                    vec![e, js::arrow(vec![Param::new("v")], boxed(id("v")))],
                );
            }
            return boxed(e);
        }
        if self.g.is_interface(target)
            && self.g.is_value_type(source)
            && !self.g.is_interface(source)
        {
            // Stdlib interfaces (`Error`, `Codable`) have no adapter classes
            // (`decl::adapters`): their values stay as they are.
            if let (Some(s), Some(i)) = (
                source.decl(),
                target.decl().filter(|&i| !self.g.view.decl(i).stdlib),
            ) {
                let adapter = format!("{}$as${}", self.g.name(s), self.g.name(i));
                let r = self.use_decl(s, adapter);
                if matches!(from, Type::Optional(_)) {
                    return rt(
                        "mapOpt",
                        vec![
                            e,
                            js::arrow(vec![Param::new("v")], Expr::New(Box::new(r), vec![id("v")])),
                        ],
                    );
                }
                return Expr::New(Box::new(r), vec![e]);
            }
        }
        // A list's elements, also of an optional list (`List<Card>` as a
        // `List<Priced>?`, sema's covariance rules in sema.md §9.6).
        let optional = matches!(from, Type::Optional(_));
        if let (Type::Named(fd, fa), Type::Named(td, ta)) = (source, target) {
            if fd == td && self.g.is_list(source) {
                if let (Some(fe), Some(te)) = (fa.first(), ta.first()) {
                    if self.g.is_interface(te.unwrap_optional())
                        && self.g.is_value_type(fe.unwrap_optional())
                    {
                        let conv = self.coerce(id("v"), fe, te);
                        let map = |list: Expr| {
                            call(
                                member(list, "map"),
                                vec![js::arrow(vec![Param::new("v")], conv.clone())],
                            )
                        };
                        if optional {
                            return rt(
                                "mapOpt",
                                vec![e, js::arrow(vec![Param::new("l")], map(id("l")))],
                            );
                        }
                        return map(e);
                    }
                }
            }
        }
        e
    }

    // -------------------------------------------------------- expressions

    /// Lowers an expression.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn expr(&mut self, e: &ast::Expr) -> Expr {
        if let Some(x) = self.overrides.get(&e.syntax().text_range()) {
            return x.clone();
        }
        match e {
            ast::Expr::Literal(l) => {
                let t = self.ty(l.syntax());
                match l.token() {
                    Some(tok) => self.literal_token(&tok, &t),
                    None => Expr::Null,
                }
            }
            ast::Expr::StringLit(s) => self.string(s),
            ast::Expr::NameExpr(n) => self.name_expr(n),
            ast::Expr::ImplicitMemberExpr(m) => self.implicit_member(m.syntax(), None),
            ast::Expr::ParenExpr(p) => match p.syntax().children().find_map(ast::Expr::cast) {
                Some(i) => {
                    if self.is_statementy(&i) {
                        let t = self.fresh("v");
                        self.emit(Stmt::Let(t.clone(), None));
                        self.expr_into(&i, &Dest::Assign(t.clone()));
                        id(t)
                    } else {
                        self.expr(&i)
                    }
                }
                None => Expr::Null,
            },
            ast::Expr::TupleExpr(t) => {
                let items: Vec<Expr> = t
                    .syntax()
                    .children()
                    .filter_map(ast::Expr::cast)
                    .map(|e| self.expr(&e))
                    .collect();
                Expr::Array(items)
            }
            ast::Expr::ListExpr(l) => self.list_literal(l.syntax()),
            ast::Expr::MapExpr(m) => self.map_literal(m.syntax()),
            ast::Expr::LambdaExpr(l) => self.lambda(l, None),
            ast::Expr::IfExpr(i) => {
                if self.if_is_simple(i.syntax()) {
                    self.simple_if(i.syntax())
                } else {
                    let t = self.fresh("v");
                    self.emit(Stmt::Let(t.clone(), None));
                    self.if_into(i.syntax(), &Dest::Assign(t.clone()));
                    id(t)
                }
            }
            ast::Expr::MatchExpr(m) => {
                let t = self.fresh("v");
                self.emit(Stmt::Let(t.clone(), None));
                self.match_into(m.syntax(), &Dest::Assign(t.clone()));
                id(t)
            }
            ast::Expr::TryExpr(t) => {
                if has_token(t.syntax(), SyntaxKind::ElseKw) {
                    let v = self.fresh("v");
                    self.emit(Stmt::Let(v.clone(), None));
                    self.try_else_into(t.syntax(), &Dest::Assign(v.clone()));
                    id(v)
                } else {
                    match t.syntax().children().find_map(ast::Expr::cast) {
                        Some(i) => {
                            let thrown =
                                self.throws_of_call(&crate::db::strip_parens(&strip_await(&i)));
                            let tag = self.error_tag(thrown.as_ref());
                            self.tagged(tag, |b| b.expr(&i))
                        }
                        None => Expr::Null,
                    }
                }
            }
            ast::Expr::AwaitExpr(a) => {
                let inner = a.syntax().children().find_map(ast::Expr::cast);
                match inner {
                    Some(i) => {
                        // A repository query or write runs as SQL.
                        if let Some(q) = crate::db::awaited(self, &i) {
                            return self.awaited(q);
                        }
                        // `await` covers the whole expression to its right
                        // (grammar.md §"try and await"): in a compound one,
                        // each call is awaited where it is evaluated, so
                        // `await next() == 2` compares the value.
                        if covers_compound(&i) {
                            self.await_calls_in(&i);
                            return self.expr(&i);
                        }
                        let x = self.expr(&i);
                        self.awaited(x)
                    }
                    None => Expr::Null,
                }
            }
            ast::Expr::PrefixExpr(p) => {
                let op = p.op().map(|t| t.text().to_owned()).unwrap_or_default();
                let Some(operand) = p.operand() else {
                    return Expr::Null;
                };
                let x = self.expr(&operand);
                match op.as_str() {
                    "not" | "!" => un("!", x),
                    "-" => {
                        let t = self.ty(operand.syntax());
                        if let Expr::Num(n) = x {
                            return Expr::Num(-n);
                        }
                        if self.g.is(&t, self.g.b.money) || self.g.is(&t, self.g.b.decimal) {
                            return call(member(x, "negate"), vec![]);
                        }
                        if self.g.is_css_text(&t) {
                            if let Expr::Str(text) = &x {
                                return js::str(match text.strip_prefix('-') {
                                    Some(rest) => rest.to_owned(),
                                    None => format!("-{text}"),
                                });
                            }
                            return rt("calc", vec![bin("+", js::str("-1 * "), x)]);
                        }
                        un("-", x)
                    }
                    _ => x,
                }
            }
            ast::Expr::BinaryExpr(b) => self.binary(b),
            ast::Expr::CallExpr(c) => self.chain(c.syntax(), |b| b.call_expr(c)),
            ast::Expr::MemberExpr(m) => self.chain(m.syntax(), |b| b.member_expr(m)),
            ast::Expr::IndexExpr(i) => self.chain(i.syntax(), |b| b.index_expr(i)),
            ast::Expr::BindExpr(b) => match b.syntax().children().find_map(ast::Expr::cast) {
                Some(inner) => self.binding_pair(&inner),
                None => Expr::Null,
            },
            ast::Expr::WhenExpr(w) => {
                let mut exprs = w.syntax().children().filter_map(ast::Expr::cast);
                let v = exprs.next().map_or(Expr::Null, |e| self.expr(&e));
                let c = exprs.next().map_or(Expr::Bool(true), |e| self.expr(&e));
                Expr::Cond(Box::new(c), Box::new(v), Box::new(Expr::Bool(false)))
            }
        }
    }

    /// Lowers `base[index]`.
    fn index_expr(&mut self, i: &ast::IndexExpr) -> Expr {
        let mut exprs = i.syntax().children().filter_map(ast::Expr::cast);
        let (Some(base), Some(index)) = (exprs.next(), exprs.next()) else {
            return Expr::Null;
        };
        let bty = self.ty(base.syntax());
        let b = self.expr(&base);
        let ix = self.expr(&index);
        if bty.decl().is_some() && bty.decl() == self.g.view.lookup("k.http.HttpHeaders") {
            // `headers[name]`: the values joined, or `nil`.
            crate::native::http_native(self, "HttpHeaders", "get", vec![b, ix])
        } else if self.g.is(&bty, self.g.b.map) {
            let nested = matches!(bty.args().get(1), Some(Type::Optional(_)));
            rt(if nested { "mapGetNested" } else { "mapGet" }, vec![b, ix])
        } else {
            rt("at", vec![b, ix])
        }
    }

    /// Lowers one link of a member, call or index chain with `lower`; the
    /// outermost link wraps the chain in the nil tests its `?.` links
    /// raised, so a nil skips everything after it (`a?.b.c()` is
    /// `($o1 = a) == null ? null : c($o1.b)`, codegen.md §5).
    fn chain(&mut self, node: &SyntaxNode, lower: impl FnOnce(&mut Self) -> Expr) -> Expr {
        let mut e = lower(self);
        if chain_parent(node).is_some() {
            return e;
        }
        let range = node.text_range();
        if self.opt_undefined.remove(&range) {
            e = bin("??", e, Expr::Null);
        }
        let mut tests = Vec::new();
        self.opt_guards.retain(|(top, test)| {
            if *top == range {
                tests.push(test.clone());
                false
            } else {
                true
            }
        });
        match tests.into_iter().reduce(|a, b| bin("||", a, b)) {
            Some(test) => Expr::Cond(Box::new(test), Box::new(Expr::Null), Box::new(e)),
            None => e,
        }
    }

    /// The `?.` link at `node` with receiver `r`: `r` goes into a temporary
    /// `t`, assigned inside the nil test so it is evaluated in place, and the
    /// test is raised to the chain's outermost link. Returns `t`, the
    /// non-nil receiver for the rest of the link.
    fn opt_link(&mut self, node: &SyntaxNode, r: Expr) -> Expr {
        let t = self.fresh("o");
        self.emit(Stmt::Let(t.clone(), None));
        let test = bin(
            "==",
            Expr::Assign("=", Box::new(id(&t)), Box::new(r)),
            Expr::Null,
        );
        self.opt_guards.push((chain_top(node).text_range(), test));
        id(t)
    }

    /// `true` if `m` reads a plain project field, which is a JavaScript
    /// property read (`r.name`).
    fn plain_field(&self, m: &ast::MemberExpr) -> bool {
        let Some(name) = m.name() else {
            return false;
        };
        match self.resolved(name.text_range()) {
            Some(Resolved::Member(mid)) => {
                let info = self.g.view.member(mid);
                info.kind == MemberKind::Field
                    && !self.g.view.decl(info.owner).stdlib
                    && !(info.has_annotation("State") && self.target == Target::Client)
            }
            _ => false,
        }
    }

    /// `true` if every link of the chain after `node` is a plain field read,
    /// so JavaScript's own `?.` at `node` short-circuits all of them.
    fn plain_after(&self, node: &SyntaxNode) -> bool {
        let mut cur = node.clone();
        while let Some(p) = chain_parent(&cur) {
            match ast::MemberExpr::cast(p.clone()) {
                Some(m) if self.plain_field(&m) => cur = p,
                _ => return false,
            }
        }
        true
    }

    /// Lowers an `if` that [`Body::if_is_simple`] accepted to `c ? a : b`; if a
    /// branch turns out to need statements, the `if` is lowered as a
    /// statement into a temporary instead.
    fn simple_if(&mut self, node: &SyntaxNode) -> Expr {
        let blocks: Vec<SyntaxNode> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::Block)
            .collect();
        let else_if = node
            .children()
            .skip_while(|c| c.kind() != SyntaxKind::Block)
            .find(|c| c.kind() == SyntaxKind::IfExpr);
        // A branch whose value needs statements first (a `@State` list's
        // copy-on-write in `items.remove(at: 0)`) can't be one arm of a
        // conditional expression: its statements would run whichever arm
        // is taken. Such an `if` is lowered as a statement into a
        // temporary instead.
        let (pre_then, then) =
            self.capture(|b| blocks.first().map_or(Expr::Null, |t| b.block_value(t)));
        let (pre_else, otherwise) = self.capture(|b| {
            if let Some(ei) = &else_if {
                b.simple_if(ei)
            } else {
                blocks.get(1).map_or(Expr::Null, |e| b.block_value(e))
            }
        });
        if !pre_then.is_empty() || !pre_else.is_empty() {
            let t = self.fresh("v");
            self.emit(Stmt::Let(t.clone(), None));
            self.if_into(node, &Dest::Assign(t.clone()));
            return id(t);
        }
        let cond = node
            .children()
            .find_map(ast::Expr::cast)
            .map_or(Expr::Bool(true), |c| self.expr(&c));
        Expr::Cond(Box::new(cond), Box::new(then), Box::new(otherwise))
    }

    /// The value of a block whose single statement is an expression; `null`
    /// otherwise.
    fn block_value(&mut self, b: &SyntaxNode) -> Expr {
        let e = ast::Block::cast(b.clone()).and_then(|b| b.statements().next());
        match e {
            Some(ast::Stmt::Expr(e)) => {
                let x = self.expr(&e);
                match self.branch_type(b) {
                    Some(to) => {
                        let from = self.ty(e.syntax());
                        self.coerce(x, &from, &to)
                    }
                    None => x,
                }
            }
            _ => Expr::Null,
        }
    }

    /// A binding pair `{ get, set }` over a place (`bind x`).
    pub(crate) fn binding_pair(&mut self, place: &ast::Expr) -> Expr {
        // Forward an existing binding unchanged.
        if let ast::Expr::NameExpr(n) = place {
            if let Some(tok) = n.name() {
                if let Some(Resolved::Member(m)) = self.resolved(tok.text_range()) {
                    if let Some(MemberAccess::Binding(name)) = self.access.get(&m) {
                        return member(id("props"), name.clone());
                    }
                }
            }
        }
        if let Some(store) = self.store_path(place) {
            let (s, path) = store;
            return call(
                member(id(s), "binding"),
                vec![Expr::Array(path.into_iter().map(js::str).collect())],
            );
        }
        let (stmts, getter) = self.capture(|b| b.expr(place));
        let (set_stmts, ()) = self.capture(|b| b.store(place, id("v")));
        let get = if stmts.is_empty() {
            js::arrow(vec![], getter)
        } else {
            let mut s = stmts;
            s.push(Stmt::Return(Some(getter)));
            js::arrow_block(vec![], s, false)
        };
        let set = match set_stmts.as_slice() {
            [Stmt::Expr(e)] => js::arrow(vec![Param::new("v")], e.clone()),
            _ => js::arrow_block(vec![Param::new("v")], set_stmts, false),
        };
        Expr::Object(
            vec![
                Prop::KeyValue("get".into(), get),
                Prop::KeyValue("set".into(), set),
            ],
            false,
        )
    }

    /// For a place rooted in a `@State` struct backed by a path store: the
    /// store's name and the path below it.
    pub(crate) fn store_path(&self, place: &ast::Expr) -> Option<(String, Vec<String>)> {
        let mut path = Vec::new();
        let mut cur = place.clone();
        loop {
            match &cur {
                ast::Expr::MemberExpr(m) => {
                    path.push(m.name()?.text().to_owned());
                    cur = m.receiver()?;
                }
                ast::Expr::NameExpr(n) => {
                    let tok = n.name()?;
                    let Some(Resolved::Member(mid)) = self.resolved(tok.text_range()) else {
                        return None;
                    };
                    let Some(MemberAccess::Store(s)) = self.access.get(&mid) else {
                        return None;
                    };
                    path.reverse();
                    return Some((s.clone(), path));
                }
                _ => return None,
            }
        }
    }

    /// Lowers a literal token of type `ty`: booleans, `nil`, numbers (with
    /// `_` separators removed), and unit literals such as `10ms` or `90deg`,
    /// which fold to the base unit.
    fn literal_token(&mut self, tok: &SyntaxToken, ty: &Type) -> Expr {
        let text = tok.text();
        match tok.kind() {
            SyntaxKind::TrueKw => Expr::Bool(true),
            SyntaxKind::FalseKw => Expr::Bool(false),
            SyntaxKind::NilKw => Expr::Null,
            SyntaxKind::IntLit => {
                let clean: String = text.chars().filter(|c| *c != '_').collect();
                let v = if let Some(h) = clean
                    .strip_prefix("0x")
                    .or_else(|| clean.strip_prefix("0X"))
                {
                    i64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(0.0)
                } else if let Some(b) = clean
                    .strip_prefix("0b")
                    .or_else(|| clean.strip_prefix("0B"))
                {
                    i64::from_str_radix(b, 2).map(|v| v as f64).unwrap_or(0.0)
                } else if let Some(o) = clean.strip_prefix("0o") {
                    i64::from_str_radix(o, 8).map(|v| v as f64).unwrap_or(0.0)
                } else {
                    clean.parse::<f64>().unwrap_or(0.0)
                };
                if self.g.is(ty, self.g.b.decimal) {
                    return rt("decimal", vec![js::str(clean)]);
                }
                Expr::Num(v)
            }
            SyntaxKind::FloatLit => {
                let clean: String = text.chars().filter(|c| *c != '_').collect();
                if self.g.is(ty, self.g.b.decimal) {
                    return rt("decimal", vec![js::str(clean)]);
                }
                Expr::Num(clean.parse::<f64>().unwrap_or(0.0))
            }
            SyntaxKind::StringText => js::str(text),
            _ => self.unit_literal(text, ty),
        }
    }

    /// A number with a unit: `16px`, `200ms`, `50%`, `90deg`, `#3B82F6`.
    fn unit_literal(&self, text: &str, ty: &Type) -> Expr {
        let clean: String = text.chars().filter(|c| *c != '_').collect();
        let split = clean
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .unwrap_or(clean.len());
        let (num, unit) = clean.split_at(split);
        let n: f64 = num.parse().unwrap_or(0.0);
        if self.g.is(ty, self.g.b.duration) {
            let ms = match unit {
                "ms" => n,
                "s" => n * 1000.0,
                "m" | "min" => n * 60_000.0,
                "h" => n * 3_600_000.0,
                "d" => n * 86_400_000.0,
                _ => n,
            };
            return Expr::Num(ms);
        }
        if self.g.is(ty, self.g.b.percent) || self.g.is(ty, self.g.b.angle) {
            let v = match unit {
                "rad" => n * 180.0 / std::f64::consts::PI,
                "turn" => n * 360.0,
                _ => n,
            };
            return Expr::Num(v);
        }
        js::str(clean)
    }

    /// Lowers a string literal: escapes decoded at build time, and each
    /// interpolation turned into a template part through
    /// [`Body::text_of`].
    fn string(&mut self, s: &ast::StringLit) -> Expr {
        if let Some(v) = s.plain_value() {
            return js::str(v);
        }
        let mut parts: Vec<TemplatePart> = Vec::new();
        let mut text = String::new();
        for el in s.syntax().children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SyntaxKind::StringText => text.push_str(t.text()),
                    SyntaxKind::StringEscape => k_syntax::dump::decode_escape(t.text(), &mut text),
                    _ => {}
                },
                rowan::NodeOrToken::Node(n) => {
                    if n.kind() == SyntaxKind::Interpolation {
                        if let Some(e) = n.children().find_map(ast::Expr::cast) {
                            if !text.is_empty() {
                                parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                            }
                            let t = self.ty(e.syntax());
                            let x = self.expr(&e);
                            let f = self.text_of(x, &t);
                            parts.push(TemplatePart::Expr(f));
                        }
                    }
                }
            }
        }
        if !text.is_empty() {
            parts.push(TemplatePart::Text(text));
        }
        Expr::Template(parts)
    }

    /// The interpolation text of a value of type `t` (codegen.md §6.4).
    pub(crate) fn text_of(&mut self, x: Expr, t: &Type) -> Expr {
        if let Type::Optional(inner) = t {
            if self.g.is_string(inner) {
                return bin("??", x, js::str("nil"));
            }
            let f = self.text_fn(inner);
            return rt("optText", vec![x, f]);
        }
        if self.g.is_string(t) || self.g.is_css_text(t) {
            return x;
        }
        if self.g.is_int(t) || self.g.is_bool(t) {
            return call(id("String"), vec![x]);
        }
        if self.g.is_double(t) || self.g.is(t, self.g.b.angle) {
            return rt("formatDouble", vec![x]);
        }
        if self.g.is(t, self.g.b.percent) {
            return Expr::Template(vec![TemplatePart::Expr(x), TemplatePart::Text("%".into())]);
        }
        if self.g.is(t, self.g.b.error) {
            return member(x, "text");
        }
        let f = self.text_fn(t);
        match f {
            Expr::Ident(n) if n == "String" => call(id("String"), vec![x]),
            f => call(f, vec![x]),
        }
    }

    /// The CSS value of `x`, of type `t`, for a reactive style value
    /// (06-styles.md, "Reactive values"): an `Angle` (degrees) gets `deg`
    /// and a `Duration` (milliseconds) `ms`; lengths and colors are CSS
    /// text already, and anything else is its interpolation text.
    pub(crate) fn css_value_of(&mut self, x: Expr, t: &Type) -> Expr {
        let unit = if self.g.is(t, self.g.b.angle) {
            "deg"
        } else if self.g.is(t, self.g.b.duration) {
            "ms"
        } else {
            return self.text_of(x, t);
        };
        Expr::Template(vec![TemplatePart::Expr(x), TemplatePart::Text(unit.into())])
    }

    /// A function giving the interpolation text of a `t`.
    pub(crate) fn text_fn(&mut self, t: &Type) -> Expr {
        if self.g.is_string(t) || self.g.is_css_text(t) {
            return member(id("$k"), "textOf");
        }
        if self.g.is_int(t) || self.g.is_bool(t) {
            return id("String");
        }
        if self.g.is_double(t) {
            return member(id("$k"), "formatDouble");
        }
        if let Type::Optional(inner) = t {
            let f = self.text_fn(inner);
            return js::arrow(vec![Param::new("v")], rt("optText", vec![id("v"), f]));
        }
        if self.g.is(t, self.g.b.error) {
            return js::arrow(vec![Param::new("v")], member(id("v"), "text"));
        }
        if let Some(d) = t.decl() {
            if Some(d) == self.g.view.lookup("k.core.Page") {
                return self.use_decl(d, "Page$href".into());
            }
            let info = self.g.view.decl(d);
            if !info.stdlib
                && matches!(
                    info.named_kind(),
                    Some(DeclarationKind::Enum | DeclarationKind::Struct)
                )
            {
                let n = format!("{}$$text", self.g.name(d));
                return self.use_decl(d, n);
            }
            if info.stdlib && info.named_kind() == Some(DeclarationKind::Enum) {
                let n = format!("{}$$text", self.g.name(d));
                return self.use_decl(d, n);
            }
            if self.g.is(t, self.g.b.secret) {
                return js::arrow(vec![], js::str("Secret(•••)"));
            }
            if self.g.is_list(t) {
                let inner = t.args().first().cloned().unwrap_or(Type::Error);
                let f = self.text_fn(&inner);
                return js::arrow(vec![Param::new("v")], rt("listText", vec![id("v"), f]));
            }
        }
        member(id("$k"), "textOf")
    }

    /// Lowers a list literal to an array, each element coerced to the element
    /// type; a literal of `Set` type becomes a `Set` or `$k.HashSet`.
    fn list_literal(&mut self, node: &SyntaxNode) -> Expr {
        let t = self.ty(node);
        let elem = t.args().first().cloned().unwrap_or(Type::Error);
        let items: Vec<Expr> = node
            .children()
            .filter_map(ast::Expr::cast)
            .map(|e| {
                let x = self.expr(&e);
                let from = self.ty(e.syntax());
                self.coerce(x, &from, &elem)
            })
            .collect();
        if self.g.is(&t, self.g.b.set) {
            let k = t.args().first().cloned().unwrap_or(Type::Error);
            return self.new_set(&k, Expr::Array(items));
        }
        Expr::Array(items)
    }

    /// `new Set(items)` or `new $k.HashSet(items)` for the key type.
    pub(crate) fn new_set(&mut self, k: &Type, items: Expr) -> Expr {
        if self.g.is_primitive_key(k) {
            Expr::New(Box::new(id("Set")), vec![items])
        } else {
            Expr::New(Box::new(member(id("$k"), "HashSet")), vec![items])
        }
    }

    /// `new Map(entries)` or `new $k.HashMap(entries)` for the key type.
    pub(crate) fn new_map(&mut self, k: &Type, entries: Vec<Expr>) -> Expr {
        let args = if entries.is_empty() {
            Vec::new()
        } else {
            vec![Expr::Array(entries)]
        };
        if self.g.is_primitive_key(k) {
            Expr::New(Box::new(id("Map")), args)
        } else {
            Expr::New(Box::new(member(id("$k"), "HashMap")), args)
        }
    }

    /// Lowers a map literal to `new Map([[k, v], …])` or `$k.HashMap` (by key
    /// type), keys and values coerced to the map's types.
    fn map_literal(&mut self, node: &SyntaxNode) -> Expr {
        let t = self.ty(node);
        let k = t.args().first().cloned().unwrap_or(Type::Error);
        let v = t.args().get(1).cloned().unwrap_or(Type::Error);
        let mut entries = Vec::new();
        for entry in node.children().filter(|c| c.kind() == SyntaxKind::MapEntry) {
            let mut es = entry.children().filter_map(ast::Expr::cast);
            if let (Some(a), Some(b)) = (es.next(), es.next()) {
                let ka = self.expr(&a);
                let vb = self.expr(&b);
                let vt = self.ty(b.syntax());
                let vb = self.coerce(vb, &vt, &v);
                entries.push(Expr::Array(vec![ka, vb]));
            }
        }
        self.new_map(&k, entries)
    }

    /// Lowers a lambda that is called without a cancellation context (an
    /// event handler): never `async` unless it awaits, and without `$c`.
    pub(crate) fn sync_lambda(&mut self, l: &ast::LambdaExpr) -> Expr {
        self.force_sync = true;
        let e = self.lambda(l, None);
        self.force_sync = false;
        e
    }

    /// Lowers a lambda that is called with its K arguments alone (a prop
    /// or event handler of a component, `form.handle(_:)`'s action). An
    /// async one is bound to its owner, so its work is owned by it
    /// (03-reactivity.md, "Async work is owned"): `$k.own(L, ctx)`, where
    /// `ctx` is the run that creates it (`$c`) or, outside one, the
    /// component's lifetime `$life`. Anywhere but a component's client
    /// renderer, and for a synchronous lambda, it is the plain lambda.
    pub(crate) fn owned_lambda(&mut self, l: &ast::LambdaExpr) -> Expr {
        let is_async = matches!(self.ty(l.syntax()), Type::Func(f) if f.is_async);
        let saved = std::mem::take(&mut self.own_args);
        let x = self.lambda(l, None);
        self.own_args = saved;
        if !is_async
            || self.self_mode != SelfMode::Component
            || self.target != Target::Client
            || self.ssr
        {
            return x;
        }
        let ctx = if self.has_c {
            id("$c")
        } else {
            self.uses_life = true;
            id("$life")
        };
        rt("own", vec![x, ctx])
    }

    /// Lowers a lambda; `expected` is the parameter's function type when
    /// known.
    pub(crate) fn lambda(&mut self, l: &ast::LambdaExpr, _expected: Option<&Type>) -> Expr {
        let lt = self.ty(l.syntax());
        let awaits = l
            .syntax()
            .descendants()
            .any(|n| n.kind() == SyntaxKind::AwaitExpr);
        let force_sync = std::mem::take(&mut self.force_sync);
        let is_async = matches!(&lt, Type::Func(f) if f.is_async) && (awaits || !force_sync);
        let saved_c = self.has_c;
        let saved_owned = std::mem::take(&mut self.owned);
        // A lambda's `try`s throw where it is called, not here.
        let saved_tags = std::mem::take(&mut self.err_tags);
        let saved_try_tags = std::mem::take(&mut self.try_tags);
        let saved_ret = std::mem::replace(
            &mut self.ret,
            match &lt {
                Type::Func(f) => Some(f.ret.clone()),
                _ => None,
            },
        );
        self.kill_aliases_captured(l.syntax());
        self.push_scope();
        let mut params = Vec::new();
        if is_async {
            params.push(Param::new("$c"));
            self.has_c = true;
        }
        let mut names = Vec::new();
        if let Some(list) = l.param_list() {
            for p in list
                .syntax()
                .children()
                .filter(|c| c.kind() == SyntaxKind::LambdaParam)
            {
                if let Some(t) = p
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::Underscore)
                {
                    if t.kind() == SyntaxKind::Underscore {
                        let n = self.fresh("_");
                        names.push(n);
                    } else {
                        names.push(self.bind(&t));
                    }
                }
            }
        }
        // `it`-style implicit parameter lambdas are not part of K; a lambda
        // without parameters whose type has one ignores it.
        for n in &names {
            params.push(Param::new(n.clone()));
        }
        let ret_void = matches!(&lt, Type::Func(f) if f.ret.decl() == self.g.b.void);
        let stmts: Vec<ast::Stmt> = l.statements().collect();
        let saved = std::mem::take(&mut self.stmts);
        let n = stmts.len();
        let mut tail: Option<Expr> = None;
        for (i, s) in stmts.iter().enumerate() {
            if i + 1 == n {
                if let ast::Stmt::Expr(e) = s {
                    if ret_void {
                        self.mapped(e.syntax(), |b| b.expr_stmt(e));
                    } else if self.is_statementy(e) {
                        self.mapped(e.syntax(), |b| b.expr_into(e, &Dest::Return));
                    } else {
                        let x = self.expr(e);
                        let from = self.ty(e.syntax());
                        let to = match &lt {
                            Type::Func(f) => f.ret.clone(),
                            _ => Type::Error,
                        };
                        let x = self.coerce(x, &from, &to);
                        tail = Some(self.at(e.syntax(), x));
                    }
                    continue;
                }
            }
            self.stmt(s);
        }
        let body = std::mem::replace(&mut self.stmts, saved);
        self.pop_scope();
        self.has_c = saved_c;
        self.owned = saved_owned;
        self.err_tags = saved_tags;
        self.try_tags = saved_try_tags;
        self.ret = saved_ret;
        let f = if body.is_empty() {
            match tail {
                Some(t) => js::Function {
                    params,
                    body: js::Body::Expr(Box::new(t)),
                    is_async,
                    ..js::Function::default()
                },
                None => js::Function {
                    params,
                    body: js::Body::Block(Vec::new()),
                    is_async,
                    ..js::Function::default()
                },
            }
        } else {
            let mut body = body;
            if let Some(t) = tail {
                body.push(Stmt::Return(Some(t)));
            }
            js::Function {
                params,
                body: js::Body::Block(body),
                is_async,
                ..js::Function::default()
            }
        };
        Expr::Arrow(Box::new(f))
    }

    /// Marks the collections a lambda captures as no longer owned. The same as
    /// [`Body::kill_aliases`] (the save and restore of the owned set around it
    /// has no effect).
    fn kill_aliases_captured(&mut self, node: &SyntaxNode) {
        // Collections captured by a lambda are no longer owned.
        let saved = self.owned.clone();
        self.owned = saved;
        self.kill_aliases(node);
    }

    // ------------------------------------------------------------- names

    /// Lowers a name: `self`, a local or parameter (read through its signal
    /// accessor for a keyed-row item), a component member, a member of the
    /// owner reached implicitly, or a top-level declaration (recording the
    /// use).
    #[allow(clippy::too_many_lines)]
    fn name_expr(&mut self, n: &ast::NameExpr) -> Expr {
        let Some(tok) = n.name() else {
            return Expr::Null;
        };
        if tok.kind() == SyntaxKind::SelfKw {
            return self.self_expr();
        }
        match self.resolution(tok.text_range()) {
            Some(
                Resolution::Local { name, definition } | Resolution::Parameter { name, definition },
            ) => {
                let js = self.local_name(&name, definition.range);
                if self.signal_locals.contains(&js) {
                    call(id(format!("{js}$")), vec![])
                } else {
                    id(js)
                }
            }
            Some(r) => match self.g.view.resolve(&r) {
                Some(Resolved::Decl(d)) => self.decl_value(d),
                Some(Resolved::Member(m)) => self.implicit_self_member(m),
                None if tok.text() == "reload" && self.self_mode == SelfMode::Component => {
                    js::arrow(vec![], call(member(id("$loads"), "reload"), vec![]))
                }
                None => id(mangle(tok.text())),
            },
            None => {
                if tok.text() == "reload" && self.self_mode == SelfMode::Component {
                    return js::arrow(vec![], call(member(id("$loads"), "reload"), vec![]));
                }
                // `untracked` and friends resolve to nothing: plain names.
                id(mangle(tok.text()))
            }
        }
    }

    /// A top-level declaration used as a value.
    fn decl_value(&mut self, d: DeclId) -> Expr {
        let info = self.g.view.decl(d);
        if info.named_kind() == Some(DeclarationKind::Constant) && info.stdlib {
            return self.decl_ref(d);
        }
        self.decl_ref(d)
    }

    /// The value of the shorthand `if let name` / `guard let name`: the
    /// visible local `name`, else the owner's member `name` (`if let
    /// region` in a method of a struct with a `region` field).
    fn shorthand_value(&mut self, n: &SyntaxToken) -> Expr {
        // `if let user` naming a member (a `@State` or `@Derived` of a
        // component, whose JS local is the signal) reads the member's
        // value, so a `@Derived` initializer tracks it.
        if let Some(Resolved::Member(m)) = self.resolved(n.text_range()) {
            if self.access.contains_key(&m) {
                return self.implicit_self_member(m);
            }
        }
        if let Some(local) = self.local_by_name(n.text()) {
            return id(local);
        }
        if let Some(m) = self.owner.and_then(|d| self.g.find_member(d, n.text())) {
            return self.implicit_self_member(m);
        }
        id(mangle(n.text()))
    }

    /// A member reached without a receiver: through `self`, or a static
    /// of the owner.
    pub(crate) fn implicit_self_member(&mut self, m: MemberId) -> Expr {
        if let Some(acc) = self.access.get(&m).cloned() {
            // An async function of a component used as a value (passed to
            // `form.handle(_:)`, stored in a closure): its cancellation
            // context is the component's lifetime, so the value takes the
            // arguments of the K function type alone.
            let info = self.g.view.member(m);
            if let MemberAccess::Local(n) = &acc {
                if info.kind == MemberKind::Func
                    && self.self_mode == SelfMode::Component
                    && self.target == Target::Client
                    && !self.ssr
                    && info.signature.as_ref().is_some_and(|s| s.is_async)
                {
                    self.uses_life = true;
                    return rt("own", vec![id(n.clone()), id("$life")]);
                }
            }
            return self.read_access(&acc);
        }
        let info = self.g.view.member(m).clone();
        if info.is_static || info.kind == MemberKind::Case {
            return self.static_ref(m);
        }
        match self.self_mode {
            SelfMode::StructInit => {
                if info.kind == MemberKind::Field {
                    return id(mangle(&info.name));
                }
                // A computed property or method inside an init: build self.
                let obj = self.init_object();
                self.member_on(obj, m, &info)
            }
            SelfMode::This | SelfMode::Component => {
                let recv = Expr::This;
                self.member_on(recv, m, &info)
            }
            SelfMode::Value | SelfMode::None => {
                let recv = id("self");
                self.member_on(recv, m, &info)
            }
        }
    }

    /// Reads a component member.
    pub(crate) fn read_access(&mut self, acc: &MemberAccess) -> Expr {
        match acc {
            MemberAccess::Local(n) => id(n.clone()),
            MemberAccess::Signal(n) | MemberAccess::Computed(n) => {
                if self.ssr {
                    id(n.clone())
                } else {
                    call(member(id(n.clone()), "get"), vec![])
                }
            }
            MemberAccess::Prop(n) => call(member(id("props"), n.clone()), vec![]),
            MemberAccess::Binding(n) => call(member(member(id("props"), n.clone()), "get"), vec![]),
            MemberAccess::Store(n) => {
                if self.ssr {
                    id(n.clone())
                } else {
                    call(member(id(n.clone()), "get"), vec![Expr::Array(Vec::new())])
                }
            }
            MemberAccess::Raw(e) => e.clone(),
        }
    }

    /// A member read on a receiver value (field, computed, method value).
    fn member_on(&mut self, recv: Expr, m: MemberId, info: &k_sema::view::MemberInfo) -> Expr {
        let owner = self.g.view.decl(info.owner);
        let owner_is_value = matches!(
            owner.named_kind(),
            Some(DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::Entity)
        ) || owner.extends.is_some();
        match info.kind {
            MemberKind::Field => {
                if info.has_annotation("State")
                    && self.is_service_like(info.owner)
                    && self.target == Target::Client
                {
                    return call(member(member(recv, info.name.clone()), "get"), vec![]);
                }
                if info.has_annotation("Derived")
                    && self.is_service_like(info.owner)
                    && self.target == Target::Client
                {
                    return call(member(member(recv, info.name.clone()), "get"), vec![]);
                }
                member(recv, info.name.clone())
            }
            MemberKind::Computed => {
                if owner_is_value {
                    let f = self.fn_ref(m);
                    call(f, vec![recv])
                } else {
                    member(recv, info.name.clone())
                }
            }
            MemberKind::Func
                if self.g.view.decl(info.owner).members.iter().any(|&o| {
                    let other = self.g.view.member(o);
                    other.name == info.name
                        && matches!(other.kind, MemberKind::Field | MemberKind::Computed)
                }) =>
            {
                // A read of a name that is both a property and a method
                // (`HttpResponse.body`, the received text, and
                // `body(_:)`, the builder step) is the property; only a
                // call is the method.
                let field = self
                    .g
                    .view
                    .decl(info.owner)
                    .members
                    .iter()
                    .copied()
                    .find(|&o| {
                        let other = self.g.view.member(o);
                        other.name == info.name
                            && matches!(other.kind, MemberKind::Field | MemberKind::Computed)
                    })
                    .unwrap_or(m);
                let fi = self.g.view.member(field).clone();
                self.member_on(recv, field, &fi)
            }
            MemberKind::Func => {
                if owner_is_value {
                    let f = self.fn_ref(m);
                    js::arrow(
                        vec![Param {
                            name: "a".into(),
                            default: None,
                            rest: true,
                        }],
                        call(f, vec![recv, Expr::Spread(Box::new(id("a")))]),
                    )
                } else {
                    // A method value keeps its receiver: `todos.add` is
                    // `todos.add.bind(todos)`.
                    let name = self.g.member_name(m);
                    let simple = {
                        let mut e = &recv;
                        loop {
                            match e {
                                Expr::Ident(_) | Expr::This => break true,
                                Expr::Member(inner, _) => e = inner,
                                _ => break false,
                            }
                        }
                    };
                    if simple {
                        call(member(member(recv.clone(), name), "bind"), vec![recv])
                    } else {
                        let a = Param {
                            name: "a".into(),
                            default: None,
                            rest: true,
                        };
                        js::arrow(
                            vec![a],
                            call(member(recv, name), vec![Expr::Spread(Box::new(id("a")))]),
                        )
                    }
                }
            }
            _ => member(recv, info.name.clone()),
        }
    }

    /// A static member or enum case: `Point$origin`, `Shape.empty`.
    pub(crate) fn static_ref(&mut self, m: MemberId) -> Expr {
        let info = self.g.view.member(m).clone();
        let owner = info.owner;
        if info.kind == MemberKind::Case
            || (info.kind == MemberKind::Generated && crate::app::is_generated_enum(self.g, owner))
        {
            let o = self.decl_ref(owner);
            return member(o, info.name.clone());
        }
        let od = self.g.view.decl(owner);
        if od.stdlib {
            if let Some(e) = crate::native::static_value(self, m) {
                return e;
            }
        }
        match od.named_kind() {
            Some(DeclarationKind::Class | DeclarationKind::Service) => {
                // The stdlib's classes are the runtime's, by their K names.
                let stdlib = od.stdlib;
                let o = self.decl_ref(owner);
                let n = if stdlib {
                    info.name.clone()
                } else {
                    self.g.member_name(m)
                };
                member(o, n)
            }
            _ => {
                if info.kind == MemberKind::Func {
                    self.fn_ref(m)
                } else if info.kind == MemberKind::Computed {
                    let f = self.fn_ref(m);
                    call(f, vec![])
                } else {
                    self.fn_ref(m)
                }
            }
        }
    }

    /// A `Page` case used as a value (`.search`, not `.search(…)`): the
    /// case of a page with parameters is a function of them, so its value
    /// is the case at its defaults.
    fn page_case_value(&mut self, m: MemberId) -> Option<Expr> {
        let info = self.g.view.member(m).clone();
        let owner = info.owner;
        if Some(owner) != self.g.view.lookup("k.core.Page") {
            return None;
        }
        let with_params = crate::app::pages(self.g).into_iter().any(|p| {
            crate::app::case_name(&self.g.view.decl(p).name) == info.name
                && !crate::app::params(self.g, p).is_empty()
        });
        if !with_params {
            return None;
        }
        let o = self.decl_ref(owner);
        Some(call(member(o, info.name.clone()), Vec::new()))
    }

    /// `true` for `k.validation.FieldName` or a struct's generated
    /// `FieldName` enum.
    fn is_field_name(&self, t: &Type) -> bool {
        t.unwrap_optional().decl().is_some_and(|d| {
            Some(d) == self.g.view.lookup("k.validation.FieldName")
                || self.g.view.decl(d).name == "FieldName"
        })
    }

    /// `true` if `node` is an argument passed to a parameter of type
    /// `FieldName` (`rules.check(…, field: .confirmPassword, …)`).
    fn is_field_name_arg(&self, node: &SyntaxNode) -> bool {
        let Some(arg) = node.parent().and_then(ast::Arg::cast) else {
            return false;
        };
        let Some(call) = arg.syntax().ancestors().find_map(ast::CallExpr::cast) else {
            return false;
        };
        let callee_name = match call.callee() {
            Some(ast::Expr::MemberExpr(m)) => m.name(),
            Some(ast::Expr::NameExpr(n)) => n.name(),
            _ => None,
        };
        let Some(Resolved::Member(m)) = callee_name.and_then(|t| self.resolved(t.text_range()))
        else {
            return false;
        };
        let label = arg.label().map(|t| t.text().to_owned());
        self.g
            .view
            .member(m)
            .signature
            .as_ref()
            .and_then(|s| s.params.iter().find(|p| p.label == label))
            .is_some_and(|p| self.is_field_name(&p.ty))
    }

    /// Makes each call among the operands of compound expression `e`
    /// (through nested operators and parentheses) lower as an awaited call,
    /// in place (an override of its range), so it is awaited where it is
    /// evaluated and short-circuiting keeps its meaning.
    fn await_calls_in(&mut self, e: &ast::Expr) {
        let operands: Vec<ast::Expr> = match e {
            ast::Expr::BinaryExpr(b) => [b.lhs(), b.rhs()].into_iter().flatten().collect(),
            ast::Expr::PrefixExpr(p) => p.operand().into_iter().collect(),
            ast::Expr::ParenExpr(p) => p.syntax().children().filter_map(ast::Expr::cast).collect(),
            ast::Expr::MemberExpr(m) => m.receiver().into_iter().collect(),
            ast::Expr::IndexExpr(i) => index_base(i).into_iter().collect(),
            _ => Vec::new(),
        };
        for o in operands {
            if covers_compound(&o) {
                self.await_calls_in(&o);
            } else if matches!(o, ast::Expr::CallExpr(_)) {
                let x = self.expr(&o);
                let awaited = self.awaited(x);
                self.overrides.insert(o.syntax().text_range(), awaited);
            }
        }
    }

    /// Lowers an implicit member `.name`: a `FieldName` as its string, a
    /// theme token as its CSS value, a `Page` case as its value, or a static
    /// member or case of the expected type.
    fn implicit_member(&mut self, node: &SyntaxNode, _args: Option<Vec<Expr>>) -> Expr {
        let Some(name) = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind().is_name_like())
        else {
            return Expr::Null;
        };
        let t = self.ty(node);
        // A `FieldName` case is the field's name (validation paths), even
        // where the name was also recorded as a theme token.
        if self.is_field_name(&t) || self.is_field_name_arg(node) {
            return js::str(name.text().to_owned());
        }
        match self.resolution(name.text_range()) {
            Some(Resolution::Token {
                category,
                name: tok,
            }) => {
                let _ = category;
                return crate::native::token_value(self, &t, &tok);
            }
            Some(r) => {
                if let Some(Resolved::Member(m)) = self.g.view.resolve(&r) {
                    if let Some(v) = self.page_case_value(m) {
                        return v;
                    }
                    return self.static_ref(m);
                }
            }
            None => {}
        }
        // A token or case by name on the expected type.
        if let Some(d) = t.unwrap_optional().decl() {
            if let Some(m) = self.g.find_member(d, name.text()) {
                return self.static_ref(m);
            }
        }
        crate::native::token_value(self, &t, name.text())
    }

    // -------------------------------------------------------- operators

    /// Lowers a binary operator by operand types: short-circuit operators
    /// keep their laziness (a right operand that needs statements runs behind
    /// an `if`), `==` is structural where needed, arithmetic on `Decimal`,
    /// `Money` and the like goes through the runtime, and constant CSS
    /// arithmetic folds.
    #[allow(clippy::too_many_lines)]
    fn binary(&mut self, b: &ast::BinaryExpr) -> Expr {
        let op = b.op().map(|t| t.text().to_owned()).unwrap_or_default();
        let (Some(lhs), Some(rhs)) = (b.lhs(), b.rhs()) else {
            return Expr::Null;
        };
        let lt = self.ty(lhs.syntax());
        let rtt = self.ty(rhs.syntax());
        let result = self.ty(b.syntax());
        match op.as_str() {
            "and" | "&&" | "or" | "||" | "??" => {
                let jsop: &'static str = match op.as_str() {
                    "and" | "&&" => "&&",
                    "or" | "||" => "||",
                    _ => "??",
                };
                let l = self.expr(&lhs);
                let (pre, r) = self.capture(|bd| bd.expr(&rhs));
                if pre.is_empty() {
                    return bin(jsop, l, r);
                }
                let t = self.fresh("v");
                self.emit(Stmt::Let(t.clone(), Some(l)));
                let test = match jsop {
                    "&&" => id(&t),
                    "||" => un("!", id(&t)),
                    _ => bin("===", id(&t), Expr::Null),
                };
                let mut inner = pre;
                inner.push(Stmt::Expr(js::assign(id(&t), r)));
                self.emit(Stmt::If(test, inner, None));
                id(t)
            }
            "==" | "!=" => {
                let l = self.expr(&lhs);
                let r = self.expr(&rhs);
                let identity = self.g.is_identity_eq(&lt)
                    || self.g.is_identity_eq(&rtt)
                    || matches!(l, Expr::Null)
                    || matches!(r, Expr::Null);
                if identity {
                    bin(if op == "==" { "===" } else { "!==" }, l, r)
                } else {
                    let e = rt("equals", vec![l, r]);
                    if op == "==" { e } else { un("!", e) }
                }
            }
            "<" | "<=" | ">" | ">=" => {
                let l = self.expr(&lhs);
                let r = self.expr(&rhs);
                let jsop: &'static str = match op.as_str() {
                    "<" => "<",
                    "<=" => "<=",
                    ">" => ">",
                    _ => ">=",
                };
                if self.g.is_number(&lt) || self.g.is_string(&lt) {
                    bin(jsop, l, r)
                } else {
                    bin(jsop, rt("compare", vec![l, r]), Expr::Num(0.0))
                }
            }
            "..<" | "..." => {
                let l = self.expr(&lhs);
                let r = self.expr(&rhs);
                rt(
                    if op == "..<" { "range" } else { "closedRange" },
                    vec![l, r],
                )
            }
            "+" | "-" | "*" | "/" | "%" => {
                if let Some(folded) = self.fold_units(b, &op, &lt, &rtt, &result) {
                    return folded;
                }
                let l = self.expr(&lhs);
                let r = self.expr(&rhs);
                self.arith(&op, l, r, &lt, &rtt, &result)
            }
            _ => {
                let l = self.expr(&lhs);
                let r = self.expr(&rhs);
                bin("+", l, r)
            }
        }
    }

    /// Lowers an arithmetic operator by operand types (codegen.md §5, §6.1).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn arith(
        &mut self,
        op: &str,
        l: Expr,
        r: Expr,
        lt: &Type,
        rtt: &Type,
        result: &Type,
    ) -> Expr {
        let jsop: &'static str = match op {
            "+" => "+",
            "-" => "-",
            "*" => "*",
            "/" => "/",
            _ => "%",
        };
        let g = self.g;
        if g.is_list(lt) && op == "+" {
            return Expr::Array(vec![Expr::Spread(Box::new(l)), Expr::Spread(Box::new(r))]);
        }
        if g.is(lt, g.b.bytes) && op == "+" {
            // `Bytes + Bytes`: a new `Uint8Array` with both, in order.
            return Expr::New(
                Box::new(id("Uint8Array")),
                vec![Expr::Array(vec![
                    Expr::Spread(Box::new(l)),
                    Expr::Spread(Box::new(r)),
                ])],
            );
        }
        if g.is_string(lt) {
            return bin("+", l, r);
        }
        if g.is(lt, g.b.money) || g.is(rtt, g.b.money) {
            return match op {
                "+" => call(member(l, "plus"), vec![r]),
                "-" => call(member(l, "minus"), vec![r]),
                "*" => {
                    if g.is(lt, g.b.money) {
                        call(member(l, "times"), vec![r])
                    } else {
                        call(member(r, "times"), vec![l])
                    }
                }
                _ => call(member(l, "dividedBy"), vec![r]),
            };
        }
        if g.is(lt, g.b.decimal) || g.is(rtt, g.b.decimal) {
            let method = match op {
                "+" => "plus",
                "-" => "minus",
                "*" => "times",
                "/" => "dividedBy",
                _ => "remainder",
            };
            return rt(&format!("decimal{}", capitalize(method)), vec![l, r]);
        }
        if g.is(lt, g.b.date) {
            if g.is(rtt, g.b.date) {
                return call(member(l, "minus"), vec![r]);
            }
            return match op {
                "-" => call(member(l, "plus"), vec![un("-", r)]),
                _ => call(member(l, "plus"), vec![r]),
            };
        }
        if g.is_css_text(lt) || g.is_css_text(rtt) {
            let pct = |e: Expr, t: &Type| {
                if g.is(t, g.b.percent) {
                    Expr::Template(vec![TemplatePart::Expr(e), TemplatePart::Text("%".into())])
                } else {
                    e
                }
            };
            let l = pct(l, lt);
            let r = pct(r, rtt);
            let text = Expr::Template(vec![
                TemplatePart::Expr(l),
                TemplatePart::Text(format!(" {jsop} ")),
                TemplatePart::Expr(r),
            ]);
            return rt("calc", vec![text]);
        }
        let int = g.is_int(result) || (g.is_int(lt) && g.is_int(rtt));
        if int {
            return match op {
                "/" => rt("intDiv", vec![l, r]),
                "%" => rt("intRem", vec![l, r]),
                _ => {
                    let e = bin(jsop, l, r);
                    if g.checked() {
                        rt("checkInt", vec![e])
                    } else {
                        e
                    }
                }
            };
        }
        if g.is(result, g.b.duration) && (g.is_int(lt) || g.is_int(rtt)) && op == "/" {
            return call(member(id("Math"), "trunc"), vec![bin("/", l, r)]);
        }
        bin(jsop, l, r)
    }

    /// Folds constant unit arithmetic: `16px * 2` → `"32px"`, durations to
    /// milliseconds, mixed lengths to `calc()` (codegen.md §6.5).
    fn fold_units(
        &mut self,
        b: &ast::BinaryExpr,
        op: &str,
        lt: &Type,
        rtt: &Type,
        result: &Type,
    ) -> Option<Expr> {
        let (lhs, rhs) = (b.lhs()?, b.rhs()?);
        if self.g.is(result, self.g.b.duration) {
            let l = self.const_number(&lhs)?;
            let r = self.const_number(&rhs)?;
            let v = match op {
                "+" => l + r,
                "-" => l - r,
                "*" => l * r,
                "/" if r != 0.0 => (l / r).trunc(),
                _ => return None,
            };
            return Some(Expr::Num(v));
        }
        if !(self.g.is_css_text(lt) || self.g.is_css_text(rtt)) {
            return None;
        }
        let l = self.const_css(&lhs)?;
        let r = self.const_css(&rhs)?;
        Some(js::str(fold_css(op, &l, &r)))
    }

    /// The constant numeric value of a literal-only expression (durations
    /// in milliseconds).
    fn const_number(&mut self, e: &ast::Expr) -> Option<f64> {
        match e {
            ast::Expr::Literal(_)
            | ast::Expr::MemberExpr(_)
            | ast::Expr::BinaryExpr(_)
            | ast::Expr::ParenExpr(_) => {
                let saved = self.stmts.len();
                let x = self.expr(e);
                self.stmts.truncate(saved);
                match x {
                    Expr::Num(n) => Some(n),
                    _ => None,
                }
            }
            ast::Expr::PrefixExpr(_) => match self.expr(e) {
                Expr::Num(n) => Some(n),
                _ => None,
            },
            _ => None,
        }
    }

    /// The CSS text of `e` when it lowers to a constant (a literal, a
    /// parenthesized or binary expression of constants); `None` otherwise.
    /// Statements emitted while trying are discarded.
    fn const_css(&mut self, e: &ast::Expr) -> Option<String> {
        let saved = self.stmts.len();
        let x = match e {
            ast::Expr::Literal(_) | ast::Expr::BinaryExpr(_) | ast::Expr::ParenExpr(_) => {
                self.expr(e)
            }
            _ => return None,
        };
        self.stmts.truncate(saved);
        let percent = self.g.is(&self.ty(e.syntax()), self.g.b.percent);
        match x {
            Expr::Str(s) => Some(s),
            Expr::Num(n) if percent => Some(format!("{}%", fmt_num(n))),
            Expr::Num(n) => Some(fmt_num(n)),
            _ => None,
        }
    }

    // ----------------------------------------------------------- members

    /// Lowers a member access `recv.name` (or `recv?.name`): static members and
    /// cases on a type, native stdlib members, component and service members,
    /// tuple elements and plain fields.
    #[allow(clippy::too_many_lines)]
    fn member_expr(&mut self, m: &ast::MemberExpr) -> Expr {
        let Some(name) = m.name() else {
            return Expr::Null;
        };
        let Some(recv) = m.receiver() else {
            return Expr::Null;
        };
        let optional = m.is_optional();
        let rty = self.ty(recv.syntax());
        let resolved = self.resolved(name.text_range());
        // Static access on a type: `Point.origin`, `Shape.empty`.
        if let Some(Resolved::Member(mid)) = resolved {
            let info = self.g.view.member(mid).clone();
            if (info.is_static || info.kind == MemberKind::Case)
                && (matches!(rty, Type::Meta(_)) || self.is_type_name(&recv))
            {
                // `Page.signIn`: the case with its defaults, a value.
                if let Some(v) = self.page_case_value(mid) {
                    return v;
                }
                return self.static_ref(mid);
            }
            if let Some(acc) = self.access.get(&mid).cloned() {
                if matches!(&recv, ast::Expr::NameExpr(n) if n.name().is_some_and(|t| t.kind() == SyntaxKind::SelfKw))
                {
                    return self.read_access(&acc);
                }
            }
        }
        if let Some(Resolution::Token { name: tok, .. }) = self.resolution(name.text_range()) {
            let t = self.ty(m.syntax());
            return crate::native::token_value(self, &t, &tok);
        }
        if self.self_mode == SelfMode::StructInit && is_self(&recv) {
            if let Some(Resolved::Member(mid)) = resolved {
                return self.implicit_self_member(mid);
            }
        }
        if !self.ssr {
            if let Some((st, path)) = self.store_path(&ast::Expr::MemberExpr(m.clone())) {
                return call(
                    member(id(st), "get"),
                    vec![Expr::Array(path.into_iter().map(js::str).collect())],
                );
            }
        }
        let base = rty.unwrap_optional().clone();
        if self.is_form_value(&recv, &base) {
            let r = self.expr(&recv);
            let n = name.text();
            if matches!(
                n,
                "value" | "isValid" | "isSubmitting" | "isTouched" | "errors"
            ) {
                return call(member(r, n.to_owned()), vec![]);
            }
            return call(
                member(call(member(r, "field"), vec![js::str(n)]), "value"),
                vec![],
            );
        }
        // Fold number extensions on constants: `10.minutes`.
        if let ast::Expr::Literal(_) = &recv {
            if let Some(v) = crate::native::fold_number_member(self, &recv, name.text(), &base) {
                return v;
            }
        }
        let r = self.expr(&recv);
        if optional {
            return self.optional_member(r, &base, name.text(), resolved, m);
        }
        self.member_read(r, &base, name.text(), resolved, m.syntax())
    }

    /// `true` if `e` is a name that resolves to a type declaration (anything
    /// but a function or a constant), so `T.name` is static access.
    fn is_type_name(&self, e: &ast::Expr) -> bool {
        if let ast::Expr::NameExpr(n) = e {
            if let Some(t) = n.name() {
                if let Some(Resolved::Decl(d)) = self.resolved(t.text_range()) {
                    return self.g.view.decl(d).named_kind().is_some_and(|k| {
                        !matches!(k, DeclarationKind::Function | DeclarationKind::Constant)
                    });
                }
            }
        }
        false
    }

    /// Reads member `name` of `recv` (of type `base`).
    fn member_read(
        &mut self,
        recv: Expr,
        base: &Type,
        name: &str,
        resolved: Option<Resolved>,
        node: &SyntaxNode,
    ) -> Expr {
        if let Some(Resolved::Member(mid)) = resolved {
            let info = self.g.view.member(mid).clone();
            let owner = self.g.view.decl(info.owner);
            if owner.stdlib && (info.kind != MemberKind::Field || self.g.is_native_member(mid)) {
                let result = self.ty(node);
                if let Some(e) = crate::native::property(self, recv.clone(), base, mid, &result) {
                    return e;
                }
            }
            return self.member_on(recv, mid, &info);
        }
        // Tuples: `.0`, `.1`.
        if let Ok(ix) = name.parse::<usize>() {
            return Expr::Index(Box::new(recv), Box::new(Expr::Num(ix as f64)));
        }
        member(recv, name.to_owned())
    }

    /// Lowers `r?.name`: JavaScript optional chaining for a plain project
    /// field followed only by plain field reads, otherwise a nil test raised
    /// to the chain's outermost link ([`Body::opt_link`]).
    fn optional_member(
        &mut self,
        r: Expr,
        base: &Type,
        name: &str,
        resolved: Option<Resolved>,
        m: &ast::MemberExpr,
    ) -> Expr {
        if self.plain_field(m) && self.plain_after(m.syntax()) {
            // JavaScript's `?.` yields `undefined`; the chain's outermost
            // link turns it into `null` (codegen.md §6.1).
            let top = chain_top(m.syntax()).text_range();
            self.opt_undefined.insert(top);
            return Expr::OptMember(Box::new(r), name.to_owned());
        }
        let t = self.opt_link(m.syntax(), r);
        self.member_read(t, base, name, resolved, m.syntax())
    }

    // ------------------------------------------------------------- calls

    /// The arguments of a call in parameter order (codegen.md §4): labels
    /// erased, omitted defaults as `undefined`, trailing `undefined`s
    /// dropped.
    pub(crate) fn call_args(
        &mut self,
        c: &ast::CallExpr,
        sig: Option<&Signature>,
        subst_ty: &dyn Fn(&Type) -> Type,
    ) -> Vec<Expr> {
        let mut given: Vec<(Option<String>, ast::Expr)> = Vec::new();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                if let Some(v) = a.value() {
                    given.push((a.label().map(|t| t.text().to_owned()), v));
                }
            }
        }
        if let Some(l) = c.trailing_lambda() {
            given.push((Some("\u{0}trailing".into()), ast::Expr::LambdaExpr(l)));
        }
        let Some(sig) = sig else {
            return given.iter().map(|(_, e)| self.expr(e)).collect();
        };
        let mut out: Vec<Expr> = Vec::new();
        let mut gi = 0;
        let nparams = sig.params.len();
        for (pi, p) in sig.params.iter().enumerate() {
            let next = given.get(gi);
            let matches = match next {
                Some((label, _)) => {
                    label.as_deref() == p.label.as_deref()
                        || label.as_deref() == Some("\u{0}trailing")
                        || (p.label.is_none() && label.is_none())
                }
                None => false,
            };
            // A trailing lambda goes to the last parameter, skipping
            // defaults before it.
            let trailing_here = matches!(next, Some((Some(l), _)) if l == "\u{0}trailing")
                && pi + 1 < nparams
                && p.has_default
                && !matches!(p.ty, Type::Func(_));
            if matches && !trailing_here {
                let (_, e) = given[gi].clone();
                gi += 1;
                let x = self.arg(&e, &subst_ty(&p.ty));
                out.push(x);
            } else if p.has_default {
                out.push(Expr::Undefined);
            } else if let Some((_, e)) = given.get(gi).cloned() {
                gi += 1;
                let x = self.arg(&e, &subst_ty(&p.ty));
                out.push(x);
            } else {
                out.push(Expr::Undefined);
            }
        }
        while matches!(out.last(), Some(Expr::Undefined)) {
            out.pop();
        }
        out
    }

    /// Lowers one call argument for a parameter of type `param_ty`: a lambda
    /// with the expected type, a statement-like expression through a
    /// temporary, and the result coerced to the parameter type.
    fn arg(&mut self, e: &ast::Expr, param_ty: &Type) -> Expr {
        self.kill_aliases(e.syntax());
        // Only this call's own arguments are bound, not those of calls
        // nested in them.
        let own = std::mem::take(&mut self.own_args);
        let bind = std::mem::take(&mut self.bind_args);
        let x = if let ast::Expr::LambdaExpr(l) = e {
            if own {
                self.owned_lambda(l)
            } else {
                self.lambda(l, Some(param_ty))
            }
        } else if let Some(f) = (!own).then(|| self.plain_method_value(e)).flatten() {
            f
        } else if self.is_statementy(e) {
            let t = self.fresh("v");
            self.emit(Stmt::Let(t.clone(), None));
            self.expr_into(e, &Dest::Assign(t.clone()));
            id(t)
        } else {
            self.expr(e)
        };
        self.own_args = own;
        self.bind_args = bind;
        let from = self.ty(e.syntax());
        let x = self.coerce(x, &from, param_ty);
        if bind && !own {
            self.bound_arg(x, param_ty)
        } else {
            x
        }
    }

    /// A component's async function named as a call argument, as the plain
    /// function: the parameter is called the way K calls a function value,
    /// with the caller's cancellation context first, so the value must not
    /// be bound with its K arguments alone (`$k.own`), which would shift
    /// them by one (codegen.md §8). `None` for any other argument.
    fn plain_method_value(&mut self, e: &ast::Expr) -> Option<Expr> {
        let ast::Expr::NameExpr(n) = e else {
            return None;
        };
        let tok = n.name()?;
        let r = self.resolution(tok.text_range())?;
        let Some(Resolved::Member(m)) = self.g.view.resolve(&r) else {
            return None;
        };
        let info = self.g.view.member(m);
        if info.kind != MemberKind::Func
            || self.self_mode != SelfMode::Component
            || self.target != Target::Client
            || self.ssr
            || !info.signature.as_ref().is_some_and(|s| s.is_async)
        {
            return None;
        }
        match self.access.get(&m) {
            Some(MemberAccess::Local(local)) => Some(id(local.clone())),
            _ => None,
        }
    }

    /// A function value a component passes to a service's method, bound to
    /// the component's lifetime: `$k.bound(f, $life, mode)` (03-reactivity.md,
    /// "What a component leaves behind"). The service may keep it (a
    /// listener) or call it later (a `Clock` timer's action), after the
    /// component is disposed, and then it runs nothing. `mode` is 0 for a
    /// synchronous `Void` function, 1 for one with a value, and 2 and 3 for
    /// the async ones. Anywhere but a component's client renderer, and for
    /// an argument that isn't a function, it is `x`.
    fn bound_arg(&mut self, x: Expr, param_ty: &Type) -> Expr {
        let Type::Func(f) = param_ty else {
            return x;
        };
        if self.self_mode != SelfMode::Component || self.target != Target::Client || self.ssr {
            return x;
        }
        let void =
            self.g.is(&f.ret, self.g.b.void) || matches!(&f.ret, Type::Tuple(v) if v.is_empty());
        let mode = match (f.is_async, void) {
            (false, true) => 0,
            (false, false) => 1,
            (true, true) => 2,
            (true, false) => 3,
        };
        self.uses_life = true;
        rt("bound", vec![x, id("$life"), Expr::Num(f64::from(mode))])
    }

    /// Lowers a call: functions, methods (native or not), initializers, enum
    /// case constructors, lambdas and function values, with labelled
    /// arguments reordered to the signature, defaults filled in, witnesses and
    /// `$c` passed where needed.
    #[allow(clippy::too_many_lines)]
    fn call_expr(&mut self, c: &ast::CallExpr) -> Expr {
        let Some(callee) = c.callee() else {
            return Expr::Null;
        };
        let result_ty = self.ty(c.syntax());
        // Builtin pseudo-functions.
        if let ast::Expr::NameExpr(n) = &callee {
            if let Some(tok) = n.name() {
                // `reload()` of a declaration's `@Load` group, also from
                // inside closures (an effect's timer).
                let generated = match self.resolution(tok.text_range()) {
                    None => true,
                    // The generated `reload()`, unless the declaration has
                    // a function of that name.
                    Some(Resolution::Member { name, .. }) => {
                        name == "reload()"
                            && !self.owner.is_some_and(|d| {
                                self.g.view.decl(d).members.iter().any(|&m| {
                                    let mi = self.g.view.member(m);
                                    mi.name == "reload" && mi.kind == MemberKind::Func
                                })
                            })
                    }
                    Some(_) => false,
                };
                if generated
                    && tok.text() == "reload"
                    && matches!(self.self_mode, SelfMode::Component)
                {
                    return call(member(id("$loads"), "reload"), vec![]);
                }
                if self.resolution(tok.text_range()).is_none() {
                    if let Some(e) = crate::native::builtin_call(self, tok.text(), c) {
                        return e;
                    }
                }
            }
        }
        match &callee {
            ast::Expr::NameExpr(n) => {
                let Some(tok) = n.name() else {
                    return Expr::Null;
                };
                match self.resolution(tok.text_range()) {
                    Some(
                        Resolution::Local { name, definition }
                        | Resolution::Parameter { name, definition },
                    ) => {
                        let f = id(self.local_name(&name, definition.range));
                        self.call_value(f, c, &self.ty(callee.syntax()))
                    }
                    Some(r) => match self.g.view.resolve(&r) {
                        Some(Resolved::Decl(d)) => self.call_decl(d, c, &result_ty),
                        Some(Resolved::Member(m)) => {
                            let info = self.g.view.member(m).clone();
                            if let Some(acc) = self.access.get(&m).cloned() {
                                let f = match acc {
                                    MemberAccess::Local(n) => id(n),
                                    other => self.read_access(&other),
                                };
                                let sig = info.signature.clone();
                                let mut args = Vec::new();
                                if sig.as_ref().is_some_and(|s| s.is_async) {
                                    args.push(self.c_expr());
                                }
                                args.extend(self.call_args(c, sig.as_ref(), &|t| t.clone()));
                                if info.kind == MemberKind::Field {
                                    // An @Emits event or function field.
                                    return Expr::OptCall(Box::new(f), args);
                                }
                                return call(f, args);
                            }
                            if info.is_static || info.kind == MemberKind::Case {
                                return self.call_static(m, c, &result_ty);
                            }
                            let recv = match self.self_mode {
                                SelfMode::This | SelfMode::Component => Expr::This,
                                SelfMode::StructInit => self.init_object(),
                                _ => id("self"),
                            };
                            let self_ty = self
                                .owner
                                .map(|o| Type::Named(o, Vec::new()))
                                .unwrap_or(Type::Error);
                            self.call_method(recv, &self_ty, m, c, &result_ty)
                        }
                        None => {
                            let f = id(mangle(tok.text()));
                            let args = self.call_args(c, None, &|t| t.clone());
                            call(f, args)
                        }
                    },
                    None => {
                        let f = id(mangle(tok.text()));
                        let args = self.call_args(c, None, &|t| t.clone());
                        call(f, args)
                    }
                }
            }
            ast::Expr::MemberExpr(me) => {
                let Some(name) = me.name() else {
                    return Expr::Null;
                };
                let Some(recv) = me.receiver() else {
                    return Expr::Null;
                };
                let rty = self.ty(recv.syntax());
                let resolved = self.resolved(name.text_range());
                match resolved {
                    Some(Resolved::Member(m)) => {
                        let info = self.g.view.member(m).clone();
                        if (info.is_static
                            || info.kind == MemberKind::Case
                            || info.kind == MemberKind::Init)
                            && (matches!(rty, Type::Meta(_)) || self.is_type_name(&recv))
                        {
                            return self.call_static(m, c, &result_ty);
                        }
                        if let Some(acc) = self.access.get(&m).cloned() {
                            if matches!(&recv, ast::Expr::NameExpr(n) if n.name().is_some_and(|t| t.kind() == SyntaxKind::SelfKw))
                            {
                                let f = match acc {
                                    MemberAccess::Local(n) => id(n),
                                    other => self.read_access(&other),
                                };
                                let sig = info.signature.clone();
                                let mut args = Vec::new();
                                if sig.as_ref().is_some_and(|s| s.is_async) {
                                    args.push(self.c_expr());
                                }
                                args.extend(self.call_args(c, sig.as_ref(), &|t| t.clone()));
                                return call(f, args);
                            }
                        }
                        // A chained field of a `@Builder` struct
                        // (`request.query(q)`): a copy with the field set.
                        if info.kind == MemberKind::Field
                            && self.g.view.decl(info.owner).has_annotation("Builder")
                            && !info.ty.as_ref().is_some_and(|t| matches!(t, Type::Func(_)))
                        {
                            let r = self.expr(&recv);
                            let arg = c
                                .arg_list()
                                .and_then(|l| l.args().next())
                                .and_then(|a| a.value())
                                .map_or(Expr::Null, |v| {
                                    let x = self.expr(&v);
                                    let from = self.ty(v.syntax());
                                    self.coerce(x, &from, info.ty.as_ref().unwrap_or(&Type::Error))
                                });
                            return Expr::Object(
                                vec![Prop::Spread(r), Prop::KeyValue(info.name.clone(), arg)],
                                false,
                            );
                        }
                        // Sort chains: `sortedBy(a).thenBy(b)`.
                        if let Some(e) = crate::native::sort_chain(self, c) {
                            return e;
                        }
                        let r = self.expr(&recv);
                        if me.is_optional() {
                            let t = self.opt_link(c.syntax(), r);
                            let base = rty.unwrap_optional().clone();
                            return self.call_method(t, &base, m, c, &result_ty);
                        }
                        let base = rty.unwrap_optional().clone();
                        self.call_method(r, &base, m, c, &result_ty)
                    }
                    None if rty.decl().is_some()
                        && rty.decl() == self.g.view.lookup("k.core.Page") =>
                    {
                        // A page modifier `.tab(.reviews)`: a copy with the parameter set.
                        let r = self.expr(&recv);
                        let arg = c
                            .arg_list()
                            .and_then(|l| l.args().next())
                            .and_then(|a| a.value())
                            .map_or(Expr::Null, |v| self.expr(&v));
                        Expr::Object(
                            vec![Prop::Spread(r), Prop::KeyValue(name.text().to_owned(), arg)],
                            false,
                        )
                    }
                    None if name.text() == "copy"
                        && matches!(
                            self.resolution(name.text_range()),
                            Some(Resolution::Member { .. })
                        ) =>
                    {
                        // `copy(field: value)` of a `Copyable` struct.
                        let r = self.expr(&recv);
                        self.copy_call(r, &rty, c)
                    }
                    Some(Resolved::Decl(_)) | None => {
                        // A function-typed field or tuple element.
                        let f = self.expr(&callee);
                        let fty = self.ty(callee.syntax());
                        self.call_value(f, c, &fty)
                    }
                }
            }
            ast::Expr::ImplicitMemberExpr(im) => {
                let Some(name) = im
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|t| t.kind().is_name_like())
                else {
                    return Expr::Null;
                };
                let m = match self.resolved(name.text_range()) {
                    Some(Resolved::Member(m)) => Some(m),
                    _ => result_ty
                        .unwrap_optional()
                        .decl()
                        .and_then(|d| self.g.find_member(d, name.text())),
                };
                match m {
                    Some(m) => self.call_static(m, c, &result_ty),
                    None => Expr::Null,
                }
            }
            _ => {
                let f = self.expr(&callee);
                let fty = self.ty(callee.syntax());
                self.call_value(f, c, &fty)
            }
        }
    }

    /// Calls a function value (a local, parameter or field of function
    /// type).
    fn call_value(&mut self, f: Expr, c: &ast::CallExpr, fty: &Type) -> Expr {
        let mut args = Vec::new();
        let sig = match fty {
            Type::Func(ft) => {
                if ft.is_async {
                    args.push(self.c_expr());
                }
                Some(Signature {
                    generics: Vec::new(),
                    params: ft
                        .params
                        .iter()
                        .map(|p| k_sema::view::Param {
                            label: None,
                            name: String::new(),
                            ty: p.clone(),
                            has_default: false,
                            name_range: TextRange::default(),
                        })
                        .collect(),
                    is_async: ft.is_async,
                    throws: ft.throws.clone(),
                    ret: ft.ret.clone(),
                })
            }
            _ => None,
        };
        // Function values take positional arguments; labels don't apply.
        let mut given: Vec<ast::Expr> = Vec::new();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                if let Some(v) = a.value() {
                    given.push(v);
                }
            }
        }
        if let Some(l) = c.trailing_lambda() {
            given.push(ast::Expr::LambdaExpr(l));
        }
        for (i, e) in given.iter().enumerate() {
            let pt = sig
                .as_ref()
                .and_then(|s| s.params.get(i))
                .map(|p| p.ty.clone())
                .unwrap_or(Type::Error);
            let x = self.arg(e, &pt);
            args.push(x);
        }
        call(f, args)
    }

    /// Calls a top-level declaration: a function or a constructor.
    #[allow(clippy::too_many_lines)]
    fn call_decl(&mut self, d: DeclId, c: &ast::CallExpr, result_ty: &Type) -> Expr {
        let info = self.g.view.decl(d).clone();
        match info.named_kind() {
            Some(DeclarationKind::Function) => {
                let sig = info.signature.clone();
                let mut args = Vec::new();
                if sig.as_ref().is_some_and(|s| s.is_async) {
                    args.push(self.c_expr());
                }
                // Witnesses for constrained generics.
                let subst = sig
                    .as_ref()
                    .map(|s| self.infer_subst(s, c, result_ty))
                    .unwrap_or_default();
                if let Some(s) = &sig {
                    for g in &s.generics {
                        let ws = self.witnesses_for_generic(*g, subst.get(g));
                        args.extend(ws);
                    }
                }
                let st = subst.clone();
                args.extend(self.call_args(c, sig.as_ref(), &move |t| subst_map(t, &st)));
                if info.stdlib && self.g.view.qualified_name(d) == "k.test.render" {
                    if let Some(e) = crate::suite::render_call(self, c) {
                        return e;
                    }
                }
                let f = if info.stdlib {
                    match crate::native::function(self, d) {
                        Some(f) => f,
                        None => self.decl_ref(d),
                    }
                } else {
                    self.decl_ref(d)
                };
                call(f, args)
            }
            Some(DeclarationKind::Struct | DeclarationKind::Entity) => {
                if info.stdlib {
                    if let Some(e) = crate::native::construct(self, d, c, result_ty) {
                        return e;
                    }
                }
                let inits: Vec<MemberId> = info
                    .members
                    .iter()
                    .copied()
                    .filter(|&m| self.g.view.member(m).kind == MemberKind::Init)
                    .collect();
                if inits.is_empty() {
                    // Memberwise: the fields in order.
                    let fields = self.g.stored_fields(d);
                    let sig = memberwise_sig(self.g, d, &fields);
                    let owner_ty = result_ty.clone();
                    let args = self.call_args(c, Some(&sig), &|t| t.clone());
                    let _ = owner_ty;
                    let f = self.decl_ref(d);
                    return call(f, args);
                }
                let init = self.select_init(&inits, c);
                let sig = self.g.view.member(init).signature.clone();
                let args = self.call_args(c, sig.as_ref(), &|t| t.clone());
                let f = self.fn_ref(init);
                call(f, args)
            }
            Some(DeclarationKind::Class) => {
                if info.stdlib {
                    if let Some(e) = crate::native::construct(self, d, c, result_ty) {
                        return e;
                    }
                }
                let inits: Vec<MemberId> = info
                    .members
                    .iter()
                    .copied()
                    .filter(|&m| self.g.view.member(m).kind == MemberKind::Init)
                    .collect();
                let cls = self.decl_ref(d);
                if inits.is_empty() {
                    let fields = self.g.stored_fields(d);
                    let sig = memberwise_sig(self.g, d, &fields);
                    let args = self.call_args(c, Some(&sig), &|t| t.clone());
                    return Expr::New(Box::new(cls), args);
                }
                let init = self.select_init(&inits, c);
                let sig = self.g.view.member(init).signature.clone();
                let args = self.call_args(c, sig.as_ref(), &|t| t.clone());
                if inits.len() > 1 {
                    let name = self.g.member_name(init);
                    return call(member(cls, format!("${name}")), args);
                }
                Expr::New(Box::new(cls), args)
            }
            _ => {
                if info.stdlib {
                    if let Some(e) = crate::native::construct(self, d, c, result_ty) {
                        return e;
                    }
                }
                let f = self.decl_ref(d);
                let args = self.call_args(c, None, &|t| t.clone());
                call(f, args)
            }
        }
    }

    /// Picks the initializer of `inits` that the call's argument labels fit:
    /// every label is a parameter label and every required parameter is
    /// given. Falls back to the first initializer.
    ///
    /// # Panics
    ///
    /// If `inits` is empty and none fits; callers pass at least one.
    fn select_init(&self, inits: &[MemberId], c: &ast::CallExpr) -> MemberId {
        let mut labels: Vec<Option<String>> = Vec::new();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                labels.push(a.label().map(|t| t.text().to_owned()));
            }
        }
        for &i in inits {
            let sig = self.g.view.member(i).signature.as_ref();
            let Some(sig) = sig else { continue };
            let required: Vec<Option<String>> = sig
                .params
                .iter()
                .filter(|p| !p.has_default)
                .map(|p| p.label.clone())
                .collect();
            let all: Vec<Option<String>> = sig.params.iter().map(|p| p.label.clone()).collect();
            if labels.iter().all(|l| all.contains(l)) && required.iter().all(|r| labels.contains(r))
            {
                return i;
            }
        }
        inits[0]
    }

    /// Calls a static member: an enum case constructor, a static function,
    /// an init.
    fn call_static(&mut self, m: MemberId, c: &ast::CallExpr, result_ty: &Type) -> Expr {
        let info = self.g.view.member(m).clone();
        let owner = self.g.view.decl(info.owner).clone();
        if info.kind == MemberKind::Generated && crate::app::is_generated_enum(self.g, info.owner) {
            // A page value: the page's parameters in declaration order.
            let page = crate::app::pages(self.g)
                .into_iter()
                .find(|&p| crate::app::case_name(&self.g.view.decl(p).name) == info.name);
            let sig = page.map(|p| {
                let fields: Vec<MemberId> = crate::app::params(self.g, p)
                    .into_iter()
                    .map(|(m, _, _, _)| m)
                    .collect();
                memberwise_sig(self.g, p, &fields)
            });
            let args = self.call_args(c, sig.as_ref(), &|t| t.clone());
            let o = self.decl_ref(info.owner);
            return call(member(o, info.name.clone()), args);
        }
        if owner.stdlib {
            if let Some(e) = crate::native::static_call(self, m, c, result_ty) {
                return e;
            }
        }
        let sig = info.signature.clone();
        let mut args = Vec::new();
        if info.kind != MemberKind::Case && sig.as_ref().is_some_and(|s| s.is_async) {
            args.push(self.c_expr());
        }
        if info.kind != MemberKind::Case {
            let subst = sig
                .as_ref()
                .map(|s| self.infer_subst(s, c, result_ty))
                .unwrap_or_default();
            if let Some(s) = &sig {
                for g in &s.generics {
                    let ws = self.witnesses_for_generic(*g, subst.get(g));
                    args.extend(ws);
                }
            }
            let st = subst.clone();
            args.extend(self.call_args(c, sig.as_ref(), &move |t| subst_map(t, &st)));
        } else {
            let rt_ty = result_ty.clone();
            let this = self as *const Self;
            let _ = this;
            let owner_generics = owner.generics.clone();
            let args_ty = rt_ty.unwrap_optional().args().to_vec();
            args.extend(self.call_args(c, sig.as_ref(), &move |t| {
                subst(t, &|g| {
                    owner_generics
                        .iter()
                        .position(|x| *x == g)
                        .and_then(|i| args_ty.get(i).cloned())
                })
            }));
        }
        if info.kind == MemberKind::Init {
            if matches!(owner.named_kind(), Some(DeclarationKind::Class)) {
                let cls = self.decl_ref(info.owner);
                return Expr::New(Box::new(cls), args);
            }
            let f = self.fn_ref(m);
            return call(f, args);
        }
        let f = self.static_ref(m);
        call(f, args)
    }

    /// Calls instance method `m` on `recv` (of type `base`).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_method(
        &mut self,
        recv: Expr,
        base: &Type,
        m: MemberId,
        c: &ast::CallExpr,
        result_ty: &Type,
    ) -> Expr {
        let info = self.g.view.member(m).clone();
        let owner = self.g.view.decl(info.owner).clone();
        // A service may keep a function value it is given, or call it
        // later: a component's are bound to its lifetime.
        let bind = owner.named_kind() == Some(DeclarationKind::Service);
        if owner.stdlib {
            self.bind_args = bind;
            let native = crate::native::method(self, recv.clone(), base, m, c, result_ty);
            self.bind_args = false;
            if let Some(e) = native {
                return e;
            }
        }
        // `.product(id: 1).tab(.reviews)`: a page's query parameter chained on
        // a `Page` value is a copy with that field set.
        if info.kind == MemberKind::Field
            && info.has_annotation("Param")
            && owner.named_kind() == Some(DeclarationKind::Page)
        {
            let value = c
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())
                .map_or(Expr::Null, |v| {
                    let x = self.expr(&v);
                    let from = self.ty(v.syntax());
                    self.coerce(x, &from, info.ty.as_ref().unwrap_or(&Type::Error))
                });
            return Expr::Object(
                vec![Prop::Spread(recv), Prop::KeyValue(info.name.clone(), value)],
                false,
            );
        }
        let sig = info.signature.clone();
        let mut args = Vec::new();
        if sig.as_ref().is_some_and(|s| s.is_async) {
            args.push(self.c_expr());
        }
        let inferred = sig
            .as_ref()
            .map(|s| self.infer_subst(s, c, result_ty))
            .unwrap_or_default();
        if let Some(s) = &sig {
            for g in &s.generics {
                let ws = self.witnesses_for_generic(*g, inferred.get(g));
                args.extend(ws);
            }
        }
        let base2 = base.clone();
        let this_owner_generics = self.g.view.decl(info.owner).generics.clone();
        let st = inferred.clone();
        // `form.handle(_:)` calls its action with no cancellation context
        // (forms.js), so an inline action is bound to its owner.
        self.own_args =
            info.name == "handle" && self.g.view.qualified_name(info.owner) == "k.validation.Form";
        self.bind_args = bind;
        let user_args = self.call_args(c, sig.as_ref(), &move |t| {
            let t = subst_map(t, &st);
            subst(&t, &|g| {
                this_owner_generics
                    .iter()
                    .position(|x| *x == g)
                    .and_then(|i| base2.args().get(i).cloned())
            })
        });
        self.own_args = false;
        self.bind_args = false;
        if info.kind == MemberKind::Generated && info.name == "copy" {
            return self.copy_call(recv, base, c);
        }
        // An interface member called on a struct or enum value: its
        // implementation, or the interface's default through a box.
        if owner.named_kind() == Some(DeclarationKind::Interface)
            && self.g.is_value_type(base)
            && !self.g.is_interface(base)
        {
            if let Some(d) = base.decl() {
                let implementation = self.g.find_member(d, &info.name).filter(|&x| {
                    let xi = self.g.view.member(x);
                    xi.owner != info.owner
                        && matches!(xi.kind, MemberKind::Func | MemberKind::Computed)
                });
                let mut a = args;
                a.extend(user_args);
                if let Some(imp) = implementation {
                    let f = self.fn_ref(imp);
                    let mut all = vec![recv];
                    all.extend(a);
                    return call(f, all);
                }
                let boxed = self.coerce(recv, base, &Type::Named(info.owner, Vec::new()));
                let f = self.fn_ref(m);
                let mut all = vec![boxed];
                all.extend(a);
                return call(f, all);
            }
        }
        if info.kind == MemberKind::Field || info.kind == MemberKind::Computed {
            // A field of function type.
            let f = self.member_on(recv, m, &info);
            let mut a = args;
            a.extend(user_args);
            return call(f, a);
        }
        // A type parameter with an interface constraint: the witness.
        if let Type::Param(g) = base {
            if let Some(w) = self.witnesses.get(g).cloned() {
                let name = self.g.member_name(m);
                let mut a = vec![recv];
                a.extend(args);
                a.extend(user_args);
                return call(member(id(w), name), a);
            }
        }
        let owner_is_value = matches!(
            owner.named_kind(),
            Some(DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::Entity)
        ) || owner.kind == k_sema::view::DeclKind::Extension;
        let recv_is_interface = self.g.is_interface(base);
        if owner_is_value && !recv_is_interface {
            let f = self.fn_ref(m);
            let mut a = vec![recv];
            a.extend(args);
            a.extend(user_args);
            return call(f, a);
        }
        let name = self.g.member_name(m);
        let mut a = args;
        a.extend(user_args);
        call(member(recv, name), a)
    }

    /// `copy(field: value)`: a struct copy with the given fields replaced.
    fn copy_call(&mut self, recv: Expr, base: &Type, c: &ast::CallExpr) -> Expr {
        let mut props = vec![Prop::Spread(recv)];
        let fields = base
            .unwrap_optional()
            .decl()
            .map(|d| self.g.stored_fields(d))
            .unwrap_or_default();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                if let (Some(l), Some(v)) = (a.label(), a.value()) {
                    let ft = self.ty(v.syntax());
                    let x = self.expr(&v);
                    let target = fields
                        .iter()
                        .find(|&&f| self.g.view.member(f).name == l.text())
                        .and_then(|&f| self.g.view.member(f).ty.clone())
                        .unwrap_or(Type::Error);
                    let x = self.coerce(x, &ft, &target);
                    props.push(Prop::KeyValue(l.text().to_owned(), x));
                }
            }
        }
        Expr::Object(props, false)
    }

    /// Infers the substitution of a signature's own generics from the
    /// recorded argument and result types.
    pub(crate) fn infer_subst(
        &self,
        sig: &Signature,
        c: &ast::CallExpr,
        result_ty: &Type,
    ) -> HashMap<GenId, Type> {
        let mut out = HashMap::new();
        if sig.generics.is_empty() {
            return out;
        }
        // The call's type first: it is the checker's instantiation, which
        // an argument converts to (`same(A.x)` as an `Error` binds `E` to
        // `Error`, so `A.x` is boxed).
        unify(&sig.ret, result_ty, &mut out);
        let mut given: Vec<(Option<String>, ast::Expr)> = Vec::new();
        if let Some(list) = c.arg_list() {
            for a in list.args() {
                if let Some(v) = a.value() {
                    given.push((a.label().map(|t| t.text().to_owned()), v));
                }
            }
        }
        for (i, p) in sig.params.iter().enumerate() {
            let arg = given
                .iter()
                .find(|(l, _)| l.is_some() && l.as_deref() == p.label.as_deref())
                .or_else(|| {
                    if p.label.is_none() {
                        given.get(i)
                    } else {
                        None
                    }
                })
                .map(|(_, e)| e.clone());
            if let Some(a) = arg {
                let at = self.ty(a.syntax());
                unify(&p.ty, &at, &mut out);
            }
        }
        out
    }

    /// The witness arguments for generic `g` instantiated at `t`, in the
    /// order of [`crate::decl::params`]: the one its constraints need
    /// (Codable, interfaces with methods), then its error witness when it
    /// is constrained to `Error`.
    pub(crate) fn witnesses_for_generic(&mut self, g: GenId, t: Option<&Type>) -> Vec<Expr> {
        let mut out: Vec<Expr> = self.witness_for_generic(g, t).into_iter().collect();
        if self.is_error_generic(g) {
            out.push(self.error_witness(t));
        }
        out
    }

    /// `true` for a generic parameter constrained to `Error`, which takes
    /// an error witness (`$e_E`, codegen.md §7).
    pub(crate) fn is_error_generic(&self, g: GenId) -> bool {
        self.g.view.generics.get(g.0 as usize).is_some_and(|info| {
            info.constraints
                .iter()
                .any(|c| self.g.is(c, self.g.b.error))
        })
    }

    /// The error witness for a generic parameter constrained to `Error`
    /// instantiated at `t`: the text function of a concrete type, `null`
    /// for `Error` itself (its values are boxes already) or an unknown
    /// type, and the caller's own witness for one of its parameters.
    fn error_witness(&mut self, t: Option<&Type>) -> Expr {
        match t {
            Some(Type::Param(h)) => self
                .error_witnesses
                .get(h)
                .map_or(Expr::Null, |w| id(w.clone())),
            Some(t) if t.decl().is_some() && !self.g.is(t, self.g.b.error) => self.text_fn(t),
            _ => Expr::Null,
        }
    }

    /// The witness argument for generic `g` instantiated at `t`, when its
    /// constraints need one (Codable, interfaces with methods).
    pub(crate) fn witness_for_generic(&mut self, g: GenId, t: Option<&Type>) -> Option<Expr> {
        let constraints = self.g.view.generics.get(g.0 as usize)?.constraints.clone();
        let codable = constraints.iter().any(|c| self.g.is(c, self.g.b.codable));
        if codable {
            let t = t.cloned().unwrap_or(Type::Error);
            return Some(crate::codec::codec_expr(self, &t));
        }
        for c in &constraints {
            let Some(i) = c.decl() else { continue };
            if self.needs_witness(i) {
                let t = t.cloned().unwrap_or(Type::Error);
                return Some(self.method_table(&t, i));
            }
        }
        None
    }

    /// `true` for interfaces whose operations need a method table
    /// (user interfaces with functions).
    pub(crate) fn needs_witness(&self, iface: DeclId) -> bool {
        let d = self.g.view.decl(iface);
        !d.stdlib && d.named_kind() == Some(DeclarationKind::Interface)
    }

    /// The method table of type `t` for interface `iface`.
    pub(crate) fn method_table(&mut self, t: &Type, iface: DeclId) -> Expr {
        if let Type::Param(g) = t {
            if let Some(w) = self.witnesses.get(g) {
                return id(w.clone());
            }
        }
        match t.decl() {
            Some(d) if self.g.is_value_type(t) => {
                let n = format!("{}$w${}", self.g.name(d), self.g.name(iface));
                self.use_decl(d, n)
            }
            _ => {
                // Classes and existentials dispatch through their methods.
                let n = format!("{}$dyn", self.g.name(iface));
                self.use_decl(iface, n)
            }
        }
    }
}

// ------------------------------------------------------------------ utils

/// A lowered match arm: its test, bindings, guard (statements and test)
/// and body.
type MatchArmCode = (
    Option<Expr>,
    Vec<Stmt>,
    Option<(Vec<Stmt>, Expr)>,
    Vec<Stmt>,
);

/// The memberwise-constructor signature of a struct or class.
pub(crate) fn memberwise_sig(g: &Gen<'_>, _d: DeclId, fields: &[MemberId]) -> Signature {
    Signature {
        generics: Vec::new(),
        params: fields
            .iter()
            .map(|&f| {
                let m = g.view.member(f);
                k_sema::view::Param {
                    label: Some(m.name.clone()),
                    name: m.name.clone(),
                    ty: m.ty.clone().unwrap_or(Type::Error),
                    has_default: m.has_default,
                    name_range: m.name_range,
                }
            })
            .collect(),
        is_async: false,
        throws: None,
        ret: Type::Error,
    }
}

/// Substitutes generic parameters through `f`.
pub(crate) fn subst(t: &Type, f: &dyn Fn(GenId) -> Option<Type>) -> Type {
    match t {
        Type::Param(g) => f(*g).unwrap_or_else(|| t.clone()),
        Type::Named(d, args) => Type::Named(*d, args.iter().map(|a| subst(a, f)).collect()),
        Type::Optional(i) => Type::Optional(Box::new(subst(i, f))),
        Type::Tuple(items) => Type::Tuple(items.iter().map(|a| subst(a, f)).collect()),
        Type::Func(ft) => Type::Func(Box::new(k_sema::view::FuncType {
            params: ft.params.iter().map(|a| subst(a, f)).collect(),
            is_async: ft.is_async,
            throws: ft.throws.as_ref().map(|a| subst(a, f)),
            ret: subst(&ft.ret, f),
        })),
        Type::Meta(i) => Type::Meta(Box::new(subst(i, f))),
        other => other.clone(),
    }
}

/// `t` with each generic parameter bound in `m` replaced by its binding.
fn subst_map(t: &Type, m: &HashMap<GenId, Type>) -> Type {
    subst(t, &|g| m.get(&g).cloned())
}

/// Binds the generic parameters of `pattern` by matching it against
/// `actual`.
pub(crate) fn unify(pattern: &Type, actual: &Type, out: &mut HashMap<GenId, Type>) {
    match (pattern, actual) {
        (Type::Param(g), a) if !matches!(a, Type::Error | Type::Never) => {
            out.entry(*g).or_insert_with(|| a.clone());
        }
        (Type::Named(d1, a1), Type::Named(d2, a2)) if d1 == d2 => {
            for (x, y) in a1.iter().zip(a2) {
                unify(x, y, out);
            }
        }
        (Type::Optional(x), Type::Optional(y)) => unify(x, y, out),
        (Type::Optional(x), y) => unify(x, y, out),
        (Type::Tuple(x), Type::Tuple(y)) => {
            for (a, b) in x.iter().zip(y) {
                unify(a, b, out);
            }
        }
        (Type::Func(f), Type::Func(g)) => {
            for (a, b) in f.params.iter().zip(&g.params) {
                unify(a, b, out);
            }
            unify(&f.ret, &g.ret, out);
        }
        _ => {}
    }
}

/// The bound name and the value of a `let` condition (`if let x = e`);
/// the value is `None` for the shorthand `if let x`.
fn let_condition(c: &SyntaxNode) -> (Option<SyntaxToken>, Option<ast::Expr>) {
    let name = c
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident);
    let value = c.children().find_map(ast::Expr::cast);
    (name, value)
}

/// `true` for the expression `self`.
fn is_self(e: &ast::Expr) -> bool {
    matches!(e, ast::Expr::NameExpr(n) if n.name().is_some_and(|t| t.kind() == SyntaxKind::SelfKw))
}

/// `true` for an expression covered by `await` whose value isn't itself
/// the awaited call: an operator expression (`a() == b`, `not a()`), a
/// parenthesized one, or a member or element of a call (`a()?.x`,
/// `a()[0]`), whose calls are each awaited in place.
fn covers_compound(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::BinaryExpr(_) | ast::Expr::PrefixExpr(_) | ast::Expr::ParenExpr(_) => true,
        // `await point()?.x`: the member of an awaited call.
        ast::Expr::MemberExpr(m) => m
            .receiver()
            .is_some_and(|r| matches!(r, ast::Expr::CallExpr(_)) || covers_compound(&r)),
        // `await items()[0]`: an element of one.
        ast::Expr::IndexExpr(i) => index_base(i)
            .is_some_and(|r| matches!(r, ast::Expr::CallExpr(_)) || covers_compound(&r)),
        _ => false,
    }
}

/// The outermost link of the chain `node` is part of.
fn chain_top(node: &SyntaxNode) -> SyntaxNode {
    let mut top = node.clone();
    while let Some(p) = chain_parent(&top) {
        top = p;
    }
    top
}

/// The member, call or index expression that continues the chain at `node`
/// (`node` is its receiver, callee or base), if any.
fn chain_parent(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.parent().filter(|p| {
        matches!(
            p.kind(),
            SyntaxKind::MemberExpr | SyntaxKind::CallExpr | SyntaxKind::IndexExpr
        ) && p.first_child().as_ref() == Some(node)
    })
}

/// The indexed expression of `base[index]`.
fn index_base(i: &ast::IndexExpr) -> Option<ast::Expr> {
    i.syntax().children().find_map(ast::Expr::cast)
}

/// `e` without any enclosing `await` and `try` markers.
fn strip_await(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::AwaitExpr(a) => a
            .syntax()
            .children()
            .find_map(ast::Expr::cast)
            .map_or_else(|| e.clone(), |i| strip_await(&i)),
        ast::Expr::TryExpr(t) => t
            .syntax()
            .children()
            .find_map(ast::Expr::cast)
            .map_or_else(|| e.clone(), |i| strip_await(&i)),
        other => other.clone(),
    }
}

/// `true` for awaits of runtime primitives that honour `$c` themselves.
fn is_guarded_primitive(e: &Expr) -> bool {
    matches!(e, Expr::Call(f, _) if matches!(&**f, Expr::Member(o, n) if matches!(&**o, Expr::Ident(k) if k == "$k") && (n == "rpc" || n == "sleep")))
}

/// `text` trimmed, with every run of whitespace replaced by one space.
pub(crate) fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `if ($k.isPanic(e)) throw e;`: the first statement of a `catch (e)`,
/// so a panic is never handled as a typed error.
pub(crate) fn rethrow_panics() -> Stmt {
    rethrow("e")
}

/// An `Error` value (codegen.md §7): `{ error, text }`, the thrown value
/// and its interpolation text, since an enum value doesn't carry its enum.
fn error_box(error: Expr, text: Expr) -> Expr {
    Expr::Object(
        vec![
            Prop::KeyValue("error".into(), error),
            Prop::KeyValue("text".into(), text),
        ],
        false,
    )
}

/// [`rethrow_panics`] for a catch variable named `e`.
fn rethrow(e: &str) -> Stmt {
    Stmt::If(rt("isPanic", vec![id(e)]), vec![Stmt::Throw(id(e))], None)
}

/// Prepends `flag = true;` to a catch clause's body (inside its guard when
/// the body is a single `if`), so later clauses know the error was
/// handled.
fn mark_handled(body: &mut Vec<Stmt>, flag: &str) {
    let set = Stmt::Expr(js::assign(id(flag), Expr::Bool(true)));
    if let [only] = body.as_mut_slice()
        && let Stmt::If(_, inner, _) = only.peel_mut()
    {
        inner.insert(0, set);
    } else {
        body.insert(0, set);
    }
}

/// `true` when the statements end in a jump (no fall-through).
pub(crate) fn ends_flow(stmts: &[Stmt]) -> bool {
    match stmts.last().map(Stmt::peel) {
        Some(Stmt::Return(_) | Stmt::Throw(_) | Stmt::Break(_) | Stmt::Continue(_)) => true,
        Some(Stmt::If(_, a, Some(b))) => ends_flow(a) && ends_flow(b),
        Some(Stmt::Block(b)) => ends_flow(b),
        Some(Stmt::Expr(Expr::Call(f, _))) => {
            matches!(&**f, Expr::Member(o, n) if matches!(&**o, Expr::Ident(k) if k == "$k") && n == "unreachable")
        }
        _ => false,
    }
}

/// `s` with its first character upper-cased (`add` → `Add`).
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Formats a number like CSS: no trailing `.0`.
pub(crate) fn fmt_num(n: f64) -> String {
    let mut s = String::new();
    js::print_number(&mut s, n);
    s
}

/// Splits CSS text `16px` into number and unit.
fn split_unit(s: &str) -> Option<(f64, &str)> {
    let i = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    let (n, u) = s.split_at(i);
    Some((n.parse().ok()?, u))
}

/// Folds constant CSS arithmetic (codegen.md §6.5).
pub(crate) fn fold_css(op: &str, l: &str, r: &str) -> String {
    let lu = split_unit(l);
    let ru = split_unit(r);
    let ln: Option<f64> = l.parse().ok();
    let rn: Option<f64> = r.parse().ok();
    match (op, lu, ru, ln, rn) {
        ("*", Some((a, u)), None, _, Some(b)) => return format!("{}{u}", fmt_num(a * b)),
        ("*", None, Some((b, u)), Some(a), _) => return format!("{}{u}", fmt_num(a * b)),
        ("/", Some((a, u)), None, _, Some(b)) if b != 0.0 => {
            return format!("{}{u}", fmt_num(a / b));
        }
        ("+" | "-", Some((a, u1)), Some((b, u2)), _, _) if u1 == u2 => {
            let v = if op == "+" { a + b } else { a - b };
            return format!("{}{u1}", fmt_num(v));
        }
        _ => {}
    }
    let strip = |s: &str| -> String {
        s.strip_prefix("calc(")
            .and_then(|x| x.strip_suffix(')'))
            .map_or_else(|| s.to_owned(), |x| x.to_owned())
    };
    let r_inner = if op == "-" && r.starts_with("calc(") {
        format!("({})", strip(r))
    } else {
        strip(r)
    };
    format!("calc({} {op} {})", strip(l), r_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_constants_fold() {
        assert_eq!(fold_css("*", "16px", "2"), "32px");
        assert_eq!(fold_css("+", "100px", "2rem"), "calc(100px + 2rem)");
        assert_eq!(
            fold_css("+", "calc(1px + 2rem)", "3em"),
            "calc(1px + 2rem + 3em)"
        );
        assert_eq!(fold_css("+", "4px", "6px"), "10px");
    }
}
