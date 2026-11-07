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
//!   type of any of them, adding a required parameter or field, or a
//!   recorded protocol the app no longer serves breaks callers: **K8001**
//!   at the protocol (at the app for one it no longer serves), with one note
//!   per change and a label at the lock, unless the protocol is marked
//!   `@Breaking("reason")`;
//! * adding an rpc, an optional field, an optional parameter or an error
//!   case is compatible;
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
    /// App and library types by qualified name.
    types: BTreeMap<String, Shape>,
}

/// One rpc.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rpc {
    /// The parameters: name, type, and whether it may be left out
    /// (optional or defaulted; not recorded, only used for comparing).
    params: Vec<(String, String, bool)>,
    /// The result type.
    returns: String,
    /// The error type.
    throws: String,
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
    /// An enum's case names.
    Enum(Vec<String>),
}

/// Why the lock can't be read: a message and the byte offset of the
/// problem.
struct Invalid {
    message: String,
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

/// The program's shape of the served protocols and the types they carry.
fn current(cx: &Cx<'_, '_>, served: &[(String, DeclId)]) -> Lock {
    let p = cx.p;
    let mut out = Lock::default();
    let mut pending: Vec<Ty> = Vec::new();
    for (name, d) in served {
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
            pending.push(sig.ret.clone());
            let throws = sig.throws.clone().unwrap_or(Ty::Error);
            pending.push(throws.clone());
            rpcs.insert(
                member.name.clone(),
                Rpc {
                    params,
                    returns: text(cx, &sig.ret),
                    throws: text(cx, &throws),
                },
            );
        }
        out.protocols.insert(name.clone(), rpcs);
    }
    // The app and library types reachable from the signatures.
    let mut seen: BTreeSet<DeclId> = BTreeSet::new();
    while let Some(t) = pending.pop() {
        match &t {
            Ty::Optional(inner) => pending.push((**inner).clone()),
            Ty::Named(d, args) => {
                pending.extend(args.iter().cloned());
                let d = *d;
                if p.is_stdlib(d) || !seen.insert(d) {
                    continue;
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
                        for m in cx.cases(d) {
                            cases.push(p.member(m).name.clone());
                            pending.extend(cx.case_sig(m).params.iter().map(|sp| sp.ty.clone()));
                        }
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
            if !fields.iter().any(|f| &f.0 == field) {
                if let Some(old) = old_fields.iter().find(|f| &f.0 == field) {
                    reserved.insert(old.2);
                }
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
        for (rpc, r) in rpcs {
            let _ = writeln!(out, "\n[protocols.{}.rpcs.{}]", quote(name), key(rpc));
            let params: Vec<String> = r
                .params
                .iter()
                .map(|(n, t, _)| format!("{{ name = {}, type = {} }}", quote(n), quote(t)))
                .collect();
            let _ = writeln!(out, "params = [{}]", params.join(", "));
            let _ = writeln!(out, "returns = {}", quote(&r.returns));
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
                let list: Vec<String> = cases.iter().map(|c| quote(c)).collect();
                let _ = writeln!(out, "cases = [{}]", list.join(", "));
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
                    rpcs.insert(
                        rpc.clone(),
                        Rpc {
                            params,
                            returns: s("returns").ok_or_else(bad)?,
                            throws: s("throws").ok_or_else(bad)?,
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
                    list.push(c.as_str().ok_or_else(bad)?.to_owned());
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
            if old.returns == new.returns {
                types.insert(old.returns.clone());
            } else {
                changes.push(format!(
                    "rpc `{rpc}` returned `{}` and now returns `{}`",
                    old.returns, new.returns
                ));
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
                        if !nc.contains(c) {
                            type_changes.push(format!("case `{c}` of `{t}` was removed"));
                        }
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
            Shape::Enum(vec!["gone".into(), "rpcFailure".into()]),
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
            if let Shape::Struct { fields, .. } = t {
                for f in fields {
                    f.3 = false;
                }
            }
        }
        l
    }

    #[test]
    fn written_locks_read_back() {
        let text = write(&lock());
        let back = read(&text).ok().unwrap();
        assert_eq!(back, strip(lock()));
        assert_eq!(write(&back), text);
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
            "[protocols.\"a.P\".rpcs.r]\nparams = [1]\nreturns = \"Int\"\nthrows = \"E\"\n",
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
