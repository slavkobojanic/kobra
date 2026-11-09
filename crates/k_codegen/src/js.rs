//! The JavaScript IR: a small AST of the ES2022 subset generated code uses,
//! and its printer (`docs/compiler/codegen.md` §3, §17).
//!
//! Lowering builds this IR and never JavaScript text, so escaping happens
//! in exactly one place: [`print_string`] and [`print_template_text`].
//! String literals are printed with `"` and JSON-compatible escapes, plus
//! `\u2028`/`\u2029` escaped, and `<!--` and `</script` broken up so the
//! output is safe to inline in HTML.
//!
//! The printer is precedence-aware: it adds parentheses only where the
//! tree needs them, so the output reads like hand-written code.

use std::fmt::Write as _;

/// A JavaScript expression of the generated subset.
#[derive(Clone, PartialEq, Debug)]
pub enum Expr {
    /// An identifier (already mangled).
    Ident(String),
    /// A number literal.
    Num(f64),
    /// A string literal.
    Str(String),
    /// A string literal of compiler-generated HTML (templates, `out.raw`):
    /// printed like [`Expr::Str`] but keeping `<!--` and `</`, which the
    /// markup needs; user text inside it was escaped at build time.
    Html(String),
    /// `true` / `false`.
    Bool(bool),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// `this`.
    This,
    /// A template literal: static text and expressions, alternating as given.
    Template(Vec<TemplatePart>),
    /// `[a, b]`.
    Array(Vec<Expr>),
    /// `...e` (inside arrays, objects and calls).
    Spread(Box<Expr>),
    /// `{ k: v }`; printed on several lines when `multiline`.
    Object(Vec<Prop>, bool),
    /// `e.name`.
    Member(Box<Expr>, String),
    /// `e?.name`.
    OptMember(Box<Expr>, String),
    /// `e[i]`.
    Index(Box<Expr>, Box<Expr>),
    /// `f(args)`.
    Call(Box<Expr>, Vec<Expr>),
    /// `f?.(args)`.
    OptCall(Box<Expr>, Vec<Expr>),
    /// `new C(args)`.
    New(Box<Expr>, Vec<Expr>),
    /// A prefix operator: `!`, `-`, `typeof `, `void `.
    Unary(&'static str, Box<Expr>),
    /// A binary operator.
    Binary(&'static str, Box<Expr>, Box<Expr>),
    /// `c ? a : b`.
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    /// An arrow function.
    Arrow(Box<Function>),
    /// `place op value` (`=`, `+=`, `??=`).
    Assign(&'static str, Box<Expr>, Box<Expr>),
    /// `await e`.
    Await(Box<Expr>),
    /// `import(e)`.
    Import(Box<Expr>),
}

/// One part of a template literal.
#[derive(Clone, PartialEq, Debug)]
pub enum TemplatePart {
    /// Static text (escaped by the printer).
    Text(String),
    /// `${e}`.
    Expr(Expr),
}

/// One property of an object literal.
#[derive(Clone, PartialEq, Debug)]
pub enum Prop {
    /// `key: value`; printed as shorthand when the value is the identifier
    /// `key`.
    KeyValue(String, Expr),
    /// `"key": value`, the key always quoted: a JSON wire key (an
    /// encoder's field), which no property-renaming minifier may touch
    /// (codegen.md §17.3).
    Quoted(String, Expr),
    /// `...e`.
    Spread(Expr),
}

/// A function, method or arrow function.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Function {
    /// The name (empty for arrows and anonymous functions).
    pub name: String,
    /// The parameters, in order.
    pub params: Vec<Param>,
    /// The body: a block, or an arrow's expression.
    pub body: Body,
    /// `true` for an `async` function.
    pub is_async: bool,
    /// For a class method: a plain method, a getter, a static method or
    /// the constructor; [`MethodKind::Plain`] for everything else.
    pub kind: MethodKind,
}

/// What kind of method a [`Function`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MethodKind {
    /// A plain method or function.
    #[default]
    Plain,
    /// `get name()`.
    Getter,
    /// `static name()`.
    Static,
    /// `constructor()`.
    Constructor,
}

/// The body of a [`Function`]: a block, or an arrow's single expression.
#[derive(Clone, PartialEq, Debug)]
pub enum Body {
    /// `{ statements }`.
    Block(Vec<Stmt>),
    /// An arrow's expression body.
    Expr(Box<Expr>),
}

impl Default for Body {
    fn default() -> Body {
        Body::Block(Vec::new())
    }
}

/// One parameter of a [`Function`]: a name with an optional default, or a
/// rest parameter.
#[derive(Clone, PartialEq, Debug)]
pub struct Param {
    /// The name, or a destructuring pattern printed as is (generated only).
    pub name: String,
    /// The default value.
    pub default: Option<Expr>,
    /// `...name`.
    pub rest: bool,
}

impl Param {
    /// A plain parameter.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Param {
        Param {
            name: name.into(),
            default: None,
            rest: false,
        }
    }
}

/// A class declaration: its name, its `extends` clause and its methods.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Class {
    /// The class name, already mangled.
    pub name: String,
    /// The superclass expression of `extends`, if any.
    pub extends: Option<Expr>,
    /// Methods (constructor first by convention).
    pub methods: Vec<Function>,
}

/// One `case` of a `switch`.
#[derive(Clone, PartialEq, Debug)]
pub struct Case {
    /// The labels; empty for `default`.
    pub tests: Vec<Expr>,
    /// The statements that run for the case.
    pub body: Vec<Stmt>,
}

/// A JavaScript statement of the generated subset, including the
/// module-level `import` and `export` forms.
#[derive(Clone, PartialEq, Debug)]
pub enum Stmt {
    /// An expression statement.
    Expr(Expr),
    /// `const name = value;`.
    Const(String, Expr),
    /// `let name = value;` / `let name;`.
    Let(String, Option<Expr>),
    /// `return e;`.
    Return(Option<Expr>),
    /// `throw e;`.
    Throw(Expr),
    /// `if (c) { … } else { … }`; an `else` holding one `If` prints as
    /// `else if`.
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    /// `switch (e) { … }`.
    Switch(Expr, Vec<Case>),
    /// `for (init; test; update) { … }`.
    For(Box<Stmt>, Expr, Expr, Vec<Stmt>),
    /// `for (const name of e) { … }` (`await` for `for await`).
    ForOf(String, Expr, Vec<Stmt>, bool),
    /// `while (c) { … }`.
    While(Expr, Vec<Stmt>),
    /// `break;` / `break label;`.
    Break(Option<String>),
    /// `continue;` / `continue label;`.
    Continue(Option<String>),
    /// `label: stmt`.
    Labeled(String, Box<Stmt>),
    /// `try { … } catch (name) { … }`.
    Try(Vec<Stmt>, String, Vec<Stmt>),
    /// `{ … }`.
    Block(Vec<Stmt>),
    /// A function declaration; `export` when the flag is set.
    Function(Function, bool),
    /// A class declaration; `export` when the flag is set.
    Class(Class, bool),
    /// `export const name = value;`.
    ExportConst(String, Expr),
    /// `import * as name from "spec";`.
    ImportAll(String, String),
    /// `import { a, b as c } from "spec";`.
    Import(Vec<(String, String)>, String),
}

// ------------------------------------------------------------ constructors

/// Builds the identifier `name`, which must already be mangled.
#[must_use]
pub fn id(name: impl Into<String>) -> Expr {
    Expr::Ident(name.into())
}

/// Builds the string literal `"text"`; escaping happens when it is
/// printed.
#[must_use]
pub fn str(text: impl Into<String>) -> Expr {
    Expr::Str(text.into())
}

/// Builds the member access `e.name`.
#[must_use]
pub fn member(e: Expr, name: impl Into<String>) -> Expr {
    Expr::Member(Box::new(e), name.into())
}

/// Builds the call `f(args)`.
#[must_use]
pub fn call(f: Expr, args: Vec<Expr>) -> Expr {
    Expr::Call(Box::new(f), args)
}

/// Builds a call of the runtime function `name`, `$k.name(args)`
/// (generated modules import the runtime as `$k`).
#[must_use]
pub fn rt(name: &str, args: Vec<Expr>) -> Expr {
    call(member(id("$k"), name), args)
}

/// Builds the binary expression `a op b`; the printer parenthesizes the
/// operands as precedence requires.
#[must_use]
pub fn bin(op: &'static str, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

/// Builds the prefix expression `op e` (`!`, `-`, `typeof`, …).
#[must_use]
pub fn un(op: &'static str, e: Expr) -> Expr {
    Expr::Unary(op, Box::new(e))
}

/// Builds the assignment `place = value`.
#[must_use]
pub fn assign(place: Expr, value: Expr) -> Expr {
    Expr::Assign("=", Box::new(place), Box::new(value))
}

/// Builds a synchronous arrow function with an expression body,
/// `(params) => body`.
#[must_use]
pub fn arrow(params: Vec<Param>, body: Expr) -> Expr {
    Expr::Arrow(Box::new(Function {
        params,
        body: Body::Expr(Box::new(body)),
        ..Function::default()
    }))
}

/// Builds an arrow function with a block body, `async` when `is_async`.
#[must_use]
pub fn arrow_block(params: Vec<Param>, body: Vec<Stmt>, is_async: bool) -> Expr {
    Expr::Arrow(Box::new(Function {
        params,
        body: Body::Block(body),
        is_async,
        ..Function::default()
    }))
}

// ---------------------------------------------------------------- walking

/// Calls `f` on every expression of `stmts`, an expression before the
/// expressions inside it, and descends into nested statements, functions,
/// classes, object properties, template parts and parameter defaults. `f`
/// may rewrite the expression it is given; the walk then descends into the
/// result. Linear in the size of the tree.
pub fn walk_exprs_mut(stmts: &mut [Stmt], f: &mut impl FnMut(&mut Expr)) {
    for s in stmts {
        walk_stmt(s, f);
    }
}

/// [`walk_exprs_mut`] for one statement.
fn walk_stmt(s: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    match s {
        Stmt::Expr(e) | Stmt::Const(_, e) | Stmt::Throw(e) | Stmt::ExportConst(_, e) => {
            walk_expr(e, f);
        }
        Stmt::Let(_, e) | Stmt::Return(e) => {
            if let Some(e) = e {
                walk_expr(e, f);
            }
        }
        Stmt::If(c, then, otherwise) => {
            walk_expr(c, f);
            walk_exprs_mut(then, f);
            if let Some(otherwise) = otherwise {
                walk_exprs_mut(otherwise, f);
            }
        }
        Stmt::Switch(e, cases) => {
            walk_expr(e, f);
            for case in cases {
                for t in &mut case.tests {
                    walk_expr(t, f);
                }
                walk_exprs_mut(&mut case.body, f);
            }
        }
        Stmt::For(init, test, update, body) => {
            walk_stmt(init, f);
            walk_expr(test, f);
            walk_expr(update, f);
            walk_exprs_mut(body, f);
        }
        Stmt::ForOf(_, e, body, _) | Stmt::While(e, body) => {
            walk_expr(e, f);
            walk_exprs_mut(body, f);
        }
        Stmt::Labeled(_, s) => walk_stmt(s, f),
        Stmt::Try(body, _, handler) => {
            walk_exprs_mut(body, f);
            walk_exprs_mut(handler, f);
        }
        Stmt::Block(body) => walk_exprs_mut(body, f),
        Stmt::Function(func, _) => walk_function(func, f),
        Stmt::Class(class, _) => walk_class(class, f),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::ImportAll(..) | Stmt::Import(..) => {}
    }
}

/// [`walk_exprs_mut`] for a function: its parameter defaults and body.
fn walk_function(func: &mut Function, f: &mut impl FnMut(&mut Expr)) {
    for p in &mut func.params {
        if let Some(d) = &mut p.default {
            walk_expr(d, f);
        }
    }
    match &mut func.body {
        Body::Block(body) => walk_exprs_mut(body, f),
        Body::Expr(e) => walk_expr(e, f),
    }
}

/// [`walk_exprs_mut`] for a class: its `extends` clause and methods.
fn walk_class(class: &mut Class, f: &mut impl FnMut(&mut Expr)) {
    if let Some(e) = &mut class.extends {
        walk_expr(e, f);
    }
    for m in &mut class.methods {
        walk_function(m, f);
    }
}

/// [`walk_exprs_mut`] for one expression: `e` itself, then what is inside.
fn walk_expr(e: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    f(e);
    match e {
        Expr::Ident(_)
        | Expr::Num(_)
        | Expr::Str(_)
        | Expr::Html(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Undefined
        | Expr::This => {}
        Expr::Template(parts) => {
            for p in parts {
                if let TemplatePart::Expr(e) = p {
                    walk_expr(e, f);
                }
            }
        }
        Expr::Array(es) => {
            for e in es {
                walk_expr(e, f);
            }
        }
        Expr::Object(props, _) => {
            for p in props {
                match p {
                    Prop::KeyValue(_, e) | Prop::Quoted(_, e) | Prop::Spread(e) => walk_expr(e, f),
                }
            }
        }
        Expr::Spread(e)
        | Expr::Member(e, _)
        | Expr::OptMember(e, _)
        | Expr::Unary(_, e)
        | Expr::Await(e)
        | Expr::Import(e) => walk_expr(e, f),
        Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Assign(_, a, b) => {
            walk_expr(a, f);
            walk_expr(b, f);
        }
        Expr::Call(callee, args) | Expr::OptCall(callee, args) | Expr::New(callee, args) => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        Expr::Cond(c, a, b) => {
            walk_expr(c, f);
            walk_expr(a, f);
            walk_expr(b, f);
        }
        Expr::Arrow(func) => walk_function(func, f),
    }
}

// --------------------------------------------------------------- printing

/// How [`print_module`] lays out its output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PrintOptions {
    /// Strip indentation, line breaks between statements and comments.
    pub compact: bool,
}

/// Prints a module (a list of top-level statements). Top-level functions,
/// classes and multi-line constants are separated by a blank line.
#[must_use]
pub fn print_module(stmts: &[Stmt], options: PrintOptions) -> String {
    let mut p = Printer {
        out: String::new(),
        indent: 0,
        compact: options.compact,
    };
    let mut prev: Option<&Stmt> = None;
    for s in stmts {
        if !p.compact {
            if let Some(prev) = prev {
                let imports = |s: &Stmt| matches!(s, Stmt::Import(..) | Stmt::ImportAll(..));
                let blank = !(imports(prev) && imports(s))
                    && !(is_simple_const(prev) && is_simple_const(s));
                if blank {
                    p.out.push('\n');
                }
            }
        }
        p.stmt(s);
        prev = Some(s);
    }
    p.out
}

/// `true` for a top-level `const` (exported or not) whose value is not a
/// multi-line object: [`print_module`] keeps runs of these together without
/// blank lines between them.
fn is_simple_const(s: &Stmt) -> bool {
    match s {
        Stmt::Const(_, e) | Stmt::ExportConst(_, e) => !matches!(e, Expr::Object(_, true)),
        _ => false,
    }
}

/// Prints one expression in readable form (tests only).
#[cfg(test)]
#[must_use]
pub fn print_expr(e: &Expr) -> String {
    let mut p = Printer {
        out: String::new(),
        indent: 0,
        compact: false,
    };
    p.expr(e, 0);
    p.out
}

/// The printer's state: the text so far, the indentation and the mode.
///
/// One printer prints one module or expression; its methods append to
/// [`Printer::out`] and never fail.
struct Printer {
    /// The text printed so far.
    out: String,
    /// The current indentation depth, two spaces per level (ignored when
    /// compact).
    indent: usize,
    /// `true` for release output: no indentation, line breaks or optional
    /// spaces.
    compact: bool,
}

// Precedence levels (higher binds tighter), after ECMAScript's operator
// precedence table.
/// The comma operator; the lowest level, used where any expression is
/// allowed (statements, parenthesized operands).
const P_SEQ: u8 = 1;
/// Assignment, arrow functions, spread and `yield`: the level of an
/// argument, an array element or an object value.
const P_ASSIGN: u8 = 2;
/// The conditional operator `c ? a : b`.
const P_COND: u8 = 3;
/// `??`.
const P_NULLISH: u8 = 4;
/// `||`.
const P_OR: u8 = 5;
/// `&&` (the bitwise `|`, `^`, `&` follow at 7, 8 and 9).
const P_AND: u8 = 6;
/// Equality: `===`, `!==`, `==`, `!=`.
const P_EQ: u8 = 10;
/// Relational: `<`, `<=`, `>`, `>=`, `instanceof`, `in` (the shifts follow
/// at 12).
const P_REL: u8 = 11;
/// Additive: `+`, `-`.
const P_ADD: u8 = 13;
/// Multiplicative: `*`, `/`, `%`.
const P_MUL: u8 = 14;
/// Exponentiation, `**` (right-associative).
const P_EXP: u8 = 15;
/// Prefix operators and `await`; also a negative number literal, which
/// prints with a leading `-`.
const P_UNARY: u8 = 16;
/// Postfix `++` and `--`.
const P_POSTFIX: u8 = 18;
/// Member access, calls, `new` and `import()`: the level of a callee or a
/// receiver.
const P_CALL: u8 = 19;
/// Literals, identifiers, `this`, templates, arrays and objects: never
/// parenthesized.
const P_PRIMARY: u8 = 20;

/// The precedence of the binary operator `op` (ECMAScript's table); an
/// unknown operator gets [`P_EQ`].
fn binary_prec(op: &str) -> u8 {
    match op {
        "??" => P_NULLISH,
        "||" => P_OR,
        "&&" => P_AND,
        "|" => 7,
        "^" => 8,
        "&" => 9,
        "===" | "!==" | "==" | "!=" => P_EQ,
        "<" | "<=" | ">" | ">=" | "instanceof" | "in" => P_REL,
        "<<" | ">>" | ">>>" => 12,
        "+" | "-" => P_ADD,
        "*" | "/" | "%" => P_MUL,
        "**" => P_EXP,
        _ => P_EQ,
    }
}

/// The precedence of `e` as an operand: the binding strength of its
/// outermost operator, or [`P_PRIMARY`] for a literal or name.
fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Num(n) if *n < 0.0 || (*n == 0.0 && n.is_sign_negative()) => P_UNARY,
        Expr::Ident(_)
        | Expr::Num(_)
        | Expr::Str(_)
        | Expr::Html(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Undefined
        | Expr::This
        | Expr::Template(_)
        | Expr::Array(_)
        | Expr::Object(..) => P_PRIMARY,
        Expr::Member(..)
        | Expr::OptMember(..)
        | Expr::Index(..)
        | Expr::Call(..)
        | Expr::OptCall(..)
        | Expr::New(..)
        | Expr::Import(_) => P_CALL,
        Expr::Unary(..) | Expr::Await(_) => P_UNARY,
        Expr::Binary(op, ..) => binary_prec(op),
        Expr::Cond(..) => P_COND,
        Expr::Arrow(_) | Expr::Assign(..) | Expr::Spread(_) => P_ASSIGN,
    }
}

impl Printer {
    /// Appends a line break, except in compact output.
    fn nl(&mut self) {
        if self.compact {
            return;
        }
        self.out.push('\n');
    }

    /// Appends the indentation of the current depth, except in compact output.
    fn pad(&mut self) {
        if self.compact {
            return;
        }
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
    }

    /// Appends an optional space, one that only readability needs; compact
    /// output leaves it out.
    fn sp(&mut self) {
        if !self.compact {
            self.out.push(' ');
        }
    }

    /// A binary operator between its operands: spaced in readable output,
    /// and in compact output only when it is a word (`instanceof`, `in`),
    /// which would otherwise run into the names around it.
    fn binary_op(&mut self, op: &str) {
        let word = op.bytes().all(|b| b.is_ascii_alphabetic());
        if word {
            self.out.push(' ');
        } else {
            self.sp();
        }
        self.out.push_str(op);
        if word {
            self.out.push(' ');
        } else {
            self.sp();
        }
    }

    /// Ends a statement's line, except in compact output.
    fn line_end(&mut self) {
        if self.compact {
            return;
        }
        self.out.push('\n');
    }

    /// Prints `{ … }` with `stmts` one level deeper; the caller has already
    /// printed what precedes the brace.
    fn block(&mut self, stmts: &[Stmt]) {
        self.out.push('{');
        self.nl();
        self.indent += 1;
        for s in stmts {
            self.stmt(s);
        }
        self.indent -= 1;
        self.pad();
        self.out.push('}');
    }

    /// Prints `s` on a line of its own at the current indentation.
    fn stmt(&mut self, s: &Stmt) {
        self.pad();
        self.stmt_inline(s);
        self.line_end();
    }

    /// Prints `s` without the leading indentation or the trailing line break,
    /// for [`Printer::stmt`] and for positions such as `else if` and a `for`
    /// initializer.
    ///
    /// An expression statement that would start with `{` or `function` is
    /// parenthesized ([`starts_ambiguously`]).
    #[allow(clippy::too_many_lines)]
    fn stmt_inline(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => {
                let needs_paren = starts_ambiguously(e);
                if needs_paren {
                    self.out.push('(');
                }
                self.expr(e, P_SEQ);
                if needs_paren {
                    self.out.push(')');
                }
                self.out.push(';');
            }
            Stmt::Const(name, e) => {
                let _ = write!(self.out, "const {name}");
                self.sp();
                self.out.push('=');
                self.sp();
                self.expr(e, P_ASSIGN);
                self.out.push(';');
            }
            Stmt::ExportConst(name, e) => {
                let _ = write!(self.out, "export const {name}");
                self.sp();
                self.out.push('=');
                self.sp();
                self.expr(e, P_ASSIGN);
                self.out.push(';');
            }
            Stmt::Let(name, e) => {
                let _ = write!(self.out, "let {name}");
                if let Some(e) = e {
                    self.sp();
                    self.out.push('=');
                    self.sp();
                    self.expr(e, P_ASSIGN);
                }
                self.out.push(';');
            }
            Stmt::Return(e) => {
                self.out.push_str("return");
                if let Some(e) = e {
                    self.out.push(' ');
                    self.expr(e, P_SEQ);
                }
                self.out.push(';');
            }
            Stmt::Throw(e) => {
                self.out.push_str("throw ");
                self.expr(e, P_SEQ);
                self.out.push(';');
            }
            Stmt::If(c, then, otherwise) => {
                self.out.push_str("if");
                self.sp();
                self.out.push('(');
                self.expr(c, P_SEQ);
                self.out.push(')');
                // `if (c) throw e;` — a lone rethrow stays on one line.
                if let ([Stmt::Throw(_)], None) = (then.as_slice(), otherwise) {
                    self.out.push(' ');
                    self.stmt_inline(&then[0]);
                    return;
                }
                self.sp();
                self.block(then);
                let mut otherwise = otherwise.as_ref();
                while let Some(o) = otherwise {
                    self.sp();
                    self.out.push_str("else");
                    if let [Stmt::If(c2, t2, o2)] = o.as_slice() {
                        self.out.push_str(" if");
                        self.sp();
                        self.out.push('(');
                        self.expr(c2, P_SEQ);
                        self.out.push(')');
                        self.sp();
                        self.block(t2);
                        otherwise = o2.as_ref();
                    } else {
                        self.sp();
                        self.block(o);
                        otherwise = None;
                    }
                }
            }
            Stmt::Switch(e, cases) => {
                self.out.push_str("switch");
                self.sp();
                self.out.push('(');
                self.expr(e, P_SEQ);
                self.out.push(')');
                self.sp();
                self.out.push('{');
                self.nl();
                self.indent += 1;
                for case in cases {
                    if case.tests.is_empty() {
                        self.pad();
                        self.out.push_str("default:");
                    } else {
                        for (i, t) in case.tests.iter().enumerate() {
                            if i > 0 {
                                self.nl();
                            }
                            self.pad();
                            self.out.push_str("case ");
                            self.expr(t, P_SEQ);
                            self.out.push(':');
                        }
                    }
                    let braced = case.body.len() != 1 || declares(&case.body[0]);
                    if braced {
                        self.sp();
                        self.block(&case.body);
                        self.nl();
                    } else {
                        self.nl();
                        self.indent += 1;
                        for s in &case.body {
                            self.stmt(s);
                        }
                        self.indent -= 1;
                    }
                }
                self.indent -= 1;
                self.pad();
                self.out.push('}');
            }
            Stmt::For(init, test, update, body) => {
                self.out.push_str("for");
                self.sp();
                self.out.push('(');
                self.stmt_inline(init);
                self.sp();
                self.expr(test, P_SEQ);
                self.out.push(';');
                self.sp();
                self.expr(update, P_SEQ);
                self.out.push(')');
                self.sp();
                self.block(body);
            }
            Stmt::ForOf(name, e, body, is_await) => {
                self.out.push_str("for");
                if *is_await {
                    self.out.push_str(" await");
                }
                self.sp();
                let _ = write!(self.out, "(const {name} of ");
                self.expr(e, P_ASSIGN);
                self.out.push(')');
                self.sp();
                self.block(body);
            }
            Stmt::While(c, body) => {
                self.out.push_str("while");
                self.sp();
                self.out.push('(');
                self.expr(c, P_SEQ);
                self.out.push(')');
                self.sp();
                self.block(body);
            }
            Stmt::Break(label) => match label {
                Some(l) => {
                    let _ = write!(self.out, "break {l};");
                }
                None => self.out.push_str("break;"),
            },
            Stmt::Continue(label) => match label {
                Some(l) => {
                    let _ = write!(self.out, "continue {l};");
                }
                None => self.out.push_str("continue;"),
            },
            Stmt::Labeled(label, s) => {
                let _ = write!(self.out, "{label}:");
                self.sp();
                self.stmt_inline(s);
            }
            Stmt::Try(body, name, handler) => {
                self.out.push_str("try");
                self.sp();
                self.block(body);
                self.sp();
                let _ = write!(self.out, "catch");
                self.sp();
                let _ = write!(self.out, "({name})");
                self.sp();
                self.block(handler);
            }
            Stmt::Block(stmts) => self.block(stmts),
            Stmt::Function(f, export) => {
                if *export {
                    self.out.push_str("export ");
                }
                self.function_decl(f);
            }
            Stmt::Class(c, export) => {
                if *export {
                    self.out.push_str("export ");
                }
                self.class(c);
            }
            Stmt::ImportAll(name, spec) => {
                let _ = write!(self.out, "import * as {name} from ");
                print_string(&mut self.out, spec);
                self.out.push(';');
            }
            Stmt::Import(names, spec) => {
                self.out.push_str("import");
                self.sp();
                self.out.push('{');
                self.sp();
                for (i, (name, alias)) in names.iter().enumerate() {
                    if i > 0 {
                        self.out.push(',');
                        self.sp();
                    }
                    self.out.push_str(name);
                    if name != alias {
                        let _ = write!(self.out, " as {alias}");
                    }
                }
                self.sp();
                self.out.push('}');
                self.sp();
                self.out.push_str("from");
                self.sp();
                print_string(&mut self.out, spec);
                self.out.push(';');
            }
        }
    }

    /// Prints a function declaration, `async function name(params) { … }`; an
    /// expression body is printed as a block that returns it.
    fn function_decl(&mut self, f: &Function) {
        if f.is_async {
            self.out.push_str("async ");
        }
        let _ = write!(self.out, "function {}", f.name);
        self.params(&f.params);
        self.sp();
        match &f.body {
            Body::Block(stmts) => self.block(stmts),
            Body::Expr(e) => self.block(&[Stmt::Return(Some((**e).clone()))]),
        }
    }

    /// Prints a class declaration with its `extends` clause and one method per
    /// line.
    fn class(&mut self, c: &Class) {
        let _ = write!(self.out, "class {}", c.name);
        if let Some(e) = &c.extends {
            self.out.push_str(" extends ");
            self.expr(e, P_CALL);
        }
        self.sp();
        self.out.push('{');
        self.nl();
        self.indent += 1;
        for m in &c.methods {
            self.pad();
            self.method(m);
            self.nl();
        }
        self.indent -= 1;
        self.pad();
        self.out.push('}');
    }

    /// Prints one class method: a getter, a static method, the constructor or a
    /// plain method, with `async` when set.
    fn method(&mut self, f: &Function) {
        match f.kind {
            MethodKind::Getter => self.out.push_str("get "),
            MethodKind::Static => self.out.push_str("static "),
            MethodKind::Plain | MethodKind::Constructor => {}
        }
        if f.is_async {
            self.out.push_str("async ");
        }
        if f.kind == MethodKind::Constructor {
            self.out.push_str("constructor");
        } else {
            self.out.push_str(&f.name);
        }
        self.params(&f.params);
        self.sp();
        match &f.body {
            Body::Block(stmts) => self.block(stmts),
            Body::Expr(e) => self.block(&[Stmt::Return(Some((**e).clone()))]),
        }
    }

    /// Prints a parenthesized parameter list, with `...` for a rest parameter
    /// and `= default` for a default value.
    fn params(&mut self, params: &[Param]) {
        self.out.push('(');
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                self.sp();
            }
            if p.rest {
                self.out.push_str("...");
            }
            self.out.push_str(&p.name);
            if let Some(d) = &p.default {
                self.sp();
                self.out.push('=');
                self.sp();
                self.expr(d, P_ASSIGN);
            }
        }
        self.out.push(')');
    }

    /// Prints a parenthesized argument list, each argument at [`P_ASSIGN`] so a
    /// comma expression is parenthesized.
    fn args(&mut self, args: &[Expr]) {
        self.out.push('(');
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                self.sp();
            }
            self.expr(a, P_ASSIGN);
        }
        self.out.push(')');
    }

    /// Prints `e` where an operand of at least precedence `min` is expected,
    /// wrapping it in parentheses when its own precedence ([`prec`]) is lower.
    #[allow(clippy::too_many_lines)]
    fn expr(&mut self, e: &Expr, min: u8) {
        let own = prec(e);
        let paren = own < min;
        if paren {
            self.out.push('(');
        }
        match e {
            Expr::Ident(n) => self.out.push_str(n),
            Expr::Num(n) => print_number(&mut self.out, *n),
            Expr::Str(s) => print_string(&mut self.out, s),
            Expr::Html(s) => print_html_string(&mut self.out, s),
            Expr::Bool(b) => self.out.push_str(if *b { "true" } else { "false" }),
            Expr::Null => self.out.push_str("null"),
            Expr::Undefined => self.out.push_str("undefined"),
            Expr::This => self.out.push_str("this"),
            Expr::Template(parts) => {
                self.out.push('`');
                for part in parts {
                    match part {
                        TemplatePart::Text(t) => print_template_text(&mut self.out, t),
                        TemplatePart::Expr(e) => {
                            self.out.push_str("${");
                            self.expr(e, P_SEQ);
                            self.out.push('}');
                        }
                    }
                }
                self.out.push('`');
            }
            Expr::Array(items) => {
                self.out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push(',');
                        self.sp();
                    }
                    self.expr(item, P_ASSIGN);
                }
                self.out.push(']');
            }
            Expr::Spread(inner) => {
                self.out.push_str("...");
                self.expr(inner, P_ASSIGN);
            }
            Expr::Object(props, multiline) => self.object(props, *multiline && !self.compact),
            Expr::Member(obj, name) => {
                self.receiver(obj);
                self.out.push('.');
                self.out.push_str(name);
            }
            Expr::OptMember(obj, name) => {
                self.receiver(obj);
                self.out.push_str("?.");
                self.out.push_str(name);
            }
            Expr::Index(obj, ix) => {
                self.receiver(obj);
                self.out.push('[');
                self.expr(ix, P_SEQ);
                self.out.push(']');
            }
            Expr::Call(f, args) => {
                self.callee(f);
                self.args(args);
            }
            Expr::OptCall(f, args) => {
                self.callee(f);
                self.out.push_str("?.");
                self.args(args);
            }
            Expr::New(c, args) => {
                self.out.push_str("new ");
                self.expr(c, P_CALL + 1);
                self.args(args);
            }
            Expr::Unary(op, inner) => {
                self.out.push_str(op);
                // `- -x` and `-(-1)` must not print as `--`.
                let clash = matches!(**inner, Expr::Unary(o, _) if o.starts_with(op.chars().next().unwrap_or(' ')))
                    || (*op == "-" && matches!(**inner, Expr::Num(n) if n < 0.0));
                if clash {
                    self.out.push('(');
                    self.expr(inner, P_SEQ);
                    self.out.push(')');
                } else {
                    self.expr(inner, P_UNARY);
                }
            }
            Expr::Await(inner) => {
                self.out.push_str("await ");
                self.expr(inner, P_UNARY);
            }
            Expr::Binary(op, a, b) => {
                let p = binary_prec(op);
                // `??` can't be mixed with `&&`/`||` without parentheses.
                let mixes = |x: &Expr| {
                    matches!(x, Expr::Binary(o, ..) if (*op == "??") != (*o == "??") && matches!(*o, "??" | "&&" | "||"))
                        && matches!(*op, "??" | "&&" | "||")
                };
                if mixes(a) || mixes(b) {
                    let wrap = |pr: &mut Printer, x: &Expr, m: u8| {
                        if mixes(x) {
                            pr.out.push('(');
                            pr.expr(x, P_SEQ);
                            pr.out.push(')');
                        } else {
                            pr.expr(x, m);
                        }
                    };
                    wrap(self, a, p);
                    self.binary_op(op);
                    wrap(self, b, p + 1);
                    // (`??`, `&&` and `||` never end with a sign.)
                    if paren {
                        self.out.push(')');
                    }
                    return;
                }
                // Mixing `??` with `&&`/`||` requires parentheses.
                let left_min =
                    if *op == "**" || *op == "??" || matches!(**a, Expr::Binary("??", ..)) {
                        p + 1
                    } else {
                        p
                    };
                self.expr(a, left_min);
                self.binary_op(op);
                let right_min = if *op == "??" && !matches!(**b, Expr::Binary("??", ..)) {
                    p + 1
                } else if *op == "??" {
                    p
                } else {
                    p + 1
                };
                let at = self.out.len();
                self.expr(b, right_min);
                // Without spaces (release builds) `a - -b` and `a + +b` would
                // print as `a--b` / `a++b`, which JS reads as `--`/`++`.
                for sign in ["-", "+"] {
                    if self.compact && op.ends_with(sign) && self.out[at..].starts_with(sign) {
                        self.out.insert(at, ' ');
                    }
                }
            }
            Expr::Cond(c, a, b) => {
                self.expr(c, P_NULLISH);
                self.sp();
                self.out.push('?');
                self.sp();
                self.expr(a, P_ASSIGN);
                self.sp();
                self.out.push(':');
                self.sp();
                self.expr(b, P_ASSIGN);
            }
            Expr::Arrow(f) => {
                if f.is_async {
                    self.out.push_str("async ");
                }
                self.params(&f.params);
                self.sp();
                self.out.push_str("=>");
                self.sp();
                match &f.body {
                    Body::Block(stmts) => self.block(stmts),
                    Body::Expr(body) => {
                        if matches!(**body, Expr::Object(..)) {
                            self.out.push('(');
                            self.expr(body, P_SEQ);
                            self.out.push(')');
                        } else {
                            self.expr(body, P_ASSIGN);
                        }
                    }
                }
            }
            Expr::Assign(op, place, value) => {
                self.expr(place, P_POSTFIX);
                self.sp();
                self.out.push_str(op);
                self.sp();
                self.expr(value, P_ASSIGN);
            }
            Expr::Import(spec) => {
                self.out.push_str("import(");
                self.expr(spec, P_ASSIGN);
                self.out.push(')');
            }
        }
        if paren {
            self.out.push(')');
        }
    }

    /// Prints the object of a member access or index: parenthesized when it is
    /// a number (`(1).toString()`), an object literal, or binds looser than a
    /// call.
    fn receiver(&mut self, obj: &Expr) {
        // `1.toString()` would parse as a decimal point; numbers and
        // object/function literals are parenthesized as receivers.
        let needs = matches!(obj, Expr::Num(_) | Expr::Object(..)) || prec(obj) < P_CALL;
        if needs {
            self.out.push('(');
            self.expr(obj, P_SEQ);
            self.out.push(')');
        } else {
            self.expr(obj, P_CALL);
        }
    }

    /// Prints the callee of a call: parenthesized when it is an object literal,
    /// an arrow function, or binds looser than a call.
    fn callee(&mut self, f: &Expr) {
        let needs = matches!(f, Expr::Object(..) | Expr::Arrow(_)) || prec(f) < P_CALL;
        if needs {
            self.out.push('(');
            self.expr(f, P_SEQ);
            self.out.push(')');
        } else {
            self.expr(f, P_CALL);
        }
    }

    /// Prints an object literal, one property per line with trailing commas
    /// when `multiline`, otherwise on one line; `{ k: k }` is shortened to
    /// `{ k }`.
    fn object(&mut self, props: &[Prop], multiline: bool) {
        if props.is_empty() {
            self.out.push_str("{}");
            return;
        }
        self.out.push('{');
        if multiline {
            self.nl();
            self.indent += 1;
        } else {
            self.sp();
        }
        for (i, p) in props.iter().enumerate() {
            if multiline {
                self.pad();
            } else if i > 0 {
                self.out.push(',');
                self.sp();
            }
            match p {
                Prop::KeyValue(k, v) => {
                    if matches!(v, Expr::Ident(n) if n == k) && is_ident(k) {
                        self.out.push_str(k);
                    } else {
                        print_key(&mut self.out, k);
                        self.out.push(':');
                        self.sp();
                        self.expr(v, P_ASSIGN);
                    }
                }
                Prop::Quoted(k, v) => {
                    print_string(&mut self.out, k);
                    self.out.push(':');
                    self.sp();
                    self.expr(v, P_ASSIGN);
                }
                Prop::Spread(e) => {
                    self.out.push_str("...");
                    self.expr(e, P_ASSIGN);
                }
            }
            if multiline {
                self.out.push(',');
                self.nl();
            }
        }
        if multiline {
            self.indent -= 1;
            self.pad();
        } else {
            self.sp();
        }
        self.out.push('}');
    }
}

/// `true` if `s` declares a binding (`const`, `let`, a function or a
/// class), which a `switch` case body must scope in braces.
fn declares(s: &Stmt) -> bool {
    matches!(
        s,
        Stmt::Const(..) | Stmt::Let(..) | Stmt::Function(..) | Stmt::Class(..)
    )
}

/// `true` when an expression statement would start with `{` or `function`
/// and must be parenthesized.
fn starts_ambiguously(e: &Expr) -> bool {
    match e {
        Expr::Object(..) => true,
        Expr::Member(o, _)
        | Expr::OptMember(o, _)
        | Expr::Index(o, _)
        | Expr::Call(o, _)
        | Expr::OptCall(o, _) => starts_ambiguously(o),
        Expr::Binary(_, a, _) | Expr::Assign(_, a, _) => starts_ambiguously(a),
        Expr::Cond(c, ..) => starts_ambiguously(c),
        _ => false,
    }
}

/// `true` for a valid JavaScript identifier made of ASCII letters, digits,
/// `_` and `$`.
#[must_use]
pub fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Prints an object property key: bare when it is an identifier
/// ([`is_ident`]), otherwise as a string literal.
fn print_key(out: &mut String, k: &str) {
    if is_ident(k) {
        out.push_str(k);
    } else {
        print_string(out, k);
    }
}

/// Prints a number in the shortest form that reads back as the same
/// value: `2`, `0.1`, `1e+21`.
pub fn print_number(out: &mut String, n: f64) {
    if n.is_nan() {
        out.push_str("NaN");
    } else if n.is_infinite() {
        out.push_str(if n > 0.0 { "Infinity" } else { "-Infinity" });
    } else if n == 0.0 && n.is_sign_negative() {
        out.push_str("-0");
    } else if n.fract() == 0.0 && n.abs() < 1e21 {
        let _ = write!(out, "{n:.0}");
    } else {
        let _ = write!(out, "{n}");
    }
}

/// Prints `s` as a double-quoted JavaScript string literal that is also
/// safe inside an HTML `<script>` element.
pub fn print_string(out: &mut String, s: &str) {
    out.push('"');
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' if chars.get(i + 1) == Some(&'!')
                && chars.get(i + 2) == Some(&'-')
                && chars.get(i + 3) == Some(&'-') =>
            {
                out.push_str("<\\u0021");
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'/') && starts_with_script(&chars[i + 2..]) => {
                out.push_str("<\\/");
                i += 1;
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
        i += 1;
    }
    out.push('"');
}

/// `true` if `chars` start with `script`, in any case.
fn starts_with_script(chars: &[char]) -> bool {
    let word: String = chars.iter().take(6).collect();
    word.eq_ignore_ascii_case("script")
}

/// Prints compiler-generated HTML as a string literal: JSON-compatible
/// escapes, `\u2028`/`\u2029`, and `</script` broken up; comment markers
/// stay as they are.
pub fn print_html_string(out: &mut String, s: &str) {
    out.push('"');
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' if chars.get(i + 1) == Some(&'/') && starts_with_script(&chars[i + 2..]) => {
                out.push_str("<\\/");
                i += 1;
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
        i += 1;
    }
    out.push('"');
}

/// Prints static template-literal text: `` ` ``, `\` and `${` escaped,
/// plus the same HTML-safety escapes as [`print_string`].
pub fn print_template_text(out: &mut String, s: &str) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '`' => out.push_str("\\`"),
            '\\' => out.push_str("\\\\"),
            '$' if chars.get(i + 1) == Some(&'{') => out.push_str("\\$"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' if chars.get(i + 1) == Some(&'!') => {
                out.push_str("<\\u0021");
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'/') => {
                out.push_str("<\\/");
                i += 1;
            }
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_exprs_mut_reaches_every_nested_expression() {
        // `$k.a` in a const, a call argument, an arrow body, a class
        // method, a template, an object, a switch case, a parameter
        // default and a `for` header; each becomes `$x.a`.
        let ka = || member(id("$k"), "a");
        let method = Function {
            name: "m".into(),
            params: vec![Param {
                name: "p".into(),
                default: Some(ka()),
                rest: false,
            }],
            body: Body::Block(vec![Stmt::Return(Some(Expr::Template(vec![
                TemplatePart::Text("t".into()),
                TemplatePart::Expr(ka()),
            ])))]),
            ..Function::default()
        };
        let mut stmts = vec![
            Stmt::Const("c".into(), call(id("f"), vec![ka(), arrow(vec![], ka())])),
            Stmt::Class(
                Class {
                    name: "C".into(),
                    extends: Some(ka()),
                    methods: vec![method],
                },
                false,
            ),
            Stmt::Switch(
                Expr::Object(
                    vec![Prop::KeyValue("k".into(), ka()), Prop::Spread(ka())],
                    false,
                ),
                vec![Case {
                    tests: vec![ka()],
                    body: vec![Stmt::Throw(ka())],
                }],
            ),
            Stmt::For(
                Box::new(Stmt::Let("i".into(), Some(ka()))),
                ka(),
                ka(),
                vec![Stmt::If(
                    ka(),
                    vec![Stmt::Expr(ka())],
                    Some(vec![Stmt::Expr(ka())]),
                )],
            ),
        ];
        let mut seen = 0;
        walk_exprs_mut(&mut stmts, &mut |e| {
            if let Expr::Member(base, _) = e
                && **base == id("$k")
            {
                seen += 1;
                **base = id("$x");
            }
        });
        assert_eq!(seen, 15);
        let text = print_module(&stmts, PrintOptions { compact: false });
        assert!(!text.contains("$k"), "{text}");
        assert_eq!(text.matches("$x.a").count(), 15, "{text}");
    }

    #[test]
    fn quoted_props_always_quote_their_key() {
        let e = Expr::Object(
            vec![
                Prop::Quoted("isDone".into(), id("d")),
                Prop::KeyValue("title".into(), id("title")),
            ],
            false,
        );
        assert_eq!(print_expr(&e), "{ \"isDone\": d, title }");
    }

    #[test]
    fn strings_are_safe_to_inline() {
        let mut out = String::new();
        print_string(&mut out, "</script><!-- \u{2028}\"\\\n");
        assert_eq!(out, r#""<\/script><\u0021-- \u2028\"\\\n""#);
        let mut html = String::new();
        print_html_string(&mut html, "<p><!--[--></p></script>");
        assert_eq!(html, r#""<p><!--[--></p><\/script>""#);
    }

    #[test]
    fn template_text_escapes_backticks_and_placeholders() {
        let mut out = String::new();
        print_template_text(&mut out, "`a` ${b} \\ $c");
        assert_eq!(out, r"\`a\` \${b} \\ $c");
    }

    #[test]
    fn numbers_print_shortest() {
        for (n, s) in [
            (2.0, "2"),
            (0.1, "0.1"),
            (-3.0, "-3"),
            (1e21, "1000000000000000000000"),
        ] {
            let mut out = String::new();
            print_number(&mut out, n);
            assert_eq!(out, s);
        }
    }

    #[test]
    fn precedence_adds_only_needed_parentheses() {
        let e = bin("*", bin("+", id("a"), id("b")), id("c"));
        assert_eq!(print_expr(&e), "(a + b) * c");
        let e = bin("+", id("a"), bin("*", id("b"), id("c")));
        assert_eq!(print_expr(&e), "a + b * c");
        let e = bin("-", id("a"), bin("-", id("b"), id("c")));
        assert_eq!(print_expr(&e), "a - (b - c)");
        let e = un("-", Expr::Num(-1.0));
        assert_eq!(print_expr(&e), "-(-1)");
        let e = bin("??", bin("||", id("a"), id("b")), id("c"));
        assert_eq!(print_expr(&e), "(a || b) ?? c");
    }

    #[test]
    fn compact_printing_never_fuses_signs_into_increments() {
        let compact = |e: Expr| print_module(&[Stmt::Expr(e)], PrintOptions { compact: true });
        // `a - (-b)`, `a - (-3)`, `a + (+b)` and `a - (-(-b))`.
        assert_eq!(compact(bin("-", id("a"), un("-", id("b")))), "a- -b;");
        assert_eq!(compact(bin("-", id("a"), Expr::Num(-3.0))), "a- -3;");
        assert_eq!(compact(bin("+", id("a"), un("+", id("b")))), "a+ +b;");
        assert_eq!(compact(bin("+", id("a"), un("-", id("b")))), "a+-b;");
        assert_eq!(
            compact(bin("-", id("a"), un("-", un("-", id("b"))))),
            "a- -(-b);"
        );
        // Spaced printing is unchanged.
        assert_eq!(print_expr(&bin("-", id("a"), un("-", id("b")))), "a - -b");
    }

    #[test]
    fn compact_printing_keeps_spaces_around_word_operators() {
        let compact = |e: Expr| print_module(&[Stmt::Expr(e)], PrintOptions { compact: true });
        // `e instanceof Error` printed as `einstanceofError` is one name.
        assert_eq!(
            compact(bin("instanceof", id("e"), id("Error"))),
            "e instanceof Error;"
        );
        assert_eq!(compact(bin("in", str("k"), id("o"))), "\"k\" in o;");
        // Also when mixed with `||`, which takes the parenthesizing branch.
        assert_eq!(
            compact(bin(
                "||",
                bin("instanceof", id("e"), id("Error")),
                bin("??", id("a"), id("b"))
            )),
            "e instanceof Error||(a??b);"
        );
    }

    #[test]
    fn arrow_object_bodies_are_parenthesized() {
        let e = arrow(
            vec![Param::new("level")],
            Expr::Object(
                vec![
                    Prop::KeyValue("$".into(), Expr::Num(1.0)),
                    Prop::KeyValue("level".into(), id("level")),
                ],
                false,
            ),
        );
        assert_eq!(print_expr(&e), "(level) => ({ $: 1, level })");
    }
}
