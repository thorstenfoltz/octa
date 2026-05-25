//! Schema export: render an octa `DataTable`'s column list into another
//! language or DSL. Targets currently supported:
//!
//! * SQL DDL (Postgres, MySQL, SQLite, Databricks, Snowflake) —
//!   `CREATE TABLE …` statements
//! * Pydantic v2 — a `BaseModel` subclass with annotated fields
//! * TypeScript — an `interface` declaration
//! * JSON Schema (draft 2020-12)
//! * Rust — a `struct` with serde derives
//!
//! Each target's `export` function is a pure transformation
//! `(&[ColumnInfo], &str) -> String`. UI lives separately in
//! `src/app/dialogs/schema_export.rs`. The strict separation keeps
//! every renderer integration-testable without a GUI.
//!
//! Adding a new target is a drop-in: create `targets/<name>.rs` with
//! a `pub fn export(columns: &[ColumnInfo], table_name: &str) -> String`,
//! then extend [`SchemaTarget`] with a new variant + the four trivial
//! `match` arms (label, extension, syntax_lang, export).
//!
//! Type-name input is the same Arrow-style string we put in
//! `ColumnInfo.data_type` ("Utf8", "Int64", "Timestamp(Microsecond,
//! None)" …). Unknown types fall through to each target's TEXT-
//! equivalent with a `/* TODO: unknown Arrow type "<name>" */` (or
//! the language's comment equivalent) so the output is never silently
//! wrong.

pub mod json_schema;
pub mod pydantic;
pub mod rust;
pub mod sql;
pub mod typescript;

use crate::data::ColumnInfo;

/// Identifier the user picks in **View → Export schema as…**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaTarget {
    PostgresSqlDdl,
    MysqlSqlDdl,
    SqliteSqlDdl,
    DatabricksSqlDdl,
    SnowflakeSqlDdl,
    PydanticV2,
    TypeScript,
    JsonSchema,
    RustStruct,
}

impl SchemaTarget {
    /// Every target, ordered alphabetically by [`label`](Self::label) —
    /// the order the Export Schema dialog's chip row shows them in.
    /// Adding a new variant flows through automatically; slot it into
    /// its alphabetical position.
    pub const ALL: &'static [SchemaTarget] = &[
        SchemaTarget::DatabricksSqlDdl,
        SchemaTarget::JsonSchema,
        SchemaTarget::MysqlSqlDdl,
        SchemaTarget::PostgresSqlDdl,
        SchemaTarget::PydanticV2,
        SchemaTarget::RustStruct,
        SchemaTarget::SnowflakeSqlDdl,
        SchemaTarget::SqliteSqlDdl,
        SchemaTarget::TypeScript,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PostgresSqlDdl => "Postgres",
            Self::MysqlSqlDdl => "MySQL",
            Self::SqliteSqlDdl => "SQLite",
            Self::DatabricksSqlDdl => "Databricks",
            Self::SnowflakeSqlDdl => "Snowflake",
            Self::PydanticV2 => "Pydantic v2",
            Self::TypeScript => "TypeScript interface",
            Self::JsonSchema => "JSON Schema",
            Self::RustStruct => "Rust struct",
        }
    }

    /// File extension the Save-as dialog should default to.
    pub fn extension(self) -> &'static str {
        match self {
            Self::PostgresSqlDdl
            | Self::MysqlSqlDdl
            | Self::SqliteSqlDdl
            | Self::DatabricksSqlDdl
            | Self::SnowflakeSqlDdl => "sql",
            Self::PydanticV2 => "py",
            Self::TypeScript => "ts",
            Self::JsonSchema => "json",
            Self::RustStruct => "rs",
        }
    }

    /// Hint for the syntax highlighter so the preview pane colors
    /// correctly. Matches `syntect`'s short language names where
    /// possible; the preview falls back to plain text if syntect
    /// doesn't recognise it.
    pub fn syntax_lang(self) -> &'static str {
        match self {
            Self::PostgresSqlDdl
            | Self::MysqlSqlDdl
            | Self::SqliteSqlDdl
            | Self::DatabricksSqlDdl
            | Self::SnowflakeSqlDdl => "sql",
            Self::PydanticV2 => "py",
            Self::TypeScript => "ts",
            Self::JsonSchema => "json",
            Self::RustStruct => "rs",
        }
    }

    /// Render the column list to this target's source. `table_name` is
    /// the identifier used for `CREATE TABLE`, the class / interface /
    /// struct name, etc. Callers typically pass the source filename's
    /// stem (or `"data"` for unsaved tabs).
    pub fn export(self, columns: &[ColumnInfo], table_name: &str) -> String {
        match self {
            Self::PostgresSqlDdl => sql::export_postgres(columns, table_name),
            Self::MysqlSqlDdl => sql::export_mysql(columns, table_name),
            Self::SqliteSqlDdl => sql::export_sqlite(columns, table_name),
            Self::DatabricksSqlDdl => sql::export_databricks(columns, table_name),
            Self::SnowflakeSqlDdl => sql::export_snowflake(columns, table_name),
            Self::PydanticV2 => pydantic::export(columns, table_name),
            Self::TypeScript => typescript::export(columns, table_name),
            Self::JsonSchema => json_schema::export(columns, table_name),
            Self::RustStruct => rust::export(columns, table_name),
        }
    }
}

/// Sanitise a string into a safe identifier for a target language.
/// First char must be `[A-Za-z_]`; remaining `[A-Za-z0-9_]`. Anything
/// outside that set becomes `_`. Empty input returns `_`.
pub(crate) fn sanitize_ident(s: &str) -> String {
    if s.is_empty() {
        return "_".to_string();
    }
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        let ok = if i == 0 {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        out.push(if ok { c } else { '_' });
    }
    out
}

/// Whether `s` is already a valid identifier per [`sanitize_ident`]'s
/// rules — used to decide whether a generated alias / rename
/// annotation is needed.
pub(crate) fn is_safe_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().enumerate().all(|(i, c)| {
            if i == 0 {
                c.is_ascii_alphabetic() || c == '_'
            } else {
                c.is_ascii_alphanumeric() || c == '_'
            }
        })
}

/// Pick a fallback when the data_type string isn't recognised. Each
/// target has its own TEXT-equivalent and comment syntax for the
/// `unknown:` annotation.
pub(crate) fn unknown_marker(data_type: &str) -> String {
    format!("unknown Arrow type \"{}\"", data_type)
}
