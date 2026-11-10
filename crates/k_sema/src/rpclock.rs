//! `rpc.lock`: the shape of every protocol a serving app implements, so a
//! deploy can't break the callers already deployed
//! (`docs/design/40-workspaces-and-services.md`, "Compatibility between
//! deploys"; the file format is in `docs/compiler/sema.md` §12.16).
//!
//! An app that implements protocols records, for each one, its rpcs (the
//! parameters' names and types, the result and the error type) and the
//! shapes of the app types those carry: a struct's fields with their
//! Protobuf field numbers and the numbers of removed fields
//! (`reserved`), an enum's cases. Types are written as `storage.lock`
//! writes them: stdlib types by their simple name (`Int`, `List<String>`),
//! app and library types qualified (`pay.ChargeRequest`).
//!
//! [`check`] compares the program with the lock and returns the lock the
//! program implies, which `k build` writes:
//!
//! * removing an rpc, a parameter, a field or an error case, changing the
//!   type of any of them, adding a required parameter or field, changing
//!   the protocol's wire name (`service`: the Protobuf package from the
//!   project's name and the module, and the protocol's name), or a
//!   recorded protocol the app no longer serves breaks callers: **K8001**
//!   at the protocol (at the app for one it no longer serves), with one note
//!   per change and a label at the lock, unless the protocol is marked
//!   `@Breaking("reason")`;
//! * adding an rpc, an optional field, an optional parameter or an error
//!   case is compatible;
//! * an enum case's payload fields are recorded and compared as a struct's
//!   fields are (by name: a label, or `_0`, `_1`… for an unlabelled one, as
//!   the wire names them), except that they have no numbers; a lock written
//!   before they were recorded lists bare case names, which still read and
//!   whose fields aren't compared;
//! * a lock that isn't valid TOML of this structure, or that gives two
//!   fields of a type one number, is **K8002**.
//!
//! Field numbers are assigned once and kept: a field keeps its number
//! while it exists, a new field gets the next free one, and the number of a
//! removed field is reserved and never reused.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use k_diagnostics::{Diagnostic, Span, TextRange, TextSize, codes};
use k_syntax::ast::{self, AstNode};

use crate::cx::{Cx, SigKey};
use crate::program::{DeclId, MemberKind};
use crate::project::LockfileKind;
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// What the lock records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Lock {
    /// Protocols by qualified name.
    protocols: BTreeMap<String, BTreeMap<String, Rpc>>,
    /// The wire name of each protocol's service, `shop.store.StoreProtocol`
    /// (`docs/design/28-rpc-wire-format.md`, "Names"); absent from locks
    /// written before it was recorded.
    services: BTreeMap<String, String>,
    /// App and library types by qualified name.
    types: BTreeMap<String, Shape>,
    /// The program's types that a client sends or an rpc carries, which
    /// keep every case (not recorded; only the program's lock has it). A
    /// type reached only as a channel's server message `S` may lose a case
    /// (44-channels.md, "rpc.lock and compatibility").
    strict: BTreeSet<String>,
    /// The types of each enum's case payloads in the program (not
    /// recorded), so the comparison reaches them through the enum.
    payloads: BTreeMap<String, BTreeSet<String>>,
}

/// One rpc.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rpc {
    /// The parameters: name, type, and whether it may be left out
    /// (optional or defaulted; not recorded, only used for comparing).
    params: Vec<(String, String, bool)>,
    /// The result type (empty for a channel, which records [`Rpc::channel`]
    /// instead).
    returns: String,
    /// The error type.
    throws: String,
    /// A channel rpc's message types (`kind = "channel"`): what the client
    /// sends (`send`, `C`), what it receives (`receive`, `S`) and the
    /// refusal (`refusal`, `E`).
    channel: Option<Messages>,
}

/// The three types of a channel rpc's `Channel<C, S, E>`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Messages {
    /// `C`, client to server.
    send: String,
    /// `S`, server to client.
    receive: String,
    /// `E`, the refusal of a client message.
    refusal: String,
}

/// A recorded type.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Shape {
    /// A struct's fields and the numbers of removed ones.
    Struct {
        /// Name, type, number, and whether it is optional (for comparing).
        fields: Vec<(String, String, u32, bool)>,
        /// Numbers never to be used again.
        reserved: BTreeSet<u32>,
    },
    /// An enum's cases.
    Enum(Vec<Case>),
}

/// One enum case.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Case {
    /// The case's name.
    name: String,
    /// The payload fields: name (the label, or `_0`, `_1`… for an
    /// unlabelled one), type, and whether it is optional (for comparing).
    /// `None` for a case recorded as a bare name, before payloads were
    /// recorded: its fields are unknown and aren't compared.
    fields: Option<Vec<(String, String, bool)>>,
}

/// Why the lock can't be read: a message and the byte offset of the
/// problem.
struct Invalid {
    /// What is wrong, for the diagnostic.
    message: String,
    /// The byte offset in the lockfile where the problem is.
    offset: usize,
}

/// Runs the check and returns the text of the lock the program implies, or
/// `None` for a project that serves no protocol and has no lock.
pub(crate) fn check(cx: &Cx<'_, '_>) -> Option<String> {
    let p = cx.p;
    let app = p
        .decls
        .iter()
        .position(|d| d.named_kind() == Some(K::App) && !p.file(d.file).stdlib)?;
    let app = DeclId(u32::try_from(app).unwrap_or(u32::MAX));
    let served = served(cx);
    let lockfile = p.lockfiles.iter().find(|l| l.kind == LockfileKind::Rpc);
    let now = current(cx, &served);
    let mut recorded = Lock::default();
    if let Some(lock) = lockfile {
        match read(&lock.text) {
            Ok(r) => recorded = r,
            Err(invalid) => {
                cx.diags.borrow_mut().push(Diagnostic::new(
                    codes::INVALID_LOCKFILE,
                    Span::new(lock.id, line_at(&lock.text, invalid.offset)),
                    format!("rpc.lock can't be read: {}", invalid.message.trim()),
                ));
                // An unreadable lock is rewritten from the program.
                return Some(write(&merge(&now, &Lock::default())));
            }
        }
        compare(cx, lock, app, &served, &recorded, &now);
    } else if served.is_empty() {
        return None;
    }
    Some(write(&merge(&now, &recorded)))
}

/// The protocols the app serves: those its own (non-test) services
/// implement, sorted by qualified name.
fn served(cx: &Cx<'_, '_>) -> Vec<(String, DeclId)> {
    let p = cx.p;
    let mut out: Vec<(String, DeclId)> = p
        .decls
        .iter()
        .enumerate()
        .filter(|(_, d)| d.named_kind() == Some(K::Protocol) && !p.file(d.file).stdlib)
        .map(|(i, _)| DeclId(u32::try_from(i).unwrap_or(u32::MAX)))
        .filter(|&d| !crate::di::protocol_implementations(cx, d).is_empty())
        .map(|d| (p.decl_name(d), d))
        .collect();
    out.sort();
    out
}

/// A type as the lock writes it.
fn text(cx: &Cx<'_, '_>, t: &Ty) -> String {
    let p = cx.p;
    match t {
        Ty::Optional(inner) => format!("{}?", text(cx, inner)),
        Ty::Named(d, args) => {
            let name = if p.is_stdlib(*d) {
                p.decl(*d).name.clone()
            } else {
                p.decl_name(*d)
            };
            if args.is_empty() {
                name
            } else {
                let args: Vec<String> = args.iter().map(|a| text(cx, a)).collect();
                format!("{name}<{}>", args.join(", "))
            }
        }
        other => other.render(p),
    }
}

/// The `[package] name` of the `k.toml` text `manifest`.
fn manifest_name(manifest: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(manifest).ok()?;
    table
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

/// The wire name of protocol `d`'s service: the Protobuf package (the
/// Protobuf form of the declaring package's name, then the module) and the
/// protocol's name (`docs/design/28-rpc-wire-format.md`, "Names"). A
/// library's protocol takes the library's name, as code generation does.
fn wire_service(cx: &Cx<'_, '_>, d: DeclId) -> String {
    let p = cx.p;
    let decl = p.decl(d);
    let file = p.file(decl.file);
    let manifest = match (
        file.path.and_then(crate::ProjectPath::dependency),
        crate::ProjectPath::new("k.toml"),
    ) {
        (Some((dir, _)), Ok(own)) => crate::ProjectPath::mounted(&dir, &own).as_str().to_owned(),
        _ => "k.toml".to_owned(),
    };
    let project = p
        .manifests
        .iter()
        .find(|(path, _)| *path == manifest)
        .and_then(|(_, text)| manifest_name(text))
        .unwrap_or_default();
    // As code generation derives it (`k_codegen::server::package_of`), so
    // the lock records the path the server answers.
    let prefix = project.replace('-', "_");
    let module = file
        .module
        .and_then(|m| p.modules.get(usize::try_from(m.0).unwrap_or(usize::MAX)))
        .map(|m| m.path.clone())
        .unwrap_or_default();
    let package =
        if prefix.is_empty() || module == prefix || module.starts_with(&format!("{prefix}.")) {
            module
        } else if module.is_empty() {
            prefix
        } else {
            format!("{prefix}.{module}")
        };
    if package.is_empty() {
        decl.name.clone()
    } else {
        format!("{package}.{}", decl.name)
    }
}

/// The program's shape of the served protocols and the types they carry.
fn current(cx: &Cx<'_, '_>, served: &[(String, DeclId)]) -> Lock {
    let p = cx.p;
    let mut out = Lock::default();
    // The types reached strictly (parameters, results, errors, a channel's
    // client messages and refusals), and those reached only as a channel's
    // server messages.
    let mut pending: Vec<Ty> = Vec::new();
    let mut lenient: Vec<Ty> = Vec::new();
    for (name, d) in served {
        out.services.insert(name.clone(), wire_service(cx, *d));
        let mut rpcs = BTreeMap::new();
        for &m in &p.decl(*d).members {
            let member = p.member(m);
            if member.kind != MemberKind::Rpc {
                continue;
            }
            let sig = cx.sig(SigKey::Member(m));
            let params = sig
                .params
                .iter()
                .map(|sp| {
                    pending.push(sp.ty.clone());
                    (
                        sp.name.clone(),
                        text(cx, &sp.ty),
                        sp.has_default || matches!(sp.ty, Ty::Optional(_)),
                    )
                })
                .collect();
            let channel = crate::channels::channel_args(cx, &sig.ret).map(|args| {
                pending.push(args[0].clone());
                lenient.push(args[1].clone());
                pending.push(args[2].clone());
                Messages {
                    send: text(cx, &args[0]),
                    receive: text(cx, &args[1]),
                    refusal: text(cx, &args[2]),
                }
            });
            if channel.is_none() {
                pending.push(sig.ret.clone());
            }
            let throws = sig.throws.clone().unwrap_or(Ty::Error);
            pending.push(throws.clone());
            rpcs.insert(
                member.name.clone(),
                Rpc {
                    params,
                    returns: if channel.is_some() {
                        String::new()
                    } else {
                        text(cx, &sig.ret)
                    },
                    throws: text(cx, &throws),
                    channel,
                },
            );
        }
        out.protocols.insert(name.clone(), rpcs);
    }
    // The app and library types reachable from the signatures: the strict
    // ones first, then those only a channel's server messages reach.
    let mut seen: BTreeSet<DeclId> = BTreeSet::new();
    let mut strict = true;
    loop {
        let Some(t) = pending.pop() else {
            if lenient.is_empty() {
                break;
            }
            strict = false;
            pending = std::mem::take(&mut lenient);
            continue;
        };
        match &t {
            Ty::Optional(inner) => pending.push((**inner).clone()),
            Ty::Named(d, args) => {
                pending.extend(args.iter().cloned());
                let d = *d;
                if p.is_stdlib(d) || !seen.insert(d) {
                    continue;
                }
                if strict {
                    out.strict.insert(p.decl_name(d));
                }
                match p.decl(d).named_kind() {
                    Some(K::Struct) => {
                        let fields = cx
                            .stored_fields(d)
                            .into_iter()
                            .map(|m| {
                                let ty = cx.field_ty(m);
                                pending.push(ty.clone());
                                let defaulted = p.member(m).node.as_ref().is_some_and(|n| {
                                    n.children().any(|c| ast::Expr::can_cast(c.kind()))
                                });
                                (
                                    p.member(m).name.clone(),
                                    text(cx, &ty),
                                    0,
                                    defaulted || matches!(ty, Ty::Optional(_)),
                                )
                            })
                            .collect();
                        out.types.insert(
                            p.decl_name(d),
                            Shape::Struct {
                                fields,
                                reserved: BTreeSet::new(),
                            },
                        );
                    }
                    Some(K::Enum) => {
                        let mut cases = Vec::new();
                        let mut payloads = BTreeSet::new();
                        for m in cx.cases(d) {
                            let mut fields = Vec::new();
                            for (i, sp) in cx.case_sig(m).params.iter().enumerate() {
                                payloads.extend(names_in(&text(cx, &sp.ty)));
                                pending.push(sp.ty.clone());
                                let name = if sp.name.is_empty() {
                                    format!("_{i}")
                                } else {
                                    sp.name.clone()
                                };
                                fields.push((
                                    name,
                                    text(cx, &sp.ty),
                                    sp.has_default || matches!(sp.ty, Ty::Optional(_)),
                                ));
                            }
                            cases.push(Case {
                                name: p.member(m).name.clone(),
                                fields: Some(fields),
                            });
                        }
                        out.payloads.insert(p.decl_name(d), payloads);
                        out.types.insert(p.decl_name(d), Shape::Enum(cases));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// The lock to write: the program's shapes with the recorded field
/// numbers kept, new fields numbered after every number used so far, and
/// removed fields' numbers reserved.
fn merge(now: &Lock, recorded: &Lock) -> Lock {
    let mut out = now.clone();
    for (name, shape) in &mut out.types {
        let Shape::Struct { fields, reserved } = shape else {
            continue;
        };
        let (old_fields, old_reserved) = match recorded.types.get(name) {
            Some(Shape::Struct { fields, reserved }) => (fields.clone(), reserved.clone()),
            _ => (Vec::new(), BTreeSet::new()),
        };
        let mut used: BTreeSet<u32> = old_reserved.clone();
        used.extend(old_fields.iter().map(|f| f.2));
        *reserved = old_reserved;
        for (field, _, _, _) in &old_fields {
            if !fields.iter().any(|f| &f.0 == field)
                && let Some(old) = old_fields.iter().find(|f| &f.0 == field)
            {
                reserved.insert(old.2);
            }
        }
        let mut next = used.iter().next_back().copied().unwrap_or(0);
        for f in fields.iter_mut() {
            match old_fields.iter().find(|o| o.0 == f.0) {
                Some(o) => f.2 = o.2,
                None => {
                    next += 1;
                    f.2 = next;
                }
            }
        }
    }
    out
}

/// A TOML string.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A TOML key: bare when it is one, quoted otherwise.
fn key(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        s.to_owned()
    } else {
        quote(s)
    }
}

/// The lock's text.
fn write(lock: &Lock) -> String {
    let mut out = String::from(
        "# rpc.lock: generated by `k build`; commit it. The shape of every protocol\n\
         # this app serves (docs/design/40-workspaces-and-services.md).\n",
    );
    for (name, rpcs) in &lock.protocols {
        let _ = write!(out, "\n[protocols.{}]\n", quote(name));
        if let Some(service) = lock.services.get(name) {
            let _ = writeln!(out, "service = {}", quote(service));
        }
        for (rpc, r) in rpcs {
            let _ = writeln!(out, "\n[protocols.{}.rpcs.{}]", quote(name), key(rpc));
            let params: Vec<String> = r
                .params
                .iter()
                .map(|(n, t, _)| format!("{{ name = {}, type = {} }}", quote(n), quote(t)))
                .collect();
            if r.channel.is_some() {
                out.push_str("kind = \"channel\"\n");
            }
            let _ = writeln!(out, "params = [{}]", params.join(", "));
            match &r.channel {
                Some(m) => {
                    let _ = writeln!(out, "send = {}", quote(&m.send));
                    let _ = writeln!(out, "receive = {}", quote(&m.receive));
                    let _ = writeln!(out, "refusal = {}", quote(&m.refusal));
                }
                None => {
                    let _ = writeln!(out, "returns = {}", quote(&r.returns));
                }
            }
            let _ = writeln!(out, "throws = {}", quote(&r.throws));
        }
    }
    for (name, shape) in &lock.types {
        let _ = write!(out, "\n[types.{}]\n", quote(name));
        match shape {
            Shape::Struct { fields, reserved } => {
                if fields.is_empty() {
                    out.push_str("fields = []\n");
                } else {
                    out.push_str("fields = [\n");
                    for (n, t, number, _) in fields {
                        let _ = writeln!(
                            out,
                            "    {{ name = {}, type = {}, number = {number} }},",
                            quote(n),
                            quote(t)
                        );
                    }
                    out.push_str("]\n");
                }
                if !reserved.is_empty() {
                    let list: Vec<String> = reserved.iter().map(u32::to_string).collect();
                    let _ = writeln!(out, "reserved = [{}]", list.join(", "));
                }
            }
            Shape::Enum(cases) => {
                // Every case is a table, even one without a payload, so a
                // bare name always means "recorded before payloads were".
                if cases.is_empty() {
                    out.push_str("cases = []\n");
                } else {
                    out.push_str("cases = [\n");
                    for c in cases {
                        match &c.fields {
                            Some(fields) if !fields.is_empty() => {
                                let list: Vec<String> = fields
                                    .iter()
                                    .map(|(n, t, _)| {
                                        format!("{{ name = {}, type = {} }}", quote(n), quote(t))
                                    })
                                    .collect();
                                let _ = writeln!(
                                    out,
                                    "    {{ name = {}, fields = [{}] }},",
                                    quote(&c.name),
                                    list.join(", ")
                                );
                            }
                            Some(_) => {
                                let _ = writeln!(out, "    {{ name = {} }},", quote(&c.name));
                            }
                            None => {
                                let _ = writeln!(out, "    {},", quote(&c.name));
                            }
                        }
                    }
                    out.push_str("]\n");
                }
            }
        }
    }
    out
}

/// Reads `rpc.lock`.
fn read(text: &str) -> Result<Lock, Invalid> {
    let table: toml::Table = toml::from_str(text).map_err(|e| Invalid {
        message: e.message().to_owned(),
        offset: e.span().map_or(0, |s| s.start),
    })?;
    let at = |key: &str, message: &str| Invalid {
        message: message.to_owned(),
        offset: find_key(text, key),
    };
    let structure = "doesn't have the structure of docs/compiler/sema.md §12.16";
    let mut out = Lock::default();
    if let Some(protocols) = table.get("protocols") {
        let protocols = protocols
            .as_table()
            .ok_or_else(|| at("protocols", &format!("`protocols` {structure}")))?;
        for (name, proto) in protocols {
            let bad = || at(name, &format!("`{name}` {structure}"));
            let proto = proto.as_table().ok_or_else(bad)?;
            if let Some(service) = proto.get("service") {
                let service = service.as_str().ok_or_else(bad)?;
                out.services.insert(name.clone(), service.to_owned());
            }
            let mut rpcs = BTreeMap::new();
            if let Some(list) = proto.get("rpcs") {
                for (rpc, r) in list.as_table().ok_or_else(bad)? {
                    let bad = || at(rpc, &format!("rpc `{rpc}` of `{name}` {structure}"));
                    let r = r.as_table().ok_or_else(bad)?;
                    let s = |k: &str| r.get(k).and_then(toml::Value::as_str).map(str::to_owned);
                    let mut params = Vec::new();
                    for param in r
                        .get("params")
                        .and_then(toml::Value::as_array)
                        .ok_or_else(bad)?
                    {
                        let t = param.as_table().ok_or_else(bad)?;
                        let n = t
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .ok_or_else(bad)?;
                        let ty = t
                            .get("type")
                            .and_then(toml::Value::as_str)
                            .ok_or_else(bad)?;
                        params.push((n.to_owned(), ty.to_owned(), false));
                    }
                    let channel = match r.get("kind") {
                        None => None,
                        Some(k) if k.as_str() == Some("channel") => Some(Messages {
                            send: s("send").ok_or_else(bad)?,
                            receive: s("receive").ok_or_else(bad)?,
                            refusal: s("refusal").ok_or_else(bad)?,
                        }),
                        Some(_) => return Err(bad()),
                    };
                    let returns = if channel.is_some() {
                        String::new()
                    } else {
                        s("returns").ok_or_else(bad)?
                    };
                    rpcs.insert(
                        rpc.clone(),
                        Rpc {
                            params,
                            returns,
                            throws: s("throws").ok_or_else(bad)?,
                            channel,
                        },
                    );
                }
            }
            out.protocols.insert(name.clone(), rpcs);
        }
    }
    if let Some(types) = table.get("types") {
        let types = types
            .as_table()
            .ok_or_else(|| at("types", &format!("`types` {structure}")))?;
        for (name, t) in types {
            let bad = || at(name, &format!("type `{name}` {structure}"));
            let t = t.as_table().ok_or_else(bad)?;
            let shape = if let Some(fields) = t.get("fields") {
                let mut list = Vec::new();
                let mut numbers = BTreeSet::new();
                for f in fields.as_array().ok_or_else(bad)? {
                    let f = f.as_table().ok_or_else(bad)?;
                    let n = f
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or_else(bad)?;
                    let ty = f
                        .get("type")
                        .and_then(toml::Value::as_str)
                        .ok_or_else(bad)?;
                    let number = f
                        .get("number")
                        .and_then(toml::Value::as_integer)
                        .and_then(|i| u32::try_from(i).ok())
                        .filter(|&i| i > 0)
                        .ok_or_else(bad)?;
                    if !numbers.insert(number) {
                        let needle = format!("number = {number}");
                        let offset = text
                            .match_indices(&needle)
                            .nth(1)
                            .map_or_else(|| find_key(text, name), |(i, _)| i);
                        return Err(Invalid {
                            message: format!("field number {number} of `{name}` is used twice"),
                            offset,
                        });
                    }
                    list.push((n.to_owned(), ty.to_owned(), number, false));
                }
                let mut reserved = BTreeSet::new();
                if let Some(r) = t.get("reserved") {
                    for v in r.as_array().ok_or_else(bad)? {
                        let n = v
                            .as_integer()
                            .and_then(|i| u32::try_from(i).ok())
                            .ok_or_else(bad)?;
                        if numbers.contains(&n) {
                            return Err(at(
                                name,
                                &format!("field number {n} of `{name}` is reserved and used"),
                            ));
                        }
                        reserved.insert(n);
                    }
                }
                Shape::Struct {
                    fields: list,
                    reserved,
                }
            } else if let Some(cases) = t.get("cases") {
                let mut list = Vec::new();
                for c in cases.as_array().ok_or_else(bad)? {
                    if let Some(name) = c.as_str() {
                        list.push(Case {
                            name: name.to_owned(),
                            fields: None,
                        });
                        continue;
                    }
                    let c = c.as_table().ok_or_else(bad)?;
                    let name = c
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .ok_or_else(bad)?;
                    let mut fields = Vec::new();
                    if let Some(list) = c.get("fields") {
                        for f in list.as_array().ok_or_else(bad)? {
                            let f = f.as_table().ok_or_else(bad)?;
                            let n = f
                                .get("name")
                                .and_then(toml::Value::as_str)
                                .ok_or_else(bad)?;
                            let ty = f
                                .get("type")
                                .and_then(toml::Value::as_str)
                                .ok_or_else(bad)?;
                            fields.push((n.to_owned(), ty.to_owned(), false));
                        }
                    }
                    list.push(Case {
                        name: name.to_owned(),
                        fields: Some(fields),
                    });
                }
                Shape::Enum(list)
            } else {
                return Err(bad());
            };
            out.types.insert(name.clone(), shape);
        }
    }
    Ok(out)
}

/// The offset of the line of the table header for `key`, else of the first
/// line mentioning it, else 0.
fn find_key(text: &str, key: &str) -> usize {
    [format!("{key}\"]"), format!(".{key}]"), key.to_owned()]
        .iter()
        .find_map(|needle| text.find(needle.as_str()))
        .map_or(0, |i| text[..i].rfind('\n').map_or(0, |n| n + 1))
}

/// The range of the line containing `offset`, without its line break.
fn line_at(text: &str, offset: usize) -> TextRange {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let start = text[..offset].rfind('\n').map_or(0, |n| n + 1);
    let end = text[start..].find('\n').map_or(text.len(), |n| start + n);
    let end = if text[start..end].ends_with('\r') {
        end - 1
    } else {
        end
    };
    let size = |n: usize| TextSize::from(u32::try_from(n).unwrap_or(u32::MAX));
    TextRange::new(size(start), size(end))
}

/// `true` when the declaration carries `@name`.
fn has_annotation(cx: &Cx<'_, '_>, d: DeclId, name: &str) -> bool {
    cx.p.decl(d)
        .node
        .children()
        .filter_map(ast::Annotation::cast)
        .any(|a| a.name().is_some_and(|t| t.text() == name))
}

/// Reports the breaking changes between the lock and the program (K8001).
fn compare(
    cx: &Cx<'_, '_>,
    lock: &crate::locks::Lockfile,
    app: DeclId,
    served: &[(String, DeclId)],
    recorded: &Lock,
    now: &Lock,
) {
    let p = cx.p;
    let header = |name: &str| line_at(&lock.text, find_key(&lock.text, name));
    for (name, old_rpcs) in &recorded.protocols {
        let Some((_, d)) = served.iter().find(|(n, _)| n == name) else {
            let decl = p.decl(app);
            cx.report(
                decl.file,
                Diagnostic::new(
                    codes::BREAKING_CHANGE,
                    p.span(decl.file, decl.name_range),
                    format!("`{name}` is recorded in rpc.lock, but the app no longer serves it"),
                )
                .with_label(Span::new(lock.id, header(name)), "recorded here")
                .with_note(
                    "callers already deployed still call it; keep implementing it, or delete its entries from rpc.lock once none does",
                ),
            );
            continue;
        };
        let new_rpcs = now.protocols.get(name).cloned().unwrap_or_default();
        let mut changes = Vec::new();
        if let (Some(old), Some(new)) = (recorded.services.get(name), now.services.get(name))
            && old != new
        {
            changes.push(format!(
                    "its wire name changed from `{old}` to `{new}` (the project's name and the protocol's module are part of it), so deployed callers' paths no longer match"
                ));
        }
        let mut types: BTreeSet<String> = BTreeSet::new();
        for (rpc, old) in old_rpcs {
            let Some(new) = new_rpcs.get(rpc) else {
                changes.push(format!("rpc `{rpc}` was removed"));
                continue;
            };
            for (n, t, _) in &old.params {
                match new.params.iter().find(|(m, _, _)| m == n) {
                    None => changes.push(format!("parameter `{n}` of rpc `{rpc}` was removed")),
                    Some((_, u, _)) if u != t => changes.push(format!(
                        "parameter `{n}` of rpc `{rpc}` was `{t}` and is now `{u}`"
                    )),
                    Some(_) => {
                        types.insert(t.clone());
                    }
                }
            }
            for (n, _, optional) in &new.params {
                if !optional && !old.params.iter().any(|(m, _, _)| m == n) {
                    changes.push(format!(
                        "parameter `{n}` of rpc `{rpc}` is new and required; a new parameter must be optional"
                    ));
                }
            }
            match (&old.channel, &new.channel) {
                (None, None) if old.returns == new.returns => {
                    types.insert(old.returns.clone());
                }
                (None, None) => changes.push(format!(
                    "rpc `{rpc}` returned `{}` and now returns `{}`",
                    old.returns, new.returns
                )),
                (Some(_), None) => changes.push(format!(
                    "rpc `{rpc}` was a channel and now returns `{}`; open tabs still open it as a channel",
                    new.returns
                )),
                (None, Some(_)) => changes.push(format!(
                    "rpc `{rpc}` returned `{}` and is now a channel; deployed callers still call it as an rpc",
                    old.returns
                )),
                (Some(o), Some(n)) => {
                    for (what, a, b) in [
                        ("client messages", &o.send, &n.send),
                        ("server messages", &o.receive, &n.receive),
                        ("refusal", &o.refusal, &n.refusal),
                    ] {
                        if a == b {
                            types.insert(a.clone());
                        } else {
                            changes.push(format!(
                                "the {what} of channel `{rpc}` changed from `{a}` to `{b}`"
                            ));
                        }
                    }
                }
            }
            if old.throws == new.throws {
                types.insert(old.throws.clone());
            } else {
                changes.push(format!(
                    "rpc `{rpc}` threw `{}` and now throws `{}`",
                    old.throws, new.throws
                ));
            }
        }
        // The recorded types still carried by the rpcs, transitively.
        let mut todo: Vec<String> = types.into_iter().flat_map(|t| names_in(&t)).collect();
        let mut done: BTreeSet<String> = BTreeSet::new();
        let mut type_changes: Vec<String> = Vec::new();
        while let Some(t) = todo.pop() {
            if !done.insert(t.clone()) {
                continue;
            }
            let (Some(old), Some(new)) = (recorded.types.get(&t), now.types.get(&t)) else {
                continue;
            };
            match (old, new) {
                (Shape::Struct { fields: of, .. }, Shape::Struct { fields: nf, .. }) => {
                    for (n, ty, _, _) in of {
                        match nf.iter().find(|f| &f.0 == n) {
                            None => type_changes.push(format!("field `{n}` of `{t}` was removed")),
                            Some(f) if &f.1 != ty => type_changes.push(format!(
                                "field `{n}` of `{t}` was `{ty}` and is now `{}`",
                                f.1
                            )),
                            Some(_) => todo.extend(names_in(ty)),
                        }
                    }
                    for (n, _, _, optional) in nf {
                        if !optional && !of.iter().any(|f| &f.0 == n) {
                            type_changes.push(format!(
                                "field `{n}` of `{t}` is new and required; a new field must be optional"
                            ));
                        }
                    }
                }
                (Shape::Enum(oc), Shape::Enum(nc)) => {
                    for c in oc {
                        let Some(n) = nc.iter().find(|n| n.name == c.name) else {
                            // A type only a channel's server messages reach
                            // may lose a case: the server stops sending it.
                            if now.strict.contains(&t) {
                                type_changes
                                    .push(format!("case `{}` of `{t}` was removed", c.name));
                            }
                            continue;
                        };
                        // A case's fields follow a struct's rules, in both
                        // directions (44-channels.md, "rpc.lock and
                        // compatibility"); a bare recorded name has none to
                        // compare.
                        let (Some(of), Some(nf)) = (&c.fields, &n.fields) else {
                            continue;
                        };
                        let case = &c.name;
                        for (f, ty, _) in of {
                            match nf.iter().find(|g| &g.0 == f) {
                                None => type_changes.push(format!(
                                    "field `{f}` of case `{case}` of `{t}` was removed"
                                )),
                                Some(g) if &g.1 != ty => type_changes.push(format!(
                                    "field `{f}` of case `{case}` of `{t}` was `{ty}` and is now `{}`",
                                    g.1
                                )),
                                Some(_) => todo.extend(names_in(ty)),
                            }
                        }
                        for (f, _, optional) in nf {
                            if !optional && !of.iter().any(|g| &g.0 == f) {
                                type_changes.push(format!(
                                    "field `{f}` of case `{case}` of `{t}` is new and required; a new field must be optional"
                                ));
                            }
                        }
                    }
                    if let Some(payloads) = now.payloads.get(&t) {
                        todo.extend(payloads.iter().cloned());
                    }
                }
                _ => type_changes.push(format!("`{t}` changed between a struct and an enum")),
            }
        }
        type_changes.sort();
        changes.sort();
        changes.extend(type_changes);
        if changes.is_empty() || has_annotation(cx, *d, "Breaking") {
            continue;
        }
        let decl = p.decl(*d);
        let mut diag = Diagnostic::new(
            codes::BREAKING_CHANGE,
            p.span(decl.file, decl.name_range),
            format!(
                "`{name}` changed in a way that breaks callers already deployed, without `@Breaking`"
            ),
        )
        .with_label(Span::new(lock.id, header(name)), "recorded here");
        for c in changes {
            diag = diag.with_note(c);
        }
        diag = diag.with_note(
            "keep the recorded shape (adding an rpc, an optional field or an error case is compatible), or mark the protocol `@Breaking(\"reason\")` once no deployed caller relies on it",
        );
        cx.report(decl.file, diag);
    }
}

/// The type names in a written type: `List<cart.Item>?` → `List`,
/// `cart.Item`.
fn names_in(t: &str) -> Vec<String> {
    t.split(['<', '>', ',', '?', ' '])
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> Lock {
        let mut l = Lock::default();
        let mut rpcs = BTreeMap::new();
        rpcs.insert(
            "add".to_owned(),
            Rpc {
                params: vec![("item".into(), "cart.Item".into(), false)],
                returns: "Int".into(),
                throws: "cart.CartError".into(),
                channel: None,
            },
        );
        rpcs.insert(
            "follow".to_owned(),
            Rpc {
                params: vec![("id".into(), "Int".into(), false)],
                returns: String::new(),
                throws: "cart.CartError".into(),
                channel: Some(Messages {
                    send: "cart.Item".into(),
                    receive: "cart.CartError".into(),
                    refusal: "cart.CartError".into(),
                }),
            },
        );
        l.protocols.insert("cart.CartApi".into(), rpcs);
        l.types.insert(
            "cart.Item".into(),
            Shape::Struct {
                fields: vec![
                    ("id".into(), "Int".into(), 1, false),
                    ("note".into(), "String?".into(), 3, true),
                ],
                reserved: BTreeSet::from([2]),
            },
        );
        l.types.insert(
            "cart.CartError".into(),
            Shape::Enum(vec![
                Case {
                    name: "gone".into(),
                    fields: Some(vec![("left".into(), "Int?".into(), true)]),
                },
                Case {
                    name: "full".into(),
                    fields: Some(Vec::new()),
                },
                Case {
                    name: "rpcFailure".into(),
                    fields: Some(vec![("_0".into(), "RpcFailure".into(), false)]),
                },
            ]),
        );
        l
    }

    /// The comparable part of a lock: what `read` gets back.
    fn strip(mut l: Lock) -> Lock {
        for rpcs in l.protocols.values_mut() {
            for r in rpcs.values_mut() {
                for p in &mut r.params {
                    p.2 = false;
                }
            }
        }
        for t in l.types.values_mut() {
            match t {
                Shape::Struct { fields, .. } => {
                    for f in fields {
                        f.3 = false;
                    }
                }
                Shape::Enum(cases) => {
                    for f in cases.iter_mut().flat_map(|c| c.fields.iter_mut().flatten()) {
                        f.2 = false;
                    }
                }
            }
        }
        l
    }

    #[test]
    fn written_locks_read_back() {
        let text = write(&lock());
        assert!(text.contains(
            "[protocols.\"cart.CartApi\".rpcs.follow]\nkind = \"channel\"\nparams = [{ name = \"id\", type = \"Int\" }]\nsend = \"cart.Item\"\nreceive = \"cart.CartError\"\nrefusal = \"cart.CartError\"\nthrows = \"cart.CartError\"\n"
        ));
        assert!(text.contains(
            "[types.\"cart.CartError\"]\ncases = [\n    { name = \"gone\", fields = [{ name = \"left\", type = \"Int?\" }] },\n    { name = \"full\" },\n    { name = \"rpcFailure\", fields = [{ name = \"_0\", type = \"RpcFailure\" }] },\n]\n"
        ));
        let back = read(&text).ok().unwrap();
        assert_eq!(back, strip(lock()));
        assert_eq!(write(&back), text);
    }

    #[test]
    fn bare_case_names_read_as_unrecorded_payloads() {
        let back = read("[types.\"a.E\"]\ncases = [\"x\", { name = \"y\" }]\n")
            .ok()
            .unwrap();
        assert_eq!(
            back.types.get("a.E"),
            Some(&Shape::Enum(vec![
                Case {
                    name: "x".into(),
                    fields: None,
                },
                Case {
                    name: "y".into(),
                    fields: Some(Vec::new()),
                },
            ]))
        );
    }

    #[test]
    fn merging_keeps_numbers_and_reserves_removed_ones() {
        let mut now = lock();
        if let Some(Shape::Struct { fields, reserved }) = now.types.get_mut("cart.Item") {
            // `id` removed, `size` added, `note` kept.
            *fields = vec![
                ("note".into(), "String?".into(), 0, true),
                ("size".into(), "Int?".into(), 0, true),
            ];
            reserved.clear();
        }
        let merged = merge(&now, &lock());
        let Some(Shape::Struct { fields, reserved }) = merged.types.get("cart.Item") else {
            panic!("no struct");
        };
        assert_eq!(fields[0].2, 3);
        assert_eq!(fields[1].2, 4);
        assert_eq!(reserved, &BTreeSet::from([1, 2]));
    }

    #[test]
    fn bad_locks_are_rejected_with_a_line() {
        for text in [
            "[protocols\n",
            "protocols = 1\n",
            "[types.\"a.B\"]\nfields = [{ name = \"x\", type = \"Int\", number = 0 }]\n",
            "[types.\"a.B\"]\nfields = [{ name = \"x\", type = \"Int\", number = 1 }, { name = \"y\", type = \"Int\", number = 1 }]\n",
            "[types.\"a.B\"]\nfields = []\nreserved = [\"x\"]\n",
            "[types.\"a.B\"]\nnothing = 1\n",
            "[types.\"a.E\"]\ncases = [1]\n",
            "[types.\"a.E\"]\ncases = [{ fields = [] }]\n",
            "[types.\"a.E\"]\ncases = [{ name = \"x\", fields = [{ name = \"y\" }] }]\n",
            "[protocols.\"a.P\".rpcs.r]\nparams = [1]\nreturns = \"Int\"\nthrows = \"E\"\n",
            "[protocols.\"a.P\".rpcs.r]\nkind = \"stream\"\nparams = []\nreturns = \"Int\"\nthrows = \"E\"\n",
            "[protocols.\"a.P\".rpcs.r]\nkind = \"channel\"\nparams = []\nsend = \"A\"\nthrows = \"E\"\n",
        ] {
            let Err(e) = read(text) else {
                panic!("{text:?} was read");
            };
            assert!(e.offset <= text.len());
            let _ = line_at(text, e.offset);
        }
    }

    #[test]
    fn keys_and_strings_are_escaped() {
        assert_eq!(key("add"), "add");
        assert_eq!(key("a.b"), "\"a.b\"");
        assert_eq!(quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(
            names_in("Map<String, List<cart.Item>>?"),
            ["Map", "String", "List", "cart.Item"]
        );
    }
}
