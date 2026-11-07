//! The query IR: what a repository query or write means, independent of
//! the SQL dialect.
//!
//! `k_codegen` produces it from repository lambdas (`docs/compiler/db.md`
//! §4): field references become [`Operand::Column`]s with their locked
//! column names from `schema.lock`, captured K values become
//! [`Operand::Param`]s numbered from 1 in order of use, and everything
//! else fails the build. There is no literal operand: a
//! constant in a lambda (`note.isPinned == true`) is a parameter too, so
//! every value reaches the database as a bind parameter.

/// A comparison operator.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CmpOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

/// One side of a comparison, or the value a bulk update assigns.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Operand {
    /// A column of the query's table, by its locked name.
    Column(String),
    /// Bind parameter number `n` (1-based).
    Param(u32),
    /// Integer arithmetic on two operands, rendered parenthesized:
    /// `product.stock -= quantity` assigns `("stock" - ?1)`, so a bulk
    /// update is computed by the database in one statement instead of a
    /// read and a write that could race.
    Arith {
        /// The left operand.
        left: Box<Operand>,
        /// The operator.
        op: ArithOp,
        /// The right operand.
        right: Box<Operand>,
    },
}

/// An arithmetic operator of [`Operand::Arith`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ArithOp {
    /// `+`.
    Add,
    /// `-`.
    Sub,
}

/// A string predicate (`title.contains(text)`), compiled to `LIKE` with an
/// escaped pattern built by [`crate::sql::like_pattern`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Like {
    /// `contains`
    Contains,
    /// `hasPrefix`
    StartsWith,
    /// `hasSuffix`
    EndsWith,
}

/// A boolean condition over one row.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Predicate {
    /// `left op right`. Comparing with `nil` is never a `Compare`: it is
    /// [`Predicate::IsNull`] or [`Predicate::IsNotNull`].
    Compare {
        /// The left operand.
        left: Operand,
        /// The operator.
        op: CmpOp,
        /// The right operand.
        right: Operand,
    },
    /// `x == nil`
    IsNull(Operand),
    /// `x != nil`
    IsNotNull(Operand),
    /// `a and b and …`; an empty list is true.
    And(Vec<Predicate>),
    /// `a or b or …`; an empty list is false.
    Or(Vec<Predicate>),
    /// `not p`
    Not(Box<Predicate>),
    /// `ids.contains(row.id)`: membership in a list parameter. The driver
    /// expands the list (`docs/compiler/db.md`, "Lists").
    In {
        /// The tested operand.
        operand: Operand,
        /// The parameter holding the list.
        list: u32,
    },
    /// A string predicate on a column.
    Like {
        /// The text column the predicate tests, by its locked name.
        column: String,
        /// Which predicate.
        kind: Like,
        /// The parameter holding the (unescaped) text.
        param: u32,
    },
}

/// One `ORDER BY` key.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Order {
    /// The sort column, by its locked name.
    pub column: String,
    /// `true` for `sortedByDescending` / `thenByDescending`.
    pub descending: bool,
}

/// A row count for `LIMIT` and `OFFSET`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Count {
    /// A constant written in the source (`limit(50)`).
    Const(u64),
    /// A captured value.
    Param(u32),
}

/// A query over one table.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Select {
    /// The locked table name.
    pub table: String,
    /// The selected columns, in entity field order.
    pub columns: Vec<String>,
    /// The `WHERE` condition, if any.
    pub filter: Option<Predicate>,
    /// The `ORDER BY` keys, in order.
    pub order: Vec<Order>,
    /// `LIMIT` (`first` is a limit of 1).
    pub limit: Option<Count>,
    /// `OFFSET`: the number of rows to skip (`offset(n)`).
    pub offset: Option<Count>,
}

/// A statement a repository runs.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Statement {
    /// Rows of a query.
    Select(Select),
    /// The number of rows of a query (`count`); `columns`, `order`,
    /// `limit` and `offset` of the select are ignored.
    Count(Select),
    /// `insert(entity)` / `insert(all:)`: one row of parameters per entity,
    /// numbered row by row (`rows` × `columns.len()` parameters).
    Insert {
        /// The locked table name.
        table: String,
        /// The columns, in entity field order.
        columns: Vec<String>,
        /// The number of rows.
        rows: u32,
    },
    /// `update(entity)` and bulk `update { … }`: `SET` each column to an
    /// operand where the filter holds. A `@Version` update adds its version
    /// column to the filter and the set list.
    Update {
        /// The locked table name.
        table: String,
        /// The assignments.
        set: Vec<(String, Operand)>,
        /// The condition; `None` updates every row (only bulk updates).
        filter: Option<Predicate>,
    },
    /// `delete(entity)` and bulk `delete()`.
    Delete {
        /// The locked table name.
        table: String,
        /// The condition; `None` deletes every row (only bulk deletes).
        filter: Option<Predicate>,
    },
}
