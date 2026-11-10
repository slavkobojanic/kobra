//! **Generated `Codable` types and values**, for the round-trip and
//! decoding properties of the JSON wire format
//! (`docs/design/28-rpc-wire-format.md`, `docs/compiler/codegen.md` §5.1,
//! `docs/compiler/testing.md`, "Stage 3 properties").
//!
//! [`generate`] declares one to four `Codable, Equatable` structs and enums
//! over `Int`, `Double`, `Bool`, `String`, `T?`, `List<T>`,
//! `Map<String, T>` and earlier declarations, builds a value of a root
//! type, and computes in Rust:
//!
//! * the **reference encoding** of the value: fields by K name, payload-free
//!   cases as their name, payload cases as `{"case": name, label: …}`,
//!   `nil` as `null`, maps as objects;
//! * **mutations** of that JSON (a node replaced by a value of another
//!   shape, a key removed or added, `__proto__` keys, an unknown case name,
//!   truncated or padded text) and what the **reference decoder** makes of
//!   each: a value (whose encoding it predicts), `malformed` (not JSON) or
//!   `mismatch` (JSON of the wrong shape). Unknown keys are ignored; a
//!   missing or `null` optional field is `nil`; an `Int` is any JSON number
//!   with a safe-integer value (`1.0` and `1e2` included, since JavaScript
//!   can't tell them from `1` and `100`).
//!
//! The generated `main` encodes the value, decodes the text, compares, and
//! decodes every mutation through `Json.decode`, which shares its codecs
//! with RPC dispatch.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::time::Duration;

use arbitrary::Unstructured;
use serde_json::{Map, Value};

use crate::coracle;
use crate::exprgen::{MAX_SAFE, PIECES, string_literal};

/// A `Codable` type.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CTy {
    /// `Int`
    Int,
    /// `Double`
    Double,
    /// `Bool`
    Bool,
    /// `String`
    Str,
    /// `T?` (never of an optional)
    Opt(Box<CTy>),
    /// `List<T>`
    List(Box<CTy>),
    /// `Map<String, T>`
    Map(Box<CTy>),
    /// A declaration, by index.
    Named(usize),
}

/// A declaration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Decl {
    /// `struct C<i> { let f<j>: T … }`
    Struct(Vec<CTy>),
    /// `enum C<i> { case c<j>(a<k>: T, …) … }`
    Enum(Vec<Vec<CTy>>),
}

/// A value.
#[derive(Clone, PartialEq, Debug)]
pub enum CVal {
    /// An `Int`.
    Int(i64),
    /// A `Double`.
    Double(f64),
    /// A `Bool`.
    Bool(bool),
    /// A `String`.
    Str(String),
    /// `nil`.
    Nil,
    /// A list.
    List(Vec<CVal>),
    /// A map, in insertion order, keys unique.
    Map(Vec<(String, CVal)>),
    /// A struct's fields, in order.
    Struct(Vec<CVal>),
    /// An enum case and its payload.
    Case(usize, Vec<CVal>),
}

/// What decoding one text gives.
#[derive(Clone, PartialEq, Debug)]
pub enum Decoded {
    /// A value, by its reference encoding.
    Ok(Value),
    /// `DecodingError.malformed`.
    Malformed,
    /// `DecodingError.mismatch`.
    Mismatch,
    /// The text has a shape the wire format doesn't specify (a payload-free
    /// case written as `{"case": "c0"}`, or a payload case whose fields are
    /// all optional written as its bare name `"c0"`): a value or
    /// `mismatch`, but never `malformed` or an exception.
    Unspecified,
}

/// A generated case.
#[derive(Clone, Debug)]
pub struct Case {
    /// The declarations.
    pub decls: Vec<Decl>,
    /// The root type.
    pub root: CTy,
    /// The value.
    pub value: CVal,
    /// Its reference encoding.
    pub json: Value,
    /// Mutated texts and what decoding each must give.
    pub inputs: Vec<(String, Decoded)>,
    /// The project.
    pub files: Vec<(String, String)>,
}

/// Map keys and strings: the string pieces plus keys that are special to
/// JavaScript objects, `__proto__` included.
const KEYS: &[&str] = &[
    "__proto__",
    "a",
    "b",
    "constructor",
    "",
    "toString",
    "x y",
    "\u{1F642}",
    "hasOwnProperty",
];

/// Code generation and runtime findings this generator steers around, each
/// pinned by a pending case; when it is fixed, the case fails and the entry
/// and the workaround go together. None are open: the `__proto__` map key
/// (`map-key-proto-is-encoded.k`) and arrays decoded as maps
/// (`map-decodes-only-objects.k`) are fixed, and those cases now guard it.
pub const KNOWN_FINDINGS: &[(&str, &str)] = &[];

const INTS: &[i64] = &[
    0,
    1,
    -1,
    42,
    9_007_199_254_740_991,
    -9_007_199_254_740_991,
    1_000_000,
];

const DOUBLES: &[f64] = &[0.0, 0.5, -1.5, 3.0, 0.1, 1.0e10, -2.25, 123.456];

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    decls: Vec<Decl>,
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

    fn ty(&mut self, depth: u32, earlier: usize) -> CTy {
        let n = if depth >= 2 { 4 } else { 7 } + usize::from(earlier > 0);
        match self.pick(n) {
            0 => CTy::Int,
            1 => CTy::Double,
            2 => CTy::Bool,
            3 => CTy::Str,
            4 => {
                let inner = self.ty(depth + 1, earlier);
                if matches!(inner, CTy::Opt(_)) {
                    inner
                } else {
                    CTy::Opt(Box::new(inner))
                }
            }
            5 => CTy::List(Box::new(self.ty(depth + 1, earlier))),
            6 => CTy::Map(Box::new(self.ty(depth + 1, earlier))),
            _ => CTy::Named(self.pick(earlier)),
        }
    }

    fn declarations(&mut self) {
        let n = 1 + self.pick(4);
        for i in 0..n {
            let decl = if self.chance(2) {
                let cases = 1 + self.pick(4);
                Decl::Enum(
                    (0..cases)
                        .map(|_| {
                            let np = if self.chance(2) { 1 + self.pick(2) } else { 0 };
                            (0..np).map(|_| self.ty(1, i)).collect()
                        })
                        .collect(),
                )
            } else {
                let nf = 1 + self.pick(3);
                Decl::Struct((0..nf).map(|_| self.ty(1, i)).collect())
            };
            self.decls.push(decl);
        }
    }

    fn string(&mut self) -> String {
        if self.chance(4) {
            return KEYS[self.pick(KEYS.len())].to_owned();
        }
        let n = self.pick(4);
        (0..n).map(|_| PIECES[self.pick(PIECES.len())]).collect()
    }

    fn value(&mut self, ty: &CTy, depth: u32) -> CVal {
        match ty {
            CTy::Int => CVal::Int(INTS[self.pick(INTS.len())]),
            CTy::Double => CVal::Double(DOUBLES[self.pick(DOUBLES.len())]),
            CTy::Bool => CVal::Bool(self.chance(2)),
            CTy::Str => CVal::Str(self.string()),
            CTy::Opt(t) => {
                if self.chance(3) {
                    CVal::Nil
                } else {
                    self.value(t, depth)
                }
            }
            CTy::List(t) => {
                let n = if depth > 4 { 0 } else { self.pick(3) };
                CVal::List((0..n).map(|_| self.value(t, depth + 1)).collect())
            }
            CTy::Map(t) => {
                let n = if depth > 4 { 0 } else { self.pick(3) };
                let mut entries: Vec<(String, CVal)> = Vec::new();
                for _ in 0..n {
                    let k = KEYS[self.pick(KEYS.len())].to_owned();
                    if entries.iter().any(|(e, _)| *e == k) {
                        continue;
                    }
                    let v = self.value(t, depth + 1);
                    entries.push((k, v));
                }
                CVal::Map(entries)
            }
            CTy::Named(k) => match self.decls[*k].clone() {
                Decl::Struct(fields) => {
                    CVal::Struct(fields.iter().map(|f| self.value(f, depth + 1)).collect())
                }
                Decl::Enum(cases) => {
                    let c = self.pick(cases.len());
                    CVal::Case(
                        c,
                        cases[c].iter().map(|t| self.value(t, depth + 1)).collect(),
                    )
                }
            },
        }
    }
}

/// The K name of type `ty`.
pub fn ty_name(ty: &CTy) -> String {
    match ty {
        CTy::Int => "Int".to_owned(),
        CTy::Double => "Double".to_owned(),
        CTy::Bool => "Bool".to_owned(),
        CTy::Str => "String".to_owned(),
        CTy::Opt(t) => format!("{}?", ty_name(t)),
        CTy::List(t) => format!("List<{}>", ty_name(t)),
        CTy::Map(t) => format!("Map<String, {}>", ty_name(t)),
        CTy::Named(k) => format!("C{k}"),
    }
}

/// Writes the declarations.
pub fn declarations(decls: &[Decl]) -> String {
    let mut out = String::new();
    for (i, d) in decls.iter().enumerate() {
        match d {
            Decl::Struct(fields) => {
                let _ = writeln!(out, "struct C{i} implements Codable, Equatable {{");
                for (j, f) in fields.iter().enumerate() {
                    let _ = writeln!(out, "    let f{j}: {}", ty_name(f));
                }
            }
            Decl::Enum(cases) => {
                let _ = writeln!(out, "enum C{i} implements Codable, Equatable {{");
                for (j, payload) in cases.iter().enumerate() {
                    let _ = write!(out, "    case c{j}");
                    if !payload.is_empty() {
                        let parts: Vec<String> = payload
                            .iter()
                            .enumerate()
                            .map(|(k, t)| format!("a{k}: {}", ty_name(t)))
                            .collect();
                        let _ = write!(out, "({})", parts.join(", "));
                    }
                    out.push('\n');
                }
            }
        }
        out.push_str("}\n\n");
    }
    out
}

fn double_literal(x: f64) -> String {
    let body = format!("{:?}", x.abs());
    let body = if body.contains('e') {
        format!("{:.1}", x.abs())
    } else {
        body
    };
    if x.is_sign_negative() && x != 0.0 {
        format!("(-{body})")
    } else {
        body
    }
}

/// Writes `v` (of type `ty`) as a K expression, where `ty` is expected.
pub fn literal(v: &CVal, ty: &CTy, decls: &[Decl]) -> String {
    match (v, ty) {
        (CVal::Nil, _) => "nil".to_owned(),
        (_, CTy::Opt(t)) => literal(v, t, decls),
        (CVal::Int(i), _) if *i < 0 => format!("(-{})", i.unsigned_abs()),
        (CVal::Int(i), _) => i.to_string(),
        (CVal::Double(x), _) => double_literal(*x),
        (CVal::Bool(b), _) => b.to_string(),
        (CVal::Str(s), _) => string_literal(s),
        (CVal::List(items), CTy::List(t)) => {
            let parts: Vec<String> = items.iter().map(|i| literal(i, t, decls)).collect();
            format!("[{}]", parts.join(", "))
        }
        (CVal::Map(entries), CTy::Map(t)) => {
            if entries.is_empty() {
                return "[:]".to_owned();
            }
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", string_literal(k), literal(v, t, decls)))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        (CVal::Struct(values), CTy::Named(k)) => {
            let Decl::Struct(fields) = &decls[*k] else {
                return "nil".to_owned();
            };
            let parts: Vec<String> = values
                .iter()
                .zip(fields)
                .enumerate()
                .map(|(j, (v, t))| format!("f{j}: {}", literal(v, t, decls)))
                .collect();
            format!("C{k}({})", parts.join(", "))
        }
        (CVal::Case(c, payload), CTy::Named(k)) => {
            let Decl::Enum(cases) = &decls[*k] else {
                return "nil".to_owned();
            };
            if payload.is_empty() {
                format!("C{k}.c{c}")
            } else {
                let parts: Vec<String> = payload
                    .iter()
                    .zip(&cases[*c])
                    .enumerate()
                    .map(|(j, (v, t))| format!("a{j}: {}", literal(v, t, decls)))
                    .collect();
                format!("C{k}.c{c}({})", parts.join(", "))
            }
        }
        _ => "nil".to_owned(),
    }
}

/// The reference encoding of `v`.
pub fn encode(v: &CVal, decls: &[Decl], ty: &CTy) -> Value {
    match (v, ty) {
        (CVal::Nil, _) => Value::Null,
        (_, CTy::Opt(t)) => encode(v, decls, t),
        (CVal::Int(i), _) => Value::from(*i),
        (CVal::Double(x), _) => Value::from(*x),
        (CVal::Bool(b), _) => Value::Bool(*b),
        (CVal::Str(s), _) => Value::String(s.clone()),
        (CVal::List(items), CTy::List(t)) => {
            Value::Array(items.iter().map(|i| encode(i, decls, t)).collect())
        }
        (CVal::Map(entries), CTy::Map(t)) => Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), encode(v, decls, t)))
                .collect(),
        ),
        (CVal::Struct(values), CTy::Named(k)) => {
            let Decl::Struct(fields) = &decls[*k] else {
                return Value::Null;
            };
            Value::Object(
                values
                    .iter()
                    .zip(fields)
                    .enumerate()
                    .map(|(j, (v, t))| (format!("f{j}"), encode(v, decls, t)))
                    .collect(),
            )
        }
        (CVal::Case(c, payload), CTy::Named(k)) => {
            let Decl::Enum(cases) = &decls[*k] else {
                return Value::Null;
            };
            if payload.is_empty() {
                return Value::String(format!("c{c}"));
            }
            let mut m = Map::new();
            m.insert("case".to_owned(), Value::String(format!("c{c}")));
            for (j, (v, t)) in payload.iter().zip(&cases[*c]).enumerate() {
                m.insert(format!("a{j}"), encode(v, decls, t));
            }
            Value::Object(m)
        }
        _ => Value::Null,
    }
}

/// The reference decoder: the decoded value's encoding, or `None` for a
/// mismatch.
pub fn decode(json: &Value, ty: &CTy, decls: &[Decl]) -> Option<Value> {
    decode_noting(json, ty, decls, &mut false)
}

/// [`decode`], setting `unspecified` when the JSON has a shape the wire
/// format leaves open.
fn decode_noting(json: &Value, ty: &CTy, decls: &[Decl], unspecified: &mut bool) -> Option<Value> {
    let decode = |j: &Value, t: &CTy, u: &mut bool| decode_noting(j, t, decls, u);
    if is_huge(json) {
        return match ty {
            CTy::Double => Some(Value::Null),
            CTy::Opt(t) => decode(json, t, unspecified),
            _ => None,
        };
    }
    match ty {
        CTy::Int => {
            let x = json.as_f64()?;
            (x.fract() == 0.0 && x.abs() <= MAX_SAFE as f64).then(|| {
                // `-0` decodes to the Int 0.
                #[allow(clippy::cast_possible_truncation)]
                Value::from(x as i64)
            })
        }
        CTy::Double => json.as_f64().map(Value::from),
        CTy::Bool => json.as_bool().map(Value::Bool),
        CTy::Str => json.as_str().map(|s| Value::String(s.to_owned())),
        CTy::Opt(t) => {
            if json.is_null() {
                Some(Value::Null)
            } else {
                decode(json, t, unspecified)
            }
        }
        CTy::List(t) => json
            .as_array()?
            .iter()
            .map(|i| decode(i, t, unspecified))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        CTy::Map(t) => json
            .as_object()?
            .iter()
            .map(|(k, v)| decode(v, t, unspecified).map(|v| (k.clone(), v)))
            .collect::<Option<Map<_, _>>>()
            .map(Value::Object),
        CTy::Named(k) => match &decls[*k] {
            Decl::Struct(fields) => {
                let obj = json.as_object()?;
                let mut out = Map::new();
                for (j, f) in fields.iter().enumerate() {
                    let key = format!("f{j}");
                    let v = match obj.get(&key) {
                        Some(v) => decode(v, f, unspecified)?,
                        None if matches!(f, CTy::Opt(_)) => Value::Null,
                        None => return None,
                    };
                    out.insert(key, v);
                }
                Some(Value::Object(out))
            }
            Decl::Enum(cases) => {
                if let Some(name) = json.as_str() {
                    let c = case_index(name)?;
                    // A payload case whose fields are all optional, written
                    // as its bare name: the mirror image of the shape below.
                    if cases.get(c).is_some_and(|p| {
                        !p.is_empty() && p.iter().all(|t| matches!(t, CTy::Opt(_)))
                    }) {
                        *unspecified = true;
                        return None;
                    }
                    return cases
                        .get(c)
                        .filter(|p| p.is_empty())
                        .map(|_| Value::String(name.to_owned()));
                }
                let obj = json.as_object()?;
                let name = obj.get("case")?.as_str()?;
                let c = case_index(name)?;
                if cases.get(c).is_some_and(Vec::is_empty) {
                    *unspecified = true;
                    return None;
                }
                let payload = cases.get(c).filter(|p| !p.is_empty())?;
                let mut out = Map::new();
                out.insert("case".to_owned(), Value::String(name.to_owned()));
                for (j, t) in payload.iter().enumerate() {
                    let key = format!("a{j}");
                    let v = match obj.get(&key) {
                        Some(v) => decode(v, t, unspecified)?,
                        None if matches!(t, CTy::Opt(_)) => Value::Null,
                        None => return None,
                    };
                    out.insert(key, v);
                }
                Some(Value::Object(out))
            }
        },
    }
}

/// The key of the object [`tame_huge_numbers`] writes for a number beyond
/// the `Double` range: no reference encoding has it, and it is only ever
/// a value, so it never meets a map key or a field.
pub const HUGE: &str = "\u{0}huge";

fn is_huge(json: &Value) -> bool {
    json.as_object()
        .is_some_and(|m| m.len() == 1 && m.contains_key(HUGE))
}

/// `text` with each JSON number beyond the `Double` range (outside
/// strings) replaced by `{"\u0000huge": true}`, or `None` when it has none.
pub fn tame_huge_numbers(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut from, mut any) = (0, 0, false);
    let mut in_string = false;
    while i < b.len() {
        let c = b[i];
        if in_string {
            match c {
                b'\\' => i += 1,
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if c == b'-' || c.is_ascii_digit() {
            let end = i + b[i..]
                .iter()
                .take_while(|&&d| {
                    d.is_ascii_digit() || matches!(d, b'-' | b'+' | b'.' | b'e' | b'E')
                })
                .count();
            let token = &text[i..end];
            if is_json_number(token) && token.parse::<f64>().is_ok_and(f64::is_infinite) {
                out.push_str(&text[from..i]);
                out.push_str(r#"{"\u0000huge":true}"#);
                from = end;
                any = true;
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out.push_str(&text[from..]);
    any.then_some(out)
}

/// Whether `t` is exactly a JSON number (RFC 8259 §6).
fn is_json_number(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let s = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - s
    };
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(d) if d.is_ascii_digit() => {
            digits(&mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// `c<n>` → `n`, rejecting anything else (`c01`, `c`, `x1`).
fn case_index(name: &str) -> Option<usize> {
    let digits = name.strip_prefix('c')?;
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// What `Json.decode` must make of `text` for type `ty`.
///
/// A number beyond the `Double` range (`1e999`, or `90071e099254740991`,
/// which mutation 4 writes when the integer it rewrites is a digit of a
/// longer one) is JSON: RFC 8259 leaves the range to the implementation,
/// and `JSON.parse` reads it as an infinity. serde_json refuses it ("number
/// out of range"), so it is read here as [`HUGE`]: never an `Int` (a
/// `mismatch`), and as a `Double` the infinity, which encodes as `null`
/// (28-rpc-wire-format.md, "Numbers").
pub fn decode_text(text: &str, ty: &CTy, decls: &[Decl]) -> Decoded {
    let parsed = serde_json::from_str::<Value>(text).or_else(|e| {
        tame_huge_numbers(text)
            .ok_or(e)
            .and_then(|t| serde_json::from_str(&t))
    });
    match parsed {
        Err(_) => Decoded::Malformed,
        Ok(v) => {
            let mut unspecified = false;
            match decode_noting(&v, ty, decls, &mut unspecified) {
                _ if unspecified => Decoded::Unspecified,
                Some(v) => Decoded::Ok(v),
                None => Decoded::Mismatch,
            }
        }
    }
}

/// Collects the paths of every node of `v`.
fn paths(v: &Value, here: &mut Vec<PathStep>, out: &mut Vec<Vec<PathStep>>) {
    out.push(here.clone());
    match v {
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                here.push(PathStep::Index(i));
                paths(item, here, out);
                here.pop();
            }
        }
        Value::Object(m) => {
            for (k, item) in m {
                here.push(PathStep::Key(k.clone()));
                paths(item, here, out);
                here.pop();
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug)]
enum PathStep {
    Index(usize),
    Key(String),
}

fn node_mut<'v>(v: &'v mut Value, path: &[PathStep]) -> Option<&'v mut Value> {
    let mut cur = v;
    for step in path {
        cur = match (step, cur) {
            (PathStep::Index(i), Value::Array(items)) => items.get_mut(*i)?,
            (PathStep::Key(k), Value::Object(m)) => m.get_mut(k)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Replacement nodes: every JSON shape, and the numbers at the edges of
/// `Int`.
fn replacement(i: usize) -> Value {
    match i {
        0 => Value::Null,
        1 => Value::from(0),
        2 => Value::from(1.5),
        3 => Value::from("c0"),
        4 => Value::Bool(true),
        5 => Value::Array(vec![]),
        6 => Value::Object(Map::new()),
        7 => Value::from(9_007_199_254_740_992_i64),
        8 => Value::from(-9_007_199_254_740_991_i64),
        9 => Value::from("nope"),
        10 => serde_json::json!({ "case": "c0" }),
        _ => serde_json::json!({ "case": "c1", "a0": 1 }),
    }
}

impl Gen<'_, '_> {
    /// One mutated text of `json`.
    fn mutate(&mut self, json: &Value) -> String {
        let mut v = json.clone();
        let mut all = Vec::new();
        paths(&v, &mut Vec::new(), &mut all);
        let path = all[self.pick(all.len())].clone();
        match self.pick(9) {
            0 | 1 => {
                if let Some(node) = node_mut(&mut v, &path) {
                    *node = replacement(self.pick(12));
                }
            }
            2 => {
                // Remove a key of an object.
                if let Some(Value::Object(m)) = node_mut(&mut v, &path)
                    && !m.is_empty()
                {
                    let k = m
                        .keys()
                        .nth(self.pick(m.len()))
                        .cloned()
                        .unwrap_or_default();
                    m.remove(&k);
                }
            }
            3 => {
                // Add an unknown key, or one special to JS objects.
                if let Some(Value::Object(m)) = node_mut(&mut v, &path) {
                    let k = ["zz", "valueOf", "constructor", "case", "__proto__"][self.pick(5)];
                    m.insert(k.to_owned(), replacement(self.pick(12)));
                }
            }
            4 => {
                // An integral number written as a fraction or exponent.
                if let Some(node) = node_mut(&mut v, &path)
                    && let Some(i) = node.as_i64()
                {
                    let text = serde_json::to_string(&v).unwrap_or_default();
                    let from = i.to_string();
                    let to = if self.chance(2) {
                        format!("{i}.0")
                    } else {
                        format!("{i}e0")
                    };
                    return text.replacen(&from, &to, 1);
                }
            }
            5 => {
                // Truncate the text at a char boundary.
                let text = serde_json::to_string(&v).unwrap_or_default();
                let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
                let cut = cuts.get(self.pick(cuts.len())).copied().unwrap_or(0);
                return text[..cut].to_owned();
            }
            6 => {
                let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                return match self.pick(3) {
                    0 => format!(" \n\t{text}\r\n "),
                    1 => format!("{text}x"),
                    _ => format!("{text},"),
                };
            }
            7 => {
                // A key written twice: the last one wins.
                // Only the root is rewritten as text.
                if let Value::Object(m) = &v
                    && let Some(k) = m.keys().next()
                {
                    let mut text = String::from("{");
                    let _ = write!(
                        text,
                        "{}:{},",
                        serde_json::to_string(k).unwrap_or_default(),
                        serde_json::to_string(&replacement(self.pick(12))).unwrap_or_default()
                    );
                    let inner = serde_json::to_string(&v).unwrap_or_default();
                    text.push_str(inner.trim_start_matches('{'));
                    return text;
                }
            }
            _ => {}
        }
        serde_json::to_string(&v).unwrap_or_default()
    }
}

/// The module of a generated case.
pub const MODULE: &str = "wire";

/// The entry of a generated case.
pub const ENTRY: &str = "wire.main";

/// Generates one case from `u`.
pub fn generate(u: &mut Unstructured<'_>) -> Case {
    let mut g = Gen {
        u,
        decls: Vec::new(),
    };
    g.declarations();
    let last = g.decls.len() - 1;
    let root = match g.pick(4) {
        0 => CTy::List(Box::new(CTy::Named(last))),
        1 => CTy::Opt(Box::new(CTy::Named(last))),
        _ => CTy::Named(last),
    };
    let value = g.value(&root, 0);
    let json = encode(&value, &g.decls, &root);
    let n = g.pick(6);
    let texts: Vec<String> = (0..n).map(|_| g.mutate(&json)).collect();
    let decls = g.decls;
    case(decls, root, value, texts)
}

/// The case that decodes each of `texts` as a `root`, after round-tripping
/// `value`.
pub fn case(decls: Vec<Decl>, root: CTy, value: CVal, texts: Vec<String>) -> Case {
    let json = encode(&value, &decls, &root);
    let inputs: Vec<(String, Decoded)> = texts
        .into_iter()
        .map(|text| {
            let expected = decode_text(&text, &root, &decls);
            (text, expected)
        })
        .collect();
    let rt = ty_name(&root);
    let mut source = String::from("import k.encoding.Json\nimport k.http.DecodingError\n\n");
    source.push_str(&declarations(&decls));
    let _ = write!(
        source,
        "func attempt(_ text: String) -> String {{
    do {{
        let v: {rt} = try Json.decode(text)
        return \"ok \" + Json.encode(v)
    }} catch .malformed {{
        return \"malformed\"
    }} catch .mismatch {{
        return \"mismatch\"
    }}
}}

func main() throws DecodingError -> List<String> {{
    let v: {rt} = {}
    let text = Json.encode(v)
    let back: {rt} = try Json.decode(text)
    return [text, Json.encode(back), \"{{back == v}}\"",
        literal(&value, &root, &decls)
    );
    for (text, _) in &inputs {
        let _ = write!(source, ", attempt({})", string_literal(text));
    }
    source.push_str("]\n}\n");
    Case {
        decls,
        root,
        value,
        json,
        inputs,
        files: vec![
            (
                "k.toml".to_owned(),
                "[package]\nname = \"gen\"\nk = \"0.1\"\n".to_owned(),
            ),
            (format!("src/{MODULE}/Main.k"), source),
        ],
    }
}

/// Checks one case: the codegen invariants, no unexpected exception, the
/// encoding equals the reference, decoding it gives an equal value that
/// encodes the same, and every mutation decodes as the reference says.
///
/// # Errors
///
/// A description of the disagreement, with the project.
pub fn check(case: &Case, limit: Duration) -> Result<(), String> {
    let run = coracle::check_terminating_program(&case.files, ENTRY, limit)?;
    let fail = |what: String| {
        Err(format!(
            "{what}\n  value: {:?}\n  reference JSON: {}\n{}",
            case.value,
            case.json,
            coracle::dump(&case.files)
        ))
    };
    let coracle::Outcome::Returned(Value::Array(items)) = &run.outcome else {
        return fail(format!("main did not return a list: {:?}", run.outcome));
    };
    let text = |i: usize| items.get(i).and_then(Value::as_str).unwrap_or("<missing>");
    let parse = |t: &str| serde_json::from_str::<Value>(t).ok();
    match parse(text(0)) {
        Some(v) if coracle::json_eq(&v, &case.json) => {}
        _ => return fail(format!("Json.encode wrote {}", text(0))),
    }
    if parse(text(1)).is_none_or(|v| !coracle::json_eq(&v, &case.json)) {
        return fail(format!("the decoded value encodes as {}", text(1)));
    }
    if text(2) != "true" {
        return fail("the decoded value is not equal to the original".to_owned());
    }
    for (i, (input, expected)) in case.inputs.iter().enumerate() {
        let got = text(3 + i);
        let ok = match expected {
            Decoded::Malformed => got == "malformed",
            Decoded::Mismatch => got == "mismatch",
            Decoded::Unspecified => got == "mismatch" || got.starts_with("ok "),
            Decoded::Ok(want) => got
                .strip_prefix("ok ")
                .and_then(parse)
                .is_some_and(|v| coracle::json_eq(&v, want)),
        };
        if !ok {
            return fail(format!(
                "decoding {input:?} gave `{got}`, the reference says {expected:?}"
            ));
        }
    }
    Ok(())
}
