//! A generator of **well-typed** K projects, driven by the [`arbitrary`]
//! crate.
//!
//! [`generate`] reads choices from an [`Unstructured`] byte source and
//! writes a whole project that the checker must accept with **zero
//! errors**: a `k.toml`, a `model` module of public types (one per file,
//! named like the file, as the file rule requires) and a `logic` module of
//! private types, functions and top-level constants spread over one to four
//! files that import the `model` types they name.
//!
//! Unlike the syntax generator (`crates/k_syntax/tests/support/kgen.rs`),
//! which only knows the grammar, this one keeps a type environment: every
//! expression is generated *for* a type, from the literals, locals, fields,
//! calls, methods and operators that produce it. The subset is deliberately
//! the core of the language, where every rule is settled in
//! `docs/design/08-types.md` and `docs/compiler/sema.md`:
//!
//! * types `Int`, `Double`, `Bool`, `String`, `T?`, `List<T>` (of a
//!   primitive), user enums (with and without payloads) and structs (with
//!   `let`/`var` fields, defaults and methods);
//! * expressions: literals, memberwise initializers, enum cases (qualified,
//!   or implicit `.case` where the expected type is known), arithmetic,
//!   comparisons, `and`/`or`/`not`, string concatenation and interpolation,
//!   `??`, `.count`, field access, calls, method calls and `if` expressions;
//! * statements: `let`/`var` (with or without an annotation), assignment
//!   and compound assignment, `if`/`else`, `while`, `for … in`, `if let`,
//!   `guard let … else`, `match` over enums and optionals (exhaustive, or
//!   with a reachable `default`), void calls and `return`.
//!
//! What it avoids, because the program would be ill-typed or the rule is
//! not this generator's business: `==` on enums and structs, comparing a
//! `Bool` with a literal, implicit members and `nil` where no type is
//! expected, shadowing, unreachable code and imports that are not used.
//!
//! The generator never fails: when the bytes run out every choice takes its
//! first option, which is always the smallest valid one (a literal), and a
//! node budget and a depth limit bound the output.
//!
//! [`inject_fixable_mistakes`] turns a generated project into one with
//! mistakes that carry quick fixes (a duplicate, unused or misspelt import,
//! a missing import, symbolic operators, a semicolon), for the property that
//! applying machine-applicable fixes never introduces errors.
//!
//! This file is shared by `crates/k_sema/tests/properties.rs` (stable) and
//! the `check_typed` fuzz target (`fuzz/src/lib.rs` includes it with
//! `#[path]`).

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fmt::Write as _;

use arbitrary::Unstructured;

/// The `k.toml` of every generated project.
pub const MANIFEST: &str = "[package]\nname = \"gen\"\nk = \"0.1\"\n";

/// The module of public types: `src/model/<Name>.k`.
pub const MODEL: &str = "model";

/// The module of private declarations: `src/logic/F<n>.k`.
pub const LOGIC: &str = "logic";

/// A generated project: every file and its text, `k.toml` first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// `(project path, text)` pairs; paths are unique.
    pub files: Vec<(String, String)>,
}

impl Program {
    /// Returns every file as `--- path ---` followed by its text, for
    /// failure messages.
    pub fn dump(&self) -> String {
        dump_files(&self.files)
    }
}

/// Formats `files` as `--- path ---` sections, for failure messages.
pub fn dump_files(files: &[(String, String)]) -> String {
    let mut out = String::new();
    for (path, text) in files {
        let _ = writeln!(out, "--- {path} ---\n{text}");
    }
    out
}

/// Generates one well-typed project from `u`.
pub fn generate(u: &mut Unstructured<'_>) -> Program {
    Gen::new(u, false).run()
}

/// The entry of a [`generate_runnable`] project: `logic.main`, a function
/// without parameters returning `Void`.
pub const RUNNABLE_ENTRY: &str = "logic.main";

/// Generates one well-typed project from `u` that also **terminates when
/// run**, for the stage 3 execution properties
/// (`crates/k_server/tests/differential.rs`).
///
/// It is the project [`generate`] writes, with three differences that keep
/// every execution finite without changing what is well-typed:
///
/// * calls are acyclic: function `f<i>` only calls `f<j>` for `j < i`,
///   and methods call neither functions nor methods, so there is no
///   recursion;
/// * every `while` loop also counts its iterations in a private counter
///   and stops after three (`while (c) and n < 3 { n += 1 … }`);
/// * `logic/F0.k` gets a `func main()` ([`RUNNABLE_ENTRY`]) that calls
///   every function once with generated arguments and logs each
///   primitive result with `Log.info`.
///
/// Running it can still panic (integer overflow), which is a K outcome,
/// not a failure.
pub fn generate_runnable(u: &mut Unstructured<'_>) -> Program {
    Gen::new(u, true).run()
}

/// Checker findings this generator steers around, so that the zero-error
/// property stays meaningful for everything else. Each is pinned as a
/// pending corpus case; when the checker is fixed, delete the `.pending`
/// file, the entry here and the workaround that cites it.
///
/// None is open. The two stage 2 findings it held (`-(if …)` under an
/// optional expected type, and unchecked arguments of an unresolved
/// callee) are fixed and their corpus cases are regular; so are the two
/// code generation findings that [`generate_runnable`] steered around
/// (`release-minus-minus`, guarded by `fixed_findings_stay_fixed` in
/// `crates/k_server/tests/differential.rs`, and a `match` with a `default`
/// arm over an enum, `tests/corpus/run/regressions/fuzz/match-default-over-an-enum.k`).
pub const KNOWN_FINDINGS: &[(&str, &str)] = &[];

// ------------------------------------------------------------------ model

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Int,
    Double,
    Bool,
    Str,
    /// A user type, by index into `Gen::types`.
    Named(usize),
    Opt(Box<Ty>),
    List(Box<Ty>),
}

impl Ty {
    fn is_prim(&self) -> bool {
        matches!(self, Ty::Int | Ty::Double | Ty::Bool | Ty::Str)
    }

    /// `true` if a value of this type can be written where no type is
    /// expected (`let x = …`): optionals need an annotation for `nil`.
    fn inferable(&self) -> bool {
        !matches!(self, Ty::Opt(_))
    }
}

#[derive(Clone, Debug)]
struct Sig {
    name: String,
    params: Vec<(String, Ty)>,
    ret: Option<Ty>,
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: Ty,
    var: bool,
    default: Option<String>,
}

#[derive(Clone, Debug)]
struct Case {
    name: String,
    payload: Vec<(String, Ty)>,
}

#[derive(Clone, Debug)]
enum Kind {
    Enum(Vec<Case>),
    Struct {
        fields: Vec<Field>,
        methods: Vec<Sig>,
    },
}

#[derive(Clone, Debug)]
struct TypeDef {
    name: String,
    /// `true` for a public type in the `model` module.
    lib: bool,
    kind: Kind,
    /// The `logic` file a private type lives in.
    file: usize,
}

#[derive(Clone, Debug)]
struct Func {
    sig: Sig,
    file: usize,
}

#[derive(Clone, Debug)]
struct Global {
    name: String,
    ty: Ty,
    file: usize,
}

#[derive(Clone, Debug)]
struct Local {
    name: String,
    ty: Ty,
    var: bool,
}

const MAX_DEPTH: u32 = 4;
const INDENT: &str = "    ";
const DOUBLES: &[&str] = &["0.5", "1.5", "2.25", "10.0", "3.0"];
const WORDS: &[&str] = &["a", "hello", "k", "", "x y", "tea"];

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    types: Vec<TypeDef>,
    funcs: Vec<Func>,
    globals: Vec<Global>,
    budget: u32,
    fresh: u32,
    /// Generating inside a `model` type: only `model` types are reachable.
    lib_only: bool,
    /// The `logic` file being written, whose imports are recorded in
    /// `uses`; `None` inside `model`.
    file: Option<usize>,
    uses: Vec<BTreeSet<String>>,
    locals: Vec<Local>,
    in_interp: bool,
    /// [`generate_runnable`]: acyclic calls, bounded loops and a `main`.
    runnable: bool,
    /// In runnable mode, only functions with an index below this one may
    /// be called (`None`: any).
    callee_limit: Option<usize>,
    /// In runnable mode, generating a method body: no calls at all.
    in_method: bool,
}

impl<'a, 'b> Gen<'a, 'b> {
    fn new(u: &'a mut Unstructured<'b>, runnable: bool) -> Gen<'a, 'b> {
        Gen {
            u,
            types: Vec::new(),
            funcs: Vec::new(),
            globals: Vec::new(),
            budget: 900,
            fresh: 0,
            lib_only: false,
            file: None,
            uses: Vec::new(),
            locals: Vec::new(),
            in_interp: false,
            runnable,
            callee_limit: None,
            in_method: false,
        }
    }

    fn run(mut self) -> Program {
        self.signatures();
        self.emit()
    }

    /// `true` if function `index` may be called here (runnable mode keeps
    /// the call graph acyclic).
    fn may_call(&self, index: usize) -> bool {
        !self.runnable || (!self.in_method && self.callee_limit.is_none_or(|l| index < l))
    }

    /// The `main` of a runnable project: every function called once.
    fn main_function(&mut self) -> String {
        let mut out = String::from("func main() {\n");
        self.locals.clear();
        for gl in self.globals.clone() {
            self.locals.push(Local {
                name: gl.name,
                ty: gl.ty,
                var: false,
            });
        }
        self.callee_limit = None;
        for f in 0..self.funcs.len() {
            let sig = self.funcs[f].sig.clone();
            let args = self.args(&sig.params, 1);
            match &sig.ret {
                None => {
                    let _ = writeln!(out, "{INDENT}{}({args})", sig.name);
                }
                Some(ret) => {
                    let name = format!("r{f}");
                    let _ = writeln!(out, "{INDENT}let {name} = {}({args})", sig.name);
                    if ret.is_prim() {
                        let _ = writeln!(out, "{INDENT}Log.info(\"{name}={{{name}}}\")");
                    } else {
                        let _ = writeln!(out, "{INDENT}Log.info(\"{name}\")");
                    }
                }
            }
        }
        out.push_str("}\n");
        self.locals.clear();
        out
    }

    // -------------------------------------------------------------- choices

    /// Returns an index below `n` (`n > 0`); 0 once the bytes run out.
    fn pick(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        self.u.choose_index(n).unwrap_or(0)
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.pick(one_in) == one_in - 1
    }

    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.pick(hi - lo + 1)
    }

    fn fresh(&mut self) -> String {
        self.fresh += 1;
        format!("l{}", self.fresh - 1)
    }

    // ----------------------------------------------------------- signatures

    fn signatures(&mut self) {
        let files = self.range(1, 4);
        self.uses = vec![BTreeSet::new(); files];
        let ntypes = self.range(0, 5);
        for i in 0..ntypes {
            let lib = self.chance(2);
            let name = format!("T{i}");
            let file = self.pick(files);
            let kind = if self.chance(2) {
                let n = self.range(1, 4);
                let mut cases = Vec::new();
                for c in 0..n {
                    let np = if self.chance(2) { self.range(1, 2) } else { 0 };
                    let mut payload = Vec::new();
                    for a in 0..np {
                        payload.push((format!("a{a}"), self.field_ty(i, lib)));
                    }
                    cases.push(Case {
                        name: format!("k{c}"),
                        payload,
                    });
                }
                Kind::Enum(cases)
            } else {
                let n = self.range(1, 3);
                let mut fields = Vec::new();
                for f in 0..n {
                    let ty = self.field_ty(i, lib);
                    let var = self.chance(2);
                    let default =
                        (var && ty.is_prim() && self.chance(2)).then(|| self.prim_literal(&ty));
                    fields.push(Field {
                        name: format!("x{f}"),
                        ty,
                        var,
                        default,
                    });
                }
                let nm = self.pick(3);
                let mut methods = Vec::new();
                for m in 0..nm {
                    let sig = self.sig(format!("m{m}"), lib, ntypes);
                    methods.push(sig);
                }
                Kind::Struct { fields, methods }
            };
            self.types.push(TypeDef {
                name,
                lib,
                kind,
                file,
            });
        }
        let nfuncs = self.range(1, 5);
        for f in 0..nfuncs {
            let sig = self.sig(format!("f{f}"), false, ntypes);
            let file = self.pick(files);
            self.funcs.push(Func { sig, file });
        }
        let nglobals = self.pick(4);
        for gl in 0..nglobals {
            let ty = self.prim_ty();
            let file = self.pick(files);
            self.globals.push(Global {
                name: format!("g{gl}"),
                ty,
                file,
            });
        }
    }

    fn prim_ty(&mut self) -> Ty {
        match self.pick(4) {
            0 => Ty::Int,
            1 => Ty::Double,
            2 => Ty::Bool,
            _ => Ty::Str,
        }
    }

    /// A field or payload type of type `owner`: a primitive, an optional or
    /// list of one, or an earlier type (so values can always be built), a
    /// `model` one if the owner is in `model`.
    fn field_ty(&mut self, owner: usize, lib: bool) -> Ty {
        let earlier: Vec<usize> = (0..owner).filter(|&k| !lib || self.types[k].lib).collect();
        match self.pick(if earlier.is_empty() { 3 } else { 4 }) {
            0 => self.prim_ty(),
            1 => Ty::Opt(Box::new(self.prim_ty())),
            2 => Ty::List(Box::new(self.prim_ty())),
            _ => {
                let k = earlier[self.pick(earlier.len())];
                if self.chance(3) {
                    Ty::Opt(Box::new(Ty::Named(k)))
                } else {
                    Ty::Named(k)
                }
            }
        }
    }

    /// A parameter or return type: anything, including types declared
    /// later (signatures don't build values).
    fn any_ty(&mut self, lib: bool, ntypes: usize) -> Ty {
        let reachable: Vec<usize> = (0..ntypes)
            .filter(|&k| !lib || self.types.get(k).is_some_and(|t| t.lib))
            .collect();
        match self.pick(if reachable.is_empty() { 3 } else { 4 }) {
            0 => self.prim_ty(),
            1 => Ty::Opt(Box::new(self.prim_ty())),
            2 => Ty::List(Box::new(self.prim_ty())),
            _ => {
                let k = reachable[self.pick(reachable.len())];
                if self.chance(3) {
                    Ty::Opt(Box::new(Ty::Named(k)))
                } else {
                    Ty::Named(k)
                }
            }
        }
    }

    fn sig(&mut self, name: String, lib: bool, ntypes: usize) -> Sig {
        // Method signatures of a model type are chosen while later types
        // don't exist yet: `any_ty` only offers the earlier model types.
        let np = self.pick(4);
        let mut params = Vec::new();
        for p in 0..np {
            params.push((format!("p{p}"), self.any_ty(lib, ntypes)));
        }
        let ret = (!self.chance(4)).then(|| self.any_ty(lib, ntypes));
        Sig { name, params, ret }
    }

    // ----------------------------------------------------------------- text

    /// Writes the name of `ty`, recording `model` types the current file
    /// must import.
    fn ty_text(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int => "Int".to_owned(),
            Ty::Double => "Double".to_owned(),
            Ty::Bool => "Bool".to_owned(),
            Ty::Str => "String".to_owned(),
            Ty::Named(k) => self.type_name(*k),
            Ty::Opt(t) => format!("{}?", self.ty_text(t)),
            Ty::List(t) => format!("List<{}>", self.ty_text(t)),
        }
    }

    fn type_name(&mut self, k: usize) -> String {
        let def = &self.types[k];
        if def.lib
            && let Some(file) = self.file
        {
            self.uses[file].insert(def.name.clone());
        }
        def.name.clone()
    }

    fn emit(&mut self) -> Program {
        let mut files = vec![("k.toml".to_owned(), MANIFEST.to_owned())];
        // Model types, one per file.
        for k in 0..self.types.len() {
            if !self.types[k].lib {
                continue;
            }
            self.lib_only = true;
            self.file = None;
            let text = format!("public {}", self.type_decl(k));
            files.push((format!("src/{MODEL}/{}.k", self.types[k].name), text));
        }
        // Logic files.
        self.lib_only = false;
        let nfiles = self.uses.len();
        let mut bodies = vec![String::new(); nfiles];
        for k in 0..self.types.len() {
            if self.types[k].lib {
                continue;
            }
            let file = self.types[k].file;
            self.file = Some(file);
            let text = self.type_decl(k);
            push_decl(&mut bodies[file], &text);
        }
        for gl in 0..self.globals.len() {
            let Global { name, ty, file } = self.globals[gl].clone();
            self.file = Some(file);
            let tt = self.ty_text(&ty);
            let init = self.prim_literal_expr(&ty);
            push_decl(&mut bodies[file], &format!("let {name}: {tt} = {init}\n"));
        }
        for f in 0..self.funcs.len() {
            let Func { sig, file } = self.funcs[f].clone();
            self.file = Some(file);
            self.callee_limit = Some(f);
            let text = self.function(&sig, 0, &[]);
            self.callee_limit = None;
            push_decl(&mut bodies[file], &text);
        }
        if self.runnable {
            self.file = Some(0);
            let text = self.main_function();
            push_decl(&mut bodies[0], &text);
        }
        for (i, body) in bodies.into_iter().enumerate() {
            let mut text = String::new();
            let log = self.runnable && i == 0;
            if log {
                text.push_str("import k.observe.Log\n");
            }
            for name in &self.uses[i] {
                let _ = writeln!(text, "import {MODEL}.{name}");
            }
            if log || !self.uses[i].is_empty() {
                text.push('\n');
            }
            text.push_str(&body);
            files.push((format!("src/{LOGIC}/F{i}.k"), text));
        }
        Program { files }
    }

    fn type_decl(&mut self, k: usize) -> String {
        let def = self.types[k].clone();
        let mut out = String::new();
        match &def.kind {
            Kind::Enum(cases) => {
                let _ = writeln!(out, "enum {} {{", def.name);
                for case in cases {
                    let _ = write!(out, "{INDENT}case {}", case.name);
                    if !case.payload.is_empty() {
                        let parts: Vec<String> = case
                            .payload
                            .iter()
                            .map(|(l, t)| format!("{l}: {}", self.ty_text(t)))
                            .collect();
                        let _ = write!(out, "({})", parts.join(", "));
                    }
                    out.push('\n');
                }
                out.push_str("}\n");
            }
            Kind::Struct { fields, methods } => {
                let _ = writeln!(out, "struct {} {{", def.name);
                for field in fields {
                    let kw = if field.var { "var" } else { "let" };
                    let tt = self.ty_text(&field.ty);
                    let _ = write!(out, "{INDENT}{kw} {}: {tt}", field.name);
                    if let Some(d) = &field.default {
                        let _ = write!(out, " = {d}");
                    }
                    out.push('\n');
                }
                let field_locals: Vec<Local> = fields
                    .iter()
                    .map(|f| Local {
                        name: f.name.clone(),
                        ty: f.ty.clone(),
                        var: false,
                    })
                    .collect();
                for sig in methods {
                    out.push('\n');
                    let outer = self.in_method;
                    self.in_method = true;
                    let text = self.function(sig, 1, &field_locals);
                    self.in_method = outer;
                    out.push_str(&text);
                }
                out.push_str("}\n");
            }
        }
        out
    }

    /// Writes a function or method at `level` with `outer` in scope (a
    /// method's fields).
    fn function(&mut self, sig: &Sig, level: usize, outer: &[Local]) -> String {
        let pad = INDENT.repeat(level);
        let params: Vec<String> = sig
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {}", self.ty_text(t)))
            .collect();
        let mut out = format!("{pad}func {}({})", sig.name, params.join(", "));
        if let Some(ret) = &sig.ret {
            let _ = write!(out, " -> {}", self.ty_text(ret));
        }
        out.push_str(" {\n");
        self.locals.clear();
        self.locals.extend_from_slice(outer);
        for (n, t) in &sig.params {
            self.locals.push(Local {
                name: n.clone(),
                ty: t.clone(),
                var: false,
            });
        }
        if !self.lib_only {
            for gl in self.globals.clone() {
                self.locals.push(Local {
                    name: gl.name,
                    ty: gl.ty,
                    var: false,
                });
            }
        }
        self.block(&mut out, level + 1, sig.ret.as_ref(), 0);
        if let Some(ret) = &sig.ret {
            let e = self.expr(ret, true, 0);
            let _ = writeln!(out, "{}return {e}", INDENT.repeat(level + 1));
        }
        let _ = writeln!(out, "{pad}}}");
        self.locals.clear();
        out
    }

    // ----------------------------------------------------------- statements

    fn block(&mut self, out: &mut String, level: usize, ret: Option<&Ty>, depth: u32) {
        let n = if depth >= 2 || self.budget < 40 {
            self.pick(2)
        } else {
            self.pick(5)
        };
        let mark = self.locals.len();
        for _ in 0..n {
            self.statement(out, level, ret, depth);
        }
        self.locals.truncate(mark);
    }

    fn statement(&mut self, out: &mut String, level: usize, ret: Option<&Ty>, depth: u32) {
        let pad = INDENT.repeat(level);
        self.budget = self.budget.saturating_sub(4);
        let nested = depth < 2 && self.budget > 40;
        match self.pick(if nested { 12 } else { 5 }) {
            0 | 1 => {
                // let / var
                let ty = self.local_ty();
                let var = self.chance(2);
                let annotate = !ty.inferable() || self.chance(3);
                let e = self.expr(&ty, annotate, 0);
                let name = self.fresh();
                let kw = if var { "var" } else { "let" };
                if annotate {
                    let tt = self.ty_text(&ty);
                    let _ = writeln!(out, "{pad}{kw} {name}: {tt} = {e}");
                } else {
                    let _ = writeln!(out, "{pad}{kw} {name} = {e}");
                }
                self.locals.push(Local { name, ty, var });
            }
            2 | 3 => {
                // assignment to a var
                let vars: Vec<Local> = self.locals.iter().filter(|l| l.var).cloned().collect();
                if vars.is_empty() {
                    return self.void_call(out, &pad);
                }
                let v = vars[self.pick(vars.len())].clone();
                let compound = matches!(v.ty, Ty::Int | Ty::Double | Ty::Str) && self.chance(2);
                let op = if !compound {
                    "="
                } else if v.ty == Ty::Str {
                    "+="
                } else {
                    ["+=", "-=", "*="][self.pick(3)]
                };
                let e = self.expr(&v.ty, true, 0);
                let _ = writeln!(out, "{pad}{} {op} {e}", v.name);
            }
            4 => self.void_call(out, &pad),
            5 | 6 => {
                let c = self.expr(&Ty::Bool, true, 1);
                let _ = writeln!(out, "{pad}if {c} {{");
                self.block(out, level + 1, ret, depth + 1);
                if self.chance(2) {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.block(out, level + 1, ret, depth + 1);
                }
                let _ = writeln!(out, "{pad}}}");
            }
            7 if self.runnable => {
                // A bounded loop: the counter is not a local of the body,
                // so nothing else assigns it.
                let c = self.expr(&Ty::Bool, true, 1);
                let n = self.fresh();
                let _ = writeln!(out, "{pad}var {n} = 0");
                let _ = writeln!(out, "{pad}while ({c}) and {n} < 3 {{");
                let _ = writeln!(out, "{pad}{INDENT}{n} += 1");
                self.block(out, level + 1, ret, depth + 1);
                let _ = writeln!(out, "{pad}}}");
            }
            7 => {
                let c = self.expr(&Ty::Bool, true, 1);
                let _ = writeln!(out, "{pad}while {c} {{");
                self.block(out, level + 1, ret, depth + 1);
                let _ = writeln!(out, "{pad}}}");
            }
            8 => {
                let elem = self.prim_ty();
                let list = self.expr(&Ty::List(Box::new(elem.clone())), false, 1);
                let name = self.fresh();
                let _ = writeln!(out, "{pad}for {name} in {list} {{");
                let mark = self.locals.len();
                self.locals.push(Local {
                    name,
                    ty: elem,
                    var: false,
                });
                self.block(out, level + 1, ret, depth + 1);
                self.locals.truncate(mark);
                let _ = writeln!(out, "{pad}}}");
            }
            9 => {
                // if let / guard let over an optional source
                let inner = self.local_ty();
                let Some(src) = self.opt_source(&inner, 1) else {
                    return self.void_call(out, &pad);
                };
                let name = self.fresh();
                if self.chance(2) {
                    let _ = writeln!(out, "{pad}if let {name} = {src} {{");
                    let mark = self.locals.len();
                    self.locals.push(Local {
                        name,
                        ty: inner,
                        var: false,
                    });
                    self.block(out, level + 1, ret, depth + 1);
                    self.locals.truncate(mark);
                    let _ = writeln!(out, "{pad}}}");
                } else {
                    let _ = writeln!(out, "{pad}guard let {name} = {src} else {{");
                    match ret {
                        Some(t) => {
                            let e = self.expr(t, true, 1);
                            let _ = writeln!(out, "{pad}{INDENT}return {e}");
                        }
                        None => {
                            let _ = writeln!(out, "{pad}{INDENT}return");
                        }
                    }
                    let _ = writeln!(out, "{pad}}}");
                    self.locals.push(Local {
                        name,
                        ty: inner,
                        var: false,
                    });
                }
            }
            _ => self.match_let(out, level),
        }
    }

    /// The type of a new local: anything reachable.
    fn local_ty(&mut self) -> Ty {
        let reachable: Vec<usize> = (0..self.types.len())
            .filter(|&k| !self.lib_only || self.types[k].lib)
            .collect();
        match self.pick(if reachable.is_empty() { 4 } else { 6 }) {
            0 => self.prim_ty(),
            1 => Ty::Opt(Box::new(self.prim_ty())),
            2 => Ty::List(Box::new(self.prim_ty())),
            3 => self.prim_ty(),
            4 => Ty::Named(reachable[self.pick(reachable.len())]),
            _ => Ty::Opt(Box::new(Ty::Named(reachable[self.pick(reachable.len())]))),
        }
    }

    fn void_call(&mut self, out: &mut String, pad: &str) {
        let voids: Vec<Sig> = if self.lib_only {
            Vec::new()
        } else {
            self.funcs
                .iter()
                .enumerate()
                .filter(|(i, f)| f.sig.ret.is_none() && self.may_call(*i))
                .map(|(_, f)| f.sig.clone())
                .collect()
        };
        if voids.is_empty() || self.budget < 20 {
            // Fall back to a harmless binding.
            let e = self.expr(&Ty::Int, false, 1);
            let name = self.fresh();
            let _ = writeln!(out, "{pad}let {name} = {e}");
            self.locals.push(Local {
                name,
                ty: Ty::Int,
                var: false,
            });
            return;
        }
        let sig = voids[self.pick(voids.len())].clone();
        let args = self.args(&sig.params, 1);
        let _ = writeln!(out, "{pad}{}({args})", sig.name);
    }

    /// `let l = match subject { … }` over an enum or an optional.
    fn match_let(&mut self, out: &mut String, level: usize) {
        let pad = INDENT.repeat(level);
        let result = self.prim_ty();
        let enums: Vec<usize> = (0..self.types.len())
            .filter(|&k| {
                (!self.lib_only || self.types[k].lib) && matches!(self.types[k].kind, Kind::Enum(_))
            })
            .collect();
        let name = self.fresh();
        let mut arms = Vec::new();
        let subject;
        if !enums.is_empty() && !self.chance(3) {
            let k = enums[self.pick(enums.len())];
            subject = self.expr(&Ty::Named(k), false, 1);
            let Kind::Enum(cases) = self.types[k].kind.clone() else {
                unreachable!()
            };
            // Every case, or a strict, non-empty subset and `default`.
            let mut keep = vec![true; cases.len()];
            if cases.len() > 1 && self.chance(3) {
                let dropped = self.pick(cases.len());
                keep[dropped] = false;
                for (i, k) in keep.iter_mut().enumerate() {
                    if i != dropped && *k && self.chance(3) && i != (dropped + 1) % cases.len() {
                        *k = false;
                    }
                }
            }
            let mut covered = 0;
            for (i, case) in cases.iter().enumerate() {
                if !keep[i] {
                    continue;
                }
                covered += 1;
                let mark = self.locals.len();
                let mut pat = format!(".{}", case.name);
                if !case.payload.is_empty() && self.pick(3) != 0 {
                    let labelled = self.chance(2);
                    let mut parts = Vec::new();
                    for (label, ty) in &case.payload {
                        let b = self.fresh();
                        parts.push(if labelled {
                            format!("{label}: let {b}")
                        } else {
                            format!("let {b}")
                        });
                        self.locals.push(Local {
                            name: b,
                            ty: ty.clone(),
                            var: false,
                        });
                    }
                    let _ = write!(pat, "({})", parts.join(", "));
                }
                let e = self.expr(&result, false, 2);
                self.locals.truncate(mark);
                arms.push(format!("case {pat} {{ {e} }}"));
            }
            if covered < cases.len() {
                let e = self.expr(&result, false, 2);
                arms.push(format!("default {{ {e} }}"));
            }
        } else {
            let inner = self.local_ty();
            let Some(src) = self.opt_source(&inner, 1) else {
                let e = self.expr(&result, false, 1);
                let _ = writeln!(out, "{pad}let {name} = {e}");
                self.locals.push(Local {
                    name,
                    ty: result,
                    var: false,
                });
                return;
            };
            subject = src;
            let e = self.expr(&result, false, 2);
            arms.push(format!("case nil {{ {e} }}"));
            let b = self.fresh();
            let mark = self.locals.len();
            self.locals.push(Local {
                name: b.clone(),
                ty: inner,
                var: false,
            });
            let e = self.expr(&result, false, 2);
            self.locals.truncate(mark);
            arms.push(format!("case .some(let {b}) {{ {e} }}"));
        }
        let _ = writeln!(out, "{pad}let {name} = match {subject} {{");
        for arm in arms {
            let _ = writeln!(out, "{pad}{INDENT}{arm}");
        }
        let _ = writeln!(out, "{pad}}}");
        self.locals.push(Local {
            name,
            ty: result,
            var: false,
        });
    }

    // ---------------------------------------------------------- expressions

    /// A primitive literal (field defaults).
    fn prim_literal(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int => self.pick(100).to_string(),
            Ty::Double => DOUBLES[self.pick(DOUBLES.len())].to_owned(),
            Ty::Bool => if self.chance(2) { "true" } else { "false" }.to_owned(),
            _ => format!("\"{}\"", WORDS[self.pick(WORDS.len())]),
        }
    }

    /// A constant expression of a primitive type (top-level `let`s).
    fn prim_literal_expr(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int | Ty::Double if self.chance(2) => {
                let a = self.prim_literal(ty);
                let b = self.prim_literal(ty);
                let op = ["+", "-", "*"][self.pick(3)];
                format!("({a} {op} {b})")
            }
            Ty::Str if self.chance(3) => {
                let a = self.prim_literal(ty);
                let b = self.prim_literal(ty);
                format!("{a} + {b}")
            }
            _ => self.prim_literal(ty),
        }
    }

    /// The smallest value of `ty`, built from literals; `known` says
    /// whether the context expects `ty` (so `.case`, `nil` and `[]` can be
    /// written).
    fn literal(&mut self, ty: &Ty, known: bool) -> String {
        match ty {
            Ty::Int | Ty::Double | Ty::Bool | Ty::Str => self.prim_literal(ty),
            Ty::Opt(t) => {
                if known && self.chance(2) {
                    "nil".to_owned()
                } else {
                    self.literal(t, known)
                }
            }
            Ty::List(t) => {
                if known && self.chance(3) {
                    "[]".to_owned()
                } else {
                    let n = self.range(1, 2);
                    let parts: Vec<String> = (0..n).map(|_| self.literal(t, known)).collect();
                    format!("[{}]", parts.join(", "))
                }
            }
            Ty::Named(k) => self.construct(*k, known, MAX_DEPTH),
        }
    }

    /// A memberwise initializer or enum case of type `k`, with arguments
    /// generated at `depth`.
    fn construct(&mut self, k: usize, known: bool, depth: u32) -> String {
        let def = self.types[k].clone();
        match def.kind {
            Kind::Enum(cases) => {
                let case = &cases[self.pick(cases.len())];
                let head = if known && self.pick(3) != 0 {
                    format!(".{}", case.name)
                } else {
                    format!("{}.{}", self.type_name(k), case.name)
                };
                if case.payload.is_empty() {
                    head
                } else {
                    let args = self.args(&case.payload, depth);
                    format!("{head}({args})")
                }
            }
            Kind::Struct { fields, .. } => {
                let name = self.type_name(k);
                let mut parts = Vec::new();
                for f in &fields {
                    if f.default.is_some() && self.chance(2) {
                        continue;
                    }
                    let e = self.expr_at(&f.ty, true, depth);
                    parts.push(format!("{}: {e}", f.name));
                }
                format!("{name}({})", parts.join(", "))
            }
        }
    }

    fn args(&mut self, params: &[(String, Ty)], depth: u32) -> String {
        let parts: Vec<String> = params
            .iter()
            .map(|(l, t)| {
                let e = self.expr_at(t, true, depth);
                format!("{l}: {e}")
            })
            .collect();
        parts.join(", ")
    }

    /// Like [`expr`](Self::expr) but never deeper than [`MAX_DEPTH`].
    fn expr_at(&mut self, ty: &Ty, known: bool, depth: u32) -> String {
        if depth >= MAX_DEPTH {
            return self.leaf(ty, known);
        }
        self.expr(ty, known, depth + 1)
    }

    /// A literal or a local of `ty`.
    fn leaf(&mut self, ty: &Ty, known: bool) -> String {
        let locals: Vec<String> = self
            .locals
            .iter()
            .filter(|l| &l.ty == ty)
            .map(|l| l.name.clone())
            .collect();
        if !locals.is_empty() && self.chance(2) {
            return locals[self.pick(locals.len())].clone();
        }
        self.literal(ty, known)
    }

    /// An expression of type `ty` (see [`literal`](Self::literal) for
    /// `known`). Composite results are parenthesized, so any result can be
    /// an operand or a receiver.
    fn expr(&mut self, ty: &Ty, known: bool, depth: u32) -> String {
        self.budget = self.budget.saturating_sub(1);
        if depth >= MAX_DEPTH || self.budget == 0 {
            return self.leaf(ty, known);
        }
        let d = depth + 1;
        // Option 0 is always the leaf; the rest are tried in order of the
        // picked index and fall back to the leaf when they don't apply.
        let choice = self.pick(10);
        let attempt = match choice {
            0 | 1 => None,
            2 => self.field_of(ty, d),
            3 => self.call_of(ty, d),
            4 => self.method_of(ty, d),
            5 if ty.inferable() && !self.in_interp => {
                let c = self.expr(&Ty::Bool, true, d);
                let a = self.expr(ty, known, d);
                let b = self.expr(ty, known, d);
                Some(format!("(if {c} {{ {a} }} else {{ {b} }})"))
            }
            5 => None,
            6 if ty.inferable() => self.opt_source(ty, d).map(|src| {
                let alt = self.expr(ty, false, d);
                format!("({src} ?? {alt})")
            }),
            _ => self.operator(ty, known, d),
        };
        attempt.unwrap_or_else(|| self.leaf(ty, known))
    }

    /// An operator expression producing `ty`, if one exists.
    fn operator(&mut self, ty: &Ty, known: bool, d: u32) -> Option<String> {
        match ty {
            Ty::Int | Ty::Double => match self.pick(3) {
                0 => {
                    let op = ["+", "-", "*"][self.pick(3)];
                    let a = self.expr(ty, false, d);
                    let b = self.expr(ty, false, d);
                    Some(format!("({a} {op} {b})"))
                }
                1 => {
                    let a = self.expr(ty, false, d);
                    Some(format!("(-{a})"))
                }
                _ if *ty == Ty::Int => {
                    let elem = self.prim_ty();
                    let list = self.expr(&Ty::List(Box::new(elem)), false, d);
                    Some(format!("{list}.count"))
                }
                _ => None,
            },
            Ty::Bool => match self.pick(4) {
                0 => {
                    let t = if self.chance(2) { Ty::Int } else { Ty::Double };
                    let op = ["<", "<=", ">", ">=", "==", "!="][self.pick(6)];
                    let a = self.expr(&t, false, d);
                    let b = self.expr(&t, false, d);
                    Some(format!("({a} {op} {b})"))
                }
                1 => {
                    let op = if self.chance(2) { "and" } else { "or" };
                    let a = self.expr(&Ty::Bool, true, d);
                    let b = self.expr(&Ty::Bool, true, d);
                    Some(format!("({a} {op} {b})"))
                }
                2 => {
                    let a = self.expr(&Ty::Bool, true, d);
                    Some(format!("(not {a})"))
                }
                _ => {
                    let op = if self.chance(2) { "==" } else { "!=" };
                    let a = self.expr(&Ty::Str, false, d);
                    let b = self.expr(&Ty::Str, false, d);
                    Some(format!("({a} {op} {b})"))
                }
            },
            Ty::Str => {
                if self.chance(2) || self.in_interp {
                    let a = self.expr(&Ty::Str, false, d);
                    let b = self.expr(&Ty::Str, false, d);
                    Some(format!("({a} + {b})"))
                } else {
                    let n = self.range(1, 3);
                    let mut s = String::from("\"");
                    let outer = self.in_interp;
                    self.in_interp = true;
                    for _ in 0..n {
                        s.push_str(WORDS[self.pick(WORDS.len())]);
                        let t = self.prim_ty();
                        let e = self.expr(&t, false, d);
                        let _ = write!(s, "{{{e}}}");
                    }
                    self.in_interp = outer;
                    s.push('"');
                    Some(s)
                }
            }
            Ty::Opt(t) => Some(self.expr(t, known, d)),
            Ty::Named(k) => Some(self.construct(*k, known, d)),
            Ty::List(t) => {
                let n = self.range(1, 3);
                let parts: Vec<String> = (0..n).map(|_| self.expr(t, known, d)).collect();
                Some(format!("[{}]", parts.join(", ")))
            }
        }
    }

    /// `receiver.field` of type `ty`.
    fn field_of(&mut self, ty: &Ty, d: u32) -> Option<String> {
        let mut found = Vec::new();
        for (k, def) in self.types.iter().enumerate() {
            if self.lib_only && !def.lib {
                continue;
            }
            if let Kind::Struct { fields, .. } = &def.kind {
                for f in fields {
                    if &f.ty == ty {
                        found.push((k, f.name.clone()));
                    }
                }
            }
        }
        if found.is_empty() {
            return None;
        }
        let (k, field) = found[self.pick(found.len())].clone();
        let recv = self.expr(&Ty::Named(k), false, d);
        Some(format!("{recv}.{field}"))
    }

    /// A call of a free function returning `ty`.
    fn call_of(&mut self, ty: &Ty, d: u32) -> Option<String> {
        if self.lib_only {
            return None;
        }
        let found: Vec<Sig> = self
            .funcs
            .iter()
            .enumerate()
            .filter(|(i, f)| f.sig.ret.as_ref() == Some(ty) && self.may_call(*i))
            .map(|(_, f)| f.sig.clone())
            .collect();
        if found.is_empty() {
            return None;
        }
        let sig = found[self.pick(found.len())].clone();
        let args = self.args(&sig.params, d);
        Some(format!("{}({args})", sig.name))
    }

    /// `receiver.method(…)` returning `ty`.
    fn method_of(&mut self, ty: &Ty, d: u32) -> Option<String> {
        if self.runnable && self.in_method {
            return None;
        }
        let mut found = Vec::new();
        for (k, def) in self.types.iter().enumerate() {
            if self.lib_only && !def.lib {
                continue;
            }
            if let Kind::Struct { methods, .. } = &def.kind {
                for m in methods {
                    if m.ret.as_ref() == Some(ty) {
                        found.push((k, m.clone()));
                    }
                }
            }
        }
        if found.is_empty() {
            return None;
        }
        let (k, sig) = found[self.pick(found.len())].clone();
        let recv = self.expr(&Ty::Named(k), false, d);
        let args = self.args(&sig.params, d);
        Some(format!("{recv}.{}({args})", sig.name))
    }

    /// An expression of type `inner?` that is not a literal (so `??`,
    /// `if let` and `match` have something that can really be `nil`): a
    /// local, field or call.
    fn opt_source(&mut self, inner: &Ty, d: u32) -> Option<String> {
        let want = Ty::Opt(Box::new(inner.clone()));
        let locals: Vec<String> = self
            .locals
            .iter()
            .filter(|l| l.ty == want)
            .map(|l| l.name.clone())
            .collect();
        if !locals.is_empty() && self.pick(3) != 0 {
            return Some(locals[self.pick(locals.len())].clone());
        }
        if d >= MAX_DEPTH {
            return locals.first().cloned();
        }
        let r = if self.chance(2) {
            self.field_of(&want, d)
        } else {
            self.call_of(&want, d)
        };
        r.or_else(|| locals.first().cloned())
    }
}

fn push_decl(body: &mut String, text: &str) {
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(text);
}

// ------------------------------------------------------- mistake injection

/// The kinds of mistake [`inject_fixable_mistakes`] makes.
pub const MISTAKES: &[&str] = &[
    "duplicate-import",
    "unused-import",
    "misspelt-import",
    "missing-import",
    "symbolic-and",
    "symbolic-or",
    "symbolic-not",
    "semicolon",
];

/// Makes one to four mistakes in `program`'s `logic` files that the checker
/// reports with a quick fix (see [`MISTAKES`]), and returns the new files.
/// Mistakes that don't apply to the program (no import to duplicate, no
/// `and` to replace) are skipped, so the result may equal the input.
pub fn inject_fixable_mistakes(
    program: &Program,
    u: &mut Unstructured<'_>,
) -> Vec<(String, String)> {
    let mut files = program.files.clone();
    let logic: Vec<usize> = (0..files.len())
        .filter(|&i| files[i].0.starts_with(&format!("src/{LOGIC}/")))
        .collect();
    let model: Vec<String> = files
        .iter()
        .filter_map(|(p, _)| {
            p.strip_prefix(&format!("src/{MODEL}/"))
                .and_then(|n| n.strip_suffix(".k"))
                .map(str::to_owned)
        })
        .collect();
    if logic.is_empty() {
        return files;
    }
    let n = 1 + u.choose_index(4).unwrap_or(0);
    for _ in 0..n {
        let f = logic[u.choose_index(logic.len()).unwrap_or(0)];
        let kind = u.choose_index(MISTAKES.len()).unwrap_or(0);
        let text = &mut files[f].1;
        let imports: Vec<String> = text
            .lines()
            .filter(|l| l.starts_with("import "))
            .map(str::to_owned)
            .collect();
        match MISTAKES[kind] {
            "duplicate-import" => {
                if let Some(line) = imports.first() {
                    text.insert_str(0, &format!("{line}\n"));
                }
            }
            "unused-import" => {
                if let Some(name) = model
                    .iter()
                    .find(|m| !imports.iter().any(|l| l == &format!("import {MODEL}.{m}")))
                {
                    text.insert_str(0, &format!("import {MODEL}.{name}\n"));
                }
            }
            "misspelt-import" => {
                if let Some(line) = imports.first() {
                    // `import model.T0` -> `import model.T0x`: one edit away.
                    *text = text.replacen(line.as_str(), &format!("{line}x"), 1);
                }
            }
            "missing-import" => {
                if let Some(line) = imports.first() {
                    *text = text.replacen(&format!("{line}\n"), "", 1);
                }
            }
            "symbolic-and" => *text = text.replacen(" and ", " && ", 1),
            "symbolic-or" => *text = text.replacen(" or ", " || ", 1),
            "symbolic-not" => *text = text.replacen("(not ", "(!", 1),
            _ => {
                if let Some(pos) = text.find("    return ")
                    && let Some(end) = text[pos..].find('\n')
                {
                    text.insert(pos + end, ';');
                }
            }
        }
    }
    files
}
