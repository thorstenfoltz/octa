//! `octa --export-schema <FILE>` — render FILE's column schema as SQL
//! DDL / a Pydantic model / a TypeScript interface / JSON Schema / a
//! Rust struct, printed to stdout.
//!
//! The dialect / language is chosen with `-t` / `--target` (see
//! [`super::SchemaTargetArg`]). The rendering itself is delegated to the
//! pure `octa::data::schema_export` functions — this file only loads the
//! table and derives a table name from the file stem.

use std::path::PathBuf;

use octa::data::schema_export::SchemaTarget;

pub fn run(path: PathBuf, target: SchemaTarget) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    // Same rule the GUI dialog uses: the file stem becomes the table /
    // class / struct name, falling back to `data` for odd paths. The
    // renderer sanitises it further (quoting, PascalCase, …).
    let table_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string());
    let rendered = target.export(&table.columns, &table_name);
    print!("{rendered}");
    Ok(())
}
