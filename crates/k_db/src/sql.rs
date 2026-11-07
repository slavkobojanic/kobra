//! SQL generation from the [`crate::ir`] for both dialects.
//!
//! Generated SQL quotes every identifier, numbers every parameter and
//! never contains a value: the only text that comes from outside the
//! compiler is a locked identifier, which [`quote_identifier`] escapes.
//! The exact output for each construct is specified in
//! `docs/compiler/db.md`, "SQL generation", and pinned by
//! `crates/k_db/tests/sql.rs`.

use std::fmt::Write;

use crate::ir::{ArithOp, CmpOp, Count, Like, Operand, Order, Predicate, Select, Statement};

/// A SQL dialect.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dialect {
    /// SQLite 3 (bundled with `rusqlite`): parameters `?1, ?2, …`.
    Sqlite,
    /// PostgreSQL: parameters `$1, $2, …`.
    Postgres,
}

/// Rendered SQL: the text and the number of parameters it binds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sql {
    /// The statement text.
    pub text: String,
    /// The highest parameter number used (parameters are 1-based).
    pub params: u32,
}

/// Quotes an identifier with double quotes, doubling any `"` inside, so
/// reserved words (`"order"`, `"user"`) and any locked name are safe in
/// both dialects.
#[must_use]
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Builds the bind value of a [`Like`] predicate from the text K code
/// passed: `\`, `%` and `_` are escaped with `\` (every generated `LIKE`
/// declares `ESCAPE '\'`), then wrapped in `%` as the predicate needs.
#[must_use]
pub fn like_pattern(kind: Like, text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    match kind {
        Like::Contains => format!("%{escaped}%"),
        Like::StartsWith => format!("{escaped}%"),
        Like::EndsWith => format!("%{escaped}"),
    }
}

struct Renderer {
    dialect: Dialect,
    out: String,
    params: u32,
}

impl Renderer {
    fn param(&mut self, n: u32) {
        self.params = self.params.max(n);
        match self.dialect {
            Dialect::Sqlite => {
                let _ = write!(self.out, "?{n}");
            }
            Dialect::Postgres => {
                let _ = write!(self.out, "${n}");
            }
        }
    }

    fn ident(&mut self, name: &str) {
        self.out.push_str(&quote_identifier(name));
    }

    fn operand(&mut self, o: &Operand) {
        match o {
            Operand::Column(c) => self.ident(c),
            Operand::Param(n) => self.param(*n),
            Operand::Arith { left, op, right } => {
                self.out.push('(');
                self.operand(left);
                self.out.push_str(match op {
                    ArithOp::Add => " + ",
                    ArithOp::Sub => " - ",
                });
                self.operand(right);
                self.out.push(')');
            }
        }
    }

    fn predicate(&mut self, p: &Predicate, nested: bool) {
        match p {
            Predicate::Compare { left, op, right } => {
                self.operand(left);
                self.out.push_str(match op {
                    CmpOp::Eq => " = ",
                    CmpOp::Ne => " <> ",
                    CmpOp::Lt => " < ",
                    CmpOp::Le => " <= ",
                    CmpOp::Gt => " > ",
                    CmpOp::Ge => " >= ",
                });
                self.operand(right);
            }
            Predicate::IsNull(o) => {
                self.operand(o);
                self.out.push_str(" IS NULL");
            }
            Predicate::IsNotNull(o) => {
                self.operand(o);
                self.out.push_str(" IS NOT NULL");
            }
            Predicate::And(ps) | Predicate::Or(ps) => {
                let and = matches!(p, Predicate::And(_));
                if ps.is_empty() {
                    self.out.push_str(if and { "1 = 1" } else { "1 = 0" });
                    return;
                }
                if ps.len() == 1 {
                    self.predicate(&ps[0], nested);
                    return;
                }
                if nested {
                    self.out.push('(');
                }
                for (i, q) in ps.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(if and { " AND " } else { " OR " });
                    }
                    self.predicate(q, true);
                }
                if nested {
                    self.out.push(')');
                }
            }
            Predicate::Not(q) => {
                self.out.push_str("NOT (");
                self.predicate(q, false);
                self.out.push(')');
            }
            Predicate::In { operand, list } => {
                self.operand(operand);
                match self.dialect {
                    // A JSON array parameter, expanded by SQLite itself.
                    Dialect::Sqlite => {
                        self.out.push_str(" IN (SELECT value FROM json_each(");
                        self.param(*list);
                        self.out.push_str("))");
                    }
                    // An array parameter.
                    Dialect::Postgres => {
                        self.out.push_str(" = ANY(");
                        self.param(*list);
                        self.out.push(')');
                    }
                }
            }
            Predicate::Like { column, param, .. } => {
                self.ident(column);
                self.out.push_str(" LIKE ");
                self.param(*param);
                self.out.push_str(" ESCAPE '\\'");
            }
        }
    }

    fn filter(&mut self, filter: Option<&Predicate>) {
        if let Some(p) = filter {
            self.out.push_str(" WHERE ");
            self.predicate(p, false);
        }
    }

    fn count(&mut self, c: &Count) {
        match c {
            Count::Const(n) => {
                let _ = write!(self.out, "{n}");
            }
            Count::Param(n) => self.param(*n),
        }
    }

    fn select(&mut self, s: &Select) {
        self.out.push_str("SELECT ");
        if s.columns.is_empty() {
            self.out.push('*');
        }
        for (i, c) in s.columns.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.ident(c);
        }
        self.out.push_str(" FROM ");
        self.ident(&s.table);
        self.filter(s.filter.as_ref());
        self.order(&s.order);
        match (&s.limit, &s.offset) {
            (Some(l), offset) => {
                self.out.push_str(" LIMIT ");
                self.count(l);
                if let Some(o) = offset {
                    self.out.push_str(" OFFSET ");
                    self.count(o);
                }
            }
            (None, Some(o)) => {
                // Both dialects need a LIMIT before OFFSET in this form;
                // -1 is SQLite's "no limit", ALL is Postgres'.
                self.out.push_str(match self.dialect {
                    Dialect::Sqlite => " LIMIT -1 OFFSET ",
                    Dialect::Postgres => " LIMIT ALL OFFSET ",
                });
                self.count(o);
            }
            (None, None) => {}
        }
    }

    fn order(&mut self, order: &[Order]) {
        for (i, o) in order.iter().enumerate() {
            self.out.push_str(if i == 0 { " ORDER BY " } else { ", " });
            self.ident(&o.column);
            // `nil` sorts before every value ascending and after them
            // descending on both databases: SQLite's default, spelled out
            // for Postgres (which otherwise puts NULL last ascending).
            match (self.dialect, o.descending) {
                (Dialect::Sqlite, false) => {}
                (Dialect::Sqlite, true) => self.out.push_str(" DESC"),
                (Dialect::Postgres, false) => self.out.push_str(" NULLS FIRST"),
                (Dialect::Postgres, true) => self.out.push_str(" DESC NULLS LAST"),
            }
        }
    }
}

/// Renders a statement in `dialect`.
///
/// Never panics. Parameters keep the IR's numbers, so the same bind list
/// works for both dialects (except list parameters of [`Predicate::In`]:
/// a JSON array text on SQLite, an array on Postgres).
#[must_use]
pub fn render(statement: &Statement, dialect: Dialect) -> Sql {
    let mut r = Renderer {
        dialect,
        out: String::new(),
        params: 0,
    };
    match statement {
        Statement::Select(s) => r.select(s),
        Statement::Count(s) => {
            r.out.push_str("SELECT COUNT(*) FROM ");
            r.ident(&s.table);
            r.filter(s.filter.as_ref());
        }
        Statement::Insert {
            table,
            columns,
            rows,
        } => {
            r.out.push_str("INSERT INTO ");
            r.ident(table);
            r.out.push_str(" (");
            for (i, c) in columns.iter().enumerate() {
                if i > 0 {
                    r.out.push_str(", ");
                }
                r.ident(c);
            }
            r.out.push_str(") VALUES ");
            let width = u32::try_from(columns.len()).unwrap_or(u32::MAX);
            for row in 0..*rows {
                if row > 0 {
                    r.out.push_str(", ");
                }
                r.out.push('(');
                for col in 0..width {
                    if col > 0 {
                        r.out.push_str(", ");
                    }
                    r.param(
                        row.saturating_mul(width)
                            .saturating_add(col)
                            .saturating_add(1),
                    );
                }
                r.out.push(')');
            }
        }
        Statement::Update { table, set, filter } => {
            r.out.push_str("UPDATE ");
            r.ident(table);
            r.out.push_str(" SET ");
            for (i, (c, o)) in set.iter().enumerate() {
                if i > 0 {
                    r.out.push_str(", ");
                }
                r.ident(c);
                r.out.push_str(" = ");
                r.operand(o);
            }
            r.filter(filter.as_ref());
        }
        Statement::Delete { table, filter } => {
            r.out.push_str("DELETE FROM ");
            r.ident(table);
            r.filter(filter.as_ref());
        }
    }
    Sql {
        text: r.out,
        params: r.params,
    }
}
