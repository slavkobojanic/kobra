//! The lockfile checks that need the persistence model and the route
//! table, which live here rather than in `k_sema`
//! (`docs/compiler/sema.md` §12.16, `docs/compiler/db.md` §11,
//! `docs/design/13-tooling.md`, "Lockfiles").
//!
//! [`project_diagnostics`] is what `k check` adds to the semantic
//! diagnostics of an app without errors:
//!
//! * **`schema.lock`** (`docs/design/19-persistence.md`, "The `schema.lock`
//!   file"): a migration that doesn't replay (**K6004**), a destructive one
//!   without the `@Breaking` it needs (**K6005**, **K6006**), migration
//!   files that don't match the lock's `migrations` list (**K8006**), a
//!   lock that the migrations don't produce (**K8005**), and entities that
//!   differ from the lock (**K8004**, with the `k migrate new` or
//!   `k migrate rename` that resolves it). An unreadable lock is
//!   **K8002**.
//! * **`routes.lock`** ([`crate::routelock`]).
//!
//! [`schema_lock`] and [`routes_lock`] are the locks `k build` writes: the
//! schema the migrations produce, with the entity and field names carried
//! over from the recorded lock, and the route table with its redirects.
//! `k check` never writes a lockfile, so CI catches what a developer
//! didn't build.

use std::collections::BTreeSet;

use k_db::lock::{LockedColumn, LockedTable, SchemaLock};
use k_diagnostics::{Diagnostic, FileId, TextRange, TextSize, codes};

use crate::cx::Gen;
use crate::db::{FieldKind, History, Problem};
use crate::{CodegenError, Options};

/// The lockfile diagnostics of the analysed project that `k_sema` doesn't
/// make: `schema.lock` and `routes.lock`. Empty for a library, for a
/// program with errors (its entities and pages aren't well defined), and
/// for a cancelled analysis.
///
/// # Examples
///
/// ```
/// use k_codegen::project_diagnostics;
/// use k_sema::{AnalysisHost, Options, ProjectPath};
///
/// let file = |p: &str, t: &str| (ProjectPath::new(p).unwrap(), t.to_owned());
/// let files = vec![
///     file("k.toml", "[package]\nname = \"demo\"\nk = \"0.1\"\n"),
///     file("App.k", "public app Demo {}\n"),
///     file(
///         "src/notes/Note.k",
///         "import k.crypto.Uuid\n\npublic entity Note {\n    @Id let id: Uuid\n}\n",
///     ),
/// ];
/// let host = AnalysisHost::with_files(Options::new(), files);
/// let found = project_diagnostics(&host.analysis());
/// assert_eq!(found.len(), 1);
/// assert_eq!(found[0].code.to_string(), "K8004", "the entity has no table yet");
/// ```
#[must_use]
pub fn project_diagnostics(analysis: &k_sema::Analysis) -> Vec<Diagnostic> {
    let Ok(Some(view)) = checked_app(analysis) else {
        return Vec::new();
    };
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut out = Vec::new();
    schema(&g, &mut out);
    crate::routelock::check(&g, &mut out);
    out.sort_by_key(|d| (d.span, d.code));
    out
}

/// The `schema.lock` part of [`project_diagnostics`]: what `k dev` and
/// `k run` refuse to serve, since a build whose entities the migrations
/// don't produce fails at its first query. `routes.lock` problems are left
/// out, as the next `k build` records a renamed page's redirect.
#[must_use]
pub fn schema_diagnostics(analysis: &k_sema::Analysis) -> Vec<Diagnostic> {
    let Ok(Some(view)) = checked_app(analysis) else {
        return Vec::new();
    };
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut out = Vec::new();
    schema(&g, &mut out);
    out.sort_by_key(|d| (d.span, d.code));
    out
}

/// The program view of an app without errors, `None` for a library or a
/// program with errors.
///
/// # Errors
///
/// [`CodegenError::Cancelled`] when the analysis was cancelled.
pub(crate) fn checked_app(
    analysis: &k_sema::Analysis,
) -> Result<Option<&k_sema::view::ProgramView>, CodegenError> {
    if !analysis.files().iter().any(|(_, p)| p.as_str() == "App.k") {
        return Ok(None);
    }
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| CodegenError::Cancelled)?;
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Ok(None);
    }
    analysis
        .program()
        .map(Some)
        .map_err(|_| CodegenError::Cancelled)
}

/// The id and text of the project file at `path`.
pub(crate) fn project_file<'a>(g: &'a Gen<'_>, path: &str) -> Option<(FileId, &'a str)> {
    let (file, _) = g
        .analysis
        .files()
        .into_iter()
        .find(|(_, p)| p.as_str() == path)?;
    Some((file, g.analysis.text(file)?))
}

/// The range of a byte range of `text`, clamped to it.
pub(crate) fn range_of(text: &str, span: std::ops::Range<usize>) -> TextRange {
    let clamp = |n: usize| TextSize::from(u32::try_from(n.min(text.len())).unwrap_or(u32::MAX));
    TextRange::new(clamp(span.start), clamp(span.end.max(span.start)))
}

/// The range of the line that starts at byte `start` of `text`, without
/// its line break.
pub(crate) fn line_at(text: &str, start: usize) -> TextRange {
    let end = text[start.min(text.len())..]
        .find('\n')
        .map_or(text.len(), |n| start + n);
    range_of(text, start..end)
}

/// The byte offset of the TOML table header `[<prefix>.<key>]` in `text`,
/// with the key bare or quoted.
pub(crate) fn header_offset(text: &str, prefix: &str, key: &str) -> Option<usize> {
    let bare = format!("[{prefix}.{key}]");
    let quoted = format!("[{prefix}.\"{key}\"]");
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t == bare || t == quoted {
            return Some(offset + (line.len() - line.trim_start().len()));
        }
        offset += line.len();
    }
    None
}

/// Where `schema.lock` records `table` (and its column `column`): that
/// column's line, else the table's header, else the start of the file.
fn lock_range(text: &str, table: &str, column: Option<&str>) -> TextRange {
    let Some(start) = header_offset(text, "tables", table) else {
        return TextRange::empty(0.into());
    };
    if let Some(column) = column {
        let needle = format!("name = \"{column}\"");
        let mut offset = start;
        for (i, line) in text[start..].split_inclusive('\n').enumerate() {
            if i > 0 && line.trim_start().starts_with('[') {
                break;
            }
            if line.contains(&needle) {
                let indent = line.len() - line.trim_start().len();
                return line_at(text, offset + indent);
            }
            offset += line.len();
        }
    }
    line_at(text, start)
}

/// Where `schema.lock`'s `migrations` list names `stem`.
fn lock_migration_range(text: &str, stem: &str) -> TextRange {
    let needle = format!("\"{stem}\"");
    text.find(&needle).map_or_else(
        || TextRange::empty(0.into()),
        |at| range_of(text, at..at + needle.len()),
    )
}

/// The `schema.lock` checks (see the module documentation).
fn schema(g: &Gen<'_>, out: &mut Vec<Diagnostic>) {
    let lock_file = project_file(g, "schema.lock");
    let lock = match lock_file.map(|(_, text)| SchemaLock::parse(text)) {
        Some(Err(e)) => {
            let (file, text) = lock_file.unwrap_or((FileId::new(0), ""));
            let range = e
                .span
                .clone()
                .map_or_else(|| TextRange::empty(0.into()), |s| range_of(text, s));
            out.push(Diagnostic::new(
                codes::INVALID_LOCKFILE,
                k_diagnostics::Span::new(file, range),
                format!("schema.lock can't be read: {}", e.reason),
            ));
            return;
        }
        Some(Ok(lock)) => lock,
        None => SchemaLock::default(),
    };
    let h = crate::db::history(g);
    let has_entities = !crate::db::model(g).entities().is_empty();
    if lock_file.is_none() && h.files.is_empty() && !has_entities {
        return;
    }
    out.extend(h.problems.iter().map(Problem::diagnostic));
    order(&h, lock_file, &lock, out);
    if h.problems.is_empty() {
        replay_matches_lock(&h, lock_file, &lock, out);
    }
    drift(g, lock_file, &lock, out);
}

/// Migration files and the lock's `migrations` list must agree: every
/// listed migration has its file, numbered by its position, and every
/// file is listed.
fn order(
    h: &History,
    lock_file: Option<(FileId, &str)>,
    lock: &SchemaLock,
    out: &mut Vec<Diagnostic>,
) {
    let Some((lock_id, lock_text)) = lock_file else {
        return;
    };
    let span = |f: FileId, r: TextRange| k_diagnostics::Span::new(f, r);
    for (i, stem) in lock.migrations.iter().enumerate() {
        let position = i + 1;
        let Some(f) = h.files.iter().find(|f| &f.stem == stem) else {
            out.push(
                Diagnostic::new(
                    codes::MIGRATION_ORDER,
                    span(lock_id, lock_migration_range(lock_text, stem)),
                    format!(
                        "schema.lock lists the migration `{stem}`, but `migrations/{stem}.k` doesn't exist"
                    ),
                )
                .with_help(
                    "restore the file: a migration is history, and databases that applied it expect it",
                ),
            );
            continue;
        };
        let number = stem.split('_').next().and_then(|n| n.parse::<usize>().ok());
        if number != Some(position) {
            out.push(
                Diagnostic::new(
                    codes::MIGRATION_ORDER,
                    span(f.file, crate::db::migration_name_range(&f.root)),
                    match number {
                        Some(n) => format!(
                            "`migrations/{stem}.k` is number {n}, but it is migration {position} in schema.lock's `migrations` list"
                        ),
                        None => format!(
                            "`migrations/{stem}.k` isn't named `NNNN_description.k`; it is migration {position} in schema.lock's `migrations` list"
                        ),
                    },
                )
                .with_label(
                    span(lock_id, lock_migration_range(lock_text, stem)),
                    "listed here",
                )
                .with_help(format!(
                    "rename the file to `migrations/{position:04}_…`, and its entry in schema.lock; when two branches each added a migration, renumber the one merged last"
                )),
            );
        }
    }
    for f in &h.files {
        if !lock.migrations.contains(&f.stem) {
            out.push(
                Diagnostic::new(
                    codes::MIGRATION_ORDER,
                    span(f.file, crate::db::migration_name_range(&f.root)),
                    format!(
                        "`migrations/{}.k` isn't in schema.lock's `migrations` list",
                        f.stem
                    ),
                )
                .with_help(
                    "`k migrate new` writes a migration and the lock together; run `k build` to record a migration written by hand",
                ),
            );
        }
    }
}

/// The differences of one column between the lock and the replay, one
/// note each.
fn column_differences(locked: &LockedColumn, replayed: &LockedColumn) -> Vec<String> {
    let mut out = Vec::new();
    let mut flag = |what: &str, l: bool, r: bool| {
        if l != r {
            out.push(format!(
                "`{}`: schema.lock says {}{what}, the migrations {}",
                locked.name,
                if l { "" } else { "not " },
                if r { "make it so" } else { "don't" }
            ));
        }
    };
    flag("primary key", locked.primary_key, replayed.primary_key);
    flag("nullable", locked.nullable, replayed.nullable);
    flag("unique", locked.unique, replayed.unique);
    flag("indexed", locked.indexed, replayed.indexed);
    flag("retired", locked.retired, replayed.retired);
    let mut text = |what: &str, l: String, r: String| {
        if l != r {
            out.push(format!(
                "`{}`: schema.lock records the {what} {l}, the migrations {r}",
                locked.name
            ));
        }
    };
    let show = |o: &Option<String>| {
        o.as_ref()
            .map_or_else(|| "none".to_owned(), |v| format!("`{v}`"))
    };
    text(
        "type",
        format!("`{}`", locked.ty),
        format!("`{}`", replayed.ty),
    );
    text(
        "cases",
        format!("{:?}", locked.cases),
        format!("{:?}", replayed.cases),
    );
    text(
        "reference",
        show(&locked.references),
        show(&replayed.references),
    );
    text("default", show(&locked.default), show(&replayed.default));
    out
}

/// What the migrations produce must be what `schema.lock` records,
/// ignoring the entity and field names, which come from the code.
fn replay_matches_lock(
    h: &History,
    lock_file: Option<(FileId, &str)>,
    lock: &SchemaLock,
    out: &mut Vec<Diagnostic>,
) {
    let help = "run `k build`, which records the schema the migrations produce in schema.lock, and review the change; if the lock was right, a migration changed after it was recorded";
    let Some((lock_id, lock_text)) = lock_file else {
        if let Some(first) = h.files.first() {
            out.push(
                Diagnostic::new(
                    codes::SCHEMA_LOCK_MISMATCH,
                    k_diagnostics::Span::new(
                        first.file,
                        crate::db::migration_name_range(&first.root),
                    ),
                    "the project has migrations but no schema.lock".to_owned(),
                )
                .with_help("run `k build` to write schema.lock, and commit it"),
            );
        }
        return;
    };
    let actual = h.replay.lock_tables();
    let names: BTreeSet<&String> = actual.keys().chain(lock.tables.keys()).collect();
    for name in names {
        let at = |column: Option<&str>| {
            k_diagnostics::Span::new(lock_id, lock_range(lock_text, name, column))
        };
        match (actual.get(name), lock.tables.get(name)) {
            (Some(_), None) => out.push(
                Diagnostic::new(
                    codes::SCHEMA_LOCK_MISMATCH,
                    at(None),
                    format!(
                        "the migrations create the table `{name}`, which schema.lock doesn't record"
                    ),
                )
                .with_help(help),
            ),
            (None, Some(_)) => out.push(
                Diagnostic::new(
                    codes::SCHEMA_LOCK_MISMATCH,
                    at(None),
                    format!(
                        "schema.lock records the table `{name}`, which the migrations don't create"
                    ),
                )
                .with_help(help),
            ),
            (Some(a), Some(l)) => {
                let notes = table_differences(l, a);
                if !notes.is_empty() {
                    let column = first_column(l, a);
                    let mut d = Diagnostic::new(
                        codes::SCHEMA_LOCK_MISMATCH,
                        at(column.as_deref()),
                        format!(
                            "schema.lock doesn't record the table `{name}` as the migrations leave it"
                        ),
                    );
                    for n in notes {
                        d = d.with_note(n);
                    }
                    out.push(d.with_help(help));
                }
            }
            (None, None) => {}
        }
    }
}

/// The column the first difference of a table is about, to point at.
fn first_column(locked: &LockedTable, replayed: &LockedTable) -> Option<String> {
    for c in &locked.columns {
        match replayed.columns.iter().find(|r| r.name == c.name) {
            None => return Some(c.name.clone()),
            Some(r) if !column_differences(c, r).is_empty() => return Some(c.name.clone()),
            Some(_) => {}
        }
    }
    None
}

/// The differences of one table between the lock and the replay.
fn table_differences(locked: &LockedTable, replayed: &LockedTable) -> Vec<String> {
    let mut out = Vec::new();
    if locked.retired != replayed.retired {
        out.push(format!(
            "schema.lock says the table is {}retired, the migrations {}",
            if locked.retired { "" } else { "not " },
            if replayed.retired {
                "retire it"
            } else {
                "don't"
            }
        ));
    }
    for c in &replayed.columns {
        match locked.columns.iter().find(|l| l.name == c.name) {
            None => out.push(format!(
                "the migrations create the column `{}`, which schema.lock doesn't record",
                c.name
            )),
            Some(l) => out.extend(column_differences(l, c)),
        }
    }
    for l in &locked.columns {
        if !replayed.columns.iter().any(|c| c.name == l.name) {
            out.push(format!(
                "schema.lock records the column `{}`, which the migrations don't create",
                l.name
            ));
        }
    }
    out
}

/// The entities must be what `schema.lock` records (K8004), one
/// diagnostic per difference, at the entity or field when the program has
/// it and at the lock's table otherwise.
fn drift(
    g: &Gen<'_>,
    lock_file: Option<(FileId, &str)>,
    lock: &SchemaLock,
    out: &mut Vec<Diagnostic>,
) {
    let mut copy = lock.clone();
    let (_, differences) = crate::migration::diff(g, &mut copy);
    for d in differences {
        let span = match (d.decl, &d.field) {
            (Some(decl), Some(field)) => {
                let info = g.view.decl(decl);
                let range = g
                    .find_member(decl, field)
                    .map_or(info.name_range, |m| g.view.member(m).name_range);
                k_diagnostics::Span::new(info.file, range)
            }
            (Some(decl), None) => {
                let info = g.view.decl(decl);
                k_diagnostics::Span::new(info.file, info.name_range)
            }
            (None, _) => match lock_file {
                Some((file, text)) => k_diagnostics::Span::new(
                    file,
                    lock_range(text, d.table.as_deref().unwrap_or_default(), None),
                ),
                None => continue,
            },
        };
        let mut diagnostic = Diagnostic::new(codes::SCHEMA_DRIFT, span, &d.message);
        if let Some(why) = &d.refusal {
            diagnostic = diagnostic.with_note(format!("`k migrate new` doesn't write it: {why}"));
        }
        let mut help = if d.writable {
            "run `k migrate new \"description\"`: it writes the migration and records it in schema.lock".to_owned()
        } else {
            "write the migration by hand (`k migrate new --empty \"description\"`), then `k build` records it in schema.lock".to_owned()
        };
        if let Some(r) = &d.rename {
            help = format!("if it was renamed, keep its storage with `{r}`; otherwise {help}");
        }
        out.push(diagnostic.with_help(help));
    }
}

/// The `schema.lock` that `k build` writes: the schema the migrations
/// produce, with the migrations in their order and each table's entity and
/// each column's field carried over from the recorded lock (through the
/// physical renames the migrations made) or, for a table or column the
/// lock doesn't record yet, taken from the entity and field stored there.
///
/// `None` when nothing changes: for a library, a program with errors, a
/// migration that doesn't replay, an unreadable lock or one listing a
/// migration whose file is gone (`k check` reports both, and they aren't
/// overwritten), a project without migrations or a lock,
/// or a lock that already records exactly this. Comments of a lock that
/// doesn't change are kept, since it isn't rewritten.
///
/// # Errors
///
/// [`CodegenError::Cancelled`] when the analysis was cancelled.
pub fn schema_lock(analysis: &k_sema::Analysis) -> Result<Option<String>, CodegenError> {
    let Some(view) = checked_app(analysis)? else {
        return Ok(None);
    };
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let lock_file = project_file(&g, "schema.lock");
    let old = match lock_file.map(|(_, t)| SchemaLock::parse(t)) {
        Some(Err(_)) => return Ok(None),
        Some(Ok(l)) => l,
        None => SchemaLock::default(),
    };
    let h = crate::db::history(&g);
    if !h.problems.is_empty() || (lock_file.is_none() && h.files.is_empty()) {
        return Ok(None);
    }
    // A listed migration whose file is gone is history `k check` reports
    // (K8006); rewriting the list would erase it.
    if old
        .migrations
        .iter()
        .any(|m| !h.files.iter().any(|f| &f.stem == m))
    {
        return Ok(None);
    }
    let model = crate::db::model(&g);
    let mut new = SchemaLock {
        migrations: h.files.iter().map(|f| f.stem.clone()).collect(),
        tables: std::collections::BTreeMap::new(),
    };
    for (name, mut table) in h.replay.lock_tables() {
        let origin = old
            .tables
            .get(&name)
            .map(|t| (name.clone(), t))
            .or_else(|| {
                old.tables
                    .iter()
                    .find(|(o, _)| {
                        !h.replay.tables.contains_key(*o)
                            && h.replay.current_table(o).as_deref() == Some(name.as_str())
                    })
                    .map(|(o, t)| (o.clone(), t))
            });
        let entity = model.entities().into_iter().find(|e| e.table == name);
        table.entity = origin
            .as_ref()
            .map(|(_, t)| t.entity.clone())
            .filter(|e| !e.is_empty())
            .or_else(|| entity.map(|e| g.view.qualified_name(e.decl)))
            .unwrap_or_default();
        for c in &mut table.columns {
            if c.retired {
                continue;
            }
            let carried = origin.as_ref().and_then(|(o, t)| {
                t.columns
                    .iter()
                    .filter(|l| !l.retired)
                    .find(|l| {
                        h.replay
                            .current_column(o, &l.name)
                            .is_some_and(|(nt, nc)| nt == name && nc == c.name)
                    })
                    .and_then(|l| l.field.clone())
            });
            c.field = carried.or_else(|| {
                entity.and_then(|e| {
                    e.fields
                        .iter()
                        .find(|f| match &f.kind {
                            FieldKind::Column { column, .. } | FieldKind::ToOne { column, .. } => {
                                *column == c.name
                            }
                            FieldKind::ToMany { .. } => false,
                        })
                        .map(|f| f.name.clone())
                })
            });
        }
        new.tables.insert(name, table);
    }
    if lock_file.is_some() && new == old {
        return Ok(None);
    }
    Ok(Some(new.to_toml()))
}

/// The `routes.lock` that `k build` writes (`docs/design/12-routing.md`): every
/// page's and endpoint's URL, the redirects recorded so far, and an
/// automatic redirect from the old URL of each page renamed since. `None`
/// when nothing changes, for a library and for a program with errors.
///
/// # Errors
///
/// [`CodegenError::Cancelled`] when the analysis was cancelled.
pub fn routes_lock(analysis: &k_sema::Analysis) -> Result<Option<String>, CodegenError> {
    let Some(view) = checked_app(analysis)? else {
        return Ok(None);
    };
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    Ok(crate::routelock::implied(&g))
}
