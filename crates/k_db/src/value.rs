//! Values crossing between the server runtime and a database driver.

/// One parameter or column value.
///
/// The set is the intersection of what both databases store natively.
/// K types map onto it in generated code (`docs/compiler/db.md`, "Column
/// types"): `Uuid`, `Date` (RFC 3339), `Decimal` and enum cases travel as
/// [`Value::Text`], booleans as [`Value::Bool`] (SQLite stores 0/1).
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// SQL `NULL`, K's `nil`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A 64-bit integer. K's `Int` is a safe integer, so every `Int` fits.
    Int(i64),
    /// A double-precision float.
    Double(f64),
    /// Text, UTF-8.
    Text(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
}

impl Value {
    /// Returns the integer, if this is [`Value::Int`].
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the text, if this is [`Value::Text`].
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// One result row: the column names of the statement and one value per
/// column, in select order.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Row {
    /// The column names, as the database reports them.
    pub columns: Vec<String>,
    /// The values, one per column.
    pub values: Vec<Value>,
}

impl Row {
    /// Returns the value of the column called `name`, or `None` if the row
    /// has no such column.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.columns
            .iter()
            .position(|c| c == name)
            .and_then(|i| self.values.get(i))
    }
}
