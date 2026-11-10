//! `k migrate new` and `k migrate rename`: the next migration from the
//! difference between the entities and `schema.lock`, and the lock
//! changes that need no migration (`docs/design/19-persistence.md`,
//! "Migrations" and "Renames"; `docs/compiler/db.md`, "Migrations").
//!
//! [`plan_migration_with`] compares the persistence model of the checked
//! program (the same [`crate::db`] model the repositories are lowered
//! from, so table and column names follow the lock) with the recorded
//! lock, and either reports that they match or drafts
//!
//! * the migration file `migrations/NNNN_<description>.k`: one
//!   `createTable` per new entity (referenced tables first), one
//!   `alterTable` per changed table with `addColumn` for each new field
//!   (its default as `defaultValue`, or `nullable(true)` for an optional
//!   field) and `retireColumn` for each removed one, and `createIndex` for
//!   a new `@Index`;
//! * the updated lock, in [`k_db::lock::SchemaLock::to_toml`]'s canonical
//!   layout, with the migration appended to `migrations`, new tables and
//!   columns recorded, and the columns of removed fields marked `retired`
//!   (their names stay reserved; expand, then contract).
//!
//! It only writes what is safe without reviewing data. A change it can't
//! write is refused with an explanation, and nothing is written: a column
//! type or option that changed, an enum case added or removed, a required
//! field added to an existing table without a default, and a removed
//! entity. Those are written by hand in a `--empty` migration. Renames are
//! never inferred: a renamed field reads as a removal and an addition,
//! which the explanation names together with the `k migrate rename` that
//! keeps the column.
//!
//! The other requests ([`MigrationRequest`]) write the contract half of a
//! change: `--drop-retired` drops every table and column the lock marks
//! retired, and `--rename` renames a table or column physically so it
//! follows its entity's or field's current name. Both are destructive, so
//! the migration is written with the `@Breaking` it needs.
//! [`record_rename`] is `k migrate rename`: a renamed entity or field keeps
//! its table or column, so only the lock changes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use k_db::lock::{LockedColumn, LockedTable, SchemaLock};
use k_db::schema::ColumnType;
use k_sema::view::DeclId;
use k_syntax::ast::{self, AstNode};

use crate::cx::Gen;
use crate::db::{EntityModel, FieldKind, FieldModel, snake};
use crate::{CodegenError, Options};

/// What [`plan_migration_with`] decided.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MigrationPlan {
    /// The entities match `schema.lock` (or, for `--drop-retired`, the
    /// lock records nothing retired): there is nothing to write.
    UpToDate,
    /// The files to write.
    Draft(MigrationDraft),
}

/// A drafted migration and the lock that records it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MigrationDraft {
    /// The project path of the new file, `migrations/0002_add_note_pinning.k`.
    pub file: String,
    /// The new file's text.
    pub source: String,
    /// The new text of `schema.lock`.
    pub lock: String,
}

/// Why no migration was drafted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MigrationError {
    /// The program can't be analysed: errors or cancellation.
    Codegen(CodegenError),
    /// The description has no word to name the migration after.
    BadDescription,
    /// The difference contains changes `k migrate new` doesn't write, one
    /// explanation each.
    Refused(Vec<String>),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::Codegen(e) => e.fmt(f),
            MigrationError::BadDescription => {
                write!(f, "the description needs at least one letter or digit")
            }
            MigrationError::Refused(problems) => write!(f, "{}", problems.join("\n")),
        }
    }
}

impl std::error::Error for MigrationError {}

/// What `k migrate new` is asked to write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MigrationRequest<'a> {
    /// The difference between the entities and `schema.lock`.
    Diff,
    /// An empty migration, for data changes written by hand (`--empty`).
    Empty,
    /// Drop every table and column `schema.lock` marks retired
    /// (`--drop-retired`).
    DropRetired,
    /// Rename the tables and columns of these entities and fields
    /// (`notes.Note`, `notes.Note.heading`) to follow their current names
    /// (`--rename`).
    Rename(&'a [String]),
}

/// Drafts the next migration of the analysed project, described by
/// `description` (`"add note pinning"`). With `empty`, the migration has
/// no schema changes (a data migration written by hand) and the entities
/// are not compared. The same as [`plan_migration_with`] with
/// [`MigrationRequest::Diff`] or [`MigrationRequest::Empty`].
///
/// # Errors
///
/// As [`plan_migration_with`].
///
/// # Examples
///
/// ```
/// use k_codegen::{MigrationPlan, plan_migration};
/// use k_sema::{AnalysisHost, Options, ProjectPath};
///
/// let files = vec![(
///     ProjectPath::new("k.toml").unwrap(),
///     "[package]\nname = \"demo\"\nk = \"0.1\"\n".to_owned(),
/// )];
/// let host = AnalysisHost::with_files(Options::new(), files);
/// let plan = plan_migration(&host.analysis(), "Backfill notes!", true).expect("drafts");
/// let MigrationPlan::Draft(draft) = plan else { panic!("an empty migration is always drafted") };
/// assert_eq!(draft.file, "migrations/0001_backfill_notes.k");
/// assert!(draft.lock.contains("0001_backfill_notes"));
/// ```
pub fn plan_migration(
    analysis: &k_sema::Analysis,
    description: &str,
    empty: bool,
) -> Result<MigrationPlan, MigrationError> {
    let request = if empty {
        MigrationRequest::Empty
    } else {
        MigrationRequest::Diff
    };
    plan_migration_with(analysis, description, request)
}

/// The words of a description, lower-cased: runs of letters and digits.
fn words_of(description: &str) -> Vec<String> {
    description
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Checks that the analysis has no errors and returns it as a [`Gen`]
/// input.
///
/// # Errors
///
/// [`MigrationError::Codegen`] for errors or a cancelled analysis.
fn checked(analysis: &k_sema::Analysis) -> Result<&k_sema::view::ProgramView, MigrationError> {
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| MigrationError::Codegen(CodegenError::Cancelled))?;
    let count = diagnostics.iter().filter(|d| d.is_error()).count();
    if count > 0 {
        return Err(MigrationError::Codegen(CodegenError::HasErrors { count }));
    }
    analysis
        .program()
        .map_err(|_| MigrationError::Codegen(CodegenError::Cancelled))
}

/// Drafts the next migration of the analysed project for `request`,
/// described by `description` (`"add note pinning"`).
///
/// The file is named after the description's words (runs of letters and
/// digits, lower-cased, joined with `_`), numbered one past the larger of
/// the lock's migration count and the highest `migrations/NNNN_*.k` file
/// number. Nothing is written to disk: the caller (`k migrate new`)
/// writes [`MigrationDraft::source`] and [`MigrationDraft::lock`].
///
/// # Errors
///
/// [`MigrationError::Codegen`] for a program with errors or a cancelled
/// analysis, [`MigrationError::BadDescription`] for a description without
/// a word, and [`MigrationError::Refused`] for differences it doesn't
/// write, or a `--rename` it can't make (an unknown entity or field, one
/// the lock doesn't record, or one already stored under its name).
pub fn plan_migration_with(
    analysis: &k_sema::Analysis,
    description: &str,
    request: MigrationRequest<'_>,
) -> Result<MigrationPlan, MigrationError> {
    let view = checked(analysis)?;
    let words = words_of(description);
    if words.is_empty() {
        return Err(MigrationError::BadDescription);
    }
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut lock = crate::db::lock(&g).unwrap_or_default();
    let (changes, breaking) = match request {
        MigrationRequest::Empty => (Changes::default(), None),
        MigrationRequest::Diff => {
            let (changes, differences) = diff(&g, &mut lock);
            let refused: Vec<String> = differences
                .iter()
                .filter(|d| !d.writable)
                .map(|d| d.explanation())
                .collect();
            if !refused.is_empty() {
                return Err(MigrationError::Refused(refused));
            }
            if changes.is_empty() {
                return Ok(MigrationPlan::UpToDate);
            }
            (changes, None)
        }
        MigrationRequest::DropRetired => {
            let changes = drop_retired(&mut lock)?;
            if changes.is_empty() {
                return Ok(MigrationPlan::UpToDate);
            }
            (changes, Some(DROP_REASON))
        }
        MigrationRequest::Rename(names) => {
            let changes = renames(&g, &mut lock, names)?;
            (changes, Some(crate::db::SINGLE_INSTANCE))
        }
    };
    let number = next_number(&g, &lock);
    let stem = format!("{number:04}_{}", words.join("_"));
    lock.migrations.push(stem.clone());
    let source = render(&words, description, request, breaking, &changes);
    Ok(MigrationPlan::Draft(MigrationDraft {
        file: format!("migrations/{stem}.k"),
        source,
        lock: lock.to_toml(),
    }))
}

/// The `@Breaking` reason of a `--drop-retired` migration.
const DROP_REASON: &str = "drops what earlier migrations retired";

/// The next migration number: one more than the lock's migrations and
/// every `migrations/NNNN_*.k` file.
fn next_number(g: &Gen<'_>, lock: &SchemaLock) -> usize {
    let files = g
        .analysis
        .files()
        .into_iter()
        .filter_map(|(_, p)| {
            let stem = p.as_str().strip_prefix("migrations/")?;
            stem.split('_').next()?.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0);
    files.max(lock.migrations.len()) + 1
}

/// The schema changes of one migration.
#[derive(Default)]
pub(crate) struct Changes {
    /// New tables, in creation order.
    creates: Vec<(String, Vec<LockedColumn>)>,
    /// New columns of existing tables, by table, with their `ColumnValue`
    /// default as K source.
    adds: BTreeMap<String, Vec<(LockedColumn, Option<String>)>>,
    /// New indexes of existing tables: (table, column).
    indexes: Vec<(String, String)>,
    /// Retired columns: (table, column, the removed field's qualified name).
    retires: Vec<(String, String, String)>,
    /// Retired columns dropped: (table, column).
    drops: Vec<(String, String)>,
    /// Retired tables dropped, referencing tables first.
    drop_tables: Vec<String>,
    /// Physical column renames: (table, from, to).
    column_renames: Vec<(String, String, String)>,
    /// Physical table renames: (from, to).
    table_renames: Vec<(String, String)>,
}

impl Changes {
    /// `true` when there is nothing to write.
    fn is_empty(&self) -> bool {
        self.creates.is_empty()
            && self.adds.is_empty()
            && self.indexes.is_empty()
            && self.retires.is_empty()
            && self.drops.is_empty()
            && self.drop_tables.is_empty()
            && self.column_renames.is_empty()
            && self.table_renames.is_empty()
    }
}

/// One difference between the entities and `schema.lock`, for `k migrate
/// new` (which writes it or refuses it) and `k check` (which reports it).
#[derive(Clone, Debug)]
pub(crate) struct Difference {
    /// The entity it concerns, when the program still has it.
    pub(crate) decl: Option<DeclId>,
    /// The field it concerns, when the entity still has it.
    pub(crate) field: Option<String>,
    /// The locked table it concerns, for a difference without a
    /// declaration (a removed entity).
    pub(crate) table: Option<String>,
    /// What differs.
    pub(crate) message: String,
    /// Why `k migrate new` doesn't write it; `None` when it does.
    pub(crate) refusal: Option<String>,
    /// `true` when `k migrate new` writes it.
    pub(crate) writable: bool,
    /// The `k migrate rename …` that records it as a name-preserving
    /// rename, when the difference looks like one.
    pub(crate) rename: Option<String>,
}

impl Difference {
    /// A difference `k migrate new` writes.
    fn writable(decl: Option<DeclId>, field: Option<&str>, message: String) -> Difference {
        Difference {
            decl,
            field: field.map(str::to_owned),
            table: None,
            message,
            refusal: None,
            writable: true,
            rename: None,
        }
    }

    /// A difference `k migrate new` refuses, with why.
    fn refused(
        decl: Option<DeclId>,
        field: Option<&str>,
        message: String,
        why: String,
    ) -> Difference {
        Difference {
            refusal: Some(why),
            writable: false,
            ..Difference::writable(decl, field, message)
        }
    }

    /// The difference and why it isn't written, as `k migrate new` prints
    /// a refusal.
    pub(crate) fn explanation(&self) -> String {
        let mut out = match &self.refusal {
            Some(why) => format!("{}; {why}", self.message),
            None => self.message.clone(),
        };
        if let Some(r) = &self.rename {
            let _ = write!(out, " (if it was renamed, keep its storage: `{r}`)");
        }
        out
    }
}

/// Compares the entities with `lock`, updating `lock` to match the
/// changes `k migrate new` writes, and returns those changes with every
/// difference found (writable or not).
#[allow(clippy::too_many_lines)]
pub(crate) fn diff(g: &Gen<'_>, lock: &mut SchemaLock) -> (Changes, Vec<Difference>) {
    let model = crate::db::model(g);
    let mut differences = Vec::new();
    let mut changes = Changes::default();
    let mut live: BTreeSet<String> = BTreeSet::new();
    let mut created: Vec<(String, Vec<LockedColumn>)> = Vec::new();
    let mut new_entities: Vec<DeclId> = Vec::new();
    for e in model.entities() {
        let qualified = g.view.qualified_name(e.decl);
        live.insert(qualified.clone());
        let desired = columns(g, e);
        let recorded = lock
            .tables
            .iter()
            .find(|(_, t)| t.entity == qualified && !t.retired)
            .map(|(n, _)| n.clone());
        let Some(table) = recorded else {
            if let Some(other) = lock.tables.get(&e.table) {
                differences.push(Difference::refused(
                    Some(e.decl),
                    None,
                    format!(
                        "`{qualified}` would be stored in table `{}`, which schema.lock records for `{}`",
                        e.table, other.entity
                    ),
                    "rename the entity, or keep the old one's table with `k migrate rename`".to_owned(),
                ));
                continue;
            }
            differences.push(Difference::writable(
                Some(e.decl),
                None,
                format!("`{qualified}` has no table in schema.lock"),
            ));
            new_entities.push(e.decl);
            created.push((
                e.table.clone(),
                desired.iter().map(|(c, _)| c.clone()).collect(),
            ));
            continue;
        };
        let Some(entry) = lock.tables.get_mut(&table) else {
            continue;
        };
        let mut added: Vec<(String, String)> = Vec::new();
        for (want, default) in &desired {
            let field = want.field.clone().unwrap_or_default();
            let path = format!("{qualified}.{field}");
            let existing = entry
                .columns
                .iter_mut()
                .find(|c| c.field == want.field && !c.retired);
            match existing {
                None => {
                    if !want.nullable && default.is_none() {
                        differences.push(Difference::refused(
                            Some(e.decl),
                            Some(&field),
                            format!("`{path}` is added to the existing table `{table}`, so it needs a default value or an optional type"),
                            "existing rows need a value, and so do the previous release's inserts".to_owned(),
                        ));
                        added.push((field.clone(), want.ty.clone()));
                        continue;
                    }
                    if entry.columns.iter().any(|c| c.name == want.name) {
                        differences.push(Difference::refused(
                            Some(e.decl),
                            Some(&field),
                            format!(
                                "`{path}` would be stored in column `{table}.{}`, which schema.lock already records",
                                want.name
                            ),
                            "retired columns keep their names until they are dropped".to_owned(),
                        ));
                        continue;
                    }
                    differences.push(Difference::writable(
                        Some(e.decl),
                        Some(&field),
                        format!("`{path}` has no column in schema.lock"),
                    ));
                    added.push((field.clone(), want.ty.clone()));
                    let mut column = want.clone();
                    column.default = default.as_ref().map(|(_, text)| text.clone());
                    entry.columns.push(column.clone());
                    changes
                        .adds
                        .entry(table.clone())
                        .or_default()
                        .push((column, default.as_ref().map(|(k, _)| k.clone())));
                }
                Some(have) => {
                    if have.ty != want.ty {
                        differences.push(Difference::refused(
                            Some(e.decl),
                            Some(&field),
                            format!("`{path}` changed from `{}` to `{}`", have.ty, want.ty),
                            "write a migration for it by hand (`k migrate new --empty`), with `@Breaking(\"reason\")` if it narrows the column".to_owned(),
                        ));
                        continue;
                    }
                    if have.cases != want.cases {
                        for case in want.cases.iter().filter(|c| !have.cases.contains(c)) {
                            differences.push(Difference::refused(
                                Some(e.decl),
                                Some(&field),
                                format!("`{path}` gained the case `{case}`"),
                                format!("the previous release can't read rows that hold it, so write it by hand after review: `table.addEnumCase(column: \"{}\", case: \"{case}\")` in a `k migrate new --empty` migration", have.name),
                            ));
                        }
                        for case in have.cases.iter().filter(|c| !want.cases.contains(c)) {
                            differences.push(Difference::refused(
                                Some(e.decl),
                                Some(&field),
                                format!("`{path}` lost the case `{case}`"),
                                format!("removing a case rewrites the rows that hold it, so write it by hand: `table.removeEnumCase(column: \"{}\", case: \"{case}\", replacement: …)` (or `renameEnumCase`) in a `k migrate new --empty` migration marked `@Breaking(\"reason\")`", have.name),
                            ));
                        }
                        continue;
                    }
                    if (
                        have.nullable,
                        have.unique,
                        have.primary_key,
                        &have.references,
                    ) != (
                        want.nullable,
                        want.unique,
                        want.primary_key,
                        &want.references,
                    ) {
                        differences.push(Difference::refused(
                            Some(e.decl),
                            Some(&field),
                            format!("the column options of `{path}` changed (optional, `@Unique`, `@Id` or relation)"),
                            "write a migration for it by hand (`k migrate new --empty`)".to_owned(),
                        ));
                        continue;
                    }
                    if want.indexed && !have.indexed {
                        differences.push(Difference::writable(
                            Some(e.decl),
                            Some(&field),
                            format!("`{path}` is `@Index`, which schema.lock doesn't record"),
                        ));
                        have.indexed = true;
                        changes.indexes.push((table.clone(), have.name.clone()));
                    }
                }
            }
        }
        let fields: BTreeSet<Option<String>> =
            desired.iter().map(|(c, _)| c.field.clone()).collect();
        let mut removed: Vec<(String, String)> = Vec::new();
        for c in &mut entry.columns {
            if c.retired || fields.contains(&c.field) {
                continue;
            }
            let message = match &c.field {
                Some(field) => {
                    removed.push((field.clone(), c.ty.clone()));
                    format!(
                        "`{qualified}.{field}` was removed, but schema.lock still stores it in `{table}.{}`",
                        c.name
                    )
                }
                None => format!(
                    "schema.lock records the column `{table}.{}`, which no field of `{qualified}` is stored in",
                    c.name
                ),
            };
            differences.push(Difference::writable(Some(e.decl), None, message));
            let field = c.field.clone().unwrap_or_default();
            changes.retires.push((
                table.clone(),
                c.name.clone(),
                format!("{qualified}.{field}"),
            ));
            c.retired = true;
            c.field = None;
            if c.default.is_none() {
                // `retireColumn` relaxes it, so the next release's inserts
                // can leave it out.
                c.nullable = true;
            }
        }
        // One field removed and one added with the same type reads as a
        // rename, which `k migrate rename` records without a migration.
        if let ([(old, old_ty)], [(new, new_ty)]) = (removed.as_slice(), added.as_slice())
            && old_ty == new_ty
        {
            let hint = format!("k migrate rename {qualified}.{old} {new}");
            for d in differences.iter_mut().filter(|d| {
                d.decl == Some(e.decl)
                    && (d.field.as_deref() == Some(new.as_str())
                        || (d.field.is_none() && d.message.contains(&format!(".{old}`"))))
            }) {
                d.rename = Some(hint.clone());
            }
        }
    }
    let gone: Vec<(String, String)> = lock
        .tables
        .iter()
        .filter(|(_, t)| !t.retired && !live.contains(&t.entity))
        .map(|(n, t)| (n.clone(), t.entity.clone()))
        .collect();
    let entity_hint = match (gone.as_slice(), new_entities.as_slice()) {
        ([(_, old)], [new]) => Some(format!(
            "k migrate rename {old} {}",
            g.view.qualified_name(*new)
        )),
        _ => None,
    };
    for (name, entity) in &gone {
        differences.push(Difference {
            table: Some(name.clone()),
            rename: entity_hint.clone(),
            ..Difference::refused(
                None,
                None,
                format!("entity `{entity}` (table `{name}`) was removed"),
                format!("`k migrate new` doesn't retire a table: retire it by hand with `schema.retireTable(name: \"{name}\")` in a `k migrate new --empty` migration"),
            )
        });
    }
    if let Some(hint) = &entity_hint {
        for d in differences.iter_mut().filter(|d| {
            d.decl.is_some() && d.decl == new_entities.first().copied() && d.field.is_none()
        }) {
            d.rename = Some(hint.clone());
        }
    }
    // Referenced tables first, then by name.
    created.sort_by(|a, b| a.0.cmp(&b.0));
    let mut ordered: Vec<(String, Vec<LockedColumn>)> = Vec::new();
    let mut placed: BTreeSet<String> = lock.tables.keys().cloned().collect();
    while !created.is_empty() {
        let ready = created
            .iter()
            .position(|(name, cols)| {
                cols.iter().all(|c| {
                    c.references
                        .as_ref()
                        .and_then(|r| r.split('.').next())
                        .is_none_or(|t| t == name || placed.contains(t))
                })
            })
            .unwrap_or(0);
        let next = created.remove(ready);
        placed.insert(next.0.clone());
        ordered.push(next);
    }
    for (name, cols) in &ordered {
        let entity = model
            .entities()
            .into_iter()
            .find(|e| &e.table == name)
            .map(|e| g.view.qualified_name(e.decl))
            .unwrap_or_default();
        lock.tables.insert(
            name.clone(),
            LockedTable {
                entity,
                retired: false,
                columns: cols.clone(),
            },
        );
    }
    changes.creates = ordered;
    (changes, differences)
}

/// `--drop-retired`: every retired column of a live table and every
/// retired table, removed from `lock`.
///
/// # Errors
///
/// [`MigrationError::Refused`] when a retired table is still referenced by
/// a column that isn't being dropped.
fn drop_retired(lock: &mut SchemaLock) -> Result<Changes, MigrationError> {
    let mut changes = Changes::default();
    let tables: Vec<String> = lock
        .tables
        .iter()
        .filter(|(_, t)| t.retired)
        .map(|(n, _)| n.clone())
        .collect();
    for (name, t) in &lock.tables {
        if t.retired {
            continue;
        }
        for c in t.columns.iter().filter(|c| c.retired) {
            changes.drops.push((name.clone(), c.name.clone()));
        }
    }
    let mut problems = Vec::new();
    for dropped in &tables {
        for (name, t) in &lock.tables {
            if tables.contains(name) {
                continue;
            }
            for c in t.columns.iter().filter(|c| !c.retired) {
                if c.references.as_deref().and_then(|r| r.split('.').next()) == Some(dropped) {
                    problems.push(format!(
                        "the retired table `{dropped}` is still referenced by the live column `{name}.{}`; retire that column first",
                        c.name
                    ));
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(MigrationError::Refused(problems));
    }
    // A table that references another retired table is dropped first.
    let mut remaining = tables;
    while !remaining.is_empty() {
        let next = remaining
            .iter()
            .position(|t| {
                !remaining.iter().any(|other| {
                    other != t
                        && lock.tables.get(other).is_some_and(|o| {
                            o.columns.iter().any(|c| {
                                c.references.as_deref().and_then(|r| r.split('.').next())
                                    == Some(t.as_str())
                            })
                        })
                })
            })
            .unwrap_or(0);
        changes.drop_tables.push(remaining.remove(next));
    }
    for t in &changes.drop_tables {
        lock.tables.remove(t);
    }
    for (table, column) in &changes.drops {
        if let Some(t) = lock.tables.get_mut(table) {
            t.columns.retain(|c| &c.name != column);
        }
    }
    Ok(changes)
}

/// The column name a field would get without a lock: `is_pinned`, or
/// `note_id` for a to-one relation.
fn derived_column(f: &FieldModel) -> String {
    match f.kind {
        FieldKind::ToOne { .. } => format!("{}_id", snake(&f.name)),
        _ => snake(&f.name),
    }
}

/// `--rename`: the physical renames that make each named entity's table and
/// each named field's column follow its current name, applied to `lock`.
///
/// # Errors
///
/// [`MigrationError::Refused`] naming each name that isn't a recorded
/// entity or field of one, or is already stored under its own name, or
/// whose new name is taken.
fn renames(
    g: &Gen<'_>,
    lock: &mut SchemaLock,
    names: &[String],
) -> Result<Changes, MigrationError> {
    let model = crate::db::model(g);
    let mut changes = Changes::default();
    let mut problems = Vec::new();
    for name in names {
        let entity = model
            .entities()
            .into_iter()
            .find(|e| g.view.qualified_name(e.decl) == *name);
        if let Some(e) = entity {
            let Some(old) = lock
                .tables
                .iter()
                .find(|(_, t)| t.entity == *name && !t.retired)
                .map(|(n, _)| n.clone())
            else {
                problems.push(format!(
                    "`{name}` has no table in schema.lock yet; `k migrate new` creates it under its current name"
                ));
                continue;
            };
            let new = snake(&g.view.decl(e.decl).name);
            if new == old {
                problems.push(format!("`{name}` is already stored in table `{old}`"));
                continue;
            }
            if lock.tables.contains_key(&new) {
                problems.push(format!(
                    "`{name}` can't move to table `{new}`, which schema.lock already records"
                ));
                continue;
            }
            if let Some(t) = lock.tables.remove(&old) {
                lock.tables.insert(new.clone(), t);
            }
            for t in lock.tables.values_mut() {
                for c in &mut t.columns {
                    if c.references.as_deref() == Some(format!("{old}.id").as_str()) {
                        c.references = Some(format!("{new}.id"));
                    }
                }
            }
            changes.table_renames.push((old, new));
            continue;
        }
        let field = name.rsplit_once('.').and_then(|(owner, field)| {
            let e = model
                .entities()
                .into_iter()
                .find(|e| g.view.qualified_name(e.decl) == owner)?;
            let f = e.fields.iter().find(|f| f.name == field)?;
            Some((owner.to_owned(), f.clone()))
        });
        let Some((owner, f)) = field else {
            problems.push(format!(
                "`{name}` is neither an entity nor a stored field of one (write `module.Entity` or `module.Entity.field`)"
            ));
            continue;
        };
        if matches!(f.kind, FieldKind::ToMany { .. }) {
            problems.push(format!(
                "`{name}` is a to-many relation, which has no column"
            ));
            continue;
        }
        let Some((table, entry)) = lock
            .tables
            .iter_mut()
            .find(|(_, t)| t.entity == owner && !t.retired)
        else {
            problems.push(format!("`{owner}` has no table in schema.lock yet"));
            continue;
        };
        let table = table.clone();
        let new = derived_column(&f);
        if entry.columns.iter().any(|c| c.name == new) {
            problems.push(format!(
                "`{name}` can't move to column `{table}.{new}`, which schema.lock already records"
            ));
            continue;
        }
        let Some(column) = entry
            .columns
            .iter_mut()
            .find(|c| c.field.as_deref() == Some(f.name.as_str()) && !c.retired)
        else {
            problems.push(format!(
                "`{name}` has no column in schema.lock yet; `k migrate new` adds it under its current name"
            ));
            continue;
        };
        let old = std::mem::replace(&mut column.name, new.clone());
        changes.column_renames.push((table, old, new));
    }
    if !problems.is_empty() {
        return Err(MigrationError::Refused(problems));
    }
    Ok(changes)
}

/// `k migrate rename <old> <new>`: records that the entity or field `old`
/// is now called `new`, keeping its table or column, and returns the new
/// text of `schema.lock`. No migration is needed: the database doesn't
/// change (`docs/design/19-persistence.md`, "Renames").
///
/// `old` is the name schema.lock records (`notes.Note` or
/// `notes.Note.title`); `new` is the entity's new qualified name
/// (`notes.Memo`), or the field's new name, alone (`heading`) or
/// qualified (`notes.Note.heading`). After an entity rename, the lock's
/// fields keep their names; rename those separately if they changed too.
///
/// # Errors
///
/// [`MigrationError::Codegen`] for a program with errors, and
/// [`MigrationError::Refused`] when `old` isn't recorded, `new` isn't an
/// entity or a stored field of `old`'s entity, or `new` is recorded
/// already.
///
/// # Examples
///
/// ```
/// use k_codegen::record_rename;
/// use k_sema::{AnalysisHost, Options, ProjectPath};
///
/// let file = |p: &str, t: &str| (ProjectPath::new(p).unwrap(), t.to_owned());
/// let files = vec![
///     file("k.toml", "[package]\nname = \"demo\"\nk = \"0.1\"\n"),
///     file("App.k", "public app Demo {}\n"),
///     file(
///         "src/notes/Note.k",
///         "import k.crypto.Uuid\n\npublic entity Note {\n    @Id let id: Uuid\n    var heading: String\n}\n",
///     ),
///     file(
///         "schema.lock",
///         "migrations = []\n\n[tables.note]\nentity = \"notes.Note\"\ncolumns = [\n    { field = \"id\", name = \"id\", type = \"uuid\", primaryKey = true },\n    { field = \"title\", name = \"title\", type = \"text\" },\n]\n",
///     ),
/// ];
/// let host = AnalysisHost::with_files(Options::new(), files);
/// let lock = record_rename(&host.analysis(), "notes.Note.title", "heading").expect("renames");
/// assert!(lock.contains(r#"{ field = "heading", name = "title", type = "text" }"#));
/// ```
pub fn record_rename(
    analysis: &k_sema::Analysis,
    old: &str,
    new: &str,
) -> Result<String, MigrationError> {
    let view = checked(analysis)?;
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut lock = crate::db::lock(&g).unwrap_or_default();
    let refuse = |m: String| Err(MigrationError::Refused(vec![m]));
    let model = crate::db::model(&g);
    let entity_named = |name: &str| -> Option<&EntityModel> {
        model
            .entities()
            .into_iter()
            .find(|e| g.view.qualified_name(e.decl) == name)
    };
    let recorded_entity = |lock: &SchemaLock, name: &str| -> Option<String> {
        lock.tables
            .iter()
            .find(|(_, t)| t.entity == name && !t.retired)
            .map(|(n, _)| n.clone())
    };
    if let Some(table) = recorded_entity(&lock, old) {
        // An entity.
        if entity_named(new).is_none() {
            return refuse(format!("`{new}` is not an entity of the program"));
        }
        if let Some(t) = recorded_entity(&lock, new) {
            return refuse(format!(
                "`{new}` is already recorded in schema.lock, with table `{t}`"
            ));
        }
        if let Some(t) = lock.tables.get_mut(&table) {
            t.entity = new.to_owned();
        }
        return Ok(lock.to_toml());
    }
    let Some((owner, field)) = old.rsplit_once('.') else {
        return refuse(format!(
            "schema.lock records no entity `{old}` (write `module.Entity` or `module.Entity.field`)"
        ));
    };
    let Some(table) = recorded_entity(&lock, owner) else {
        return refuse(format!(
            "schema.lock records no entity `{old}` or `{owner}`"
        ));
    };
    let new_field = new
        .strip_prefix(owner)
        .and_then(|r| r.strip_prefix('.'))
        .unwrap_or(new);
    if new_field.contains('.') {
        return refuse(format!(
            "a field keeps its entity: `{new}` isn't a field of `{owner}`"
        ));
    }
    let stored = entity_named(owner).is_some_and(|e| {
        e.fields
            .iter()
            .any(|f| f.name == new_field && !matches!(f.kind, FieldKind::ToMany { .. }))
    });
    if !stored {
        return refuse(format!(
            "`{owner}.{new_field}` is not a stored field of the program"
        ));
    }
    let Some(entry) = lock.tables.get_mut(&table) else {
        return refuse(format!("schema.lock records no entity `{owner}`"));
    };
    if entry
        .columns
        .iter()
        .any(|c| c.field.as_deref() == Some(new_field) && !c.retired)
    {
        return refuse(format!(
            "`{owner}.{new_field}` is already recorded in schema.lock"
        ));
    }
    let Some(column) = entry
        .columns
        .iter_mut()
        .find(|c| c.field.as_deref() == Some(field) && !c.retired)
    else {
        return refuse(format!("schema.lock records no field `{old}`"));
    };
    column.field = Some(new_field.to_owned());
    Ok(lock.to_toml())
}

/// The locked columns an entity's fields need, in field order, each with
/// its default as (`ColumnValue` source, lock text) when the field's
/// default is a literal.
fn columns(g: &Gen<'_>, e: &EntityModel) -> Vec<(LockedColumn, Option<(String, String)>)> {
    let model = crate::db::model(g);
    let mut out = Vec::new();
    for (i, f) in e.fields.iter().enumerate() {
        let member = g.find_member(e.decl, &f.name);
        let indexed = member.is_some_and(|m| g.view.member(m).has_annotation("Index"));
        let default = member.and_then(|m| default_of(g, m, f));
        let column = match &f.kind {
            FieldKind::Column {
                column,
                col,
                nullable,
                unique,
            } => {
                let ty = col.column_type(g);
                LockedColumn {
                    field: Some(f.name.clone()),
                    name: column.clone(),
                    ty: ty.lock_name().to_owned(),
                    primary_key: e.id == Some(i),
                    nullable: *nullable,
                    unique: *unique,
                    indexed,
                    references: None,
                    cases: match ty {
                        ColumnType::Enumeration(cases) => cases,
                        _ => Vec::new(),
                    },
                    default: None,
                    retired: false,
                }
            }
            FieldKind::ToOne {
                column,
                target,
                nullable,
            } => {
                let target = model.entity(*target);
                let ty = target
                    .and_then(EntityModel::id_field)
                    .and_then(|idf| match &idf.kind {
                        FieldKind::Column { col, .. } => Some(col.column_type(g)),
                        _ => None,
                    })
                    .unwrap_or(ColumnType::Uuid);
                LockedColumn {
                    field: Some(f.name.clone()),
                    name: column.clone(),
                    ty: ty.lock_name().to_owned(),
                    nullable: *nullable,
                    indexed: true,
                    references: target.map(|t| format!("{}.id", t.table)),
                    ..LockedColumn::default()
                }
            }
            FieldKind::ToMany { .. } => continue,
        };
        out.push((column, default));
    }
    out
}

/// A field's default as a `ColumnValue` (`.boolean(false)`) and as lock
/// text (`false`), when it is a literal the builder can store.
fn default_of(g: &Gen<'_>, m: k_sema::view::MemberId, f: &FieldModel) -> Option<(String, String)> {
    let node = g.member_node(m).and_then(ast::FieldDecl::cast)?;
    let value = node.syntax().children().find_map(ast::Expr::cast)?;
    let FieldKind::Column { col, .. } = &f.kind else {
        return None;
    };
    let ty = col.column_type(g);
    let text = value.syntax().text().to_string();
    let text = text.trim();
    match (&value, ty) {
        (_, ColumnType::Boolean) if text == "true" || text == "false" => {
            Some((format!(".boolean({text})"), text.to_owned()))
        }
        (_, ColumnType::Integer) if is_integer(text) => {
            let digits = text.replace('_', "");
            Some((format!(".integer({digits})"), digits))
        }
        (_, ColumnType::Decimal) if is_decimal(text) => {
            let digits = text.replace('_', "");
            Some((format!(".decimal({digits})"), digits))
        }
        (ast::Expr::StringLit(s), ColumnType::Text) => {
            let v = s.plain_value()?;
            Some((format!(".text({})", quote(&v)), v))
        }
        (ast::Expr::ImplicitMemberExpr(i), ColumnType::Enumeration(cases)) => {
            let case = i.name()?.text().to_owned();
            cases
                .contains(&case)
                .then(|| (format!(".text({})", quote(&case)), case))
        }
        _ => None,
    }
}

/// `true` for an integer literal: `0`, `-12`, `1_000`.
fn is_integer(text: &str) -> bool {
    let t = text.strip_prefix('-').unwrap_or(text);
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit() || b == b'_')
}

/// `true` for a decimal literal: `1.5`, `-0.25`.
fn is_decimal(text: &str) -> bool {
    let t = text.strip_prefix('-').unwrap_or(text);
    let mut parts = t.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("0");
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        && fraction.bytes().all(|b| b.is_ascii_digit() || b == b'_')
}

/// A K string literal of `s`.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The migration file: a header saying how it was generated, the doc
/// comment, `@Breaking` when the request needs it, and the builder calls,
/// one group per table.
#[allow(clippy::too_many_lines)]
fn render(
    words: &[String],
    description: &str,
    request: MigrationRequest<'_>,
    breaking: Option<&str>,
    changes: &Changes,
) -> String {
    let name: String = {
        let pascal: String = words
            .iter()
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            })
            .collect();
        if pascal.starts_with(|c: char| c.is_ascii_digit()) {
            format!("Migration{pascal}")
        } else {
            pascal
        }
    };
    let mut doc: String = {
        let d = description.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut c = d.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    };
    if !doc.ends_with(['.', '!', '?']) {
        doc.push('.');
    }
    let mut out = String::new();
    let quoted = quote(description);
    match request {
        MigrationRequest::Empty => {
            let _ = writeln!(
                out,
                "// Generated by `k migrate new --empty {quoted}`: write its data\n// changes with the schema builder (19-persistence.md, \"Data migrations\").\n"
            );
        }
        MigrationRequest::Diff => {
            let _ = writeln!(
                out,
                "// Generated by `k migrate new {quoted}` from the difference between\n// the entities and schema.lock. Review it before committing.\n"
            );
        }
        MigrationRequest::DropRetired => {
            let _ = writeln!(
                out,
                "// Generated by `k migrate new --drop-retired {quoted}`: drops the tables\n// and columns earlier migrations retired. Ship it only once no running\n// release reads them (19-persistence.md, \"Rolling deploys\").\n"
            );
        }
        MigrationRequest::Rename(names) => {
            let list: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
            let _ = writeln!(
                out,
                "// Generated by `k migrate new --rename` for {}: renames their\n// storage to follow their names. The previous release still reads the old\n// names, so a rolling deploy needs add, backfill, retire and drop instead\n// (19-persistence.md, \"Rolling deploys\"); `@Breaking(\"single instance\")`\n// says this app runs one instance.\n",
                list.join(", ")
            );
        }
    }
    let _ = writeln!(out, "/// {doc}");
    if let Some(reason) = breaking {
        let _ = writeln!(out, "@Breaking({})", quote(reason));
    }
    let _ = writeln!(out, "migration {name} {{");
    out.push_str("    func up(schema: Schema) {\n");
    let mut groups: Vec<String> = Vec::new();
    let tables: BTreeSet<&String> = changes
        .adds
        .keys()
        .chain(changes.indexes.iter().map(|(t, _)| t))
        .chain(changes.retires.iter().map(|(t, _, _)| t))
        .chain(changes.drops.iter().map(|(t, _)| t))
        .chain(changes.column_renames.iter().map(|(t, _, _)| t))
        .collect();
    for table in tables {
        let adds = changes.adds.get(table).map_or(&[][..], Vec::as_slice);
        let retires: Vec<_> = changes
            .retires
            .iter()
            .filter(|(t, _, _)| t == table)
            .collect();
        let drops: Vec<_> = changes.drops.iter().filter(|(t, _)| t == table).collect();
        let column_renames: Vec<_> = changes
            .column_renames
            .iter()
            .filter(|(t, _, _)| t == table)
            .collect();
        if !adds.is_empty()
            || !retires.is_empty()
            || !drops.is_empty()
            || !column_renames.is_empty()
        {
            let mut g = format!(
                "        schema.alterTable(name: {}) {{ table =>\n",
                quote(table)
            );
            for (c, default) in adds {
                let _ = writeln!(
                    g,
                    "            table.addColumn(name: {}, type: {})",
                    quote(&c.name),
                    type_source(c)
                );
                if let Some(d) = default {
                    let _ = writeln!(g, "                .defaultValue({d})");
                }
                if c.nullable {
                    g.push_str("                .nullable(true)\n");
                }
                if let Some(r) = &c.references {
                    let target = r.split('.').next().unwrap_or(r);
                    let _ = writeln!(g, "                .references(table: {})", quote(target));
                } else if c.indexed {
                    g.push_str("                .indexed(true)\n");
                }
            }
            for (_, column, field) in &retires {
                let _ = writeln!(
                    g,
                    "            // `{field}` was removed. The previous release still\n            // reads it; drop it in a later release with\n            // `k migrate new --drop-retired`.\n            table.retireColumn(name: {})",
                    quote(column)
                );
            }
            for (_, from, to) in &column_renames {
                let _ = writeln!(
                    g,
                    "            table.renameColumn(from: {}, to: {})",
                    quote(from),
                    quote(to)
                );
            }
            for (_, column) in &drops {
                let _ = writeln!(g, "            table.dropColumn(name: {})", quote(column));
            }
            g.push_str("        }\n");
            groups.push(g);
        }
        for (t, column) in changes.indexes.iter().filter(|(t, _)| t == table) {
            groups.push(format!(
                "        schema.createIndex(table: {}, columns: [{}])\n",
                quote(t),
                quote(column)
            ));
        }
    }
    for t in &changes.drop_tables {
        groups.push(format!("        schema.dropTable(name: {})\n", quote(t)));
    }
    for (from, to) in &changes.table_renames {
        groups.push(format!(
            "        schema.renameTable(from: {}, to: {})\n",
            quote(from),
            quote(to)
        ));
    }
    for (table, columns) in &changes.creates {
        let mut g = format!(
            "        schema.createTable(name: {}) {{ table =>\n",
            quote(table)
        );
        for c in columns {
            let mut line = format!(
                "            table.column(name: {}, type: {})",
                quote(&c.name),
                type_source(c)
            );
            if c.primary_key {
                line.push_str(".primaryKey(true)");
            }
            if c.nullable {
                line.push_str(".nullable(true)");
            }
            if c.unique {
                line.push_str(".unique(true)");
            }
            if let Some(r) = &c.references {
                let target = r.split('.').next().unwrap_or(r);
                let _ = write!(line, ".references(table: {})", quote(target));
            } else if c.indexed {
                line.push_str(".indexed(true)");
            }
            g.push_str(&line);
            g.push('\n');
        }
        g.push_str("        }\n");
        groups.push(g);
    }
    out.push_str(&groups.join("\n"));
    out.push_str("    }\n}\n");
    out
}

/// The builder's `ColumnType` for a locked column: `.uuid`,
/// `.enumeration(cases: ["a", "b"])`.
fn type_source(c: &LockedColumn) -> String {
    if c.ty == "enumeration" {
        let cases: Vec<String> = c.cases.iter().map(|k| quote(k)).collect();
        format!(".enumeration(cases: [{}])", cases.join(", "))
    } else {
        format!(".{}", c.ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_recognized() {
        assert!(is_integer("0") && is_integer("-12") && is_integer("1_000"));
        assert!(!is_integer("") && !is_integer("-") && !is_integer("1.5"));
        assert!(is_decimal("1.5") && is_decimal("-0.25") && is_decimal("3"));
        assert!(!is_decimal(".5") && !is_decimal("1.x"));
    }

    #[test]
    fn strings_are_quoted_for_k() {
        assert_eq!(quote("a\"b{c}\\"), "\"a\\\"b\\{c}\\\\\"");
    }
}
