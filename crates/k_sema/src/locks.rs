//! Lockfile checks (sema.md §12.16): the job payloads recorded in
//! `storage.lock` against the program's jobs.
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
//! A `storage.lock` that isn't valid TOML of that structure is **K8002** at
//! the offending line. A job the lock doesn't record yet, and a recorded
//! job that no longer exists, are not reported: when the compiler writes
//! the lockfiles is open (`docs/design/13-tooling.md`, "Lockfiles").
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
fn job_payload(cx: &Cx<'_, '_>, decl: DeclId) -> Option<Ty> {
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

/// What `storage.lock` records about jobs.
#[derive(Debug, Default)]
struct StorageLock {
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
