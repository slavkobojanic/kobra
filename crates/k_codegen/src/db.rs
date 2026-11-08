//! Persistence in generated code (`docs/compiler/db.md` §3–§10,
//! `docs/design/19-persistence.md`).
//!
//! * **The model** ([`Model`]): every entity's table and columns, with the
//!   locked names of `schema.lock` (snake case for anything the lock
//!   doesn't record yet), the column type of each field, its relations, its
//!   `@Id` and `@Version` fields.
//! * **Queries** ([`awaited`]): an awaited chain on a repository's
//!   collection (`await notes.filter { … }.sortedBy { … }.first { … }`) is
//!   lowered to one [`k_db::ir::Statement`], rendered for both dialects and
//!   registered in the build's statement table; the generated code calls
//!   `$s.dbQuery`/`$s.dbWrite`/… with the statement's number and its
//!   parameters. Every value is a parameter. A lambda that can't be
//!   translated is a build error, never an in-memory scan.
//! * **Repositories** ([`repository`]): a class with the injected
//!   dependencies, the request's `Database`, the user's functions and the
//!   inherited writes `$insert`, `$insertAll`, `$update`, `$delete`.
//! * **Migrations** ([`migrations`]): the `Schema` builder calls of each
//!   `migrations/*.k` file, replayed by [`k_db::builder::Replay`] into the
//!   statements of both dialects.
//!
//! The statement table, the migrations and the unique indexes form the
//! build's [`DbManifest`] ([`manifest`]), which the server loads at
//! startup; SQL text never appears in the JavaScript.

use std::cell::RefCell;
use std::collections::HashMap;

use k_db::builder::{AlterOp, ColumnDef, Literal, Replay, SchemaOp};
use k_db::ir::{ArithOp, CmpOp, Count, Like, Operand, Order, Predicate, Select, Statement};
use k_db::manifest::{DbManifest, StatementSql, UniqueIndex};
use k_db::migrate::Migration;
use k_db::schema::ColumnType;
use k_db::sql::{Dialect, render};
use k_diagnostics::{Code, Diagnostic, FileId, Span, TextRange, codes};
use k_sema::Resolution;
use k_sema::stdlib::DeclarationKind;
use k_sema::view::{DeclId, MemberKind, Type};
use k_syntax::ast::{self, AstNode};
use k_syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

use crate::cx::Gen;
use crate::js::{
    self, Class, Expr, Function, MethodKind, Param, Prop, Stmt, bin, call, id, member, rt,
};
use crate::lower::{Body, SelfMode};

/// The row counts `insert(all:)` has statements for, largest first.
const INSERT_BATCHES: [u32; 4] = [64, 16, 4, 1];

// ----------------------------------------------------------------- state

/// The statements registered while lowering, and persistence errors.
#[derive(Default, Debug)]
pub(crate) struct DbState {
    /// The registered statements in both dialects; a statement's index is its
    /// number in the manifest and in generated `$s.db` calls.
    statements: Vec<StatementSql>,
    /// The number of each registered statement, by its SQLite and Postgres
    /// text, so an identical statement is registered once.
    index: HashMap<(String, String), u32>,
    /// Problems that make the build fail (an untranslatable query, a
    /// migration that doesn't replay).
    pub errors: Vec<String>,
}

impl DbState {
    /// Renders `st` for both dialects and returns its number, reusing the
    /// number of an identical statement.
    fn register(&mut self, st: &Statement) -> u32 {
        let sqlite = render(st, Dialect::Sqlite).text;
        let postgres = render(st, Dialect::Postgres).text;
        let key = (sqlite.clone(), postgres.clone());
        if let Some(&n) = self.index.get(&key) {
            return n;
        }
        let n = u32::try_from(self.statements.len()).unwrap_or(u32::MAX);
        self.statements.push(StatementSql { sqlite, postgres });
        self.index.insert(key, n);
        n
    }
}

/// The per-build persistence state kept on [`Gen`].
#[derive(Default, Debug)]
pub(crate) struct DbCell {
    /// The statements and errors, mutated while lowering through a shared
    /// [`Gen`] (codegen is single-threaded, so a `RefCell` suffices).
    state: RefCell<DbState>,
    /// The persistence model, built on first use by [`model`].
    model: std::cell::OnceCell<Model>,
}

impl DbCell {
    /// The registered statements and errors.
    pub(crate) fn state(&self) -> std::cell::RefMut<'_, DbState> {
        self.state.borrow_mut()
    }
}

// ----------------------------------------------------------------- model

/// How a field's value is stored.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Col {
    /// `Uuid`.
    Uuid,
    /// `String`, `Url`.
    Text,
    /// `EmailAddress`.
    Email,
    /// `Int`.
    Int,
    /// `Bool`.
    Bool,
    /// `Decimal`.
    Decimal,
    /// `Date`.
    Date,
    /// `Money`: two columns.
    Money,
    /// A payload-free enum, by case name.
    Enum(DeclId),
    /// A `Codable` value as JSON.
    Json(Type),
    /// A struct with one stored field, stored as that field.
    Wrap(DeclId, String, Box<Col>),
}

impl Col {
    /// The number of stored columns.
    fn width(&self) -> usize {
        match self {
            Col::Money => 2,
            Col::Wrap(_, _, inner) => inner.width(),
            _ => 1,
        }
    }

    /// The `schema.lock` / DDL column type.
    pub(crate) fn column_type(&self, g: &Gen<'_>) -> ColumnType {
        match self {
            Col::Uuid => ColumnType::Uuid,
            Col::Text | Col::Email => ColumnType::Text,
            Col::Int => ColumnType::Integer,
            Col::Bool => ColumnType::Boolean,
            Col::Decimal => ColumnType::Decimal,
            Col::Date => ColumnType::Timestamp,
            Col::Money => ColumnType::Money,
            Col::Json(_) => ColumnType::Json,
            Col::Enum(d) => ColumnType::Enumeration(
                g.cases(*d)
                    .iter()
                    .map(|&c| g.view.member(c).name.clone())
                    .collect(),
            ),
            Col::Wrap(_, _, inner) => inner.column_type(g),
        }
    }
}

/// A stored field of an entity.
#[derive(Clone, Debug)]
pub(crate) enum FieldKind {
    /// A value column.
    Column {
        /// The locked column name (money: the logical name).
        column: String,
        /// How the value is stored.
        col: Col,
        /// `T?`.
        nullable: bool,
        /// `@Unique`.
        unique: bool,
    },
    /// A to-one relation: the `<field>_id` foreign key.
    ToOne {
        /// The locked column name.
        column: String,
        /// The related entity.
        target: DeclId,
        /// `T?`.
        nullable: bool,
    },
    /// A to-many relation: no column; the inverse to-one field of the
    /// target.
    ToMany {
        /// The related entity.
        target: DeclId,
        /// The target's to-one field back to this entity.
        inverse: String,
    },
}

/// One stored field.
#[derive(Clone, Debug)]
pub(crate) struct FieldModel {
    /// The K name (also the JavaScript property).
    pub name: String,
    /// How it is stored.
    pub kind: FieldKind,
}

impl FieldModel {
    /// The stored column names, in select order (none for to-many).
    fn columns(&self) -> Vec<String> {
        match &self.kind {
            FieldKind::Column { column, col, .. } if col.width() == 2 => {
                vec![format!("{column}_amount"), format!("{column}_currency")]
            }
            FieldKind::Column { column, .. } | FieldKind::ToOne { column, .. } => {
                vec![column.clone()]
            }
            FieldKind::ToMany { .. } => Vec::new(),
        }
    }
}

/// One entity and its table.
#[derive(Clone, Debug)]
pub(crate) struct EntityModel {
    /// The entity declaration.
    pub decl: DeclId,
    /// The locked table name.
    pub table: String,
    /// The stored fields in declaration order (the constructor's order).
    pub fields: Vec<FieldModel>,
    /// The index of the `@Id` field.
    pub id: Option<usize>,
    /// The index of the `@Version` field.
    pub version: Option<usize>,
}

impl EntityModel {
    /// Every stored column, in select order.
    fn select_columns(&self) -> Vec<String> {
        self.fields.iter().flat_map(FieldModel::columns).collect()
    }

    /// The field named `name` (its K name), if the entity has one.
    fn field(&self, name: &str) -> Option<&FieldModel> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The `@Id` field, if the entity has one.
    pub(crate) fn id_field(&self) -> Option<&FieldModel> {
        self.id.and_then(|i| self.fields.get(i))
    }

    /// The `@Unique` fields, in declaration order: their index is the tag
    /// of the generated `UniqueViolation` case.
    fn unique_fields(&self) -> Vec<&FieldModel> {
        self.fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::Column { unique: true, .. }))
            .collect()
    }
}

/// Every entity of the program.
#[derive(Clone, Debug, Default)]
pub(crate) struct Model {
    /// Each entity declaration of the project, by declaration.
    entities: HashMap<DeclId, EntityModel>,
}

/// The snake-case table or column name of a K name.
pub(crate) fn snake(name: &str) -> String {
    crate::css::kebab(name).replace('-', "_")
}

/// The text of the project's `schema.lock`, if it has one.
pub(crate) fn lock(g: &Gen<'_>) -> Option<k_db::lock::SchemaLock> {
    let (file, _) = g
        .analysis
        .files()
        .into_iter()
        .find(|(_, p)| p.as_str() == "schema.lock")?;
    k_db::lock::SchemaLock::parse(g.analysis.text(file)?).ok()
}

/// How a field of type `t` is stored, if it can be.
fn col_of(g: &Gen<'_>, t: &Type, depth: u32) -> Option<Col> {
    let b = &g.b;
    let d = t.decl()?;
    let is = |x: Option<DeclId>| x == Some(d);
    Some(if is(b.uuid) {
        Col::Uuid
    } else if is(b.string) || is(b.url) {
        Col::Text
    } else if is(b.email) {
        Col::Email
    } else if is(b.int) || is(b.duration) {
        // A `Duration` is whole milliseconds.
        Col::Int
    } else if is(b.bool_) {
        Col::Bool
    } else if is(b.decimal) {
        Col::Decimal
    } else if is(b.date) {
        Col::Date
    } else if is(b.money) {
        Col::Money
    } else {
        let info = g.view.decl(d);
        match info.named_kind() {
            Some(DeclarationKind::Enum)
                if !info.stdlib && g.cases(d).iter().all(|&c| g.case_fields(c).is_empty()) =>
            {
                Col::Enum(d)
            }
            Some(DeclarationKind::Struct) if !info.stdlib && depth < 4 => {
                let fields = g.stored_fields(d);
                match fields.as_slice() {
                    [only] => {
                        let fi = g.view.member(*only);
                        let inner = fi.ty.as_ref().and_then(|t| col_of(g, t, depth + 1));
                        match inner {
                            Some(c) if !matches!(c, Col::Json(_)) => {
                                Col::Wrap(d, fi.name.clone(), Box::new(c))
                            }
                            _ => Col::Json(t.clone()),
                        }
                    }
                    _ => Col::Json(t.clone()),
                }
            }
            _ => Col::Json(t.clone()),
        }
    })
}

impl Model {
    /// Every entity, sorted by table name.
    pub(crate) fn entities(&self) -> Vec<&EntityModel> {
        let mut out: Vec<&EntityModel> = self.entities.values().collect();
        out.sort_by(|a, b| a.table.cmp(&b.table));
        out
    }

    /// The entity `d`, if it is one.
    pub(crate) fn entity(&self, d: DeclId) -> Option<&EntityModel> {
        self.entities.get(&d)
    }

    /// Builds the model of every project entity, taking table and column
    /// names from `schema.lock` where it records them and falling back to the
    /// snake-case K names.
    fn build(g: &Gen<'_>) -> Model {
        let lock = lock(g);
        let entities: Vec<DeclId> = g
            .own
            .iter()
            .copied()
            .filter(|&d| g.view.decl(d).named_kind() == Some(DeclarationKind::Entity))
            .collect();
        let mut out = Model::default();
        for &e in &entities {
            let info = g.view.decl(e);
            let qualified = g.view.qualified_name(e);
            let locked = lock.as_ref().and_then(|l| {
                l.tables
                    .iter()
                    .find(|(_, t)| t.entity == qualified)
                    .map(|(name, t)| (name.clone(), t.clone()))
            });
            let table = locked
                .as_ref()
                .map_or_else(|| snake(&info.name), |(n, _)| n.clone());
            let column_name = |field: &str, fallback: String| -> String {
                locked
                    .as_ref()
                    .and_then(|(_, t)| {
                        t.columns
                            .iter()
                            .find(|c| c.field.as_deref() == Some(field) && !c.retired)
                    })
                    .map_or(fallback, |c| c.name.clone())
            };
            let mut fields = Vec::new();
            let mut id_index = None;
            let mut version = None;
            for m in g.stored_fields(e) {
                let mi = g.view.member(m);
                let ty = mi.ty.clone().unwrap_or(Type::Error);
                let (nullable, base) = match &ty {
                    Type::Optional(inner) => (true, (**inner).clone()),
                    t => (false, t.clone()),
                };
                let kind = if mi.has_annotation("Relation") {
                    if g.is(&base, g.b.list) {
                        let target = base.args().first().and_then(Type::decl);
                        let Some(target) = target else { continue };
                        let inverse = g
                            .stored_fields(target)
                            .into_iter()
                            .find(|&f| {
                                let fi = g.view.member(f);
                                fi.has_annotation("Relation")
                                    && fi.ty.as_ref().map(|t| t.unwrap_optional().decl())
                                        == Some(Some(e))
                            })
                            .map(|f| g.view.member(f).name.clone())
                            .unwrap_or_default();
                        FieldKind::ToMany { target, inverse }
                    } else {
                        let Some(target) = base.decl() else { continue };
                        FieldKind::ToOne {
                            column: column_name(&mi.name, format!("{}_id", snake(&mi.name))),
                            target,
                            nullable,
                        }
                    }
                } else {
                    if g.is(&base, g.b.double) {
                        g.db.state().errors.push(format!(
                            "`{}.{}`: a `Double` field has no column type in v0 (db.md §3); store a `Decimal`",
                            g.view.qualified_name(e),
                            mi.name
                        ));
                        continue;
                    }
                    let Some(col) = col_of(g, &base, 0) else {
                        continue;
                    };
                    FieldKind::Column {
                        column: column_name(&mi.name, snake(&mi.name)),
                        col,
                        nullable,
                        unique: mi.has_annotation("Unique"),
                    }
                };
                if mi.has_annotation("Id") {
                    id_index = Some(fields.len());
                }
                if mi.has_annotation("Version") {
                    version = Some(fields.len());
                }
                fields.push(FieldModel {
                    name: mi.name.clone(),
                    kind,
                });
            }
            out.entities.insert(
                e,
                EntityModel {
                    decl: e,
                    table,
                    fields,
                    id: id_index,
                    version,
                },
            );
        }
        out
    }
}

/// The program's persistence model.
pub(crate) fn model<'g>(g: &'g Gen<'_>) -> &'g Model {
    g.db.model.get_or_init(|| Model::build(g))
}

/// The model of entity `d`, or `None` when `d` is not an entity.
fn entity<'g>(g: &'g Gen<'_>, d: DeclId) -> Option<&'g EntityModel> {
    model(g).entities.get(&d)
}

/// The entity of `repository R for Entity`.
pub(crate) fn repository_entity(g: &Gen<'_>, d: DeclId) -> Option<DeclId> {
    let info = g.view.decl(d);
    let node = g.decl_node(d)?;
    let clause = node
        .children()
        .find(|c| c.kind() == SyntaxKind::ForClause)?;
    let name = clause
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident)
        .last()?;
    match g.resolved(info.file, name.text_range()) {
        Some(k_sema::view::Resolved::Decl(e)) => Some(e),
        _ => {
            // The for clause's type may be recorded on the whole type node.
            let ty = clause.children().find_map(ast::Type::cast)?;
            match g.resolved(info.file, ty.syntax().text_range()) {
                Some(k_sema::view::Resolved::Decl(e)) => Some(e),
                _ => g.own.iter().copied().find(|&e| {
                    g.view.decl(e).named_kind() == Some(DeclarationKind::Entity)
                        && g.view.decl(e).name == name.text()
                }),
            }
        }
    }
}

// --------------------------------------------------------------- codecs

/// `$s.name`: a member of the server runtime module, which generated
/// server code imports as `$s`.
fn s_member(name: &str) -> Expr {
    member(id("$s"), name)
}

/// `$s.dbc.name`: one of the column codecs of `db.js`.
fn dbc(name: &str) -> Expr {
    member(s_member("dbc"), name)
}

/// The column codec of `col` (db.js `dbc`).
fn codec(b: &mut Body<'_, '_>, col: &Col, nullable: bool) -> Expr {
    let c = match col {
        Col::Uuid => dbc("uuid"),
        Col::Text => dbc("text"),
        Col::Email => dbc("email"),
        Col::Int => dbc("int"),
        Col::Bool => dbc("bool"),
        Col::Decimal => dbc("decimal"),
        Col::Date => dbc("date"),
        Col::Money => dbc("money"),
        Col::Enum(d) => {
            let names: Vec<Expr> =
                b.g.cases(*d)
                    .iter()
                    .map(|&c| js::str(b.g.view.member(c).name.clone()))
                    .collect();
            let e = b.decl_ref(*d);
            call(dbc("enumeration"), vec![e, Expr::Array(names)])
        }
        Col::Json(t) => {
            let c = crate::codec::codec_expr(b, t);
            call(dbc("json"), vec![c])
        }
        Col::Wrap(d, field, inner) => {
            let make = b.decl_ref(*d);
            let inner = codec(b, inner, false);
            call(dbc("wrap"), vec![make, js::str(field.clone()), inner])
        }
    };
    if nullable {
        call(dbc("optional"), vec![c])
    } else {
        c
    }
}

/// The codec of entity `e`'s field `f` (to-one: its target's id).
fn field_codec(b: &mut Body<'_, '_>, e: &EntityModel, f: &FieldModel) -> Expr {
    match &f.kind {
        FieldKind::Column { col, nullable, .. } => codec(b, col, *nullable),
        FieldKind::ToOne {
            target, nullable, ..
        } => {
            let id = id_codec(b, *target);
            let owner = format!("{}.{}", b.g.view.decl(e.decl).name, f.name);
            let c = call(dbc("ref"), vec![id, js::str(owner)]);
            if *nullable {
                call(dbc("optional"), vec![c])
            } else {
                c
            }
        }
        FieldKind::ToMany { .. } => Expr::Null,
    }
}

/// The codec of entity `d`'s `@Id` field.
fn id_codec(b: &mut Body<'_, '_>, d: DeclId) -> Expr {
    let g = b.g;
    match entity(g, d).and_then(|e| e.id_field().map(|f| (e.clone(), f.clone()))) {
        Some((e, f)) => field_codec(b, &e, &f),
        None => dbc("uuid"),
    }
}

/// The JavaScript name of an entity's helper (`Cols`, `Row`, `Values`)
/// for the repository `owner`: `NoteRepository$Tag$dbRow`. Each repository
/// declares its own, so two repositories in one module never collide.
fn helper(g: &Gen<'_>, owner: Option<DeclId>, d: DeclId, what: &str) -> String {
    match owner {
        Some(o) => format!("{}${}$db{what}", g.name(o), g.name(d)),
        None => format!("{}$db{what}", g.name(d)),
    }
}

/// Emits (once per module) `X$dbCols`, `X$dbRow` and `X$dbValues` for
/// entity `d` into the body's prelude and returns their names.
fn entity_helpers(b: &mut Body<'_, '_>, d: DeclId) -> Option<(String, String, String)> {
    let g = b.g;
    let e = entity(g, d)?.clone();
    let cols = helper(g, b.owner, d, "Cols");
    let row = helper(g, b.owner, d, "Row");
    let values = helper(g, b.owner, d, "Values");
    let defined = b
        .prelude
        .iter()
        .any(|s| matches!(s, Stmt::Function(f, _) if f.name == cols));
    if defined {
        return Some((cols, row, values));
    }
    let mut codecs = Vec::new();
    for f in &e.fields {
        codecs.push(field_codec(b, &e, f));
    }
    let make = b.decl_ref(d);
    let mut args = Vec::new();
    let mut enc = Vec::new();
    let mut offset = 0usize;
    for (i, f) in e.fields.iter().enumerate() {
        let c = Expr::Index(Box::new(id("c")), Box::new(Expr::Num(i as f64)));
        match &f.kind {
            FieldKind::ToMany { .. } => {
                let owner = format!("{}.{}", g.view.decl(d).name, f.name);
                args.push(call(s_member("unloadedMany"), vec![js::str(owner)]));
            }
            _ => {
                args.push(call(
                    member(c.clone(), "dec"),
                    vec![id("r"), Expr::Num(offset as f64)],
                ));
                enc.push(Stmt::Expr(call(
                    member(c, "enc"),
                    vec![member(id("e"), f.name.clone()), id("out")],
                )));
                offset += f.columns().len();
            }
        }
    }
    // The codecs are built on first use: they reference the module's enums
    // and constructors, which are declared after the prelude.
    let memo = format!("{cols}$v");
    b.prelude.push(Stmt::Let(memo.clone(), Some(Expr::Null)));
    b.prelude.push(Stmt::Function(
        Function {
            name: cols.clone(),
            body: js::Body::Block(vec![Stmt::Return(Some(Expr::Assign(
                "??=",
                Box::new(id(memo)),
                Box::new(Expr::Array(codecs)),
            )))]),
            ..Function::default()
        },
        false,
    ));
    let table = Stmt::Const("c".into(), call(id(cols.clone()), vec![]));
    b.prelude.push(Stmt::Function(
        Function {
            name: row.clone(),
            params: vec![Param::new("r")],
            body: js::Body::Block(vec![table.clone(), Stmt::Return(Some(call(make, args)))]),
            ..Function::default()
        },
        false,
    ));
    let mut enc_body = vec![table];
    enc_body.extend(enc);
    b.prelude.push(Stmt::Function(
        Function {
            name: values.clone(),
            params: vec![Param::new("e"), Param::new("out")],
            body: js::Body::Block(enc_body),
            ..Function::default()
        },
        false,
    ));
    Some((helper(g, b.owner, d, "Cols"), row, values))
}

/// `X$dbCols()[i]` of the repository being lowered.
fn col_ref(b: &Body<'_, '_>, d: DeclId, i: usize) -> Expr {
    Expr::Index(
        Box::new(call(id(helper(b.g, b.owner, d, "Cols")), vec![])),
        Box::new(Expr::Num(i as f64)),
    )
}

/// `{ email: $k.unitCase(0) }` for the entity's unique fields, or `null`.
fn unique_cases(e: &EntityModel) -> Expr {
    let fields = e.unique_fields();
    if fields.is_empty() {
        return Expr::Null;
    }
    Expr::Object(
        fields
            .iter()
            .enumerate()
            .map(|(i, f)| Prop::KeyValue(f.name.clone(), rt("unitCase", vec![Expr::Num(i as f64)])))
            .collect(),
        false,
    )
}

// ---------------------------------------------------------------- queries

/// A query being lowered: its parameters and how to encode them.
struct Query<'m> {
    /// The entity the query reads or writes.
    entity: &'m EntityModel,
    /// `[codec, value]` pairs, flattened in order into the parameters.
    slots: Vec<(Expr, Expr)>,
    /// The next parameter number.
    next: u32,
    /// The row parameter's binding name range, per lambda being lowered.
    row: Option<TextRange>,
}

impl Query<'_> {
    /// Allocates `width` parameters for `value` encoded by `codec`; returns
    /// the first number.
    fn param(&mut self, codec: Expr, value: Expr, width: usize) -> u32 {
        let first = self.next;
        self.next += u32::try_from(width).unwrap_or(1);
        self.slots.push((codec, value));
        first
    }

    /// The runtime call that encodes the query's parameters,
    /// `$s.dbParams([[codec, value], …])`, in slot order.
    fn params_expr(&self) -> Expr {
        call(
            s_member("dbParams"),
            vec![Expr::Array(
                self.slots
                    .iter()
                    .map(|(c, v)| Expr::Array(vec![c.clone(), v.clone()]))
                    .collect(),
            )],
        )
    }
}

/// A column reference found in a lambda: a field of the row.
struct ColumnRef {
    /// The index of the field in [`EntityModel::fields`].
    field: usize,
    /// The stored columns compared (the foreign key for `row.rel.id`).
    columns: Vec<String>,
}

/// `e` without any enclosing parentheses.
fn strip_parens(e: &ast::Expr) -> ast::Expr {
    let mut e = e.clone();
    while let ast::Expr::ParenExpr(p) = &e {
        match p.syntax().children().find_map(ast::Expr::cast) {
            Some(i) => e = i,
            None => break,
        }
    }
    e
}

/// The lambda argument of a call: trailing or the first argument.
fn lambda_arg(c: &ast::CallExpr) -> Option<ast::LambdaExpr> {
    c.trailing_lambda().or_else(|| {
        c.arg_list()?.args().next()?.value().and_then(|v| match v {
            ast::Expr::LambdaExpr(l) => Some(l),
            _ => None,
        })
    })
}

/// The row parameter of a lambda.
fn row_param(l: &ast::LambdaExpr) -> Option<SyntaxToken> {
    l.param_list()?
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident)
}

/// The single expression a lambda's body is.
fn lambda_value(l: &ast::LambdaExpr) -> Option<ast::Expr> {
    let stmts: Vec<ast::Stmt> = l.statements().collect();
    match stmts.as_slice() {
        [ast::Stmt::Expr(e)] => Some(e.clone()),
        [ast::Stmt::Return(r)] => r.syntax().children().find_map(ast::Expr::cast),
        _ => None,
    }
}

/// `true` if `e` is the literal `nil`, parenthesized or not.
fn is_nil(e: &ast::Expr) -> bool {
    matches!(strip_parens(e), ast::Expr::Literal(l)
        if l.token().is_some_and(|t| t.kind() == SyntaxKind::NilKw))
}

/// The value of `e` when it is the literal `true` or `false`, parenthesized
/// or not.
fn bool_literal(e: &ast::Expr) -> Option<bool> {
    match strip_parens(e) {
        ast::Expr::Literal(l) => match l.token()?.kind() {
            SyntaxKind::TrueKw => Some(true),
            SyntaxKind::FalseKw => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// A short excerpt of `node` for messages.
fn excerpt(node: &SyntaxNode) -> String {
    let t: String = node
        .text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if t.chars().count() > 60 {
        format!("{}…", t.chars().take(60).collect::<String>())
    } else {
        t
    }
}

/// Records a build error and returns a stand-in expression.
fn fail(b: &mut Body<'_, '_>, node: &SyntaxNode, why: &str) -> Expr {
    let path =
        b.g.analysis
            .files()
            .into_iter()
            .find(|(f, _)| *f == b.file)
            .map(|(_, p)| p.as_str().to_owned())
            .unwrap_or_default();
    let line = b.g.analysis.text(b.file).map_or(0, |t| {
        let at = usize::from(node.text_range().start()).min(t.len());
        t[..at].matches('\n').count() + 1
    });
    b.g.db
        .state()
        .errors
        .push(format!("{path}:{line}: `{}`: {why}", excerpt(node)));
    rt(
        "panic",
        vec![js::str(format!("untranslatable query: {why}"))],
    )
}

impl Query<'_> {
    /// `true` if `e` is the row parameter.
    fn is_row(&self, b: &Body<'_, '_>, e: &ast::Expr) -> bool {
        let ast::Expr::NameExpr(n) = strip_parens(e) else {
            return false;
        };
        let Some(row) = self.row else { return false };
        match b.g.resolution(b.file, n.syntax().text_range()) {
            Some(Resolution::Local { definition, .. }) => definition.range == row,
            _ => false,
        }
    }

    /// `true` if `e` mentions the row parameter anywhere.
    fn mentions_row(&self, b: &Body<'_, '_>, e: &ast::Expr) -> bool {
        e.syntax()
            .descendants()
            .filter_map(ast::NameExpr::cast)
            .any(|n| self.is_row(b, &ast::Expr::NameExpr(n)))
    }

    /// `row.field`, or `row.relation.id`: a column of the row.
    fn column(&self, b: &Body<'_, '_>, e: &ast::Expr) -> Option<ColumnRef> {
        let ast::Expr::MemberExpr(m) = strip_parens(e) else {
            return None;
        };
        let name = m.name()?.text().to_owned();
        let recv = m.receiver()?;
        if self.is_row(b, &recv) {
            let i = self.entity.fields.iter().position(|f| f.name == name)?;
            let f = &self.entity.fields[i];
            if matches!(f.kind, FieldKind::ToMany { .. }) {
                return None;
            }
            return Some(ColumnRef {
                field: i,
                columns: f.columns(),
            });
        }
        // `row.relation.id`: the foreign key.
        if name == "id" {
            if let ast::Expr::MemberExpr(inner) = strip_parens(&recv) {
                let rel = inner.name()?.text().to_owned();
                if self.is_row(b, &inner.receiver()?) {
                    let i = self.entity.fields.iter().position(|f| f.name == rel)?;
                    if let FieldKind::ToOne { column, .. } = &self.entity.fields[i].kind {
                        return Some(ColumnRef {
                            field: i,
                            columns: vec![column.clone()],
                        });
                    }
                }
            }
        }
        None
    }

    /// The codec of the column `r`'s field (a to-one compared by `.id`:
    /// the target's id codec).
    fn column_codec(&self, b: &mut Body<'_, '_>, r: &ColumnRef) -> Expr {
        let f = &self.entity.fields[r.field];
        match &f.kind {
            FieldKind::ToOne { target, .. } => id_codec(b, *target),
            _ => col_ref(b, self.entity.decl, r.field),
        }
    }

    /// A value operand: the row must not appear in it.
    fn value(&mut self, b: &mut Body<'_, '_>, e: &ast::Expr) -> Result<Expr, String> {
        if self.mentions_row(b, e) {
            return Err(
                "only a field of the row can be compared; computing with it can't be translated to SQL"
                    .to_owned(),
            );
        }
        Ok(b.expr(e))
    }

    /// Translates a filter condition into a SQL predicate.
    ///
    /// `and`/`or` (or `&&`/`||`) nest flatly, `not` negates, comparisons go
    /// through [`Query::compare`], `contains`-like calls through
    /// [`Query::call_predicate`], and a `Bool` field on its own means `= true`.
    /// The literal `true` is the empty conjunction and `false` the empty
    /// disjunction.
    ///
    /// # Errors
    ///
    /// An explanation for anything that can't be translated to SQL, which
    /// the caller records as a build error.
    fn predicate(&mut self, b: &mut Body<'_, '_>, e: &ast::Expr) -> Result<Predicate, String> {
        let e = strip_parens(e);
        if let Some(v) = bool_literal(&e) {
            return Ok(if v {
                Predicate::And(Vec::new())
            } else {
                Predicate::Or(Vec::new())
            });
        }
        match &e {
            ast::Expr::BinaryExpr(be) => {
                let op = be.op().map(|t| t.text().to_owned()).unwrap_or_default();
                let (Some(l), Some(r)) = (be.lhs(), be.rhs()) else {
                    return Err("incomplete expression".to_owned());
                };
                match op.as_str() {
                    "and" | "&&" | "or" | "||" => {
                        let a = self.predicate(b, &l)?;
                        let c = self.predicate(b, &r)?;
                        let and = op == "and" || op == "&&";
                        let mut parts = Vec::new();
                        for p in [a, c] {
                            match (p, and) {
                                (Predicate::And(xs), true) | (Predicate::Or(xs), false)
                                    if !xs.is_empty() =>
                                {
                                    parts.extend(xs);
                                }
                                (p, _) => parts.push(p),
                            }
                        }
                        Ok(if and {
                            Predicate::And(parts)
                        } else {
                            Predicate::Or(parts)
                        })
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => self.compare(b, &op, &l, &r),
                    _ => Err(format!("`{op}` can't be translated to SQL")),
                }
            }
            ast::Expr::PrefixExpr(p) => {
                let op = p.op().map(|t| t.text().to_owned()).unwrap_or_default();
                match (op.as_str(), p.operand()) {
                    ("not" | "!", Some(inner)) => {
                        Ok(Predicate::Not(Box::new(self.predicate(b, &inner)?)))
                    }
                    _ => Err(format!("`{op}` can't be translated to SQL")),
                }
            }
            ast::Expr::CallExpr(c) => self.call_predicate(b, c),
            _ => {
                // A Bool field on its own: `note.isPinned`.
                if let Some(r) = self.column(b, &e) {
                    if let FieldKind::Column { col: Col::Bool, .. } =
                        &self.entity.fields[r.field].kind
                    {
                        let p = self.param(dbc("bool"), Expr::Bool(true), 1);
                        return Ok(Predicate::Compare {
                            left: Operand::Column(r.columns[0].clone()),
                            op: CmpOp::Eq,
                            right: Operand::Param(p),
                        });
                    }
                }
                Err("a query condition must compare fields of the row".to_owned())
            }
        }
    }

    /// Translates the comparison `l op r`, where at least one side is a column
    /// of the row; when the column is on the right the operator is mirrored so
    /// the column comes first. The other side becomes a parameter.
    ///
    /// # Errors
    ///
    /// When neither side is a column, or when two `Money` fields (two columns
    /// each) are compared with each other.
    fn compare(
        &mut self,
        b: &mut Body<'_, '_>,
        op: &str,
        l: &ast::Expr,
        r: &ast::Expr,
    ) -> Result<Predicate, String> {
        let mut cmp = match op {
            "==" => CmpOp::Eq,
            "!=" => CmpOp::Ne,
            "<" => CmpOp::Lt,
            "<=" => CmpOp::Le,
            ">" => CmpOp::Gt,
            _ => CmpOp::Ge,
        };
        let (lc, rc) = (self.column(b, l), self.column(b, r));
        let (col, other) = match (lc, rc) {
            (Some(c), None) => (c, r.clone()),
            (None, Some(c)) => {
                cmp = match cmp {
                    CmpOp::Lt => CmpOp::Gt,
                    CmpOp::Le => CmpOp::Ge,
                    CmpOp::Gt => CmpOp::Lt,
                    CmpOp::Ge => CmpOp::Le,
                    o => o,
                };
                (c, l.clone())
            }
            (Some(a), Some(c)) => {
                if a.columns.len() != 1 || c.columns.len() != 1 {
                    return Err("money fields can't be compared with each other in SQL".to_owned());
                }
                return Ok(Predicate::Compare {
                    left: Operand::Column(a.columns[0].clone()),
                    op: cmp,
                    right: Operand::Column(c.columns[0].clone()),
                });
            }
            (None, None) => {
                return Err("a comparison in a query must involve a field of the row".to_owned());
            }
        };
        let column = Operand::Column(col.columns[0].clone());
        if is_nil(&other) {
            return match cmp {
                CmpOp::Eq => Ok(Predicate::IsNull(column)),
                CmpOp::Ne => Ok(Predicate::IsNotNull(column)),
                _ => Err("`nil` can only be compared with `==` or `!=`".to_owned()),
            };
        }
        let other_ty = b.ty(other.syntax());
        let value = self.value(b, &other)?;
        let codec = self.column_codec(b, &col);
        let codec = if matches!(other_ty, Type::Optional(_)) {
            call(dbc("optional"), vec![codec])
        } else {
            codec
        };
        let p = self.param(codec, value, col.columns.len());
        let base = if col.columns.len() == 2 {
            // Money: the amounts compare, and the currencies must match.
            Predicate::And(vec![
                Predicate::Compare {
                    left: column.clone(),
                    op: cmp,
                    right: Operand::Param(p),
                },
                Predicate::Compare {
                    left: Operand::Column(col.columns[1].clone()),
                    op: if cmp == CmpOp::Ne {
                        CmpOp::Ne
                    } else {
                        CmpOp::Eq
                    },
                    right: Operand::Param(p + 1),
                },
            ])
        } else {
            Predicate::Compare {
                left: column.clone(),
                op: cmp,
                right: Operand::Param(p),
            }
        };
        if matches!(other_ty, Type::Optional(_)) && matches!(cmp, CmpOp::Eq | CmpOp::Ne) {
            // `x == value` with an optional value: nil equals nil, as in K.
            let both_nil = Predicate::And(vec![
                Predicate::IsNull(column),
                Predicate::IsNull(Operand::Param(p)),
            ]);
            let eq = Predicate::Or(vec![
                Predicate::Compare {
                    left: Operand::Column(col.columns[0].clone()),
                    op: CmpOp::Eq,
                    right: Operand::Param(p),
                },
                both_nil,
            ]);
            return Ok(if cmp == CmpOp::Eq {
                eq
            } else {
                Predicate::Not(Box::new(eq))
            });
        }
        Ok(base)
    }

    /// Translates `field.contains(x)`, `startsWith`/`hasPrefix` and
    /// `endsWith`/`hasSuffix` on a text field into a `LIKE` predicate.
    ///
    /// # Errors
    ///
    /// For any other method, a non-text field, or a call without an argument.
    fn call_predicate(
        &mut self,
        b: &mut Body<'_, '_>,
        c: &ast::CallExpr,
    ) -> Result<Predicate, String> {
        let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
            return Err("this call can't be translated to SQL".to_owned());
        };
        let name = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
        let recv = m.receiver().ok_or("incomplete call")?;
        let arg = c
            .arg_list()
            .and_then(|l| l.args().next())
            .and_then(|a| a.value())
            .ok_or("a call without an argument can't be translated to SQL")?;
        let kind = match name.as_str() {
            "contains" => 0,
            "startsWith" | "hasPrefix" => 1,
            "endsWith" | "hasSuffix" => 2,
            _ => return Err(format!("`{name}` can't be translated to SQL")),
        };
        if let Some(col) = self.column(b, &recv) {
            let f = &self.entity.fields[col.field];
            if !matches!(
                &f.kind,
                FieldKind::Column {
                    col: Col::Text | Col::Email,
                    ..
                }
            ) {
                return Err(format!(
                    "`{name}` on a non-text field can't be translated to SQL"
                ));
            }
            let text = self.value(b, &arg)?;
            let pattern = call(
                s_member("likePattern"),
                vec![Expr::Num(f64::from(kind)), text],
            );
            let p = self.param(dbc("text"), pattern, 1);
            return Ok(Predicate::Like {
                column: col.columns[0].clone(),
                kind: match kind {
                    0 => Like::Contains,
                    1 => Like::StartsWith,
                    _ => Like::EndsWith,
                },
                param: p,
            });
        }
        if kind == 0 {
            if let Some(col) = self.column(b, &arg) {
                if col.columns.len() != 1 {
                    return Err("a money field can't be looked up in a list".to_owned());
                }
                let list = self.value(b, &recv)?;
                let codec = self.column_codec(b, &col);
                let value = call(s_member("dbInList"), vec![codec, list]);
                let p = self.param(dbc("text"), value, 1);
                return Ok(Predicate::In {
                    operand: Operand::Column(col.columns[0].clone()),
                    list: p,
                });
            }
        }
        Err(format!(
            "`{name}` must be called on a field of the row (or a list, with a field as argument)"
        ))
    }

    /// The column a sort lambda names.
    fn sort_key(&mut self, b: &Body<'_, '_>, l: &ast::LambdaExpr) -> Result<String, String> {
        self.row = row_param(l).map(|t| t.text_range());
        let v = lambda_value(l).ok_or("a sort key is one expression")?;
        let c = self
            .column(b, &v)
            .ok_or("a sort key must be a field of the row")?;
        Ok(c.columns[0].clone())
    }

    /// The count of `limit(n)` or `offset(n)`: an integer literal is inlined
    /// into the SQL, anything else becomes an `Int` parameter.
    ///
    /// # Errors
    ///
    /// When the call has no argument.
    fn count(&mut self, b: &mut Body<'_, '_>, c: &ast::CallExpr) -> Result<Count, String> {
        let arg = c
            .arg_list()
            .and_then(|l| l.args().next())
            .and_then(|a| a.value())
            .ok_or("`limit` and `offset` take a count")?;
        if let ast::Expr::Literal(l) = strip_parens(&arg) {
            if let Some(n) = l
                .token()
                .and_then(|t| t.text().replace('_', "").parse::<u64>().ok())
            {
                return Ok(Count::Const(n));
            }
        }
        let v = self.value(b, &arg)?;
        Ok(Count::Param(self.param(dbc("int"), v, 1)))
    }

    /// The assignments of an `update { row => … }` lambda.
    fn assignments(
        &mut self,
        b: &mut Body<'_, '_>,
        l: &ast::LambdaExpr,
    ) -> Result<Vec<(String, Operand)>, String> {
        self.row = row_param(l).map(|t| t.text_range());
        let mut set = Vec::new();
        for s in l.statements() {
            let ast::Stmt::Assign(a) = s else {
                return Err("an update lambda may only assign fields".to_owned());
            };
            let mut exprs = a.syntax().children().filter_map(ast::Expr::cast);
            let (Some(place), Some(value)) = (exprs.next(), exprs.next()) else {
                return Err("incomplete assignment".to_owned());
            };
            let op = a
                .syntax()
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| {
                    matches!(
                        t.kind(),
                        SyntaxKind::Eq | SyntaxKind::PlusEq | SyntaxKind::MinusEq
                    )
                })
                .map(|t| t.text().to_owned())
                .ok_or("only `=`, `+=` and `-=` can be translated to SQL")?;
            // Only `row.field = …`: `row.relation.id = …` would change a
            // foreign key through the related entity.
            let direct = matches!(strip_parens(&place), ast::Expr::MemberExpr(ref m)
                if m.receiver().is_some_and(|r| self.is_row(b, &r)));
            let target = self
                .column(b, &place)
                .filter(|_| direct)
                .ok_or("an update assigns fields of the row")?;
            if Some(target.field) == self.entity.id {
                return Err("an update can't change the id".to_owned());
            }
            if op != "=" {
                let f = &self.entity.fields[target.field];
                if !matches!(
                    &f.kind,
                    FieldKind::Column {
                        col: Col::Int,
                        nullable: false,
                        ..
                    }
                ) {
                    return Err(format!("`{op}` is only translated for Int fields"));
                }
                let v = self.value(b, &value)?;
                let p = self.param(dbc("int"), v, 1);
                set.push((
                    target.columns[0].clone(),
                    Operand::Arith {
                        left: Box::new(Operand::Column(target.columns[0].clone())),
                        op: if op == "+=" {
                            ArithOp::Add
                        } else {
                            ArithOp::Sub
                        },
                        right: Box::new(Operand::Param(p)),
                    },
                ));
                continue;
            }
            if let Some(src) = self.column(b, &value) {
                if src.columns.len() != target.columns.len() {
                    return Err("a field can only be copied to a field of the same type".to_owned());
                }
                for (t, s) in target.columns.iter().zip(src.columns) {
                    set.push((t.clone(), Operand::Column(s)));
                }
                continue;
            }
            let ty = b.ty(value.syntax());
            let v = if is_nil(&value) {
                Expr::Null
            } else {
                self.value(b, &value)?
            };
            let f = self.entity.fields[target.field].clone();
            let codec = field_codec(b, self.entity, &f);
            let codec = if matches!(ty, Type::Optional(_))
                && !matches!(
                    f.kind,
                    FieldKind::Column { nullable: true, .. }
                        | FieldKind::ToOne { nullable: true, .. }
                ) {
                call(dbc("optional"), vec![codec])
            } else {
                codec
            };
            let p = self.param(codec, v, target.columns.len());
            for (k, t) in target.columns.iter().enumerate() {
                set.push((t.clone(), Operand::Param(p + u32::try_from(k).unwrap_or(0))));
            }
        }
        if set.is_empty() {
            return Err("an update must assign at least one field".to_owned());
        }
        Ok(set)
    }
}

/// One step of a query chain: `.name(args)`.
struct Step {
    /// The method name, `filter`.
    name: String,
    /// The whole call, for its arguments and its range.
    call: ast::CallExpr,
}

/// Splits `root.a(…).b(…)` into its root and steps, root first.
fn chain(e: &ast::Expr) -> Option<(ast::NameExpr, Vec<Step>)> {
    let mut steps = Vec::new();
    let mut cur = strip_parens(e);
    loop {
        match cur {
            ast::Expr::CallExpr(c) => {
                let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
                    return None;
                };
                steps.push(Step {
                    name: m.name()?.text().to_owned(),
                    call: c.clone(),
                });
                cur = strip_parens(&m.receiver()?);
            }
            ast::Expr::NameExpr(n) => {
                steps.reverse();
                return Some((n, steps));
            }
            _ => return None,
        }
    }
}

/// The repository whose body `b` is lowering, and its entity.
fn current_repository(b: &Body<'_, '_>) -> Option<(DeclId, DeclId)> {
    let d = b.owner?;
    if b.g.view.decl(d).named_kind() != Some(DeclarationKind::Repository) {
        return None;
    }
    Some((d, repository_entity(b.g, d)?))
}

/// The JavaScript of the request's database inside a repository.
fn this_db() -> Expr {
    member(Expr::This, "$db")
}

/// Lowers an awaited expression that runs SQL, or returns `None` for any
/// other expression:
///
/// * a chain on the repository's collection (`notes.filter { … }…`);
/// * an inherited write called on a repository (`accounts.insert(a)`).
///
/// The result is a promise; the caller awaits it.
pub(crate) fn awaited(b: &mut Body<'_, '_>, e: &ast::Expr) -> Option<Expr> {
    let e = strip_parens(e);
    if let ast::Expr::CallExpr(c) = &e {
        if let Some(x) = repository_write(b, c) {
            return Some(x);
        }
    }
    let (root, steps) = chain(&e)?;
    let (_, entity_decl) = current_repository(b)?;
    let g = b.g;
    let name = root.name()?;
    if b.g.resolution(b.file, root.syntax().text_range()).is_some() {
        return None;
    }
    let query_decl = g.view.lookup("k.db.Query");
    let root_ty = b.ty(root.syntax());
    if root_ty.decl() != query_decl || query_decl.is_none() {
        return None;
    }
    let _ = name;
    let model = entity(g, entity_decl)?.clone();
    Some(lower_chain(b, &e, &model, &steps))
}

/// An inherited write on a repository value: `repo.insert(x)`,
/// `repo.insert(all: xs)`, `repo.update(x)`, `repo.delete(x)`.
fn repository_write(b: &mut Body<'_, '_>, c: &ast::CallExpr) -> Option<Expr> {
    let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
        return None;
    };
    let name = m.name()?.text().to_owned();
    if !matches!(name.as_str(), "insert" | "update" | "delete") {
        return None;
    }
    let recv = m.receiver()?;
    let rty = b.ty(recv.syntax());
    let d = rty.decl()?;
    if b.g.view.decl(d).named_kind() != Some(DeclarationKind::Repository) {
        return None;
    }
    let arg = c.arg_list()?.args().next()?;
    if matches!(arg.value(), Some(ast::Expr::LambdaExpr(_))) {
        return None;
    }
    let label = arg.label().map(|t| t.text().to_owned());
    let value = arg.value()?;
    let method = match (name.as_str(), label.as_deref()) {
        ("insert", Some("all")) => "$insertAll",
        ("insert", _) => "$insert",
        ("update", _) => "$update",
        _ => "$delete",
    };
    let r = b.expr(&recv);
    let v = b.expr(&value);
    Some(call(member(r, method), vec![b.c_expr(), v]))
}

/// Lowers a query chain on entity `e` into one registered statement and the
/// runtime call that runs it.
///
/// `filter`, the sorts, `limit`, `offset` and `including` build the query;
/// the terminal step (`first`, `single`, `count`, `list`, `delete`,
/// `update`, `insert`) decides the statement and the result. A step that
/// can't be translated is reported against `whole`: its explanation is
/// recorded as a build error ([`fail`]) and the chain lowers to a stand-in.
#[allow(clippy::too_many_lines)]
fn lower_chain(b: &mut Body<'_, '_>, whole: &ast::Expr, e: &EntityModel, steps: &[Step]) -> Expr {
    let g = b.g;
    let Some((_, row_fn, _)) = entity_helpers(b, e.decl) else {
        return fail(b, whole.syntax(), "the entity has no table");
    };
    let mut q = Query {
        entity: e,
        slots: Vec::new(),
        next: 1,
        row: None,
    };
    let mut filters = Vec::new();
    let mut order = Vec::new();
    let mut limit = None;
    let mut offset = None;
    let mut includes: Vec<(String, ast::CallExpr)> = Vec::new();
    let last = steps.len().saturating_sub(1);
    for (i, s) in steps.iter().enumerate() {
        let terminal = i == last;
        let result: Result<(), String> = (|| {
            match s.name.as_str() {
                "filter" => {
                    let l = lambda_arg(&s.call).ok_or("`filter` takes a lambda")?;
                    q.row = row_param(&l).map(|t| t.text_range());
                    let v = lambda_value(&l).ok_or("a filter is one condition")?;
                    filters.push(q.predicate(b, &v)?);
                }
                "sortedBy" | "sortedByDescending" | "thenBy" | "thenByDescending" => {
                    let l = lambda_arg(&s.call).ok_or("a sort takes a lambda")?;
                    let column = q.sort_key(b, &l)?;
                    if s.name.starts_with("sorted") {
                        order.clear();
                    }
                    order.push(Order {
                        column,
                        descending: s.name.ends_with("Descending"),
                    });
                }
                "limit" => limit = Some(q.count(b, &s.call)?),
                "offset" => offset = Some(q.count(b, &s.call)?),
                "including" => {
                    let l = lambda_arg(&s.call).ok_or("`including` takes a lambda")?;
                    q.row = row_param(&l).map(|t| t.text_range());
                    let v = lambda_value(&l).ok_or("`including` names one relation")?;
                    let ast::Expr::MemberExpr(m) = strip_parens(&v) else {
                        return Err("`including` names a relation of the row".to_owned());
                    };
                    let field = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
                    let is_rel = m.receiver().is_some_and(|r| q.is_row(b, &r))
                        && e.field(&field).is_some_and(|f| {
                            matches!(f.kind, FieldKind::ToMany { .. } | FieldKind::ToOne { .. })
                        });
                    if !is_rel {
                        return Err(format!("`{field}` is not a relation of the entity"));
                    }
                    includes.push((field, s.call.clone()));
                }
                "first" | "single" if terminal => {
                    if let Some(l) = lambda_arg(&s.call) {
                        q.row = row_param(&l).map(|t| t.text_range());
                        let v = lambda_value(&l).ok_or("a condition is one expression")?;
                        filters.push(q.predicate(b, &v)?);
                    }
                }
                "count" | "list" | "delete" | "update" | "insert" if terminal => {}
                other => return Err(format!("`{other}` can't be translated to SQL here")),
            }
            Ok(())
        })();
        if let Err(why) = result {
            return fail(b, s.call.syntax(), &why);
        }
    }
    let filter = match filters.len() {
        0 => None,
        1 => filters.pop(),
        _ => Some(Predicate::And(filters)),
    };
    let terminal = steps.last().map(|s| (s.name.as_str(), &s.call));
    let entity_name = g.view.decl(e.decl).name.clone();
    let select = |limit: Option<Count>| Select {
        table: e.table.clone(),
        columns: e.select_columns(),
        filter: filter.clone(),
        order: order.clone(),
        limit,
        offset: offset.clone(),
    };
    match terminal {
        Some(("count", _)) => {
            let st = Statement::Count(Select {
                order: Vec::new(),
                ..select(limit.clone())
            });
            let n = g.db.state().register(&st);
            call(
                s_member("dbCount"),
                vec![this_db(), Expr::Num(f64::from(n)), q.params_expr()],
            )
        }
        Some(("delete", c)) if c.arg_list().is_none_or(|l| l.args().next().is_none()) => {
            let st = Statement::Delete {
                table: e.table.clone(),
                filter,
            };
            let n = g.db.state().register(&st);
            call(
                s_member("dbWrite"),
                vec![
                    this_db(),
                    Expr::Num(f64::from(n)),
                    q.params_expr(),
                    Expr::Null,
                ],
            )
        }
        Some(("update", c)) if lambda_arg(c).is_some() => {
            let l = lambda_arg(c).unwrap_or_else(|| unreachable!());
            let set = match q.assignments(b, &l) {
                Ok(s) => s,
                Err(why) => return fail(b, c.syntax(), &why),
            };
            let st = Statement::Update {
                table: e.table.clone(),
                set,
                filter,
            };
            let n = g.db.state().register(&st);
            call(
                s_member("dbWrite"),
                vec![
                    this_db(),
                    Expr::Num(f64::from(n)),
                    q.params_expr(),
                    unique_cases(e),
                ],
            )
        }
        Some(("insert" | "update" | "delete", c)) => {
            // The entity forms on the collection: the inherited writes.
            if steps.len() != 1 {
                return fail(
                    b,
                    c.syntax(),
                    "writes of an entity are called on the collection itself",
                );
            }
            let Some(arg) = c.arg_list().and_then(|l| l.args().next()) else {
                return fail(b, c.syntax(), "a write takes an entity");
            };
            let method = match (
                terminal.map(|t| t.0),
                arg.label().as_ref().map(SyntaxToken::text),
            ) {
                (Some("insert"), Some("all")) => "$insertAll",
                (Some("insert"), _) => "$insert",
                (Some("update"), _) => "$update",
                _ => "$delete",
            };
            let v = arg.value().map_or(Expr::Undefined, |v| b.expr(&v));
            call(member(Expr::This, method), vec![b.c_expr(), v])
        }
        _ => {
            let (kind, limit) = match terminal.map(|t| t.0) {
                Some("first") => (1.0, Some(Count::Const(1))),
                Some("single") => (2.0, Some(Count::Const(2))),
                _ => (0.0, limit.clone()),
            };
            let st = Statement::Select(select(limit));
            let n = g.db.state().register(&st);
            let mut rels = Vec::new();
            for (field, c) in &includes {
                match include_spec(b, e, field) {
                    Ok(x) => rels.push(x),
                    Err(why) => return fail(b, c.syntax(), &why),
                }
            }
            call(
                s_member("dbQuery"),
                vec![
                    this_db(),
                    Expr::Num(kind),
                    Expr::Num(f64::from(n)),
                    q.params_expr(),
                    id(row_fn),
                    Expr::Array(rels),
                    js::str(entity_name),
                ],
            )
        }
    }
}

/// The `Relation` object of `dbInclude` for `e.field`.
fn include_spec(b: &mut Body<'_, '_>, e: &EntityModel, field: &str) -> Result<Expr, String> {
    let g = b.g;
    let f = e.field(field).ok_or("no such relation")?.clone();
    let (many, target, key_column, inverse) = match &f.kind {
        FieldKind::ToMany { target, inverse } => {
            let t = entity(g, *target).ok_or("the related entity has no table")?;
            let inv = t.field(inverse).ok_or_else(|| {
                format!(
                    "`{}` has no to-one relation back",
                    g.view.decl(*target).name
                )
            })?;
            let FieldKind::ToOne { column, .. } = &inv.kind else {
                return Err("the inverse relation is not to-one".to_owned());
            };
            (true, *target, column.clone(), Some(inverse.clone()))
        }
        FieldKind::ToOne { target, .. } => {
            let t = entity(g, *target).ok_or("the related entity has no table")?;
            let idf = t.id_field().ok_or("the related entity has no @Id")?;
            (false, *target, idf.columns()[0].clone(), None)
        }
        FieldKind::Column { .. } => return Err("not a relation".to_owned()),
    };
    let t = entity(g, target)
        .ok_or("the related entity has no table")?
        .clone();
    let (_, row, _) = entity_helpers(b, target).ok_or("the related entity has no table")?;
    let st = Statement::Select(Select {
        table: t.table.clone(),
        columns: t.select_columns(),
        filter: Some(Predicate::In {
            operand: Operand::Column(key_column),
            list: 1,
        }),
        order: Vec::new(),
        limit: None,
        offset: None,
    });
    let n = g.db.state().register(&st);
    let key = if many {
        id_codec(b, e.decl)
    } else {
        id_codec(b, target)
    };
    let mut props = vec![
        Prop::KeyValue("many".into(), Expr::Bool(many)),
        Prop::KeyValue("field".into(), js::str(field)),
        Prop::KeyValue("statement".into(), Expr::Num(f64::from(n))),
        Prop::KeyValue("decode".into(), id(row)),
        Prop::KeyValue("key".into(), key),
    ];
    if let Some(inv) = inverse {
        props.push(Prop::KeyValue("inverse".into(), js::str(inv)));
    }
    Ok(Expr::Object(props, false))
}

/// The `@Unique` fields of the entity written by the first `try` of a
/// `do` body, when that `try` is a repository write (`try await
/// accounts.insert(a)`): the cases of its `UniqueViolation`, in tag order.
pub(crate) fn unique_fields_of_try(b: &Body<'_, '_>, body: &ast::Block) -> Option<Vec<String>> {
    let g = b.g;
    for n in body.syntax().descendants() {
        if n.kind() != SyntaxKind::TryExpr {
            continue;
        }
        let mut inner = n.children().find_map(ast::Expr::cast)?;
        while let ast::Expr::AwaitExpr(a) = &inner {
            inner = a.syntax().children().find_map(ast::Expr::cast)?;
        }
        let ast::Expr::CallExpr(c) = strip_parens(&inner) else {
            continue;
        };
        let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
            continue;
        };
        if !m
            .name()
            .is_some_and(|t| matches!(t.text(), "insert" | "update"))
        {
            continue;
        }
        let recv = m.receiver()?;
        let rty = b.ty(recv.syntax());
        let entity_decl = match rty.decl() {
            Some(d) if g.view.decl(d).named_kind() == Some(DeclarationKind::Repository) => {
                repository_entity(g, d)
            }
            Some(d) if Some(d) == g.view.lookup("k.db.Query") => {
                rty.args().first().and_then(Type::decl)
            }
            _ => None,
        };
        let e = entity(g, entity_decl?)?;
        return Some(e.unique_fields().iter().map(|f| f.name.clone()).collect());
    }
    None
}

/// The tag of `catch .name` among `fields`, if the pattern is that case.
pub(crate) fn unique_case_tag(p: &ast::Pattern, fields: &[String]) -> Option<usize> {
    let text = p.syntax().text().to_string();
    let name = text.trim().strip_prefix('.')?;
    fields.iter().position(|f| f == name)
}

// ----------------------------------------------------------- repositories

/// A repository class (db.md §7): `$db` is the request's `Database`; the
/// user's functions are lowered with their queries; the inherited writes
/// are `$insert`, `$insertAll`, `$update` and `$delete`, which call sites
/// on the repository are lowered to ([`awaited`]).
#[allow(clippy::too_many_lines)]
pub(crate) fn repository(b: &mut Body<'_, '_>, d: DeclId) -> Vec<Stmt> {
    let g = b.g;
    let info = g.view.decl(d).clone();
    b.owner = Some(d);
    b.self_mode = SelfMode::This;
    let mut ctor = vec![Stmt::Expr(js::assign(
        member(Expr::This, "$db"),
        member(id("$deps"), "Database"),
    ))];
    let mut methods = Vec::new();
    for &m in &info.members {
        let mi = g.view.member(m).clone();
        let node = g.member_node(m);
        match mi.kind {
            MemberKind::Field if !mi.is_static && mi.has_annotation("Inject") => {
                let tn = mi
                    .ty
                    .as_ref()
                    .and_then(Type::decl)
                    .map(|x| g.name(x))
                    .unwrap_or_default();
                ctor.push(Stmt::Expr(js::assign(
                    member(Expr::This, mi.name.clone()),
                    member(id("$deps"), tn),
                )));
            }
            MemberKind::Func => {
                b.self_mode = SelfMode::This;
                methods.push(crate::decl::function(
                    b,
                    g.member_name(m),
                    node.as_ref(),
                    mi.signature.as_ref(),
                    Vec::new(),
                ));
            }
            _ => {}
        }
    }
    if let Some(e) = repository_entity(g, d).and_then(|e| entity(g, e)).cloned() {
        methods.extend(inherited_writes(b, &e));
    }
    let mut all = vec![Function {
        kind: MethodKind::Constructor,
        params: vec![Param {
            name: "$deps".into(),
            default: Some(Expr::Object(Vec::new(), false)),
            rest: false,
        }],
        body: js::Body::Block(ctor),
        ..Function::default()
    }];
    all.extend(methods);
    vec![Stmt::Class(
        Class {
            name: g.name(d),
            extends: None,
            methods: all,
        },
        true,
    )]
}

/// An `async` plain method named `name` with block body `body`.
fn method(name: &str, params: &[&str], body: Vec<Stmt>) -> Function {
    Function {
        name: name.to_owned(),
        params: params.iter().map(|p| Param::new(*p)).collect(),
        body: js::Body::Block(body),
        is_async: true,
        kind: MethodKind::Plain,
    }
}

/// `[c, v]`: a codec and the value it encodes, as `dbParams` takes them.
fn pair(c: Expr, v: Expr) -> Expr {
    Expr::Array(vec![c, v])
}

/// `$insert`, `$insertAll`, `$update` and `$delete` for entity `e`.
fn inherited_writes(b: &mut Body<'_, '_>, e: &EntityModel) -> Vec<Function> {
    let g = b.g;
    let Some((_, _, values)) = entity_helpers(b, e.decl) else {
        return Vec::new();
    };
    let Some(id_index) = e.id else {
        return Vec::new();
    };
    let columns = e.select_columns();
    let unique = unique_cases(e);
    let mut out = Vec::new();
    // insert
    let one = g.db.state().register(&Statement::Insert {
        table: e.table.clone(),
        columns: columns.clone(),
        rows: 1,
    });
    let params = |v: Expr| {
        js::arrow_block(
            vec![],
            vec![
                Stmt::Const("out".into(), Expr::Array(Vec::new())),
                Stmt::Expr(call(id(values.clone()), vec![v, id("out")])),
                Stmt::Return(Some(id("out"))),
            ],
            false,
        )
    };
    out.push(method(
        "$insert",
        &["$c", "e"],
        vec![Stmt::Expr(Expr::Await(Box::new(call(
            s_member("dbWrite"),
            vec![
                this_db(),
                Expr::Num(f64::from(one)),
                call(params(id("e")), vec![]),
                unique.clone(),
            ],
        ))))],
    ));
    // insert(all:)
    let batches: Vec<Expr> = INSERT_BATCHES
        .iter()
        .map(|&rows| {
            let n = g.db.state().register(&Statement::Insert {
                table: e.table.clone(),
                columns: columns.clone(),
                rows,
            });
            Expr::Array(vec![Expr::Num(f64::from(rows)), Expr::Num(f64::from(n))])
        })
        .collect();
    out.push(method(
        "$insertAll",
        &["$c", "es"],
        vec![Stmt::Expr(Expr::Await(Box::new(call(
            s_member("dbInsertAll"),
            vec![
                this_db(),
                Expr::Array(batches),
                id(values.clone()),
                Expr::Array(vec![Expr::Spread(Box::new(id("es")))]),
                unique.clone(),
            ],
        ))))],
    ));
    // update(entity): every column but the id; @Version adds a condition.
    let id_field = &e.fields[id_index];
    let id_column = id_field.columns()[0].clone();
    let mut next = 1u32;
    let mut set = Vec::new();
    let mut slots = Vec::new();
    for (i, f) in e.fields.iter().enumerate() {
        if i == id_index || matches!(f.kind, FieldKind::ToMany { .. }) {
            continue;
        }
        let cols = f.columns();
        if Some(i) == e.version {
            set.push((
                cols[0].clone(),
                Operand::Arith {
                    left: Box::new(Operand::Column(cols[0].clone())),
                    op: ArithOp::Add,
                    right: Box::new(Operand::Param(next)),
                },
            ));
            slots.push(pair(dbc("int"), Expr::Num(1.0)));
            next += 1;
            continue;
        }
        for (k, c) in cols.iter().enumerate() {
            set.push((
                c.clone(),
                Operand::Param(next + u32::try_from(k).unwrap_or(0)),
            ));
        }
        next += u32::try_from(cols.len()).unwrap_or(1);
        slots.push(pair(col_ref(b, e.decl, i), member(id("e"), f.name.clone())));
    }
    let mut filter = vec![Predicate::Compare {
        left: Operand::Column(id_column.clone()),
        op: CmpOp::Eq,
        right: Operand::Param(next),
    }];
    slots.push(pair(
        col_ref(b, e.decl, id_index),
        member(id("e"), id_field.name.clone()),
    ));
    next += 1;
    if let Some(v) = e.version {
        let vf = &e.fields[v];
        filter.push(Predicate::Compare {
            left: Operand::Column(vf.columns()[0].clone()),
            op: CmpOp::Eq,
            right: Operand::Param(next),
        });
        slots.push(pair(dbc("int"), member(id("e"), vf.name.clone())));
    }
    if set.is_empty() {
        // Only an id: an update has nothing to write.
        out.push(method(
            "$update",
            &["$c", "e"],
            vec![Stmt::Return(Some(if e.version.is_some() {
                Expr::Bool(false)
            } else {
                Expr::Undefined
            }))],
        ));
    } else {
        let st = Statement::Update {
            table: e.table.clone(),
            set,
            filter: Some(if filter.len() == 1 {
                filter.remove(0)
            } else {
                Predicate::And(filter)
            }),
        };
        let n = g.db.state().register(&st);
        let write = Expr::Await(Box::new(call(
            s_member("dbWrite"),
            vec![
                this_db(),
                Expr::Num(f64::from(n)),
                call(s_member("dbParams"), vec![Expr::Array(slots)]),
                unique.clone(),
            ],
        )));
        out.push(method(
            "$update",
            &["$c", "e"],
            vec![if e.version.is_some() {
                Stmt::Return(Some(bin("===", write, Expr::Num(1.0))))
            } else {
                Stmt::Expr(write)
            }],
        ));
    }
    // delete(entity)
    let st = Statement::Delete {
        table: e.table.clone(),
        filter: Some(Predicate::Compare {
            left: Operand::Column(id_column),
            op: CmpOp::Eq,
            right: Operand::Param(1),
        }),
    };
    let n = g.db.state().register(&st);
    out.push(method(
        "$delete",
        &["$c", "e"],
        vec![Stmt::Expr(Expr::Await(Box::new(call(
            s_member("dbWrite"),
            vec![
                this_db(),
                Expr::Num(f64::from(n)),
                call(
                    s_member("dbParams"),
                    vec![Expr::Array(vec![pair(
                        col_ref(b, e.decl, id_index),
                        member(id("e"), id_field.name.clone()),
                    )])],
                ),
                Expr::Null,
            ],
        ))))],
    ));
    out
}

// ------------------------------------------------------------- migrations

/// The plain string of an argument, if it is one.
fn string_arg(e: &ast::Expr) -> Option<String> {
    match strip_parens(e) {
        ast::Expr::StringLit(s) => s.plain_value(),
        _ => None,
    }
}

/// The labeled arguments of a call, by label (`_` for unlabeled).
fn args(c: &ast::CallExpr) -> Vec<(String, ast::Expr)> {
    c.arg_list()
        .map(|l| {
            l.args()
                .filter_map(|a| {
                    let label = a
                        .label()
                        .map_or_else(|| "_".to_owned(), |t| t.text().to_owned());
                    a.value().map(|v| (label, v))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The value of the argument labelled `label` of `c`, if present.
fn arg(c: &ast::CallExpr, label: &str) -> Option<ast::Expr> {
    args(c)
        .into_iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v)
}

/// The strings of a list literal of plain string literals; `None` if `e`
/// is not a list literal or an element isn't a plain string.
fn string_list(e: &ast::Expr) -> Option<Vec<String>> {
    match strip_parens(e) {
        ast::Expr::ListExpr(l) => l
            .syntax()
            .children()
            .filter_map(ast::Expr::cast)
            .map(|x| string_arg(&x))
            .collect(),
        _ => None,
    }
}

/// `.uuid`, `.enumeration(cases: [...])`.
fn column_type(e: &ast::Expr) -> Result<ColumnType, String> {
    let e = strip_parens(e);
    let (name, call_node) = match &e {
        ast::Expr::ImplicitMemberExpr(m) => (m.name().map(|t| t.text().to_owned()), None),
        ast::Expr::CallExpr(c) => match c.callee() {
            Some(ast::Expr::ImplicitMemberExpr(m)) => {
                (m.name().map(|t| t.text().to_owned()), Some(c.clone()))
            }
            _ => (None, None),
        },
        _ => (None, None),
    };
    Ok(match name.as_deref() {
        Some("uuid") => ColumnType::Uuid,
        Some("text") => ColumnType::Text,
        Some("integer") => ColumnType::Integer,
        Some("decimal") => ColumnType::Decimal,
        Some("boolean") => ColumnType::Boolean,
        Some("timestamp") => ColumnType::Timestamp,
        Some("money") => ColumnType::Money,
        Some("json") => ColumnType::Json,
        Some("enumeration") => {
            let cases = call_node
                .as_ref()
                .and_then(|c| arg(c, "cases"))
                .and_then(|l| string_list(&l))
                .ok_or("`.enumeration` takes `cases:` as a list of strings")?;
            ColumnType::Enumeration(cases)
        }
        _ => return Err(format!("`{}` is not a column type", excerpt(e.syntax()))),
    })
}

/// A `ColumnValue`: `.null`, `.text("x")`, `.integer(1)`, `.boolean(true)`,
/// `.decimal(…)`, `.uuid(…)`.
fn column_value(e: &ast::Expr) -> Result<Literal, String> {
    let e = strip_parens(e);
    let bad = || format!("`{}` is not a column value", excerpt(e.syntax()));
    match &e {
        ast::Expr::ImplicitMemberExpr(m) if m.name().is_some_and(|t| t.text() == "null") => {
            return Ok(Literal::Null);
        }
        ast::Expr::CallExpr(c) => {
            let Some(ast::Expr::ImplicitMemberExpr(m)) = c.callee() else {
                return Err(bad());
            };
            let name = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
            let v = c
                .arg_list()
                .and_then(|l| l.args().next())
                .and_then(|a| a.value())
                .ok_or_else(bad)?;
            let text = || -> Option<String> {
                string_arg(&v).or_else(|| {
                    v.syntax()
                        .descendants()
                        .filter_map(ast::StringLit::cast)
                        .find_map(|s| s.plain_value())
                })
            };
            let number = || -> Option<String> {
                let s = v.syntax().text().to_string().replace([' ', '_'], "");
                (!s.is_empty()
                    && s.trim_start_matches('-')
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b == b'.'))
                .then_some(s)
            };
            return match name.as_str() {
                "text" => text().map(Literal::Text).ok_or_else(bad),
                "uuid" => text().map(Literal::Uuid).ok_or_else(bad),
                "integer" => number()
                    .and_then(|n| n.parse().ok())
                    .map(Literal::Integer)
                    .ok_or_else(bad),
                "decimal" => number().or_else(text).map(Literal::Decimal).ok_or_else(bad),
                "boolean" => bool_literal(&v).map(Literal::Boolean).ok_or_else(bad),
                _ => Err(bad()),
            };
        }
        _ => {}
    }
    Err(bad())
}

/// Reads `table.column(name:type:)` / `addColumn` with its chained options.
fn column_def(c: &ast::CallExpr) -> Result<(String, ColumnDef), String> {
    let mut options = Vec::new();
    let mut cur = c.clone();
    loop {
        let Some(ast::Expr::MemberExpr(m)) = cur.callee() else {
            return Err("expected `table.column(…)`".to_owned());
        };
        let name = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
        match m.receiver().map(|r| strip_parens(&r)) {
            Some(ast::Expr::CallExpr(inner)) => {
                options.push((name, cur.clone()));
                cur = inner;
            }
            Some(ast::Expr::NameExpr(_)) => {
                let column = arg(&cur, "name")
                    .and_then(|v| string_arg(&v))
                    .ok_or("a column needs `name:` as a string")?;
                let ty = column_type(&arg(&cur, "type").ok_or("a column needs `type:`")?)?;
                let mut def = ColumnDef::new(column, ty);
                for (option, call_node) in options.iter().rev() {
                    let value = call_node
                        .arg_list()
                        .and_then(|l| l.args().next())
                        .and_then(|a| a.value());
                    let flag = || {
                        value
                            .as_ref()
                            .and_then(bool_literal)
                            .ok_or(format!("`{option}` takes `true` or `false`"))
                    };
                    match option.as_str() {
                        "primaryKey" => def.primary_key = flag()?,
                        "unique" => def.unique = flag()?,
                        "nullable" => def.nullable = flag()?,
                        "indexed" => def.indexed = flag()?,
                        "references" => {
                            def.references = Some(
                                arg(call_node, "table")
                                    .and_then(|v| string_arg(&v))
                                    .ok_or("`references` takes `table:` as a string")?,
                            );
                        }
                        "defaultValue" => {
                            def.default = Some(column_value(
                                value.as_ref().ok_or("`defaultValue` takes a value")?,
                            )?);
                        }
                        other => return Err(format!("`{other}` is not a column option")),
                    }
                }
                return Ok((name, def));
            }
            _ => return Err("expected `table.column(…)`".to_owned()),
        }
    }
}

/// The statements of a builder lambda `{ table => … }`.
fn table_steps(l: &ast::LambdaExpr) -> Result<Vec<ast::CallExpr>, String> {
    l.statements()
        .map(|s| match s {
            ast::Stmt::Expr(ast::Expr::CallExpr(c)) => Ok(c),
            other => Err(format!(
                "`{}` is not a table builder call",
                excerpt(other.syntax())
            )),
        })
        .collect()
}

/// Reads one `schema.…` call.
fn schema_op(c: &ast::CallExpr) -> Result<SchemaOp, String> {
    let Some(ast::Expr::MemberExpr(m)) = c.callee() else {
        return Err("expected a `schema.…` call".to_owned());
    };
    let method = m.name().map(|t| t.text().to_owned()).unwrap_or_default();
    let s = |label: &str| -> Result<String, String> {
        arg(c, label)
            .and_then(|v| string_arg(&v))
            .ok_or(format!("`{method}` needs `{label}:` as a string"))
    };
    Ok(match method.as_str() {
        "createTable" => {
            let l = c
                .trailing_lambda()
                .ok_or("`createTable` takes a builder lambda")?;
            let mut columns = Vec::new();
            for step in table_steps(&l)? {
                let (name, def) = column_def(&step)?;
                if name != "column" {
                    return Err(format!(
                        "`createTable` declares columns with `table.column`, not `{name}`"
                    ));
                }
                columns.push(def);
            }
            SchemaOp::CreateTable {
                name: s("name")?,
                columns,
            }
        }
        "alterTable" => {
            let l = c
                .trailing_lambda()
                .ok_or("`alterTable` takes a builder lambda")?;
            let mut ops = Vec::new();
            for step in table_steps(&l)? {
                let Some(ast::Expr::MemberExpr(sm)) = step.callee() else {
                    return Err("expected a table builder call".to_owned());
                };
                let name = sm.name().map(|t| t.text().to_owned()).unwrap_or_default();
                let sarg = |label: &str| -> Result<String, String> {
                    arg(&step, label)
                        .and_then(|v| string_arg(&v))
                        .ok_or(format!("`{name}` needs `{label}:` as a string"))
                };
                let base = {
                    // The first call of a chain names the builder method.
                    let mut cur = step.clone();
                    loop {
                        match cur.callee() {
                            Some(ast::Expr::MemberExpr(mm)) => {
                                match mm.receiver().map(|r| strip_parens(&r)) {
                                    Some(ast::Expr::CallExpr(inner)) => cur = inner,
                                    _ => {
                                        break mm
                                            .name()
                                            .map(|t| t.text().to_owned())
                                            .unwrap_or_default();
                                    }
                                }
                            }
                            _ => break String::new(),
                        }
                    }
                };
                ops.push(match base.as_str() {
                    "addColumn" => AlterOp::AddColumn(column_def(&step)?.1),
                    "renameColumn" => AlterOp::RenameColumn {
                        from: sarg("from")?,
                        to: sarg("to")?,
                    },
                    "dropColumn" => AlterOp::DropColumn {
                        name: sarg("name")?,
                    },
                    "retireColumn" => AlterOp::RetireColumn {
                        name: sarg("name")?,
                    },
                    "addEnumCase" => AlterOp::AddEnumCase {
                        column: sarg("column")?,
                        case: sarg("case")?,
                    },
                    "renameEnumCase" => AlterOp::RenameEnumCase {
                        column: sarg("column")?,
                        from: sarg("from")?,
                        to: sarg("to")?,
                    },
                    "removeEnumCase" => AlterOp::RemoveEnumCase {
                        column: sarg("column")?,
                        case: sarg("case")?,
                        replacement: sarg("replacement")?,
                    },
                    other => return Err(format!("`{other}` is not an `alterTable` operation")),
                });
            }
            SchemaOp::AlterTable {
                name: s("name")?,
                ops,
            }
        }
        "renameTable" => SchemaOp::RenameTable {
            from: s("from")?,
            to: s("to")?,
        },
        "dropTable" => SchemaOp::DropTable { name: s("name")? },
        "retireTable" => SchemaOp::RetireTable { name: s("name")? },
        "createIndex" => SchemaOp::CreateIndex {
            table: s("table")?,
            columns: arg(c, "columns")
                .and_then(|v| string_list(&v))
                .ok_or("`createIndex` needs `columns:` as a list of strings")?,
        },
        "insert" => {
            let rows = arg(c, "rows").ok_or("`insert` needs `rows:`")?;
            let ast::Expr::ListExpr(list) = strip_parens(&rows) else {
                return Err("`rows:` is a list of maps".to_owned());
            };
            let mut out = Vec::new();
            for row in list.syntax().children().filter_map(ast::Expr::cast) {
                let ast::Expr::MapExpr(map) = strip_parens(&row) else {
                    return Err("each row is a map literal".to_owned());
                };
                let mut values = Vec::new();
                for entry in map
                    .syntax()
                    .children()
                    .filter(|n| n.kind() == SyntaxKind::MapEntry)
                {
                    let mut parts = entry.children().filter_map(ast::Expr::cast);
                    let (Some(k), Some(v)) = (parts.next(), parts.next()) else {
                        return Err("incomplete row entry".to_owned());
                    };
                    let key = string_arg(&k).ok_or("row keys are column names as strings")?;
                    values.push((key, column_value(&v)?));
                }
                out.push(values);
            }
            SchemaOp::Insert {
                table: s("into")?,
                rows: out,
            }
        }
        "execute" => SchemaOp::Execute {
            postgres: s("postgres")?,
            sqlite: s("sqlite")?,
        },
        other => return Err(format!("`schema.{other}` is not a schema builder method")),
    })
}

/// One builder call of a migration, with where it is: the call's range
/// and, for `alterTable`, the range of each step of its lambda.
type LocatedOp = (SchemaOp, TextRange, Vec<TextRange>);

/// The ranges of the steps of an `alterTable { table => … }` call.
fn step_ranges(c: &ast::CallExpr) -> Vec<TextRange> {
    c.trailing_lambda()
        .and_then(|l| table_steps(&l).ok())
        .map(|steps| steps.iter().map(|s| s.syntax().text_range()).collect())
        .unwrap_or_default()
}

/// The `up(schema:)` operations of a migration file, each with where it
/// is.
///
/// # Errors
///
/// What can't be read, with the range of the offending call or statement
/// (or of the migration's name when it declares no `up`).
fn migration_ops(root: &SyntaxNode) -> Result<Vec<LocatedOp>, (String, TextRange)> {
    let up = root
        .descendants()
        .find(|n| {
            n.kind() == SyntaxKind::FuncDecl
                && n.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .any(|t| t.kind() == SyntaxKind::Ident && t.text() == "up")
        })
        .ok_or((
            "a migration declares `func up(schema: Schema)`".to_owned(),
            migration_name_range(root),
        ))?;
    let block = up
        .children()
        .find_map(ast::Block::cast)
        .ok_or(("`up` has no body".to_owned(), up.text_range()))?;
    let mut ops = Vec::new();
    for s in block.statements() {
        match s {
            ast::Stmt::Expr(ast::Expr::CallExpr(c)) => {
                let range = c.syntax().text_range();
                ops.push((
                    schema_op(&c)
                        .map_err(|e| (format!("{e} (at `{}`)", excerpt(c.syntax())), range))?,
                    range,
                    step_ranges(&c),
                ));
            }
            other => {
                return Err((
                    format!(
                        "a migration's `up` only calls the schema builder; `{}` can't run at migration time",
                        excerpt(other.syntax())
                    ),
                    other.syntax().text_range(),
                ));
            }
        }
    }
    Ok(ops)
}

/// The range of the migration's name, or of the whole file when it
/// declares no migration.
pub(crate) fn migration_name_range(root: &SyntaxNode) -> TextRange {
    root.descendants()
        .find(|n| n.kind() == SyntaxKind::MigrationDecl)
        .and_then(|d| {
            d.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::Ident)
                .map(|t| t.text_range())
        })
        .unwrap_or_else(|| TextRange::empty(0.into()))
}

/// The reason of the migration's `@Breaking("reason")`: `None` without the
/// annotation, `Some("")` when its argument isn't a plain string.
fn breaking_reason(root: &SyntaxNode) -> Option<String> {
    let decl = root
        .descendants()
        .find(|n| n.kind() == SyntaxKind::MigrationDecl)?;
    let a = decl
        .children()
        .filter_map(ast::Annotation::cast)
        .find(|a| a.name().is_some_and(|t| t.text() == "Breaking"))?;
    Some(
        a.arg_list()
            .and_then(|l| l.args().next())
            .and_then(|arg| arg.value())
            .and_then(|v| string_arg(&v))
            .unwrap_or_default(),
    )
}

/// The `@Breaking` reason that allows dropping or renaming something the
/// previous release still reads (`docs/design/19-persistence.md`,
/// "Rolling deploys").
pub(crate) const SINGLE_INSTANCE: &str = "single instance";

/// A persistence problem with its place: a migration that doesn't replay,
/// a destructive one without `@Breaking`, or a difference between the
/// entities, the migrations and `schema.lock`.
#[derive(Clone, Debug)]
pub(crate) struct Problem {
    /// The file it is in.
    pub(crate) file: FileId,
    /// Its range there.
    pub(crate) range: TextRange,
    /// The diagnostic code.
    pub(crate) code: Code,
    /// What is wrong.
    pub(crate) message: String,
    /// Further facts, one per note.
    pub(crate) notes: Vec<String>,
    /// How to resolve it.
    pub(crate) help: Option<String>,
}

impl Problem {
    /// A problem without notes or help.
    pub(crate) fn new(file: FileId, range: TextRange, code: Code, message: String) -> Problem {
        Problem {
            file,
            range,
            code,
            message,
            notes: Vec::new(),
            help: None,
        }
    }

    /// This problem with `help`.
    pub(crate) fn help(mut self, help: impl Into<String>) -> Problem {
        self.help = Some(help.into());
        self
    }

    /// This problem with one more note.
    pub(crate) fn note(mut self, note: impl Into<String>) -> Problem {
        self.notes.push(note.into());
        self
    }

    /// The problem as a build error line, `migrations/0002_x.k:3: message`,
    /// with its help after a semicolon.
    pub(crate) fn line(&self, g: &Gen<'_>) -> String {
        let path = g
            .analysis
            .files()
            .into_iter()
            .find(|(f, _)| *f == self.file)
            .map(|(_, p)| p.as_str().to_owned())
            .unwrap_or_default();
        let line = g.analysis.text(self.file).map_or(1, |t| {
            let at = usize::from(self.range.start()).min(t.len());
            t[..at].matches('\n').count() + 1
        });
        match &self.help {
            Some(h) => format!("{path}:{line}: {}; {h}", self.message),
            None => format!("{path}:{line}: {}", self.message),
        }
    }

    /// The problem as a diagnostic.
    pub(crate) fn diagnostic(&self) -> Diagnostic {
        let mut d = Diagnostic::new(self.code, Span::new(self.file, self.range), &self.message);
        for n in &self.notes {
            d = d.with_note(n);
        }
        if let Some(h) = &self.help {
            d = d.with_help(h);
        }
        d
    }
}

/// One file of `migrations/`.
#[derive(Clone, Debug)]
pub(crate) struct MigrationFile {
    /// The file stem, `0002_add_note_pinning`.
    pub(crate) stem: String,
    /// The file.
    pub(crate) file: FileId,
    /// Its syntax tree.
    pub(crate) root: SyntaxNode,
}

/// The project's migration files in application order: the order of
/// `schema.lock`'s `migrations` list, then the files the list doesn't name,
/// by file name.
pub(crate) fn migration_files(g: &Gen<'_>) -> Vec<MigrationFile> {
    let mut files: Vec<MigrationFile> = Vec::new();
    for (file, path) in g.analysis.files() {
        let p = path.as_str();
        let Some(stem) = p
            .strip_prefix("migrations/")
            .and_then(|r| r.strip_suffix(".k"))
        else {
            continue;
        };
        if stem.contains('/') {
            continue;
        }
        if let Ok(Some(parse)) = g.analysis.parse(file) {
            files.push(MigrationFile {
                stem: stem.to_owned(),
                file,
                root: parse.syntax(),
            });
        }
    }
    files.sort_by(|a, b| a.stem.cmp(&b.stem));
    if let Some(order) = lock(g).map(|l| l.migrations).filter(|m| !m.is_empty()) {
        files.sort_by_key(|f| {
            order
                .iter()
                .position(|o| *o == f.stem)
                .unwrap_or(usize::MAX)
        });
    }
    files
}

/// The migrations replayed in order: what they render, the schema they
/// leave, and the problems found on the way.
#[derive(Debug, Default)]
pub(crate) struct History {
    /// The migration files, in application order.
    pub(crate) files: Vec<MigrationFile>,
    /// Each migration that replayed, rendered.
    pub(crate) migrations: Vec<Migration>,
    /// The schema they leave (up to the first migration that doesn't
    /// replay).
    pub(crate) replay: Replay,
    /// Migrations that don't replay, and destructive ones without the
    /// `@Breaking` they need.
    pub(crate) problems: Vec<Problem>,
}

/// Replays the project's migrations in order (`docs/compiler/db.md` §10),
/// checking every name against the schema the earlier ones leave, and that
/// a migration with a destructive operation is marked `@Breaking`: with any
/// reason for dropping what an earlier migration retired and for the enum
/// case rewrites, and with `@Breaking("single instance")` for dropping or
/// renaming anything still live. A migration that doesn't replay stops the
/// replay, so later ones aren't checked against a schema that is wrong.
pub(crate) fn history(g: &Gen<'_>) -> History {
    let mut h = History {
        files: migration_files(g),
        ..History::default()
    };
    for f in &h.files {
        let ops = match migration_ops(&f.root) {
            Ok(ops) => ops,
            Err((message, range)) => {
                h.problems.push(Problem::new(
                    f.file,
                    range,
                    codes::INVALID_MIGRATION,
                    message,
                ));
                break;
            }
        };
        let mut schema_ops = Vec::new();
        let mut ranges = Vec::new();
        for (op, range, steps) in ops {
            schema_ops.push(op);
            ranges.push((range, steps));
        }
        // The step of an `alterTable` when there is one, else the call.
        let at = |op: usize, step: Option<usize>| -> TextRange {
            ranges
                .get(op)
                .map_or_else(TextRange::default, |(range, steps)| {
                    step.and_then(|s| steps.get(s).copied()).unwrap_or(*range)
                })
        };
        let rendered = match h.replay.try_apply(&schema_ops) {
            Ok(r) => r,
            Err(e) => {
                h.problems.push(Problem::new(
                    f.file,
                    at(e.op, e.step),
                    codes::INVALID_MIGRATION,
                    format!("`{}` doesn't replay: {}", f.stem, e.message),
                ));
                break;
            }
        };
        let reason = breaking_reason(&f.root);
        for d in &rendered.destructive {
            let range = at(d.op, d.step);
            if d.live && reason.as_deref() != Some(SINGLE_INSTANCE) {
                let renames = d.what.starts_with("renames");
                let mut p = Problem::new(
                    f.file,
                    range,
                    codes::LIVE_SCHEMA_CHANGE,
                    format!(
                        "`{}` {}, which the previous release still reads during a rolling deploy",
                        f.stem, d.what
                    ),
                );
                if let Some(r) = reason.as_deref().filter(|r| !r.is_empty()) {
                    p = p.note(format!(
                        "`@Breaking(\"{r}\")` allows dropping what an earlier migration retired; only `@Breaking(\"single instance\")` allows this"
                    ));
                }
                p = p.help(if renames {
                    "rename across two releases instead (add the new column, backfill it and retire the old one; drop it later with `k migrate new --drop-retired`), or rename only the field and keep the column (`k migrate rename`); a single-instance app can mark the migration `@Breaking(\"single instance\")`"
                } else {
                    "retire it first (`retireColumn` or `retireTable`, which `k migrate new` writes for a removed field), ship that, and drop it in a later release with `k migrate new --drop-retired`; a single-instance app can mark the migration `@Breaking(\"single instance\")`"
                });
                h.problems.push(p);
            } else if !d.live && reason.is_none() {
                h.problems.push(
                    Problem::new(
                        f.file,
                        range,
                        codes::DESTRUCTIVE_MIGRATION,
                        format!(
                            "`{}` {} without `@Breaking`, which loses data",
                            f.stem, d.what
                        ),
                    )
                    .help("mark the migration `@Breaking(\"reason\")`, saying why the loss is acceptable"),
                );
            }
        }
        h.migrations.push(Migration {
            name: f.stem.clone(),
            sqlite: rendered.sqlite,
            postgres: rendered.postgres,
        });
    }
    h
}

/// The project's migrations, replayed and rendered, in application order:
/// the order of `schema.lock`'s `migrations` list when the lock has one,
/// file-name order otherwise.
pub(crate) fn migrations(g: &Gen<'_>) -> Result<Vec<Migration>, Vec<String>> {
    let h = history(g);
    let mut errors: Vec<String> = h.problems.iter().map(|p| p.line(g)).collect();
    if errors.is_empty() && !h.files.is_empty() {
        errors.extend(check_entities(g, &h.replay));
    }
    if errors.is_empty() {
        Ok(h.migrations)
    } else {
        Err(errors)
    }
}

/// Every entity's table and columns as the migrations leave them: a
/// missing table or column, or one of another type, means the migrations
/// don't produce what the code reads (`k migrate new` writes the missing
/// migration).
fn check_entities(g: &Gen<'_>, replay: &Replay) -> Vec<String> {
    let mut out = Vec::new();
    let mut entities: Vec<&EntityModel> = model(g).entities.values().collect();
    entities.sort_by(|a, b| a.table.cmp(&b.table));
    for e in entities {
        let name = g.view.qualified_name(e.decl);
        let Some(columns) = replay.tables.get(&e.table) else {
            out.push(format!(
                "entity `{name}` is stored in table `{}`, which the migrations don't create; run `k migrate new`",
                e.table
            ));
            continue;
        };
        for f in &e.fields {
            let (column, ty) = match &f.kind {
                FieldKind::Column { column, col, .. } => (column, col.column_type(g)),
                FieldKind::ToOne { column, target, .. } => {
                    let ty = entity(g, *target)
                        .and_then(|t| t.id_field().cloned())
                        .and_then(|idf| match idf.kind {
                            FieldKind::Column { col, .. } => Some(col.column_type(g)),
                            _ => None,
                        })
                        .unwrap_or(ColumnType::Uuid);
                    (column, ty)
                }
                FieldKind::ToMany { .. } => continue,
            };
            match columns.iter().find(|c| &c.name == column) {
                None => out.push(format!(
                    "`{name}.{}` is stored in column `{}.{column}`, which the migrations don't create; run `k migrate new`",
                    f.name, e.table
                )),
                Some(c) if c.ty != ty => out.push(format!(
                    "`{name}.{}` is a `{}` but the migrations make `{}.{column}` a `{}` column",
                    f.name,
                    ty.lock_name(),
                    e.table,
                    c.ty.lock_name()
                )),
                Some(_) => {}
            }
        }
    }
    out
}

/// The unique indexes of every `@Unique` field.
fn unique_indexes(g: &Gen<'_>) -> Vec<UniqueIndex> {
    let mut out = Vec::new();
    let mut entities: Vec<&EntityModel> = model(g).entities.values().collect();
    entities.sort_by(|a, b| a.table.cmp(&b.table));
    for e in entities {
        for f in e.unique_fields() {
            let FieldKind::Column { column, .. } = &f.kind else {
                continue;
            };
            out.push(UniqueIndex {
                table: e.table.clone(),
                columns: f.columns(),
                index: format!("{}_{column}_key", e.table),
                field: f.name.clone(),
            });
        }
    }
    out
}

/// The build's database manifest, or `None` for a program without
/// entities, migrations or statements.
///
/// # Errors
///
/// Every persistence problem found while lowering (untranslatable queries)
/// and replaying the migrations.
pub(crate) fn manifest(g: &Gen<'_>) -> Result<Option<DbManifest>, Vec<String>> {
    // Building the model reports entity problems (a `Double` field) even
    // when no repository was lowered.
    let _ = model(g);
    let migrations = migrations(g);
    let mut errors = std::mem::take(&mut g.db.state().errors);
    let migrations = match migrations {
        Ok(m) => m,
        Err(e) => {
            errors.extend(e);
            Vec::new()
        }
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    let statements = std::mem::take(&mut g.db.state().statements);
    let unique = unique_indexes(g);
    if statements.is_empty() && migrations.is_empty() && model(g).entities.is_empty() {
        return Ok(None);
    }
    Ok(Some(DbManifest {
        statements,
        migrations,
        unique,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_snake_case() {
        assert_eq!(snake("OrderLine"), "order_line");
        assert_eq!(snake("isPinned"), "is_pinned");
        assert_eq!(snake("imageUrl"), "image_url");
        assert_eq!(snake("id"), "id");
    }

    #[test]
    fn identical_statements_share_a_number() {
        let mut s = DbState::default();
        let a = Statement::Delete {
            table: "t".into(),
            filter: None,
        };
        let b = Statement::Delete {
            table: "u".into(),
            filter: None,
        };
        assert_eq!(s.register(&a), 0);
        assert_eq!(s.register(&b), 1);
        assert_eq!(s.register(&a), 0);
        assert_eq!(s.statements.len(), 2);
    }
}
