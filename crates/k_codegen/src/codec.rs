//! `Codable` codecs (`docs/compiler/codegen.md` §5.1): the JSON encoding
//! of every `Codable` type, following `docs/design/28-rpc-wire-format.md`.
//!
//! A codec is an object `{ encode(value) → json, decode(json, path) →
//! value }`. Builtin types use the runtime's codecs (`$k.codecs.int`, …);
//! each `Codable` struct and enum gets `T$encode`, `T$decode` and the pair
//! `T$codec` (a function of its arguments' codecs when generic). Decoding
//! validates and throws a `$k.DecodeFailure`, which `Json.decode` and the
//! RPC layer turn into the typed `DecodingError`.

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberKind, Type};

use crate::cx::Gen;
use crate::js::{self, Expr, Function, Param, Prop, Stmt, bin, call, id, member, rt};
use crate::lower::Body;

/// The codec of type `t`, as an expression.
pub(crate) fn codec_expr(b: &mut Body<'_, '_>, t: &Type) -> Expr {
    let g = b.g;
    // `Decimal`, `Currency` and `Money` live in the `money` feature chunk
    // (runtime.md §2), whose codecs are `$k.moneyCodecs`.
    let builtin = |n: &str| {
        let table = if matches!(n, "decimal" | "currency" | "money") {
            "moneyCodecs"
        } else {
            "codecs"
        };
        member(member(id("$k"), table), n.to_owned())
    };
    match t {
        Type::Optional(inner) => {
            let c = codec_expr(b, inner);
            return call(builtin("optional"), vec![c]);
        }
        Type::Param(gp) => {
            if let Some(w) = b.witnesses.get(gp) {
                return id(w.clone());
            }
            return builtin("any");
        }
        Type::Tuple(items) => {
            let cs = items.iter().map(|i| codec_expr(b, i)).collect();
            return call(builtin("tuple"), vec![Expr::Array(cs)]);
        }
        _ => {}
    }
    let Some(d) = t.decl() else {
        return builtin("any");
    };
    let simple = [
        (g.b.int, "int"),
        (g.b.double, "double"),
        (g.b.string, "string"),
        (g.b.bool_, "bool"),
        (g.b.money, "money"),
        (g.b.date, "date"),
        (g.b.duration, "duration"),
        (g.b.decimal, "decimal"),
        // Typed text values validate themselves when decoded
        // (15-forms-and-validation.md), so text that isn't one is
        // `invalid_argument`, never a value of the type.
        (g.b.uuid, "uuid"),
        (g.b.url, "url"),
        (g.b.email, "email"),
        (g.b.length, "string"),
        (g.b.color, "color"),
        (g.b.time_zone, "timeZone"),
        (g.b.time_of_day, "timeOfDay"),
        (g.b.void, "any"),
        (g.b.bytes, "bytes"),
    ];
    for (decl, name) in simple {
        if Some(d) == decl {
            return builtin(name);
        }
    }
    if Some(d) == g.b.list || Some(d) == g.b.set {
        let inner = t.args().first().cloned().unwrap_or(Type::Error);
        let c = codec_expr(b, &inner);
        if Some(d) == g.b.set {
            let prim = g.is_primitive_key(&inner);
            return call(builtin("set"), vec![c, Expr::Bool(prim)]);
        }
        return call(builtin("list"), vec![c]);
    }
    if Some(d) == g.b.map {
        let k = t.args().first().cloned().unwrap_or(Type::Error);
        let v = t.args().get(1).cloned().unwrap_or(Type::Error);
        let kc = codec_expr(b, &k);
        let vc = codec_expr(b, &v);
        let prim = g.is_primitive_key(&k);
        return call(builtin("map"), vec![kc, vc, Expr::Bool(prim)]);
    }
    if Some(d) == g.b.half_open || Some(d) == g.b.closed {
        let inner = t.args().first().cloned().unwrap_or(Type::Error);
        let c = codec_expr(b, &inner);
        return call(builtin("range"), vec![c, Expr::Bool(Some(d) == g.b.closed)]);
    }
    if Some(d) == g.b.currency {
        return builtin("currency");
    }
    let name = format!("{}$codec", g.name(d));
    let r = b.use_decl(d, name);
    if g.view.decl(d).generics.is_empty() {
        r
    } else {
        let args: Vec<Expr> = t.args().iter().map(|a| codec_expr(b, a)).collect();
        call(r, args)
    }
}

/// `true` when the generator emits a codec for `d`.
pub(crate) fn has_codec(g: &Gen<'_>, d: DeclId) -> bool {
    let info = g.view.decl(d);
    if !matches!(
        info.named_kind(),
        Some(DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::Entity)
    ) {
        return false;
    }
    if info.has_annotation("Native") {
        return false;
    }
    match g.b.codable {
        Some(c) => g.implements(d, c),
        None => false,
    }
}

/// Emits `T$encode`, `T$decode` and `T$codec` for struct or enum `d`.
#[allow(clippy::too_many_lines)]
pub(crate) fn emit_codec(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    let name = g.name(d);
    let generic = !info.generics.is_empty();
    // Generic types: the witnesses of their parameters are parameters.
    let mut wparams = Vec::new();
    for (i, gp) in info.generics.iter().enumerate() {
        let w = format!("$w{i}");
        b.witnesses.insert(*gp, w.clone());
        wparams.push(Param::new(w));
    }
    let self_ty = Type::Named(d, info.generics.iter().map(|g| Type::Param(*g)).collect());
    let mut out = Vec::new();
    let (encode_body, decode_body) = if info.named_kind() == Some(DeclarationKind::Enum) {
        let cases = g.cases(d);
        let all_unit = g.is_simple_enum(&self_ty);
        let names: Vec<String> = cases
            .iter()
            .map(|&c| g.view.member(c).name.clone())
            .collect();
        if all_unit {
            let enc = vec![Stmt::Return(Some(Expr::Index(
                Box::new(Expr::Array(names.iter().map(js::str).collect())),
                Box::new(member(id("v"), "$")),
            )))];
            let r = b.decl_ref(d);
            let dec = vec![Stmt::Return(Some(rt(
                "decodeUnitCase",
                vec![
                    id("j"),
                    id("p"),
                    r,
                    Expr::Array(names.iter().map(js::str).collect()),
                ],
            )))];
            (enc, dec)
        } else {
            let mut enc_cases = Vec::new();
            let mut dec_stmts = vec![Stmt::Const(
                "c".into(),
                rt("caseName", vec![id("j"), id("p")]),
            )];
            let r = b.decl_ref(d);
            for (tag, &c) in cases.iter().enumerate() {
                let cname = g.view.member(c).name.clone();
                let fields = g.case_fields(c);
                let ptypes: Vec<Type> = g
                    .view
                    .member(c)
                    .signature
                    .as_ref()
                    .map(|s| s.params.iter().map(|p| p.ty.clone()).collect())
                    .unwrap_or_default();
                if fields.is_empty() {
                    enc_cases.push(js::Case {
                        tests: vec![Expr::Num(tag as f64)],
                        body: vec![Stmt::Return(Some(js::str(cname.clone())))],
                    });
                    dec_stmts.push(Stmt::If(
                        bin("===", id("c"), js::str(cname.clone())),
                        vec![Stmt::Return(Some(member(r.clone(), cname.clone())))],
                        None,
                    ));
                    continue;
                }
                let mut props = vec![Prop::KeyValue("case".into(), js::str(cname.clone()))];
                let mut dargs = Vec::new();
                for (f, t) in fields.iter().zip(&ptypes) {
                    let c = codec_expr(b, t);
                    props.push(Prop::KeyValue(
                        f.clone(),
                        call(
                            member(c.clone(), "encode"),
                            vec![member(id("v"), f.clone())],
                        ),
                    ));
                    dargs.push(call(
                        member(c, "decode"),
                        vec![
                            Expr::Index(Box::new(id("j")), Box::new(js::str(f.clone()))),
                            bin("+", id("p"), js::str(format!(".{f}"))),
                        ],
                    ));
                }
                enc_cases.push(js::Case {
                    tests: vec![Expr::Num(tag as f64)],
                    body: vec![Stmt::Return(Some(Expr::Object(props, false)))],
                });
                dec_stmts.push(Stmt::If(
                    bin("===", id("c"), js::str(cname.clone())),
                    vec![Stmt::Return(Some(call(
                        member(r.clone(), cname.clone()),
                        dargs,
                    )))],
                    None,
                ));
            }
            enc_cases.push(js::Case {
                tests: Vec::new(),
                body: vec![Stmt::Return(Some(rt("unreachable", vec![])))],
            });
            dec_stmts.push(Stmt::Return(Some(rt(
                "decodeFail",
                vec![id("p"), bin("+", js::str("unknown case "), id("c"))],
            ))));
            (
                vec![Stmt::Switch(member(id("v"), "$"), enc_cases)],
                dec_stmts,
            )
        }
    } else {
        let fields = g.stored_fields(d);
        let mut props = Vec::new();
        let mut dprops = Vec::new();
        for f in &fields {
            let m = g.view.member(*f);
            let fname = m.name.clone();
            let key = json_key(g, *f).unwrap_or_else(|| fname.clone());
            let t = m.ty.clone().unwrap_or(Type::Error);
            let c = codec_expr(b, &t);
            let enc = if is_identity_codec(g, &t) {
                member(id("v"), fname.clone())
            } else {
                call(
                    member(c.clone(), "encode"),
                    vec![member(id("v"), fname.clone())],
                )
            };
            props.push(Prop::KeyValue(key.clone(), enc));
            let raw = Expr::Index(Box::new(id("j")), Box::new(js::str(key.clone())));
            let path = bin("+", id("p"), js::str(format!(".{key}")));
            let dec = if m.has_default && !matches!(t, Type::Optional(_)) {
                // A missing field with a default takes the default.
                Expr::Cond(
                    Box::new(bin("===", raw.clone(), Expr::Undefined)),
                    Box::new(Expr::Undefined),
                    Box::new(call(member(c, "decode"), vec![raw, path])),
                )
            } else {
                call(member(c, "decode"), vec![raw, path])
            };
            dprops.push(dec);
        }
        let ctor = b.decl_ref(d);
        let enc = vec![Stmt::Return(Some(Expr::Object(props, false)))];
        let dec = vec![
            Stmt::Expr(rt("expectObject", vec![id("j"), id("p")])),
            Stmt::Return(Some(call(ctor, dprops))),
        ];
        (enc, dec)
    };
    for gp in &info.generics {
        b.witnesses.remove(gp);
    }
    let enc_fn = Function {
        name: format!("{name}$encode"),
        params: vec![Param::new("v")],
        body: js::Body::Block(encode_body),
        ..Function::default()
    };
    let dec_fn = Function {
        name: format!("{name}$decode"),
        params: vec![
            Param::new("j"),
            Param {
                name: "p".into(),
                default: Some(js::str("")),
                rest: false,
            },
        ],
        body: js::Body::Block(decode_body),
        ..Function::default()
    };
    if generic {
        let pair = Expr::Object(
            vec![
                Prop::KeyValue(
                    "encode".into(),
                    Expr::Arrow(Box::new(Function {
                        params: vec![Param::new("v")],
                        body: enc_fn.body.clone(),
                        ..Function::default()
                    })),
                ),
                Prop::KeyValue(
                    "decode".into(),
                    Expr::Arrow(Box::new(Function {
                        params: dec_fn.params.clone(),
                        body: dec_fn.body.clone(),
                        ..Function::default()
                    })),
                ),
            ],
            true,
        );
        out.push(Stmt::Function(
            Function {
                name: format!("{name}$codec"),
                params: wparams,
                body: js::Body::Block(vec![Stmt::Return(Some(pair))]),
                ..Function::default()
            },
            true,
        ));
    } else {
        out.push(Stmt::Function(enc_fn, true));
        out.push(Stmt::Function(dec_fn, true));
        out.push(Stmt::ExportConst(
            format!("{name}$codec"),
            Expr::Object(
                vec![
                    Prop::KeyValue("encode".into(), id(format!("{name}$encode"))),
                    Prop::KeyValue("decode".into(), id(format!("{name}$decode"))),
                ],
                false,
            ),
        ));
    }
    out
}

/// The `@JsonKey("…")` of a field, if any.
fn json_key(g: &Gen<'_>, f: k_sema::view::MemberId) -> Option<String> {
    let node = g.member_node(f)?;
    for a in node
        .children()
        .filter(|c| c.kind() == k_syntax::SyntaxKind::Annotation)
    {
        let text = a.text().to_string();
        if text.starts_with("@JsonKey") {
            let start = text.find('"')?;
            let rest = &text[start + 1..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_owned());
        }
    }
    None
}

/// `true` for the types whose JSON form is their runtime value (`Int`,
/// `String`, `Bool`), so a struct encoder copies the field instead of
/// calling its codec.
fn is_identity_codec(g: &Gen<'_>, t: &Type) -> bool {
    g.is_int(t) || g.is_string(t) || g.is_bool(t)
}

/// `true` when a type's values have stored fields to encode.
#[allow(dead_code)]
pub(crate) fn is_struct_like(g: &Gen<'_>, d: DeclId) -> bool {
    g.view
        .decl(d)
        .members
        .iter()
        .any(|&m| g.view.member(m).kind == MemberKind::Field)
}
