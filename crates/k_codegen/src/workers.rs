//! `@Worker` services (`docs/design/55-workers.md`, codegen.md §13.4).
//!
//! On the client a worker service lowers to its **proxy**, a class of the
//! service's name (so the containers construct it unchanged) whose methods
//! encode their arguments, post them through `$k.workerClient` (the
//! `workers` runtime chunk) and decode the answer:
//!
//! ```js
//! export class Sieve {
//!   constructor($deps = {}) { this.$w = $k.workerClient(() => new Worker(new URL("./workers/sieve.<hash>.js", import.meta.url), { type: "module", name: "Sieve" }), "Sieve"); }
//!   async count($c, below) {
//!     return this.$w.call($c, "count", [$k.codecs.int.encode(below)], (v) => $k.codecs.int.decode(v, "value"), (e) => SieveError$codec.decode(e, "error"), (f) => SieveError.workerFailure(WorkerFailure[f]));
//!   }
//! }
//! ```
//!
//! The **worker chunk** is built from a second lowering of the program
//! ([`host_item`]): the service's own class, its host table
//! `Sieve$host = { count: [decodeArguments, encodeResult, encodeError] }`
//! and `$k.serveWorker(globalThis, () => new Sieve(), Sieve$host)`, kept to
//! what the service reaches. In a test bundle there is no chunk: the class
//! and its table are wrapped in `Sieve$worker` next to the proxy, which
//! opens `$k.$test.inlineWorker(...Sieve$worker)` instead of a `Worker`.
//! On the server the service is an empty class: nothing calls it there
//! (K4012).

use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberId, MemberKind, Type};

use crate::cx::{Gen, Target, mangle};
use crate::decl::Item;
use crate::js::{self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, call, id, member, rt};
use crate::lower::Body;

/// `true` for a service of the project marked `@Worker`.
pub(crate) fn is_worker(g: &Gen<'_>, d: DeclId) -> bool {
    let info = g.view.decl(d);
    !info.stdlib
        && info.named_kind() == Some(DeclarationKind::Service)
        && info.has_annotation("Worker")
}

/// The placeholder the proxy names its worker chunk by, which the bundler
/// replaces with the chunk's path once the chunk is printed.
pub(crate) fn chunk_placeholder(d: DeclId) -> String {
    format!("$worker:{}$", d.0)
}

/// The chunk stem of worker `d`: `workers/note-search`.
pub(crate) fn chunk_stem(g: &Gen<'_>, d: DeclId) -> String {
    format!("workers/{}", crate::css::kebab(&g.view.decl(d).name))
}

/// The methods the page calls: the instance functions that aren't private.
fn calls(g: &Gen<'_>, d: DeclId) -> Vec<MemberId> {
    g.view
        .decl(d)
        .members
        .iter()
        .copied()
        .filter(|&m| {
            let mi = g.view.member(m);
            mi.kind == MemberKind::Func && !mi.is_static && !mi.is_private
        })
        .collect()
}

/// `true` for `Bytes`, which crosses as its `Uint8Array` at the top level.
fn is_bytes(g: &Gen<'_>, t: &Type) -> bool {
    t.decl().is_some() && t.decl() == g.b.bytes
}

/// Encodes `x` of type `t` for the message.
fn encode(b: &mut Body<'_, '_>, t: &Type, x: Expr) -> Expr {
    if is_bytes(b.g, t) {
        return x;
    }
    crate::app::encode_expr(b, t, x)
}

/// Decodes `x` of type `t` from a message, reporting failures at `path`.
fn decode(b: &mut Body<'_, '_>, t: &Type, x: Expr, path: &str) -> Expr {
    if is_bytes(b.g, t) {
        return x;
    }
    let c = crate::codec::codec_expr(b, t);
    call(member(c, "decode"), vec![x, js::str(path)])
}

/// Lowers worker service `d` for `b.target`: its proxy on the client (with
/// its in-process host in a test bundle), an empty class on the server.
pub(crate) fn lower(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let name = g.name(d);
    if b.target == Target::Server {
        return vec![Stmt::Class(
            Class {
                name,
                extends: None,
                methods: Vec::new(),
            },
            true,
        )];
    }
    let tests = g.options.entry == crate::Entry::Tests;
    let mut out = Vec::new();
    let open = if tests {
        // The service and its table, under their own names inside a
        // function, so the class's statics and the proxy don't collide.
        let mut inner = real_class(b, d);
        inner.push(Stmt::Return(Some(Expr::Array(vec![
            js::arrow(vec![], Expr::New(Box::new(id(name.clone())), vec![])),
            host_table(b, d),
        ]))));
        let wrapped = format!("{name}$worker");
        out.push(Stmt::Const(
            wrapped.clone(),
            call(js::arrow_block(vec![], inner, false), vec![]),
        ));
        crate::suite::kit_call("inlineWorker", vec![Expr::Spread(Box::new(id(wrapped)))])
    } else {
        let url = Expr::New(
            Box::new(id("URL")),
            vec![js::str(chunk_placeholder(d)), id("import.meta.url")],
        );
        let options = Expr::Object(
            vec![
                Prop::KeyValue("type".into(), js::str("module")),
                Prop::KeyValue("name".into(), js::str(name.clone())),
            ],
            false,
        );
        js::arrow(
            vec![],
            Expr::New(Box::new(id("Worker")), vec![url, options]),
        )
    };
    let mut methods = vec![Function {
        kind: MethodKind::Constructor,
        params: vec![Param {
            name: "$deps".into(),
            default: Some(Expr::Object(Vec::new(), false)),
            rest: false,
        }],
        body: js::Body::Block(vec![Stmt::Expr(js::assign(
            member(Expr::This, "$w"),
            rt("workerClient", vec![open, js::str(name.clone())]),
        ))]),
        ..Function::default()
    }];
    for m in calls(g, d) {
        methods.push(proxy_method(b, m));
    }
    out.push(Stmt::Class(
        Class {
            name,
            extends: None,
            methods,
        },
        true,
    ));
    out
}

/// One method of the proxy: encode, post, decode.
fn proxy_method(b: &mut Body<'_, '_>, m: MemberId) -> Function {
    let g = b.g;
    let mi = g.view.member(m).clone();
    let sig = mi.signature.clone().unwrap_or(k_sema::view::Signature {
        generics: Vec::new(),
        params: Vec::new(),
        is_async: true,
        throws: None,
        ret: Type::Error,
    });
    let mut params = vec![Param::new("$c")];
    let mut args = Vec::new();
    for p in &sig.params {
        let pn = mangle(&p.name);
        params.push(Param::new(pn.clone()));
        args.push(encode(b, &p.ty, id(pn)));
    }
    let decode_result = if sig.ret.decl() == g.b.void {
        js::arrow(vec![Param::new("v")], Expr::Undefined)
    } else {
        let v = decode(b, &sig.ret, id("v"), "value");
        js::arrow(vec![Param::new("v")], v)
    };
    let (decode_error, failure) = match &sig.throws {
        Some(e) if e.decl().is_some() => {
            let err = decode(b, e, id("e"), "error");
            let ed = e.decl().unwrap_or(DeclId(0));
            let enum_ref = b.decl_ref(ed);
            let failure = match g.view.lookup("k.concurrency.WorkerFailure") {
                Some(wf) => {
                    let cases = b.decl_ref(wf);
                    call(
                        member(enum_ref, "workerFailure"),
                        vec![Expr::Index(Box::new(cases), Box::new(id("f")))],
                    )
                }
                None => Expr::Null,
            };
            (
                js::arrow(vec![Param::new("e")], err),
                js::arrow(vec![Param::new("f")], failure),
            )
        }
        _ => (
            js::arrow(vec![Param::new("e")], id("e")),
            js::arrow(vec![Param::new("f")], id("f")),
        ),
    };
    let body = vec![Stmt::Return(Some(call(
        member(member(Expr::This, "$w"), "call"),
        vec![
            id("$c"),
            js::str(g.member_name(m)),
            Expr::Array(args),
            decode_result,
            decode_error,
            failure,
        ],
    )))];
    Function {
        name: g.member_name(m),
        params,
        body: js::Body::Block(body),
        is_async: true,
        kind: MethodKind::Plain,
    }
}

/// The service's own class, as any service lowers on the client, not
/// exported: only the host builds it.
fn real_class(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let mut stmts = crate::ui::service(b, d);
    for s in &mut stmts {
        if let Stmt::Class(_, export) = s.peel_mut() {
            *export = false;
        }
    }
    stmts
}

/// `{ m: [decodeArguments, encodeResult, encodeError], … }`.
fn host_table(b: &mut Body<'_, '_>, d: DeclId) -> Expr {
    let g = b.g;
    let mut props = Vec::new();
    for m in calls(g, d) {
        let mi = g.view.member(m).clone();
        let Some(sig) = mi.signature.clone() else {
            continue;
        };
        let decoded: Vec<Expr> = sig
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let x = Expr::Index(Box::new(id("a")), Box::new(Expr::Num(i as f64)));
                decode(b, &p.ty, x, &p.name)
            })
            .collect();
        let decode_args = js::arrow(vec![Param::new("a")], Expr::Array(decoded));
        let encode_result = if sig.ret.decl() == g.b.void {
            js::arrow(vec![Param::new("v")], Expr::Null)
        } else {
            let v = encode(b, &sig.ret, id("v"));
            js::arrow(vec![Param::new("v")], v)
        };
        let encode_error = match &sig.throws {
            Some(e) if e.decl().is_some() => {
                let x = encode(b, e, id("e"));
                js::arrow(vec![Param::new("e")], x)
            }
            _ => js::arrow(vec![Param::new("e")], id("e")),
        };
        props.push(Prop::Quoted(
            g.member_name(m),
            Expr::Array(vec![decode_args, encode_result, encode_error]),
        ));
    }
    Expr::Object(props, false)
}

/// The item that stands for worker `d` in the worker chunk's lowering of
/// the program: the service's class, its host table and the call that
/// serves it on the worker's global scope.
pub(crate) fn host_item(g: &Gen<'_>, d: DeclId) -> Item {
    let file = g.view.decl(d).file;
    let mut b = Body::new(g, file, Target::Client);
    let name = g.name(d);
    let mut stmts = real_class(&mut b, d);
    let table = host_table(&mut b, d);
    let host = format!("{name}$host");
    stmts.push(Stmt::Const(host.clone(), table));
    stmts.push(Stmt::Expr(rt(
        "serveWorker",
        vec![
            id("globalThis"),
            js::arrow(vec![], Expr::New(Box::new(id(name)), vec![])),
            id(host),
        ],
    )));
    let on_demand = std::mem::take(&mut b.on_demand);
    let prelude = std::mem::take(&mut b.prelude);
    Item {
        stmts,
        uses: b.uses,
        prelude,
        on_demand,
    }
}
