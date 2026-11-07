//! `k migrate new`: the next migration from the difference between the
//! entities and `schema.lock` (`docs/design/19-persistence.md`,
//! "Migrations"; `docs/compiler/db.md`, "Migrations").
//!
//! [`plan_migration`] compares the persistence model of the checked
//! program (the same [`crate::db`] model the repositories are lowered
//! from, so table and column names follow the lock) with the recorded
//! lock, and either reports that they match or drafts
//!
//! * the migration file `migrations/NNNN_<description>.k`: one
//!   `createTable` per new entity (referenced tables first), one
//!   `alterTable` per changed table with `addColumn` for each new field
//!   (its default as `defaultValue`, or `nullable(true)` for an optional
//!   field), `createIndex` for a new `@Index`, and a comment for each
//!   retired column;
//! * the updated lock, in [`k_db::lock::SchemaLock::to_toml`]'s canonical
//!   layout, with the migration appended to `migrations`, new tables and
//!   columns recorded, and the columns of removed fields marked `retired`
//!   (their names stay reserved; expand, then contract).
//!
//! A change the builder can't express safely in v0 is refused with an
//! explanation instead of being written: a column type or option that
//! changed, an enum case added or removed (the builder's case operations
//! need a table rebuild on SQLite), a required field added to an existing
//! table without a default, a removed field whose column is `NOT NULL`
//! without a default (the next release's inserts would fail), and a removed
//! entity. Renames are never inferred: a renamed field reads as a removal
//! and an addition, which the explanation of the removal names.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use k_db::lock::{LockedColumn, LockedTable, SchemaLock};
use k_db::schema::ColumnType;
use k_syntax::ast::{self, AstNode};

use crate::cx::Gen;
use crate::db::{EntityModel, FieldKind, FieldModel};
use crate::{CodegenError, Options};

/// What [`plan_migration`] decided.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MigrationPlan {
    /// The entities match `schema.lock`: there is nothing to write.
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

/// Drafts the next migration of the analysed project, described by
/// `description` (`"add note pinning"`). With `empty`, the migration has
/// no schema changes (a data migration written by hand) and the entities
/// are not compared.
///
/// # Errors
///
/// [`MigrationError::Codegen`] for a program with errors,
/// [`MigrationError::BadDescription`] for a description without a word,
/// and [`MigrationError::Refused`] for differences it doesn't write.
pub fn plan_migration(
    analysis: &k_sema::Analysis,
    description: &str,
    empty: bool,
) -> Result<MigrationPlan, MigrationError> {
    let diagnostics = analysis
        .diagnostics()
        .map_err(|_| MigrationError::Codegen(CodegenError::Cancelled))?;
    let count = diagnostics.iter().filter(|d| d.is_error()).count();
    if count > 0 {
        return Err(MigrationError::Codegen(CodegenError::HasErrors { count }));
    }
    let words: Vec<String> = description
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return Err(MigrationError::BadDescription);
    }
    let view = analysis
        .program()
        .map_err(|_| MigrationError::Codegen(CodegenError::Cancelled))?;
    let options = Options::default();
    let g = Gen::new(analysis, view, &options);
    let mut lock = crate::db::lock(&g).unwrap_or_default();
    let changes = if empty {
        Changes::default()
    } else {
        let changes = diff(&g, &mut lock)?;
        if changes.is_empty() {
            return Ok(MigrationPlan::UpToDate);
        }
        changes
    };
    let number = next_number(&g, &lock);
    let stem = format!("{number:04}_{}", words.join("_"));
    lock.migrations.push(stem.clone());
    let source = render(&words, description, empty, &changes);
    Ok(MigrationPlan::Draft(MigrationDraft {
        file: format!("migrations/{stem}.k"),
        source,
        lock: lock.to_toml(),
    }))
}

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
struct Changes {
    /// New tables, in creation order.
    creates: Vec<(String, Vec<LockedColumn>)>,
    /// New columns of existing tables, by table, with their `ColumnValue`
    /// default as K source.
    adds: BTreeMap<String, Vec<(LockedColumn, Option<String>)>>,
    /// New indexes of existing tables: (table, column).
    indexes: Vec<(String, String)>,
    /// Retired columns: (table, column, the removed field's qualified name).
    retires: Vec<(String, String, String)>,
}

impl Changes {
    fn is_empty(&self) -> bool {
        self.creates.is_empty()
            && self.adds.is_empty()
            && self.indexes.is_empty()
            && self.retires.is_empty()
    }
}

/// Compares the entities with `lock`, updating `lock` to match them.
fn diff(g: &Gen<'_>, lock: &mut SchemaLock) -> Result<Changes, MigrationError> {
    let model = crate::db::model(g);
    let mut problems = Vec::new();
    let mut changes = Changes::default();
    let mut live: BTreeSet<String> = BTreeSet::new();
    let mut created: Vec<(String, Vec<LockedColumn>)> = Vec::new();
    for e in model.entities() {
        let qualified = g.view.qualified_name(e.decl);
        live.insert(qualified.clone());
        let desired = columns(g, e);
        let recorded = lock
            .tables
            .iter()
            .find(|(_, t)| t.entity == qualified)
            .map(|(n, _)| n.clone());
        let Some(table) = recorded else {
            if let Some(other) = lock.tables.get(&e.table) {
                problems.push(format!(
                    "`{qualified}` would be stored in table `{}`, which schema.lock records for `{}`",
                    e.table, other.entity
                ));
                continue;
            }
            created.push((
                e.table.clone(),
                desired.iter().map(|(c, _)| c.clone()).collect(),
            ));
            continue;
        };
        let Some(entry) = lock.tables.get_mut(&table) else {
            continue;
        };
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
                        problems.push(format!(
                            "`{path}` is added to the existing table `{table}`, so it needs a default value or an optional type: existing rows need a value, and so do the previous release's inserts"
                        ));
                        continue;
                    }
                    if entry.columns.iter().any(|c| c.name == want.name) {
                        problems.push(format!(
                            "`{path}` would be stored in column `{table}.{}`, which schema.lock already records (retired columns keep their names)",
                            want.name
                        ));
                        continue;
                    }
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
                        problems.push(format!(
                            "`{path}` changed from `{}` to `{}`; write a migration for it by hand (`k migrate new --empty`), with `@Breaking(\"reason\")` if it narrows the column",
                            have.ty, want.ty
                        ));
                        continue;
                    }
                    if have.cases != want.cases {
                        for case in want.cases.iter().filter(|c| !have.cases.contains(c)) {
                            problems.push(format!(
                                "`{path}` gained the case `{case}`; adding an enum case to a column needs a table rebuild on SQLite, which the v0 builder doesn't do: write it with `schema.execute(postgres:sqlite:)` in a `k migrate new --empty` migration"
                            ));
                        }
                        for case in have.cases.iter().filter(|c| !want.cases.contains(c)) {
                            problems.push(format!(
                                "`{path}` lost the case `{case}`; removing an enum case rewrites stored values and needs `@Breaking(\"reason\")`: write it by hand in a `k migrate new --empty` migration"
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
                        problems.push(format!(
                            "the column options of `{path}` changed (optional, `@Unique`, `@Id` or relation); write a migration for it by hand (`k migrate new --empty`)"
                        ));
                        continue;
                    }
                    if want.indexed && !have.indexed {
                        have.indexed = true;
                        changes.indexes.push((table.clone(), have.name.clone()));
                    }
                }
            }
        }
        let fields: BTreeSet<Option<String>> =
            desired.iter().map(|(c, _)| c.field.clone()).collect();
        for c in &mut entry.columns {
            if c.retired || fields.contains(&c.field) {
                continue;
            }
            let field = c.field.clone().unwrap_or_default();
            if !c.nullable && c.default.is_none() && !c.primary_key {
                problems.push(format!(
                    "`{qualified}.{field}` was removed (or renamed), but its column `{table}.{}` is `NOT NULL` without a default, so the next release's inserts would fail; keep the field, or write the migration by hand (`k migrate new --empty`) that relaxes the column with `schema.execute(postgres:sqlite:)`",
                    c.name
                ));
                continue;
            }
            changes.retires.push((
                table.clone(),
                c.name.clone(),
                format!("{qualified}.{field}"),
            ));
            c.retired = true;
            c.field = None;
        }
    }
    for (name, t) in &lock.tables {
        if !live.contains(&t.entity) {
            problems.push(format!(
                "entity `{}` (table `{name}`) was removed or renamed; `k migrate new` doesn't retire or drop tables in v0",
                t.entity
            ));
        }
    }
    if !problems.is_empty() {
        return Err(MigrationError::Refused(problems));
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
                columns: cols.clone(),
            },
        );
    }
    changes.creates = ordered;
    Ok(changes)
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

/// The migration file.
fn render(words: &[String], description: &str, empty: bool, changes: &Changes) -> String {
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
    if empty {
        let _ = writeln!(
            out,
            "// Generated by `k migrate new --empty {quoted}`: write its data\n// changes with the schema builder (19-persistence.md, \"Data migrations\").\n"
        );
    } else {
        let _ = writeln!(
            out,
            "// Generated by `k migrate new {quoted}` from the difference between\n// the entities and schema.lock. Review it before committing.\n"
        );
    }
    let _ = writeln!(out, "/// {doc}");
    let _ = writeln!(out, "migration {name} {{");
    out.push_str("    func up(schema: Schema) {\n");
    let mut groups: Vec<String> = Vec::new();
    let tables: BTreeSet<&String> = changes
        .adds
        .keys()
        .chain(changes.indexes.iter().map(|(t, _)| t))
        .chain(changes.retires.iter().map(|(t, _, _)| t))
        .collect();
    for table in tables {
        if let Some(adds) = changes.adds.get(table) {
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
        for (t, column, field) in changes.retires.iter().filter(|(t, _, _)| t == table) {
            groups.push(format!(
                "        // `{field}` was removed: schema.lock now marks the column\n        // `{t}.{column}` retired. Drop it in a later release, once no running\n        // release reads it (19-persistence.md, \"Rolling deploys\").\n"
            ));
        }
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
