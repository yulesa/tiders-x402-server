//! PostgreSQL implementation of the `Database` trait.
//!
//! Uses `deadpool-postgres` for async connection pooling and converts
//! query results from `tokio-postgres` rows into Arrow `RecordBatch`es.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use arrow::array::{
    ArrayRef, BinaryBuilder, BooleanBuilder, Date32Builder, Decimal128Builder,
    FixedSizeBinaryBuilder, Float32Builder, Float64Builder, Int16Builder, Int32Builder,
    Int64Builder, IntervalMonthDayNanoBuilder, ListBuilder, StringBuilder,
    Time64MicrosecondBuilder, TimestampMicrosecondBuilder,
};
use arrow::datatypes::{DataType, Field, IntervalMonthDayNano, IntervalUnit, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use postgres_types::{FromSql, Type, accepts};
use tokio_postgres::Row;

use crate::database::Database;
use crate::sql_postgresql::create_postgresql_query;
use crate::sqp_parser::AnalyzedQuery;

/// PostgreSQL database backend.
#[derive(Debug, Clone)]
pub struct PostgresqlDatabase {
    pool: Pool,
}

impl PostgresqlDatabase {
    /// Creates a new `PostgresqlDatabase` from a connection string.
    ///
    /// Parses the connection string, builds a `deadpool_postgres::Pool` (max 16
    /// connections), and verifies connectivity before returning.
    pub async fn from_connection_string(conn_str: &str) -> Result<Self> {
        let pg_config: tokio_postgres::Config = conn_str
            .parse()
            .map_err(|e| anyhow!("Failed to parse Postgres connection string: {}", e))?;

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, tokio_postgres::NoTls, mgr_config);
        let pool = Pool::builder(mgr)
            .max_size(16)
            .build()
            .map_err(|e| anyhow!("Failed to create Postgres pool: {}", e))?;

        // Verify connectivity
        let _conn = pool
            .get()
            .await
            .map_err(|e| anyhow!("Failed to connect to Postgres: {}", e))?;

        Ok(Self { pool })
    }

    /// Creates a new `PostgresqlDatabase` from individual connection parameters
    /// with full control over pool configuration.
    ///
    /// # Parameters
    /// - `host`: Database host (e.g., "localhost")
    /// - `port`: Database port (e.g., 5432)
    /// - `user`: Database user
    /// - `password`: Database password
    /// - `dbname`: Database name
    /// - `max_pool_size`: Maximum number of connections in the pool
    /// - `wait_timeout_ms`: Max time (ms) to wait for a connection from the pool (`None` = no timeout)
    /// - `create_timeout_ms`: Max time (ms) to create a new connection (`None` = no timeout)
    /// - `recycle_timeout_ms`: Max time (ms) to recycle a connection (`None` = no timeout)
    /// - `recycling_method`: Connection recycling strategy: "fast" (default), "verified", or "clean"
    pub async fn from_params(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        dbname: &str,
        max_pool_size: Option<usize>,
        wait_timeout_ms: Option<u64>,
        create_timeout_ms: Option<u64>,
        recycle_timeout_ms: Option<u64>,
        recycling_method: Option<&str>,
    ) -> Result<Self> {
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(host);
        pg_config.port(port);
        pg_config.user(user);
        pg_config.password(password);
        pg_config.dbname(dbname);

        let recycling = match recycling_method.unwrap_or("fast") {
            "fast" => RecyclingMethod::Fast,
            "verified" => RecyclingMethod::Verified,
            "clean" => RecyclingMethod::Clean,
            other => return Err(anyhow!("Unknown recycling method '{}'. Use 'fast', 'verified', or 'clean'", other)),
        };

        let mgr_config = ManagerConfig {
            recycling_method: recycling,
        };
        let mgr = Manager::from_config(pg_config, tokio_postgres::NoTls, mgr_config);

        let mut builder = Pool::builder(mgr)
            .max_size(max_pool_size.unwrap_or(16));

        if let Some(ms) = wait_timeout_ms {
            builder = builder.wait_timeout(Some(std::time::Duration::from_millis(ms)));
        }
        if let Some(ms) = create_timeout_ms {
            builder = builder.create_timeout(Some(std::time::Duration::from_millis(ms)));
        }
        if let Some(ms) = recycle_timeout_ms {
            builder = builder.recycle_timeout(Some(std::time::Duration::from_millis(ms)));
        }

        let pool = builder
            .build()
            .map_err(|e| anyhow!("Failed to create Postgres pool: {}", e))?;

        // Verify connectivity
        let _conn = pool
            .get()
            .await
            .map_err(|e| anyhow!("Failed to connect to Postgres: {}", e))?;

        Ok(Self { pool })
    }

    /// Creates a new `PostgresqlDatabase` from a user-managed pool.
    pub fn from_pool(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Database for PostgresqlDatabase {
    async fn execute_query(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| anyhow!("Failed to get Postgres connection: {}", e))?;

        let stmt = conn
            .prepare(sql)
            .await
            .map_err(|e| anyhow!("Failed to prepare query: {}", e))?;

        let rows = conn
            .query(&stmt, &[])
            .await
            .map_err(|e| anyhow!("Failed to execute query: {}", e))?;

        if rows.is_empty() {
            let fields: Vec<Field> = stmt
                .columns()
                .iter()
                .map(|col| {
                    let arrow_type = pg_type_to_arrow(col.type_(), col.type_modifier());
                    Field::new(col.name(), arrow_type, true)
                })
                .collect();
            let schema = Arc::new(Schema::new(fields));
            let batch = RecordBatch::new_empty(schema);
            return Ok(vec![batch]);
        }

        let columns = stmt.columns();
        let schema = Arc::new(Schema::new(
            columns
                .iter()
                .map(|col| Field::new(col.name(), pg_type_to_arrow(col.type_(), col.type_modifier()), true))
                .collect::<Vec<_>>(),
        ));

        let arrays = build_arrow_arrays(columns, &rows)?;
        let batch = RecordBatch::try_new(schema, arrays)
            .map_err(|e| anyhow!("Failed to create RecordBatch: {}", e))?;

        Ok(vec![batch])
    }

    async fn execute_row_count_query(&self, sql: &str) -> Result<usize> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| anyhow!("Failed to get Postgres connection: {}", e))?;

        let row = conn
            .query_one(sql, &[])
            .await
            .map_err(|e| anyhow!("Failed to execute row count query: {}", e))?;

        let count: i64 = row.get(0);
        Ok(count as usize)
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<Schema> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| anyhow!("Failed to get Postgres connection: {}", e))?;

        let query = format!("SELECT * FROM {} LIMIT 0", table_name);
        let stmt = conn
            .prepare(&query)
            .await
            .map_err(|e| anyhow!("Failed to prepare schema query: {}", e))?;

        let fields: Vec<Field> = stmt
            .columns()
            .iter()
            .map(|col| {
                let arrow_type = pg_type_to_arrow(col.type_(), col.type_modifier());
                Field::new(col.name(), arrow_type, true)
            })
            .collect();

        Ok(Schema::new(fields))
    }

    fn create_sql_query(&self, ast: &AnalyzedQuery) -> Result<String> {
        create_postgresql_query(ast)
    }
}

// --- Helper functions ---

/// Maps a Postgres type to its Arrow equivalent.
///
/// `type_modifier` in PostgreSQL defines the precision (p, total number of digits)
/// and scale (s, number of digits after the decimal point) for a NUMERIC column,
/// specified as NUMERIC(p, s). For NUMERIC it encodes precision and scale as
/// `((precision << 16) | scale) + VARHDRSZ`.
/// A value of -1 means unspecified (uses default precision=38, scale=0).
fn pg_type_to_arrow(pg_type: &Type, type_modifier: i32) -> DataType {
    match *pg_type {
        Type::BOOL => DataType::Boolean,
        Type::INT2 => DataType::Int16,
        Type::INT4 => DataType::Int32,
        Type::INT8 => DataType::Int64,
        Type::FLOAT4 => DataType::Float32,
        Type::FLOAT8 => DataType::Float64,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => DataType::Utf8,
        Type::BYTEA => DataType::Binary,
        Type::DATE => DataType::Date32,
        Type::TIMESTAMPTZ | Type::TIMESTAMP => {
            DataType::Timestamp(TimeUnit::Microsecond, None)
        }
        Type::NUMERIC => {
            // Extract precision and scale from type_modifier, or use defaults if not specified.
            let (precision, scale) = if type_modifier >= 0 {
                let modifier = type_modifier - 4; // subtract VARHDRSZ
                let precision = ((modifier >> 16) & 0xFFFF) as u8;
                let scale = (modifier & 0xFFFF) as i8;
                (precision.min(38), scale)
            } else {
                (38u8, 0i8)
            };
            DataType::Decimal128(precision, scale)
        }
        Type::UUID => DataType::FixedSizeBinary(16),
        Type::TIME | Type::TIMETZ => DataType::Time64(TimeUnit::Microsecond),
        Type::INTERVAL => DataType::Interval(IntervalUnit::MonthDayNano),
        Type::BOOL_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))),
        Type::INT2_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Int16, true))),
        Type::INT4_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
        Type::INT8_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
        Type::FLOAT4_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
        Type::FLOAT8_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Float64, true))),
        Type::TEXT_ARRAY | Type::VARCHAR_ARRAY => DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        Type::JSON | Type::JSONB => DataType::Utf8,
        _ => DataType::Utf8, // Fallback: represent as string
    }
}

/// Converts Postgres rows into a vector of Arrow arrays, one per column.
///
/// Each column is read according to its Postgres type and written into the
/// corresponding Arrow builder. Unknown types fall back to string conversion.
fn build_arrow_arrays(
    columns: &[tokio_postgres::Column],
    rows: &[Row],
) -> Result<Vec<ArrayRef>> {
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns.len());

    for (col_idx, col) in columns.iter().enumerate() {
        let array: ArrayRef = match *col.type_() {
            Type::BOOL => {
                let mut builder = BooleanBuilder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<bool> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::INT2 => {
                let mut builder = Int16Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<i16> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::INT4 => {
                let mut builder = Int32Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<i32> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::INT8 => {
                let mut builder = Int64Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<i64> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::FLOAT4 => {
                let mut builder = Float32Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<f32> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::FLOAT8 => {
                let mut builder = Float64Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<f64> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
                let mut builder = StringBuilder::with_capacity(rows.len(), rows.len() * 32);
                for row in rows {
                    let val: Option<&str> = row.get(col_idx);
                    builder.append_option(val);
                }
                Arc::new(builder.finish())
            }
            Type::BYTEA => {
                let mut builder = BinaryBuilder::with_capacity(rows.len(), rows.len() * 32);
                for row in rows {
                    let val: Option<&[u8]> = row.get(col_idx);
                    match val {
                        Some(v) => builder.append_value(v),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::DATE => {
                let mut builder = Date32Builder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<PgDate> = row.get(col_idx);
                    match val {
                        Some(d) => builder.append_value(d.0 + PG_EPOCH_DAYS_OFFSET),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::TIMESTAMP | Type::TIMESTAMPTZ => {
                let mut builder = TimestampMicrosecondBuilder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<PgTimestamp> = row.get(col_idx);
                    match val {
                        Some(ts) => builder.append_value(ts.0 + PG_EPOCH_MICROS_OFFSET),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::NUMERIC => {
                let (precision, scale) = match pg_type_to_arrow(col.type_(), col.type_modifier()) {
                    DataType::Decimal128(p, s) => (p, s),
                    _ => (38, 0),
                };
                let mut builder = Decimal128Builder::with_capacity(rows.len())
                    .with_precision_and_scale(precision, scale)
                    .map_err(|e| anyhow!("Invalid NUMERIC precision/scale: {}", e))?;
                for row in rows {
                    let val: Option<PgNumeric> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read NUMERIC column '{}': {}", col.name(), e))?;
                    match val {
                        Some(n) => builder.append_value(n.value),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::UUID => {
                let mut builder = FixedSizeBinaryBuilder::with_capacity(rows.len(), 16);
                for row in rows {
                    let val: Option<PgUuid> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read UUID column '{}': {}", col.name(), e))?;
                    match val {
                        Some(u) => builder.append_value(u.0)
                            .map_err(|e| anyhow!("Failed to append UUID value: {}", e))?,
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::TIME | Type::TIMETZ => {
                let mut builder = Time64MicrosecondBuilder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<PgTime> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read TIME column '{}': {}", col.name(), e))?;
                    match val {
                        Some(t) => builder.append_value(t.0),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::INTERVAL => {
                let mut builder = IntervalMonthDayNanoBuilder::with_capacity(rows.len());
                for row in rows {
                    let val: Option<PgInterval> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read INTERVAL column '{}': {}", col.name(), e))?;
                    match val {
                        Some(iv) => {
                            builder.append_value(IntervalMonthDayNano {
                                months: iv.months,
                                days: iv.days,
                                nanoseconds: iv.microseconds * 1_000,
                            });
                        }
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::BOOL_ARRAY => {
                let mut builder = ListBuilder::new(BooleanBuilder::new());
                for row in rows {
                    let val: Option<Vec<Option<bool>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read BOOL[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::INT2_ARRAY => {
                let mut builder = ListBuilder::new(Int16Builder::new());
                for row in rows {
                    let val: Option<Vec<Option<i16>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read INT2[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::INT4_ARRAY => {
                let mut builder = ListBuilder::new(Int32Builder::new());
                for row in rows {
                    let val: Option<Vec<Option<i32>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read INT4[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::INT8_ARRAY => {
                let mut builder = ListBuilder::new(Int64Builder::new());
                for row in rows {
                    let val: Option<Vec<Option<i64>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read INT8[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::FLOAT4_ARRAY => {
                let mut builder = ListBuilder::new(Float32Builder::new());
                for row in rows {
                    let val: Option<Vec<Option<f32>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read FLOAT4[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::FLOAT8_ARRAY => {
                let mut builder = ListBuilder::new(Float64Builder::new());
                for row in rows {
                    let val: Option<Vec<Option<f64>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read FLOAT8[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::TEXT_ARRAY | Type::VARCHAR_ARRAY => {
                let mut builder = ListBuilder::new(StringBuilder::new());
                for row in rows {
                    let val: Option<Vec<Option<String>>> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read TEXT[] column '{}': {}", col.name(), e))?;
                    match val {
                        Some(arr) => {
                            for v in arr {
                                builder.values().append_option(v);
                            }
                            builder.append(true);
                        }
                        None => builder.append(false),
                    }
                }
                Arc::new(builder.finish())
            }
            Type::JSON | Type::JSONB => {
                let mut builder = StringBuilder::with_capacity(rows.len(), rows.len() * 64);
                for row in rows {
                    // serde_json::Value implements FromSql for JSON/JSONB
                    let val: Option<serde_json::Value> = row.get(col_idx);
                    match val {
                        Some(v) => builder.append_value(v.to_string()),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            // Fallback: read as string
            _ => {
                let mut builder = StringBuilder::with_capacity(rows.len(), rows.len() * 32);
                for row in rows {
                    let val: Option<String> = row.try_get(col_idx)
                        .map_err(|e| anyhow!("Failed to read column '{}' as String: {}", col.name(), e))?;
                    builder.append_option(val.as_deref());
                }
                Arc::new(builder.finish())
            }
        };
        arrays.push(array);
    }

    Ok(arrays)
}

// --- Helper structs ---

/// Days between the Unix epoch (1970-01-01) and the Postgres epoch (2000-01-01).
const PG_EPOCH_DAYS_OFFSET: i32 = 10_957;

/// Microseconds between the Unix epoch and the Postgres epoch.
const PG_EPOCH_MICROS_OFFSET: i64 = 946_684_800_000_000;

/// Wrapper that reads a Postgres UUID column as 16 raw bytes.
struct PgUuid([u8; 16]);

impl<'a> FromSql<'a> for PgUuid {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let bytes: [u8; 16] = raw.try_into().map_err(|_| "invalid UUID length")?;
        Ok(PgUuid(bytes))
    }
    accepts!(UUID);
}

/// Wrapper that reads a Postgres TIME/TIMETZ column as raw `i64` microseconds since midnight.
///
/// For TIMETZ, the timezone offset (4 bytes) is ignored — only the time portion is kept.
struct PgTime(i64);

impl<'a> FromSql<'a> for PgTime {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() < 8 {
            return Err("invalid TIME length".into());
        }
        let bytes: [u8; 8] = raw[..8].try_into().unwrap();
        Ok(PgTime(i64::from_be_bytes(bytes)))
    }
    accepts!(TIME, TIMETZ);
}

/// Wrapper that reads a Postgres INTERVAL as months, days, and microseconds.
///
/// Postgres INTERVAL binary format: i64 microseconds + i32 days + i32 months (16 bytes).
struct PgInterval {
    microseconds: i64,
    days: i32,
    months: i32,
}

impl<'a> FromSql<'a> for PgInterval {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() < 16 {
            return Err("invalid INTERVAL length".into());
        }
        let microseconds = i64::from_be_bytes(raw[0..8].try_into().unwrap());
        let days = i32::from_be_bytes(raw[8..12].try_into().unwrap());
        let months = i32::from_be_bytes(raw[12..16].try_into().unwrap());
        Ok(PgInterval { microseconds, days, months })
    }
    accepts!(INTERVAL);
}

/// Wrapper that reads a Postgres DATE column as raw `i32` days since 2000-01-01.
struct PgDate(i32);

impl<'a> FromSql<'a> for PgDate {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let bytes: [u8; 4] = raw.try_into().map_err(|_| "invalid DATE length")?;
        Ok(PgDate(i32::from_be_bytes(bytes)))
    }
    accepts!(DATE);
}

/// Wrapper that reads a Postgres TIMESTAMP column as raw `i64` microseconds since 2000-01-01.
struct PgTimestamp(i64);

impl<'a> FromSql<'a> for PgTimestamp {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let bytes: [u8; 8] = raw.try_into().map_err(|_| "invalid TIMESTAMP length")?;
        Ok(PgTimestamp(i64::from_be_bytes(bytes)))
    }
    accepts!(TIMESTAMP, TIMESTAMPTZ);
}

/// Wrapper that decodes a Postgres NUMERIC column into `i128` with its scale.
///
/// Postgres transmits NUMERIC in a base-10000 binary format:
/// - 2 bytes: ndigits (number of base-10000 digit groups)
/// - 2 bytes: weight (exponent of the first group, in base-10000)
/// - 2 bytes: sign (0x0000=positive, 0x4000=negative, 0xC000=NaN)
/// - 2 bytes: dscale (number of digits after the decimal point)
/// - ndigits × 2 bytes: base-10000 digit groups
///
/// The value is reconstructed as an `i128` scaled by `10^dscale`, matching
/// Arrow's `Decimal128(precision, scale)` representation.
struct PgNumeric {
    value: i128,
}

const PG_NUMERIC_NEG: u16 = 0x4000;
const PG_NUMERIC_NAN: u16 = 0xC000;
const PG_BASE: i128 = 10_000;

impl<'a> FromSql<'a> for PgNumeric {
    fn from_sql(_ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() < 8 {
            return Err("NUMERIC value too short".into());
        }
        let ndigits = u16::from_be_bytes([raw[0], raw[1]]) as usize;
        let weight = i16::from_be_bytes([raw[2], raw[3]]);
        let sign = u16::from_be_bytes([raw[4], raw[5]]);
        let dscale = u16::from_be_bytes([raw[6], raw[7]]);

        if sign == PG_NUMERIC_NAN {
            return Err("NUMERIC NaN cannot be represented as Decimal128".into());
        }

        if ndigits == 0 {
            return Ok(PgNumeric { value: 0 });
        }

        // Reconstruct the unscaled integer from base-10000 digit groups.
        let mut result: i128 = 0;
        for i in 0..ndigits {
            let offset = 8 + i * 2;
            let digit = u16::from_be_bytes([raw[offset], raw[offset + 1]]) as i128;
            result = result.checked_mul(PG_BASE)
                .ok_or("NUMERIC value overflows i128")?;
            result = result.checked_add(digit)
                .ok_or("NUMERIC value overflows i128")?;
        }

        // The digit groups cover base-10000 positions weight..=(weight - ndigits + 1).
        // Current implicit decimal exponent of `result` is (weight - ndigits + 1) * 4.
        // Arrow Decimal128 stores value * 10^scale, so target exponent is -dscale.
        let current_exponent = (weight as i32 - ndigits as i32 + 1) * 4;
        let target_exponent = -(dscale as i32);
        let shift = current_exponent - target_exponent;

        if shift > 0 {
            for _ in 0..shift {
                result = result.checked_mul(10)
                    .ok_or("NUMERIC value overflows i128")?;
            }
        } else if shift < 0 {
            for _ in 0..(-shift) {
                result /= 10;
            }
        }

        if sign == PG_NUMERIC_NEG {
            result = -result;
        }

        Ok(PgNumeric { value: result })
    }
    accepts!(NUMERIC);
}