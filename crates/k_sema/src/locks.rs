//! Lockfile checks (sema.md §12.16): the storage keys and job payloads
//! recorded in `storage.lock` against the program's keys and jobs.
//!
//! Jobs queued by one release are run by the next, so a job's payload is
//! stored data: `storage.lock` records each job's name and the `Codable`
//! shape of its payload (`docs/design/30-background-jobs.md`, "Stored
//! payloads"; the file format is `docs/design/36-local-storage.md`,
//! "File format"). This module compares every recorded job with the job
//! of that name in the program:
//!
//! * a field of a recorded struct that is removed or renamed, a field whose
//!   type changes, a required field added, a case removed or renamed from a
//!   recorded enum, or a recorded optional made required is incompatible:
//!   **K8003** at the payload type argument of `implements Job<…>`, one
//!   note per change, unless the job service is marked
//!   `@Breaking("reason")`;
//! * an optional field added, a case added, or a required field made
//!   optional is compatible.
//!
//! The `[keys.<name>]` entries are compared the same way with each
//! `StorageKey<T>` constant or field (matched by its `name:` literal, else
//! by the recorded `declaration`): an incompatible value type, or a key
//! whose declaration is recorded under another name (a renamed key), is
//! **K8008** at the key's name, unless the key is marked
//! `@Breaking("reason")` (`docs/design/36-local-storage.md`,
//! "`storage.lock`").
//!
//! A `storage.lock` that isn't valid TOML of that structure is **K8002** at
//! the offending line. A job the lock doesn't record yet, and a recorded
//! job that no longer exists, are not reported: when the compiler writes
//! the lockfiles is open (`docs/design/13-tooling.md`, "Lockfiles").
//!
//! Two checks over the names those keys share live here too, since they
//! find the keys the same way: a storage key named like an earlier one is
//! **K2405**, a `DragKind` named like an earlier one with another
//! payload type is **K2406**, and a `TabChannel` or `DataChannelKind` named
//! like an earlier one of its type with another message type is **K2408**;
//! two `DataChannelKind`s whose names give the same channel id are
//! **K2409**.
//!
//! Types are compared by shape, not by name: a payload struct renamed with
//! the same fields still decodes the queued jobs, so it is compatible.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use k_diagnostics::{Diagnostic, FileId, Span, TextRange, TextSize, codes};
use k_syntax::SyntaxKind;
use k_syntax::ast::{self, AstNode};

use crate::cx::Cx;
use crate::program::{DeclId, MemberKind};
use crate::project::LockfileKind;
use crate::stdlib::DeclarationKind as K;
use crate::ty::Ty;

/// One lockfile of the project, as the host holds it.
#[derive(Clone, Debug)]
pub(crate) struct Lockfile {
    /// The host's id of the file.
    pub(crate) id: FileId,
    /// Which lockfile it is.
    pub(crate) kind: LockfileKind,
    /// Its text.
    pub(crate) text: Arc<str>,
}

/// `true` when `decl` is a service implementing `k.server.Job`.
pub(crate) fn is_job(cx: &Cx<'_, '_>, decl: DeclId) -> bool {
    job_payload(cx, decl).is_some()
}

/// The payload type of a job service: the argument of its `Job<…>`.
pub(crate) fn job_payload(cx: &Cx<'_, '_>, decl: DeclId) -> Option<Ty> {
    if cx.p.decl(decl).named_kind() != Some(K::Service) {
        return None;
    }
    cx.implements_list(decl).iter().find_map(|(t, _)| match t {
        Ty::Named(d, args) if cx.p.decl_name(*d) == "k.server.Job" => {
            Some(args.first().cloned().unwrap_or(Ty::Error))
        }
        _ => None,
    })
}

/// Runs the lockfile checks.
pub(crate) fn check(cx: &Cx<'_, '_>) {
    duplicate_keys(cx);
    conflicting_drag_kinds(cx);
    conflicting_channels(cx, tab_message, "tab channel");
    conflicting_channels(cx, data_channel_message, "data channel kind");
    colliding_data_channels(cx);
    for lock in &cx.p.lockfiles {
        if lock.kind == LockfileKind::Storage {
            storage(cx, lock);
        }
    }
}

// ------------------------------------------------------------ the lockfile

/// A type as `storage.lock` writes it: `List<cart.SavedItem>`, `String?`.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Written {
    /// `T?`.
    Optional(Box<Written>),
    /// A name with its arguments.
    Named(String, Vec<Written>),
}

impl Written {
    /// Parses a written type; `None` for text that isn't one.
    fn parse(text: &str) -> Option<Written> {
        let tokens = tokenize(text);
        let mut i = 0;
        let t = parse_type(&tokens, &mut i)?;
        (i == tokens.len()).then_some(t)
    }

    /// The text again, as written in the lock.
    fn text(&self) -> String {
        match self {
            Written::Optional(inner) => format!("{}?", inner.text()),
            Written::Named(n, args) if args.is_empty() => n.clone(),
            Written::Named(n, args) => format!(
                "{n}<{}>",
                args.iter()
                    .map(Written::text)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// The tokens of a written type: names (with dots), `<`, `>`, `,`, `?`.
fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut name = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '.' || c == '_' {
            name.push(c);
            continue;
        }
        if !name.is_empty() {
            out.push(std::mem::take(&mut name));
        }
        if matches!(c, '<' | '>' | ',' | '?') {
            out.push(c.to_string());
        } else if !c.is_whitespace() {
            // Anything else makes the text unparsable.
            out.push(String::from("\u{0}"));
        }
    }
    if !name.is_empty() {
        out.push(name);
    }
    out
}

/// Parses one type at `tokens[*i]`.
fn parse_type(tokens: &[String], i: &mut usize) -> Option<Written> {
    let name = tokens.get(*i)?;
    if !name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        return None;
    }
    let name = name.clone();
    *i += 1;
    let mut args = Vec::new();
    if tokens.get(*i).is_some_and(|t| t == "<") {
        *i += 1;
        loop {
            args.push(parse_type(tokens, i)?);
            match tokens.get(*i).map(String::as_str) {
                Some(",") => *i += 1,
                Some(">") => {
                    *i += 1;
                    break;
                }
                _ => return None,
            }
        }
    }
    let mut t = Written::Named(name, args);
    while tokens.get(*i).is_some_and(|t| t == "?") {
        *i += 1;
        if !matches!(t, Written::Optional(_)) {
            t = Written::Optional(Box::new(t));
        }
    }
    Some(t)
}

/// One field or payload entry of a recorded type.
#[derive(Clone, Debug)]
struct WrittenField {
    /// The field name; `None` for an unlabelled payload of an enum case.
    name: Option<String>,
    /// Its type.
    ty: Written,
}

/// The recorded shape of an app type.
#[derive(Clone, Debug)]
enum Shape {
    /// A struct's fields, in order.
    Struct(Vec<WrittenField>),
    /// An enum's cases, each with its payload (empty without one).
    Enum(Vec<(String, Vec<WrittenField>)>),
}

/// What `storage.lock` records about keys and jobs.
#[derive(Debug, Default)]
struct StorageLock {
    /// Storage keys by name: (declaration, value type).
    keys: BTreeMap<String, (String, Written)>,
    /// Jobs by name: (declaration, payload).
    jobs: BTreeMap<String, (String, Written)>,
    /// The shapes of app types by qualified name.
    types: BTreeMap<String, Shape>,
}

/// Why a lockfile couldn't be read: the message and the byte offset of the
/// problem.
struct Invalid {
    /// What is wrong, for the diagnostic.
    message: String,
    /// The byte offset in the lockfile where the problem is (0 when the
    /// TOML parser gives no position).
    offset: usize,
}

/// Reads `storage.lock`.
fn read_storage(text: &str) -> Result<StorageLock, Invalid> {
    let table: toml::Table = toml::from_str(text).map_err(|e| Invalid {
        message: e.message().to_owned(),
        offset: e.span().map_or(0, |s| s.start),
    })?;
    let at = |key: &str| Invalid {
        message: format!(
            "`{key}` doesn't have the structure of 36-local-storage.md, \"File format\""
        ),
        offset: find_key(text, key),
    };
    let mut out = StorageLock::default();
    if let Some(keys) = table.get("keys") {
        let keys = keys.as_table().ok_or_else(|| at("keys"))?;
        for (name, key) in keys {
            let key = key.as_table().ok_or_else(|| at(name))?;
            let declaration = key
                .get("declaration")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| at(name))?;
            let ty = key
                .get("type")
                .and_then(toml::Value::as_str)
                .and_then(Written::parse)
                .ok_or_else(|| at(name))?;
            out.keys.insert(name.clone(), (declaration.to_owned(), ty));
        }
    }
    if let Some(jobs) = table.get("jobs") {
        let jobs = jobs.as_table().ok_or_else(|| at("jobs"))?;
        for (name, job) in jobs {
            let job = job.as_table().ok_or_else(|| at(name))?;
            let declaration = job
                .get("declaration")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| at(name))?;
            let payload = job
                .get("payload")
                .and_then(toml::Value::as_str)
                .and_then(Written::parse)
                .ok_or_else(|| at(name))?;
            out.jobs
                .insert(name.clone(), (declaration.to_owned(), payload));
        }
    }
    if let Some(types) = table.get("types") {
        let types = types.as_table().ok_or_else(|| at("types"))?;
        for (name, t) in types {
            let t = t.as_table().ok_or_else(|| at(name))?;
            let shape = if let Some(fields) = t.get("fields") {
                Shape::Struct(read_fields(fields).ok_or_else(|| at(name))?)
            } else if let Some(cases) = t.get("cases").and_then(toml::Value::as_array) {
                let mut out = Vec::new();
                for c in cases {
                    match c {
                        toml::Value::String(s) => out.push((s.clone(), Vec::new())),
                        toml::Value::Table(c) => {
                            let case = c
                                .get("name")
                                .and_then(toml::Value::as_str)
                                .ok_or_else(|| at(name))?;
                            let fields = match c.get("fields") {
                                Some(f) => read_fields(f).ok_or_else(|| at(name))?,
                                None => Vec::new(),
                            };
                            out.push((case.to_owned(), fields));
                        }
                        _ => return Err(at(name)),
                    }
                }
                Shape::Enum(out)
            } else {
                return Err(at(name));
            };
            out.types.insert(name.clone(), shape);
        }
    }
    Ok(out)
}

/// Reads a `fields = [{ name = "…", type = "…" }, …]` array.
fn read_fields(v: &toml::Value) -> Option<Vec<WrittenField>> {
    v.as_array()?
        .iter()
        .map(|f| {
            let f = f.as_table()?;
            Some(WrittenField {
                name: f
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                ty: Written::parse(f.get("type")?.as_str()?)?,
            })
        })
        .collect()
}

/// The offset of the line of the table header for `key` (`[jobs.<key>]`,
/// `[types."<key>"]`), else of the first line mentioning it, else 0.
fn find_key(text: &str, key: &str) -> usize {
    [format!("{key}]"), format!("{key}\"]"), key.to_owned()]
        .iter()
        .find_map(|needle| text.find(needle.as_str()))
        .map_or(0, |i| text[..i].rfind('\n').map_or(0, |n| n + 1))
}

/// The range of the line containing `offset`, without its line break.
fn line_at(text: &str, offset: usize) -> TextRange {
    let offset = offset.min(text.len());
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

// ------------------------------------------------------------- the check

/// Compares `storage.lock`'s jobs with the program's (K8002, K8003).
fn storage(cx: &Cx<'_, '_>, lock: &Lockfile) {
    let recorded = match read_storage(&lock.text) {
        Ok(r) => r,
        Err(invalid) => {
            cx.diags.borrow_mut().push(Diagnostic::new(
                codes::INVALID_LOCKFILE,
                Span::new(lock.id, line_at(&lock.text, invalid.offset)),
                format!("storage.lock can't be read: {}", invalid.message.trim()),
            ));
            return;
        }
    };
    keys(cx, &recorded);
    let p = cx.p;
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib || p.file(decl.file).is_test {
            continue;
        }
        let Some(payload) = job_payload(cx, d) else {
            continue;
        };
        if payload.contains_error() {
            continue;
        }
        let qualified = p.decl_name(d);
        let name = job_name(cx, d).or_else(|| {
            recorded
                .jobs
                .iter()
                .find(|(_, (declaration, _))| *declaration == qualified)
                .map(|(n, _)| n.clone())
        });
        let Some((name, (_, written))) = name.and_then(|n| recorded.jobs.get_key_value(&n)) else {
            continue;
        };
        let mut changes = Vec::new();
        let mut cmp = Compare {
            cx,
            lock: &recorded,
            seen: HashSet::new(),
            changes: &mut changes,
        };
        cmp.ty(written, &payload, "the payload");
        if changes.is_empty() || has_annotation(cx, d, "Breaking") {
            continue;
        }
        let range = payload_range(cx, d).unwrap_or(decl.name_range);
        let mut diag = Diagnostic::new(
            codes::INCOMPATIBLE_JOB_PAYLOAD,
            p.span(decl.file, range),
            format!(
                "the payload of job `{name}` changed incompatibly with storage.lock; jobs queued by the previous release couldn't be decoded"
            ),
        );
        for change in changes {
            diag = diag.with_note(change);
        }
        diag = diag.with_note(
            "keep the recorded shape (only optional fields and new enum cases are compatible), or mark the job `@Breaking(\"reason\")` once no such job can be queued",
        );
        cx.report(decl.file, diag);
    }
}

/// One `StorageKey` of the program: a top-level constant or a field whose
/// type is `StorageKey<T>`. [`conflicting_drag_kinds`] uses the same shape
/// for a `DragKind<T>`, with its payload type as `value`.
struct Key {
    /// The qualified name of the constant or field,
    /// `cart.CartService.savedCart`.
    declaration: String,
    /// The name its initializer gives, `StorageKey(name: "cart")`.
    name: Option<String>,
    /// The range of that name's string literal.
    literal: Option<TextRange>,
    /// The value type `T`.
    value: Ty,
    /// Where diagnostics point: the constant's or field's name.
    file: crate::program::FileIx,
    /// Its range.
    range: TextRange,
    /// `true` when it is marked `@Breaking("reason")`.
    breaking: bool,
}

/// The value type of a `StorageKey<T>`.
pub(crate) fn key_value(cx: &Cx<'_, '_>, ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Named(d, args) if cx.p.decl_name(*d) == "k.storage.StorageKey" => {
            Some(args.first().cloned().unwrap_or(Ty::Error))
        }
        _ => None,
    }
}

/// The `name:` literal of a `StorageKey(name: "…")` initializer below
/// `node`, with the literal's range.
fn key_name(node: &k_syntax::SyntaxNode) -> Option<(String, TextRange)> {
    node.descendants()
        .filter_map(ast::CallExpr::cast)
        .find_map(|c| {
            c.arg_list()?.args().find_map(|a| {
                if a.label()?.text() != "name" {
                    return None;
                }
                match a.value()? {
                    ast::Expr::StringLit(s) => Some((s.plain_value()?, s.syntax().text_range())),
                    _ => None,
                }
            })
        })
}

/// `true` when `node` carries `@Breaking`.
fn node_has_breaking(node: &k_syntax::SyntaxNode) -> bool {
    node.children()
        .filter_map(ast::Annotation::cast)
        .any(|a| a.name().is_some_and(|t| t.text() == "Breaking"))
}

/// Every `StorageKey` of the program's non-test code.
///
/// Finding them resolves the type of every constant and field. A type that
/// doesn't resolve is reported where it is written, by the check of its
/// declaration; when that file's facts were replayed by an incremental
/// re-check, resolving it here would report it a second time, so the
/// diagnostics this function causes are dropped (as `view::build` drops
/// those of the signatures it resolves).
fn program_keys(cx: &Cx<'_, '_>) -> Vec<Key> {
    let reported = cx.diags.borrow().len();
    let out = find_keys(cx);
    cx.diags.borrow_mut().truncate(reported);
    out
}

/// [`program_keys`], with the diagnostics of the types it resolves.
fn find_keys(cx: &Cx<'_, '_>) -> Vec<Key> {
    find_named(cx, key_value, false)
}

/// The payload type of a `DragKind<T>`.
fn drag_payload(cx: &Cx<'_, '_>, ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Named(d, args) if cx.p.decl_name(*d) == "k.drag.DragKind" => {
            Some(args.first().cloned().unwrap_or(Ty::Error))
        }
        _ => None,
    }
}

/// Every top-level constant and field of the program whose type
/// `value_of` maps to a value type (a `StorageKey<T>` to `T`), with the
/// `name:` literal of its initializer; test code too when `tests` is set.
/// Like [`find_keys`], it resolves types and keeps their diagnostics.
fn find_named(
    cx: &Cx<'_, '_>,
    value_of: fn(&Cx<'_, '_>, &Ty) -> Option<Ty>,
    tests: bool,
) -> Vec<Key> {
    let p = cx.p;
    let mut out = Vec::new();
    for (i, decl) in p.decls.iter().enumerate() {
        let d = DeclId(u32::try_from(i).unwrap_or(u32::MAX));
        if p.file(decl.file).stdlib || (p.file(decl.file).is_test && !tests) {
            continue;
        }
        if decl.named_kind() == Some(K::Constant) {
            if let Some(value) = value_of(cx, &cx.constant_ty(d)) {
                let named = key_name(&decl.node);
                out.push(Key {
                    declaration: p.decl_name(d),
                    name: named.as_ref().map(|n| n.0.clone()),
                    literal: named.map(|n| n.1),
                    value,
                    file: decl.file,
                    range: decl.name_range,
                    breaking: node_has_breaking(&decl.node),
                });
            }
            continue;
        }
        for &m in &decl.members {
            let member = p.member(m);
            if member.kind != MemberKind::Field {
                continue;
            }
            let Some(value) = value_of(cx, &cx.field_ty(m)) else {
                continue;
            };
            let node = member.node.clone();
            let named = node.as_ref().and_then(key_name);
            out.push(Key {
                declaration: format!("{}.{}", p.decl_name(d), member.name),
                name: named.as_ref().map(|n| n.0.clone()),
                literal: named.map(|n| n.1),
                value,
                file: decl.file,
                range: member.name_range,
                breaking: node.as_ref().is_some_and(node_has_breaking),
            });
        }
    }
    out
}

/// K2405: a storage key named like an earlier one (in file path order) of
/// the app's non-test code, at its name literal, with a label at the
/// earlier key's. Two such keys read and write the same stored value as
/// different types (`docs/design/36-local-storage.md`, "Keys"); a test may
/// reuse a name to inspect what a service stored, so tests aren't counted.
fn duplicate_keys(cx: &Cx<'_, '_>) {
    let mut first: BTreeMap<String, (crate::program::FileIx, TextRange)> = BTreeMap::new();
    for key in program_keys(cx) {
        let (Some(name), Some(literal)) = (&key.name, key.literal) else {
            continue;
        };
        match first.get(name) {
            Some(&(file, range)) => cx.report(
                key.file,
                Diagnostic::new(
                    codes::DUPLICATE_STORAGE_KEY,
                    cx.p.span(key.file, literal),
                    format!("another storage key is named `{name}`; the two would read and write each other's values"),
                )
                .with_label(cx.p.span(file, range), format!("the key named `{name}` is declared here")),
            ),
            None => {
                first.insert(name.clone(), (key.file, literal));
            }
        }
    }
}

/// K2406: a drag kind named like an earlier one (in file path order) but
/// with another payload type, at its name literal, with a label at the
/// earlier kind's. Kinds with the same name are the same kind
/// (`docs/design/41-drag-and-drop.md`, "Kinds and payloads"), so a drop
/// target of one would accept the other's drags and hand its handler a
/// payload of the wrong type. Redeclaring a kind with the same payload type
/// is allowed. Test code counts too: a test has no reason to give a kind
/// another payload. Payload types that don't resolve aren't compared, and
/// the diagnostics of resolving them are dropped as in [`program_keys`].
fn conflicting_drag_kinds(cx: &Cx<'_, '_>) {
    let reported = cx.diags.borrow().len();
    let kinds = find_named(cx, drag_payload, true);
    cx.diags.borrow_mut().truncate(reported);
    let mut first: BTreeMap<String, (crate::program::FileIx, TextRange, Ty)> = BTreeMap::new();
    for kind in kinds {
        let (Some(name), Some(literal)) = (&kind.name, kind.literal) else {
            continue;
        };
        if kind.value.contains_error() {
            continue;
        }
        match first.get(name) {
            Some((file, range, payload)) if *payload != kind.value => cx.report(
                kind.file,
                Diagnostic::new(
                    codes::CONFLICTING_DRAG_KIND,
                    cx.p.span(kind.file, literal),
                    format!(
                        "another drag kind is named `{name}` with the payload `{}`; kinds with the same name are the same kind, so this one can't carry `{}`",
                        payload.render(cx.p),
                        kind.value.render(cx.p)
                    ),
                )
                .with_label(
                    cx.p.span(*file, *range),
                    format!("the kind named `{name}` carries `{}`", payload.render(cx.p)),
                ),
            ),
            Some(_) => {}
            None => {
                first.insert(name.clone(), (kind.file, literal, kind.value.clone()));
            }
        }
    }
}

/// K2408: a channel whose message type `value_of` finds (`TabChannel<T>`,
/// `DataChannelKind<T>`) named like an earlier one (in file path order)
/// but with another message type, at its name literal, with a label at the
/// earlier one's. Channels with the same name are the same channel
/// (`docs/design/45-web-platform.md`, "Messages between tabs";
/// `docs/design/46-media-and-rtc.md`, "Typed data channels"), so one side
/// would decode the other's messages as the wrong type. Test code counts,
/// as for drag kinds; message types that don't resolve aren't compared.
fn conflicting_channels(cx: &Cx<'_, '_>, value_of: fn(&Cx<'_, '_>, &Ty) -> Option<Ty>, what: &str) {
    let reported = cx.diags.borrow().len();
    let channels = find_named(cx, value_of, true);
    cx.diags.borrow_mut().truncate(reported);
    let mut first: BTreeMap<String, (crate::program::FileIx, TextRange, Ty)> = BTreeMap::new();
    for channel in channels {
        let (Some(name), Some(literal)) = (&channel.name, channel.literal) else {
            continue;
        };
        if channel.value.contains_error() {
            continue;
        }
        match first.get(name) {
            Some((file, range, message)) if *message != channel.value => cx.report(
                channel.file,
                Diagnostic::new(
                    codes::CONFLICTING_CHANNEL,
                    cx.p.span(channel.file, literal),
                    format!(
                        "another {what} is named `{name}` with the message type `{}`; channels with the same name are the same channel, so this one can't carry `{}`",
                        message.render(cx.p),
                        channel.value.render(cx.p)
                    ),
                )
                .with_label(
                    cx.p.span(*file, *range),
                    format!("the {what} named `{name}` carries `{}`", message.render(cx.p)),
                ),
            ),
            Some(_) => {}
            None => {
                first.insert(name.clone(), (channel.file, literal, channel.value.clone()));
            }
        }
    }
}

/// K2409: a data channel kind whose name differs from an earlier one's (in
/// file path order) but whose id, derived from the name with
/// [`data_channel_id`], is the same, at its name literal, with a label at
/// the earlier kind's. Both peers open a channel with the id of its name
/// (`negotiated: true`, `docs/design/46-media-and-rtc.md`, "Typed data
/// channels"), so two such kinds would be one channel carrying both.
fn colliding_data_channels(cx: &Cx<'_, '_>) {
    let reported = cx.diags.borrow().len();
    let kinds = find_named(cx, data_channel_message, true);
    cx.diags.borrow_mut().truncate(reported);
    let mut first: BTreeMap<u16, (String, crate::program::FileIx, TextRange)> = BTreeMap::new();
    for kind in kinds {
        let (Some(name), Some(literal)) = (&kind.name, kind.literal) else {
            continue;
        };
        let id = data_channel_id(name);
        match first.get(&id) {
            Some((other, file, range)) if other != name => cx.report(
                kind.file,
                Diagnostic::new(
                    codes::COLLIDING_DATA_CHANNELS,
                    cx.p.span(kind.file, literal),
                    format!(
                        "the data channel kinds `{other}` and `{name}` would share the channel id {id}, which both peers derive from the name; rename one"
                    ),
                )
                .with_label(cx.p.span(*file, *range), format!("`{other}` has the id {id}")),
            ),
            Some(_) => {}
            None => {
                first.insert(id, (name.clone(), kind.file, literal));
            }
        }
    }
}

/// The id both peers give the data channel of a `DataChannelKind` named
/// `name`: 1 plus the 32-bit FNV-1a hash of its UTF-8 bytes modulo 1023,
/// so `1...1023` (id 0 is the stdlib's own channel). The browser's
/// `runtime/rtc.js` computes the same (`dataChannelId`).
#[must_use]
pub(crate) fn data_channel_id(name: &str) -> u16 {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    u16::try_from(1 + h % 1023).unwrap_or(1)
}

/// The message type of a `TabChannel<T>`.
fn tab_message(cx: &Cx<'_, '_>, ty: &Ty) -> Option<Ty> {
    type_argument(cx, ty, "k.device.TabChannel")
}

/// The message type of a `DataChannelKind<T>`.
fn data_channel_message(cx: &Cx<'_, '_>, ty: &Ty) -> Option<Ty> {
    type_argument(cx, ty, "k.rtc.DataChannelKind")
}

/// The type argument of `ty` when it is the stdlib generic `qualified`.
fn type_argument(cx: &Cx<'_, '_>, ty: &Ty, qualified: &str) -> Option<Ty> {
    match ty {
        Ty::Named(d, args) if cx.p.decl_name(*d) == qualified => {
            Some(args.first().cloned().unwrap_or(Ty::Error))
        }
        _ => None,
    }
}

/// Compares `storage.lock`'s keys with the program's (K8008): a recorded
/// key's value type must still decode what users' browsers hold, and a key
/// whose declaration is recorded under another name was renamed, which
/// orphans their data. `@Breaking("reason")` on the key allows both. New
/// keys and recorded keys the program no longer declares aren't reported.
fn keys(cx: &Cx<'_, '_>, recorded: &StorageLock) {
    for key in program_keys(cx) {
        if key.breaking || key.value.contains_error() {
            continue;
        }
        let by_name = key
            .name
            .as_ref()
            .and_then(|n| recorded.keys.get_key_value(n));
        let entry = by_name.or_else(|| {
            recorded
                .keys
                .iter()
                .find(|(_, (declaration, _))| *declaration == key.declaration)
        });
        let Some((name, (_, written))) = entry else {
            continue;
        };
        let mut changes = Vec::new();
        if by_name.is_none() {
            changes.push(format!(
                "the key was renamed from `{name}` to `{}`; values stored under `{name}` are no longer read",
                key.name.as_deref().unwrap_or("?")
            ));
        }
        let mut cmp = Compare {
            cx,
            lock: recorded,
            seen: HashSet::new(),
            changes: &mut changes,
        };
        cmp.ty(written, &key.value, "the value");
        if changes.is_empty() {
            continue;
        }
        let mut diag = Diagnostic::new(
            codes::INCOMPATIBLE_STORAGE_KEY,
            cx.p.span(key.file, key.range),
            format!(
                "the storage key `{name}` changed incompatibly with storage.lock; values users' browsers already hold would read as `.unreadable`"
            ),
        );
        for change in changes {
            diag = diag.with_note(change);
        }
        diag = diag.with_note(
            "keep the recorded name and shape (only optional fields and new enum cases are compatible), or mark the key `@Breaking(\"reason\")` once the service handles `.unreadable`",
        );
        cx.report(key.file, diag);
    }
}

/// The literal of a job's `@JobName("…")`.
fn job_name(cx: &Cx<'_, '_>, d: DeclId) -> Option<String> {
    let node = &cx.p.decl(d).node;
    let a = node
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|n| n.text() == "JobName"))?;
    let lit = a.arg_list()?.args().next()?.value()?;
    match lit {
        ast::Expr::StringLit(s) => s.plain_value(),
        _ => None,
    }
}

/// `true` when the declaration carries `@name`.
fn has_annotation(cx: &Cx<'_, '_>, d: DeclId, name: &str) -> bool {
    cx.p.decl(d)
        .node
        .children()
        .filter_map(ast::Annotation::cast)
        .any(|a| a.name().is_some_and(|t| t.text() == name))
}

/// The range of the type argument of `implements Job<…>`.
fn payload_range(cx: &Cx<'_, '_>, d: DeclId) -> Option<TextRange> {
    let node = &cx.p.decl(d).node;
    let clause = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ImplementsClause)?;
    clause
        .children()
        .filter(|t| t.kind() == SyntaxKind::PathType)
        .find(|t| {
            t.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .any(|tok| tok.kind() == SyntaxKind::Ident && tok.text() == "Job")
        })?
        .children()
        .find(|c| c.kind() == SyntaxKind::GenericArgList)?
        .children()
        .next()
        .map(|t| t.text_range())
}

/// A structural comparison of recorded types with the program's.
struct Compare<'c, 'p, 'a> {
    /// The checking context, for the program's types.
    cx: &'c Cx<'p, 'a>,
    /// The lockfile whose recorded types are compared.
    lock: &'c StorageLock,
    /// Pairs already compared (recursive types).
    seen: HashSet<(String, DeclId)>,
    /// The incompatible changes found, as notes.
    changes: &'c mut Vec<String>,
}

impl Compare<'_, '_, '_> {
    /// Compares the recorded `written` with the program's `now`, at `path`
    /// (`the payload`, `field \`note\``).
    fn ty(&mut self, written: &Written, now: &Ty, path: &str) {
        match (written, now) {
            (_, Ty::Error) => {}
            (Written::Optional(w), Ty::Optional(n)) => self.ty(w, n, path),
            // A required value that becomes optional still decodes.
            (w @ Written::Named(..), Ty::Optional(n)) => self.ty(w, n, path),
            (Written::Optional(_), _) => self.changes.push(format!(
                "{path} was `{}` and is now required (`{}`): recorded jobs may not have it",
                written.text(),
                self.text(now)
            )),
            (Written::Named(name, args), Ty::Named(d, now_args)) => {
                if self.cx.p.is_stdlib(*d) {
                    let simple = &self.cx.p.decl(*d).name;
                    if simple != name || args.len() != now_args.len() {
                        self.changed(path, written, now);
                        return;
                    }
                    for (i, (w, n)) in args.iter().zip(now_args.iter()).enumerate() {
                        let inner = if args.len() == 1 {
                            format!("the element of {path}")
                        } else {
                            format!("type argument {} of {path}", i + 1)
                        };
                        self.ty(w, n, &inner);
                    }
                    return;
                }
                let Some(shape) = self.lock.types.get(name) else {
                    self.changed(path, written, now);
                    return;
                };
                if !self.seen.insert((name.clone(), *d)) {
                    return;
                }
                self.shape(name, shape, *d, path);
            }
            _ => self.changed(path, written, now),
        }
    }

    /// Records a change of type at `path`.
    fn changed(&mut self, path: &str, written: &Written, now: &Ty) {
        self.changes.push(format!(
            "{path} changed type from `{}` to `{}`",
            written.text(),
            self.text(now)
        ));
    }

    /// Compares a recorded shape with the app type `d`.
    fn shape(&mut self, name: &str, shape: &Shape, d: DeclId, path: &str) {
        let cx = self.cx;
        let kind = cx.p.decl(d).named_kind();
        match (shape, kind) {
            (Shape::Struct(fields), Some(K::Struct)) => {
                let now: Vec<(String, Ty)> = cx
                    .stored_fields(d)
                    .into_iter()
                    .map(|m| (cx.p.member(m).name.clone(), cx.field_ty(m)))
                    .collect();
                self.fields(name, fields, &now);
            }
            (Shape::Enum(cases), Some(K::Enum)) => {
                let now: Vec<(String, crate::program::MemberId)> = cx
                    .cases(d)
                    .into_iter()
                    .filter(|&m| cx.p.member(m).kind == MemberKind::Case)
                    .map(|m| (cx.p.member(m).name.clone(), m))
                    .collect();
                for (case, payload) in cases {
                    let Some((_, m)) = now.iter().find(|(n, _)| n == case) else {
                        self.changes
                            .push(format!("case `{case}` of `{name}` was removed or renamed"));
                        continue;
                    };
                    let fields: Vec<(String, Ty)> = if cx.case_has_payload(*m) {
                        cx.case_sig(*m)
                            .params
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                (
                                    p.label.clone().unwrap_or_else(|| format!("#{i}")),
                                    p.ty.clone(),
                                )
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let written: Vec<WrittenField> = payload
                        .iter()
                        .enumerate()
                        .map(|(i, f)| WrittenField {
                            name: Some(f.name.clone().unwrap_or_else(|| format!("#{i}"))),
                            ty: f.ty.clone(),
                        })
                        .collect();
                    self.fields(&format!("{name}.{case}"), &written, &fields);
                }
            }
            _ => self.changes.push(format!(
                "{path} was recorded as `{name}`, a {}, and is now a {}",
                match shape {
                    Shape::Struct(_) => "struct",
                    Shape::Enum(_) => "enum",
                },
                match kind {
                    Some(K::Enum) => "enum",
                    Some(K::Struct) => "struct",
                    _ => "different kind of type",
                }
            )),
        }
    }

    /// Compares recorded fields with the program's `(name, type)` list.
    fn fields(&mut self, owner: &str, written: &[WrittenField], now: &[(String, Ty)]) {
        for f in written {
            let Some(name) = &f.name else { continue };
            match now.iter().find(|(n, _)| n == name) {
                None => self.changes.push(format!(
                    "field `{name}` of `{owner}` was removed or renamed"
                )),
                Some((_, ty)) => self.ty(&f.ty, ty, &format!("field `{name}` of `{owner}`")),
            }
        }
        for (name, ty) in now {
            if written.iter().any(|f| f.name.as_ref() == Some(name)) {
                continue;
            }
            if !matches!(ty, Ty::Optional(_) | Ty::Error) {
                self.changes.push(format!(
                    "required field `{name}` was added to `{owner}`: recorded jobs don't have it (an optional field would be compatible)"
                ));
            }
        }
    }

    /// The program's type as `storage.lock` would write it.
    fn text(&self, t: &Ty) -> String {
        let p = self.cx.p;
        match t {
            Ty::Optional(inner) => format!("{}?", self.text(inner)),
            Ty::Named(d, args) => {
                let name = if p.is_stdlib(*d) {
                    p.decl(*d).name.clone()
                } else {
                    p.decl_name(*d)
                };
                if args.is_empty() {
                    name
                } else {
                    format!(
                        "{name}<{}>",
                        args.iter()
                            .map(|a| self.text(a))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            other => other.render(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_channel_ids_match_the_browsers() {
        // The same vectors as `rtc.test.js`'s, so the two can't drift.
        assert_eq!(data_channel_id("chat"), 721);
        assert_eq!(data_channel_id("cursor"), 467);
        assert_eq!(data_channel_id("al"), data_channel_id("el"));
        assert_eq!(data_channel_id("al"), 240);
    }

    #[test]
    fn written_types_round_trip() {
        for text in [
            "Uuid",
            "String?",
            "List<cart.SavedItem>",
            "Map<String, Int>",
            "List<Int?>?",
        ] {
            assert_eq!(
                Written::parse(text).map(|w| w.text()).as_deref(),
                Some(text)
            );
        }
        for bad in [
            "",
            "List<",
            "List<Int",
            "<Int>",
            "Int>",
            "a b",
            "List<Int>>",
            "Int!",
        ] {
            assert_eq!(Written::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_lock_with_jobs_and_types_is_read() {
        let lock = read_storage(
            "namespace = \"shop\"\n[jobs.a]\ndeclaration = \"x.A\"\npayload = \"x.P\"\n[types.\"x.P\"]\nfields = [{ name = \"id\", type = \"Uuid\" }]\n[types.\"x.E\"]\ncases = [\"a\", { name = \"b\", fields = [{ type = \"Int\" }] }]\n",
        )
        .unwrap_or_default();
        assert_eq!(lock.jobs.len(), 1);
        assert!(matches!(lock.types.get("x.P"), Some(Shape::Struct(f)) if f.len() == 1));
        assert!(matches!(lock.types.get("x.E"), Some(Shape::Enum(c)) if c.len() == 2));
    }

    #[test]
    fn bad_locks_name_the_offending_line() {
        let text = "namespace = \"x\"\n[jobs.a]\ndeclaration = 3\n";
        let start = read_storage(text)
            .err()
            .map(|invalid| usize::from(line_at(text, invalid.offset).start()));
        assert_eq!(start.and_then(|s| text.get(s..s + 8)), Some("[jobs.a]"));
        assert!(read_storage("[jobs\n").is_err());
        assert!(read_storage("").is_ok());
    }

    #[test]
    fn line_at_never_leaves_the_text() {
        assert_eq!(line_at("", 10), TextRange::empty(TextSize::from(0)));
        let r = line_at("ab\r\ncd", 1);
        assert_eq!((u32::from(r.start()), u32::from(r.end())), (0, 2));
    }
}
