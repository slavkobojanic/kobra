//! **Closed expressions with a reference evaluator**, for the differential
//! property "constant folding agrees with the runtime"
//! (`docs/compiler/testing.md`, "Stage 3 properties").
//!
//! [`generate`] builds typed expression trees over `Int`, `Double`,
//! `Bool` and `String` from an [`Unstructured`] byte source, evaluates
//! them in Rust with K's semantics (`docs/design/08-types.md`, the `run`
//! corpus under `tests/corpus/run/{ints,doubles,strings}`), and writes a
//! one-file project whose `main` returns the values twice:
//!
//! * **inline**: the expressions as written, all literals, which the code
//!   generator is free to fold at build time;
//! * **opaque**: every literal wrapped in an identity function
//!   (`oi(3)`), which it can't fold, so V8 computes everything.
//!
//! Both must equal the reference, and so must a panic: the first one in
//! evaluation order (left to right; `and`, `or` and `if` evaluate only
//! what they need), which ends `main` before anything later runs.
//!
//! The semantics the reference encodes:
//!
//! * `Int` is a safe integer (±(2^53 − 1)): `+`, `-`, `*` and unary `-`
//!   panic with `integer overflow: …` outside it; `/` truncates toward
//!   zero and `%` has the dividend's sign; both panic with `division by
//!   zero`;
//! * `Double` is IEEE 754 (`%` is `fmod`); a value that would be written
//!   non-finite, or interpolated outside the range where the shortest
//!   round-trip form has no exponent, is replaced by a literal while
//!   generating, so the reference never depends on an unspecified format;
//! * strings compare by UTF-16 code units; `count` and `prefix` count
//!   grapheme clusters, and the alphabet has no combining characters, so
//!   a grapheme is a scalar here; interpolation writes `Int` in decimal,
//!   `Bool` as `true`/`false` and `Double` in the shortest round-trip form
//!   with at least one decimal.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::time::Duration;

use arbitrary::Unstructured;

use crate::coracle;

/// The largest safe integer, `2^53 − 1`.
pub const MAX_SAFE: i128 = 9_007_199_254_740_991;

/// The value types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    /// `Int`.
    Int,
    /// `Double`.
    Double,
    /// `Bool`.
    Bool,
    /// `String`.
    Str,
}

/// A K value.
#[derive(Clone, PartialEq, Debug)]
pub enum Val {
    /// An `Int`.
    Int(i64),
    /// A `Double`.
    Double(f64),
    /// A `Bool`.
    Bool(bool),
    /// A `String`.
    Str(String),
}

/// A panic of the reference evaluation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panic {
    /// `integer overflow: …`.
    Overflow,
    /// `division by zero`.
    DivisionByZero,
}

/// An arithmetic operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arith {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
}

/// A comparison operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

/// A `String` method returning `Bool`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Search {
    /// `contains`
    Contains,
    /// `startsWith`
    StartsWith,
    /// `endsWith`
    EndsWith,
}

/// An expression.
#[derive(Clone, PartialEq, Debug)]
pub enum E {
    /// A literal (wrapped in an identity call in the opaque form).
    Lit(Val),
    /// `a op b` on `Int` or `Double` (the operands' type).
    Arith(Arith, Ty, Box<E>, Box<E>),
    /// `-a` on `Int` or `Double`.
    Neg(Ty, Box<E>),
    /// `a op b`, operands of the given type.
    Cmp(Cmp, Ty, Box<E>, Box<E>),
    /// `a and b`
    And(Box<E>, Box<E>),
    /// `a or b`
    Or(Box<E>, Box<E>),
    /// `not a`
    Not(Box<E>),
    /// `if c { a } else { b }`, of the given type.
    If(Ty, Box<E>, Box<E>, Box<E>),
    /// `a + b` on strings.
    Concat(Box<E>, Box<E>),
    /// `"lit{e}lit{e}…"`: literal text and interpolated expressions.
    Interp(Vec<(String, E, Ty)>, String),
    /// `s.count`
    Count(Box<E>),
    /// `s.isEmpty`
    IsEmpty(Box<E>),
    /// `s.contains(t)` and friends.
    Search(Search, Box<E>, Box<E>),
    /// `s.replace("target", with: r)` with a non-empty literal target.
    Replace(Box<E>, String, Box<E>),
    /// `s.prefix(n)`, `n` a literal in `0..=5`.
    Prefix(Box<E>, i64),
    /// `s.repeated(times: n)`, `n` a literal in `0..=3`.
    Repeated(Box<E>, i64),
    /// `[e, …].count` over `Int` elements.
    ListCount(Vec<E>),
}

/// A generated case: the project and what `main` must return.
#[derive(Clone, Debug)]
pub struct Case {
    /// The project.
    pub files: Vec<(String, String)>,
    /// The JSON `main` returns, or the panic that ends it.
    pub expected: Result<serde_json::Value, Panic>,
}

/// The pieces strings are made of: ASCII, characters K escapes in
/// literals, two-byte, three-byte and astral (two UTF-16 units) scalars,
/// and markup characters. None combines with a neighbour into one
/// grapheme cluster.
pub const PIECES: &[&str] = &[
    "a",
    "b",
    "ab",
    "",
    "Z",
    " ",
    "-",
    "\"",
    "\\",
    "{",
    "}",
    "\u{e9}",
    "\u{df}",
    "\u{4e2d}",
    "\u{1F642}",
    "\n",
    "\t",
    "<b>",
    "&",
    "aa",
];

const INTS: &[i64] = &[
    0,
    1,
    2,
    3,
    7,
    10,
    100,
    4_503_599_627_370_496,
    9_007_199_254_740_991,
    3_037_000_499,
];

const DOUBLES: &[&str] = &[
    "0.0", "0.5", "1.5", "0.1", "0.2", "2.25", "3.0", "1000.0", "0.001", "123.456", "1.0e10",
];

const MAX_DEPTH: u32 = 4;

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    budget: u32,
}

impl Gen<'_, '_> {
    fn pick(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        self.u.choose_index(n).unwrap_or(0)
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.pick(one_in) == one_in - 1
    }

    fn string_literal(&mut self) -> String {
        let n = self.pick(4);
        (0..n).map(|_| PIECES[self.pick(PIECES.len())]).collect()
    }

    fn literal(&mut self, ty: Ty) -> E {
        E::Lit(match ty {
            Ty::Int => {
                let v = INTS[self.pick(INTS.len())];
                Val::Int(if self.chance(3) { -v } else { v })
            }
            Ty::Double => {
                let v: f64 = DOUBLES[self.pick(DOUBLES.len())].parse().unwrap_or(0.0);
                Val::Double(if self.chance(3) { -v } else { v })
            }
            Ty::Bool => Val::Bool(self.chance(2)),
            Ty::Str => Val::Str(self.string_literal()),
        })
    }

    fn expr(&mut self, ty: Ty, depth: u32) -> E {
        self.budget = self.budget.saturating_sub(1);
        if depth >= MAX_DEPTH || self.budget == 0 || self.pick(4) == 0 {
            return self.literal(ty);
        }
        let d = depth + 1;
        let b = |e: E| Box::new(e);
        match ty {
            Ty::Int | Ty::Double => match self.pick(if ty == Ty::Int { 6 } else { 4 }) {
                0 => {
                    let op =
                        [Arith::Add, Arith::Sub, Arith::Mul, Arith::Div, Arith::Rem][self.pick(5)];
                    E::Arith(op, ty, b(self.expr(ty, d)), b(self.expr(ty, d)))
                }
                1 => E::Neg(ty, b(self.expr(ty, d))),
                2 => self.if_expr(ty, d),
                3 => {
                    let op = [Arith::Add, Arith::Sub, Arith::Mul][self.pick(3)];
                    E::Arith(op, ty, b(self.expr(ty, d)), b(self.expr(ty, d)))
                }
                4 => E::Count(b(self.expr(Ty::Str, d))),
                _ => {
                    let n = self.pick(4);
                    E::ListCount((0..n).map(|_| self.expr(Ty::Int, d)).collect())
                }
            },
            Ty::Bool => match self.pick(7) {
                0 => {
                    let t = [Ty::Int, Ty::Double, Ty::Str][self.pick(3)];
                    let op = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge][self.pick(6)];
                    E::Cmp(op, t, b(self.expr(t, d)), b(self.expr(t, d)))
                }
                1 => E::And(b(self.expr(Ty::Bool, d)), b(self.expr(Ty::Bool, d))),
                2 => E::Or(b(self.expr(Ty::Bool, d)), b(self.expr(Ty::Bool, d))),
                3 => E::Not(b(self.expr(Ty::Bool, d))),
                4 => E::IsEmpty(b(self.expr(Ty::Str, d))),
                5 => {
                    let s = [Search::Contains, Search::StartsWith, Search::EndsWith][self.pick(3)];
                    E::Search(s, b(self.expr(Ty::Str, d)), b(self.expr(Ty::Str, d)))
                }
                _ => self.if_expr(Ty::Bool, d),
            },
            Ty::Str => match self.pick(6) {
                0 => E::Concat(b(self.expr(Ty::Str, d)), b(self.expr(Ty::Str, d))),
                1 => {
                    let n = 1 + self.pick(3);
                    let mut parts = Vec::new();
                    for _ in 0..n {
                        let text = self.string_literal();
                        let t = [Ty::Int, Ty::Double, Ty::Bool, Ty::Str][self.pick(4)];
                        let e = self.expr(t, d);
                        parts.push((text, e, t));
                    }
                    let tail = self.string_literal();
                    E::Interp(parts, tail)
                }
                2 => {
                    let mut target = self.string_literal();
                    if target.is_empty() {
                        target.push('a');
                    }
                    E::Replace(b(self.expr(Ty::Str, d)), target, b(self.expr(Ty::Str, d)))
                }
                3 => {
                    let n = i64::try_from(self.pick(6)).unwrap_or(0);
                    E::Prefix(b(self.expr(Ty::Str, d)), n)
                }
                4 => {
                    let n = i64::try_from(self.pick(4)).unwrap_or(0);
                    E::Repeated(b(self.expr(Ty::Str, d)), n)
                }
                _ => self.if_expr(Ty::Str, d),
            },
        }
    }

    fn if_expr(&mut self, ty: Ty, d: u32) -> E {
        let c = self.expr(Ty::Bool, d);
        let a = self.expr(ty, d);
        let e = self.expr(ty, d);
        E::If(ty, Box::new(c), Box::new(a), Box::new(e))
    }

    /// An element of a result list: regenerated as a literal when its
    /// value could only be compared through an unspecified format.
    fn element(&mut self, ty: Ty) -> E {
        let e = self.expr(ty, 0);
        match eval(&e) {
            Err(Stop::Unformattable) => self.literal(ty),
            Ok(Val::Double(x)) if !x.is_finite() => self.literal(ty),
            _ => e,
        }
    }
}

/// Code generation findings this generator steers around, so the
/// differential property stays meaningful for everything else. Each is
/// pinned by `pinned_findings_still_reproduce` in
/// `crates/k_server/tests/differential.rs`; when it is fixed, that test
/// fails, and the entry, the pin and the workaround go together. None is
/// open: `release-minus-minus` (`a - (-b)` printed `a--b`) is fixed and
/// guarded by `fixed_findings_stay_fixed`, so `a - (-b)` is generated again.
pub const KNOWN_FINDINGS: &[(&str, &str)] = &[];

/// Why evaluation stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    /// A K panic.
    Panic(Panic),
    /// A `Double` was interpolated outside the range the reference formats.
    Unformattable,
}

fn int(v: i128) -> Result<Val, Stop> {
    if v.abs() > MAX_SAFE {
        Err(Stop::Panic(Panic::Overflow))
    } else {
        Ok(Val::Int(i64::try_from(v).unwrap_or(0)))
    }
}

fn as_int(v: &Val) -> i128 {
    match v {
        Val::Int(i) => i128::from(*i),
        _ => 0,
    }
}

fn as_f64(v: &Val) -> f64 {
    match v {
        Val::Double(x) => *x,
        _ => 0.0,
    }
}

fn as_bool(v: &Val) -> bool {
    matches!(v, Val::Bool(true))
}

fn as_str(v: &Val) -> &str {
    match v {
        Val::Str(s) => s,
        _ => "",
    }
}

/// Formats a `Double` as K interpolates it, or `None` outside the range
/// where the shortest round-trip form has no exponent (and for `-0.0`).
pub fn format_double(x: f64) -> Option<String> {
    if !x.is_finite() || (x == 0.0 && x.is_sign_negative()) {
        return None;
    }
    if x != 0.0 && !(1e-6..1e15).contains(&x.abs()) {
        return None;
    }
    let mut s = format!("{x}");
    if !s.contains('.') {
        s.push_str(".0");
    }
    Some(s)
}

/// Evaluates `e` with K's semantics.
///
/// # Errors
///
/// The panic (or unformattable value) that stops the evaluation.
pub fn eval(e: &E) -> Result<Val, Stop> {
    Ok(match e {
        E::Lit(v) => v.clone(),
        E::Arith(op, Ty::Int, a, b) => {
            let (x, y) = (as_int(&eval(a)?), as_int(&eval(b)?));
            match op {
                Arith::Add => int(x + y)?,
                Arith::Sub => int(x - y)?,
                Arith::Mul => int(x * y)?,
                Arith::Div | Arith::Rem if y == 0 => {
                    return Err(Stop::Panic(Panic::DivisionByZero));
                }
                Arith::Div => int(x / y)?,
                Arith::Rem => int(x % y)?,
            }
        }
        E::Arith(op, _, a, b) => {
            let (x, y) = (as_f64(&eval(a)?), as_f64(&eval(b)?));
            Val::Double(match op {
                Arith::Add => x + y,
                Arith::Sub => x - y,
                Arith::Mul => x * y,
                Arith::Div => x / y,
                Arith::Rem => x % y,
            })
        }
        E::Neg(Ty::Int, a) => int(-as_int(&eval(a)?))?,
        E::Neg(_, a) => Val::Double(-as_f64(&eval(a)?)),
        E::Cmp(op, t, a, b) => {
            let (x, y) = (eval(a)?, eval(b)?);
            let ord = match t {
                Ty::Int => Some(as_int(&x).cmp(&as_int(&y))),
                Ty::Double => as_f64(&x).partial_cmp(&as_f64(&y)),
                _ => Some(as_str(&x).encode_utf16().cmp(as_str(&y).encode_utf16())),
            };
            use std::cmp::Ordering::{Equal, Greater, Less};
            Val::Bool(match (op, ord) {
                (Cmp::Ne, None) => true,
                (_, None) => false,
                (Cmp::Eq, Some(o)) => o == Equal,
                (Cmp::Ne, Some(o)) => o != Equal,
                (Cmp::Lt, Some(o)) => o == Less,
                (Cmp::Le, Some(o)) => o != Greater,
                (Cmp::Gt, Some(o)) => o == Greater,
                (Cmp::Ge, Some(o)) => o != Less,
            })
        }
        E::And(a, b) => Val::Bool(as_bool(&eval(a)?) && as_bool(&eval(b)?)),
        E::Or(a, b) => Val::Bool(as_bool(&eval(a)?) || as_bool(&eval(b)?)),
        E::Not(a) => Val::Bool(!as_bool(&eval(a)?)),
        E::If(_, c, a, b) => {
            if as_bool(&eval(c)?) {
                eval(a)?
            } else {
                eval(b)?
            }
        }
        E::Concat(a, b) => {
            let x = eval(a)?;
            let y = eval(b)?;
            Val::Str(format!("{}{}", as_str(&x), as_str(&y)))
        }
        E::Interp(parts, tail) => {
            let mut s = String::new();
            for (text, e, _) in parts {
                s.push_str(text);
                match eval(e)? {
                    Val::Int(i) => {
                        let _ = write!(s, "{i}");
                    }
                    Val::Double(x) => s.push_str(&format_double(x).ok_or(Stop::Unformattable)?),
                    Val::Bool(b) => {
                        let _ = write!(s, "{b}");
                    }
                    Val::Str(t) => s.push_str(&t),
                }
            }
            s.push_str(tail);
            Val::Str(s)
        }
        E::Count(a) => Val::Int(i64::try_from(as_str(&eval(a)?).chars().count()).unwrap_or(0)),
        E::IsEmpty(a) => Val::Bool(as_str(&eval(a)?).is_empty()),
        E::Search(kind, a, b) => {
            let x = eval(a)?;
            let y = eval(b)?;
            let (x, y) = (as_str(&x), as_str(&y));
            Val::Bool(match kind {
                Search::Contains => x.contains(y),
                Search::StartsWith => x.starts_with(y),
                Search::EndsWith => x.ends_with(y),
            })
        }
        E::Replace(a, target, b) => {
            let x = eval(a)?;
            let y = eval(b)?;
            Val::Str(as_str(&x).replace(target.as_str(), as_str(&y)))
        }
        E::Prefix(a, n) => Val::Str(
            as_str(&eval(a)?)
                .chars()
                .take(usize::try_from(*n).unwrap_or(0))
                .collect(),
        ),
        E::Repeated(a, n) => Val::Str(as_str(&eval(a)?).repeat(usize::try_from(*n).unwrap_or(0))),
        E::ListCount(items) => {
            for item in items {
                eval(item)?;
            }
            Val::Int(i64::try_from(items.len()).unwrap_or(0))
        }
    })
}

/// Writes `text` as a K string literal.
pub fn string_literal(text: &str) -> String {
    let mut out = String::from("\"");
    out.push_str(&escape(text));
    out.push('"');
    out
}

/// Escapes `text` for the inside of a K string literal.
pub fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '{' => out.push_str("{{"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:X}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Writes `e`; `opaque` wraps every literal in its identity function.
pub fn render(e: &E, opaque: bool) -> String {
    let r = |e: &E| render(e, opaque);
    match e {
        E::Lit(v) => {
            let (text, f) = match v {
                Val::Int(i) if *i < 0 => (format!("(-{})", i.unsigned_abs()), "oi"),
                Val::Int(i) => (i.to_string(), "oi"),
                Val::Double(x) => {
                    let body = format!("{:?}", x.abs());
                    let body = if body.contains('e') {
                        // `1e10` is written `10000000000.0`.
                        format!("{:.1}", x.abs())
                    } else {
                        body
                    };
                    if x.is_sign_negative() {
                        (format!("(-{body})"), "od")
                    } else {
                        (body, "od")
                    }
                }
                Val::Bool(b) => (b.to_string(), "ob"),
                Val::Str(s) => (string_literal(s), "os"),
            };
            if opaque { format!("{f}({text})") } else { text }
        }
        E::Arith(op, _, a, b) => {
            let o = match op {
                Arith::Add => "+",
                Arith::Sub => "-",
                Arith::Mul => "*",
                Arith::Div => "/",
                Arith::Rem => "%",
            };
            format!("({} {o} {})", r(a), r(b))
        }
        E::Neg(_, a) => format!("(-{})", r(a)),
        E::Cmp(op, _, a, b) => {
            let o = match op {
                Cmp::Eq => "==",
                Cmp::Ne => "!=",
                Cmp::Lt => "<",
                Cmp::Le => "<=",
                Cmp::Gt => ">",
                Cmp::Ge => ">=",
            };
            format!("({} {o} {})", r(a), r(b))
        }
        E::And(a, b) => format!("({} and {})", r(a), r(b)),
        E::Or(a, b) => format!("({} or {})", r(a), r(b)),
        E::Not(a) => format!("(not {})", r(a)),
        E::If(_, c, a, b) => format!("(if {} {{ {} }} else {{ {} }})", r(c), r(a), r(b)),
        E::Concat(a, b) => format!("({} + {})", r(a), r(b)),
        E::Interp(parts, tail) => {
            let mut s = String::from("\"");
            for (text, e, _) in parts {
                s.push_str(&escape(text));
                let _ = write!(s, "{{{}}}", r(e));
            }
            s.push_str(&escape(tail));
            s.push('"');
            s
        }
        E::Count(a) => format!("{}.count", r(a)),
        E::IsEmpty(a) => format!("{}.isEmpty", r(a)),
        E::Search(kind, a, b) => {
            let m = match kind {
                Search::Contains => "contains",
                Search::StartsWith => "startsWith",
                Search::EndsWith => "endsWith",
            };
            format!("{}.{m}({})", r(a), r(b))
        }
        E::Replace(a, target, b) => {
            format!(
                "{}.replace({}, with: {})",
                r(a),
                string_literal(target),
                r(b)
            )
        }
        E::Prefix(a, n) => format!("{}.prefix({n})", r(a)),
        E::Repeated(a, n) => format!("{}.repeated(times: {n})", r(a)),
        E::ListCount(items) => {
            let parts: Vec<String> = items.iter().map(r).collect();
            if parts.is_empty() {
                // `[].count` has no element type to infer.
                "oiList([]).count".to_owned()
            } else {
                format!("[{}].count", parts.join(", "))
            }
        }
    }
}

/// The module of a generated case.
pub const MODULE: &str = "calc";

/// The entry of a generated case.
pub const ENTRY: &str = "calc.main";

const PRELUDE: &str = "struct Results implements Codable {
    let ints: List<Int>
    let doubles: List<Double>
    let bools: List<Bool>
    let strings: List<String>
}

func oi(_ x: Int) -> Int {
    return x
}

func od(_ x: Double) -> Double {
    return x
}

func ob(_ x: Bool) -> Bool {
    return x
}

func os(_ x: String) -> String {
    return x
}

func oiList(_ x: List<Int>) -> List<Int> {
    return x
}
";

/// Generates one case from `u`.
pub fn generate(u: &mut Unstructured<'_>) -> Case {
    let mut g = Gen { u, budget: 160 };
    let mut lists: [Vec<E>; 4] = Default::default();
    for (i, ty) in [Ty::Int, Ty::Double, Ty::Bool, Ty::Str]
        .into_iter()
        .enumerate()
    {
        let n = g.pick(5);
        for _ in 0..n {
            let e = g.element(ty);
            lists[i].push(e);
        }
    }
    // The expected result: the first panic in evaluation order, or the
    // two (equal) results.
    let mut expected: Result<serde_json::Value, Panic> = Ok(serde_json::Value::Null);
    let mut json = serde_json::Map::new();
    'eval: for (i, key) in ["ints", "doubles", "bools", "strings"].iter().enumerate() {
        let mut values = Vec::new();
        for e in &lists[i] {
            match eval(e) {
                Ok(Val::Int(v)) => values.push(serde_json::json!(v)),
                Ok(Val::Double(v)) => values.push(serde_json::json!(v)),
                Ok(Val::Bool(v)) => values.push(serde_json::json!(v)),
                Ok(Val::Str(v)) => values.push(serde_json::json!(v)),
                Err(Stop::Panic(p)) => {
                    expected = Err(p);
                    break 'eval;
                }
                Err(Stop::Unformattable) => {
                    unreachable!("elements are regenerated when unformattable")
                }
            }
        }
        json.insert((*key).to_owned(), serde_json::Value::Array(values));
    }
    if expected.is_ok() {
        let one = serde_json::Value::Object(json);
        expected = Ok(serde_json::Value::Array(vec![one.clone(), one]));
    }
    let results = |opaque: bool| {
        let list = |items: &[E]| {
            let parts: Vec<String> = items.iter().map(|e| render(e, opaque)).collect();
            format!("[{}]", parts.join(", "))
        };
        format!(
            "Results(ints: {}, doubles: {}, bools: {}, strings: {})",
            list(&lists[0]),
            list(&lists[1]),
            list(&lists[2]),
            list(&lists[3])
        )
    };
    let main = format!(
        "\nfunc main() -> List<Results> {{\n    let inline = {}\n    let opaque = {}\n    return [inline, opaque]\n}}\n",
        results(false),
        results(true)
    );
    let source = format!("{PRELUDE}{main}");
    Case {
        files: vec![
            (
                "k.toml".to_owned(),
                "[package]\nname = \"gen\"\nk = \"0.1\"\n".to_owned(),
            ),
            (format!("src/{MODULE}/Main.k"), source),
        ],
        expected,
    }
}

/// Checks one case: [`coracle::check_terminating_program`] (codegen
/// invariants, no unexpected exception, development equals release), and
/// the result equals the reference.
///
/// # Errors
///
/// A description of the disagreement, with the project.
pub fn check(case: &Case, limit: Duration) -> Result<(), String> {
    let run = coracle::check_terminating_program(&case.files, ENTRY, limit)?;
    let ok = match (&case.expected, &run.outcome) {
        (Ok(want), coracle::Outcome::Returned(got)) => coracle::json_eq(want, got),
        (Err(Panic::Overflow), coracle::Outcome::Panicked(m)) => {
            m.starts_with("integer overflow: ") && m.ends_with(" is outside the Int range")
        }
        (Err(Panic::DivisionByZero), coracle::Outcome::Panicked(m)) => m == "division by zero",
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "the program disagrees with the reference evaluation:\n  expected: {:?}\n  actual:   {:?}\n{}",
            case.expected,
            run.outcome,
            coracle::dump(&case.files)
        ))
    }
}
