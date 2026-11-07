//! The Postgres driver.

use std::error::Error;
use std::str::FromStr;

use bytes::BytesMut;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::{FromSql, IsNull, Kind, ToSql, Type, to_sql_checked};
use tokio_postgres::{Client, NoTls};

use super::DatabaseOptions;
use super::pgcodec;
use crate::error::DbError;
use crate::value::{Row, Value};

/// A Postgres connection; its I/O task runs on the tokio runtime.
pub struct PostgresDatabase {
    client: Client,
}

type BoxError = Box<dyn Error + Sync + Send>;

fn parse_uuid(text: &str) -> Result<[u8; 16], BoxError> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return Err(format!("`{text}` is not a UUID").into());
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)?;
    }
    Ok(out)
}

fn format_uuid(b: &[u8]) -> String {
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

impl ToSql for Value {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, BoxError> {
        match self {
            Value::Null => Ok(IsNull::Yes),
            Value::Bool(b) => b.to_sql(ty, out),
            Value::Int(i) => match *ty {
                Type::INT2 => i16::try_from(*i)?.to_sql(ty, out),
                Type::INT4 => i32::try_from(*i)?.to_sql(ty, out),
                _ => i.to_sql(ty, out),
            },
            Value::Double(d) => match *ty {
                #[allow(clippy::cast_possible_truncation)]
                Type::FLOAT4 => (*d as f32).to_sql(ty, out),
                _ => d.to_sql(ty, out),
            },
            Value::Text(s) if *ty == Type::UUID => {
                out.extend_from_slice(&parse_uuid(s)?);
                Ok(IsNull::No)
            }
            Value::Text(s) if *ty == Type::NUMERIC => {
                out.extend_from_slice(&pgcodec::encode_numeric(s)?);
                Ok(IsNull::No)
            }
            Value::Text(s) if *ty == Type::TIMESTAMPTZ || *ty == Type::TIMESTAMP => {
                out.extend_from_slice(&pgcodec::encode_timestamp(s)?);
                Ok(IsNull::No)
            }
            Value::Text(s) if *ty == Type::JSONB => {
                out.extend_from_slice(&pgcodec::encode_jsonb(s));
                Ok(IsNull::No)
            }
            Value::Text(s) if *ty == Type::JSON => {
                out.extend_from_slice(s.as_bytes());
                Ok(IsNull::No)
            }
            // An `In` list arrives as a JSON array (SQLite expands it with
            // `json_each`); Postgres binds it as an array of the column's
            // type (`"id" = ANY($1)`).
            Value::Text(s) if matches!(ty.kind(), Kind::Array(_)) => {
                let items: Vec<Value> = serde_json::from_str::<Vec<serde_json::Value>>(s)?
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::Null => Value::Null,
                        serde_json::Value::Bool(b) => Value::Bool(b),
                        serde_json::Value::Number(n) => match n.as_i64() {
                            Some(i) => Value::Int(i),
                            None => Value::Double(n.as_f64().unwrap_or(f64::NAN)),
                        },
                        serde_json::Value::String(s) => Value::Text(s),
                        other => Value::Text(other.to_string()),
                    })
                    .collect();
                items.to_sql(ty, out)
            }
            Value::Text(s) => s.to_sql(ty, out),
            Value::Bytes(b) => b.to_sql(ty, out),
        }
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

/// Reads any column as raw bytes, for the types converted by hand (UUID).
struct Raw(Vec<u8>);

impl<'a> FromSql<'a> for Raw {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Raw, BoxError> {
        Ok(Raw(raw.to_vec()))
    }

    fn accepts(_: &Type) -> bool {
        true
    }
}

fn map_error(error: tokio_postgres::Error) -> DbError {
    if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        let db = error.as_db_error();
        return DbError::UniqueViolation {
            table: db.and_then(|d| d.table()).map(str::to_owned),
            columns: db
                .and_then(|d| d.column())
                .map(|c| vec![c.to_owned()])
                .unwrap_or_default(),
            constraint: db.and_then(|d| d.constraint()).map(str::to_owned),
        };
    }
    if error.is_closed() {
        return DbError::Connection(error.to_string());
    }
    match error.as_db_error() {
        Some(db) => DbError::Statement(db.message().to_owned()),
        None => DbError::Statement(error.to_string()),
    }
}

fn read(row: &tokio_postgres::Row, i: usize) -> Result<Value, DbError> {
    let ty = row.columns()[i].type_().clone();
    let get = |e: tokio_postgres::Error| DbError::Statement(e.to_string());
    Ok(match ty {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(i)
            .map_err(get)?
            .map_or(Value::Null, Value::Bool),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(i)
            .map_err(get)?
            .map_or(Value::Null, |v| Value::Int(v.into())),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(i)
            .map_err(get)?
            .map_or(Value::Null, |v| Value::Int(v.into())),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(i)
            .map_err(get)?
            .map_or(Value::Null, Value::Int),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(i)
            .map_err(get)?
            .map_or(Value::Null, |v| Value::Double(v.into())),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(i)
            .map_err(get)?
            .map_or(Value::Null, Value::Double),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => row
            .try_get::<_, Option<String>>(i)
            .map_err(get)?
            .map_or(Value::Null, Value::Text),
        Type::BYTEA => row
            .try_get::<_, Option<Vec<u8>>>(i)
            .map_err(get)?
            .map_or(Value::Null, Value::Bytes),
        Type::UUID => row
            .try_get::<_, Option<Raw>>(i)
            .map_err(get)?
            .map_or(Value::Null, |r| Value::Text(format_uuid(&r.0))),
        Type::NUMERIC | Type::TIMESTAMPTZ | Type::TIMESTAMP | Type::JSONB | Type::JSON => {
            match row.try_get::<_, Option<Raw>>(i).map_err(get)? {
                None => Value::Null,
                Some(Raw(raw)) => {
                    let text = match ty {
                        Type::NUMERIC => pgcodec::decode_numeric(&raw),
                        Type::JSONB => pgcodec::decode_jsonb(&raw),
                        Type::JSON => String::from_utf8(raw).map_err(Into::into),
                        _ => pgcodec::decode_timestamp(&raw),
                    };
                    Value::Text(text.map_err(|e| DbError::Statement(e.to_string()))?)
                }
            }
        }
        other => return Err(DbError::UnsupportedType(other.name().to_owned())),
    })
}

impl PostgresDatabase {
    pub(super) async fn connect(
        url: &str,
        options: &DatabaseOptions,
    ) -> Result<PostgresDatabase, DbError> {
        let mut config = tokio_postgres::Config::from_str(url)
            .map_err(|_| DbError::InvalidUrl("not a valid Postgres connection URL".to_owned()))?;
        if let Some(p) = &options.password {
            config.password(p);
        }
        let (client, connection) = config
            .connect(NoTls)
            .await
            .map_err(|e| DbError::Connection(e.to_string()))?;
        tokio::spawn(async move {
            // The connection ends when the client is dropped; an error here
            // surfaces on the next statement as a closed connection.
            let _ = connection.await;
        });
        Ok(PostgresDatabase { client })
    }

    fn refs(params: &[Value]) -> Vec<&(dyn ToSql + Sync)> {
        params.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
    }

    pub(super) async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        if params.is_empty() {
            // Transaction control and DDL: the simple protocol accepts
            // several statements and needs no prepare round trip. The count
            // is the rows the statements affected, summed.
            let messages = self.client.simple_query(sql).await.map_err(map_error)?;
            return Ok(messages
                .iter()
                .map(|m| match m {
                    tokio_postgres::SimpleQueryMessage::CommandComplete(n) => *n,
                    _ => 0,
                })
                .sum());
        }
        self.client
            .execute(sql, &Self::refs(params))
            .await
            .map_err(map_error)
    }

    pub(super) async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        let rows = self
            .client
            .query(sql, &Self::refs(params))
            .await
            .map_err(map_error)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_owned()).collect();
            let mut values = Vec::with_capacity(columns.len());
            for i in 0..columns.len() {
                values.push(read(row, i)?);
            }
            out.push(Row { columns, values });
        }
        Ok(out)
    }
}
