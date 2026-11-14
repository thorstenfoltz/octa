//! The Parquet files reference data lives in: exact decimals, zstd at its
//! highest level, the source's address and validators in the footer, and a
//! write that never leaves half a file.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, bail};
use arrow::array::{Array, ArrayRef, Decimal128Array, Int32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::{Compression, Encoding, ZstdLevel};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::{WriterProperties, WriterPropertiesBuilder, WriterVersion};
use parquet::schema::types::ColumnPath;

pub const META_SOURCE: &str = "octa.source";
pub const META_ETAG: &str = "octa.etag";
pub const META_LAST_MODIFIED: &str = "octa.last_modified";
pub const META_DOWNLOADED: &str = "octa.downloaded";
pub const META_LICENCE: &str = "octa.licence";

#[derive(Debug, Clone, PartialEq)]
pub enum RefColumn {
    Text(Vec<Option<String>>),
    /// Exact decimals as integers at `scale` digits after the point.
    Decimal {
        scale: u32,
        values: Vec<Option<i128>>,
    },
    /// Whole numbers; coordinates are 1/10,000 degree.
    Int(Vec<Option<i32>>),
}

impl RefColumn {
    fn len(&self) -> usize {
        match self {
            RefColumn::Text(v) => v.len(),
            RefColumn::Decimal { values, .. } => values.len(),
            RefColumn::Int(v) => v.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefTable {
    pub names: Vec<String>,
    pub columns: Vec<RefColumn>,
    pub meta: BTreeMap<String, String>,
}

impl RefTable {
    pub fn rows(&self) -> usize {
        self.columns.first().map_or(0, RefColumn::len)
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    pub fn text(&self, name: &str) -> Option<&[Option<String>]> {
        match self.columns.get(self.index(name)?)? {
            RefColumn::Text(v) => Some(v),
            _ => None,
        }
    }
}

/// `<config dir>/reference`, created on first write.
pub fn dir() -> anyhow::Result<PathBuf> {
    crate::ui::settings::AppSettings::config_dir()
        .map(|d| d.join("reference"))
        .context("no config directory: set OCTA_CONFIG_DIR to a writable path")
}

fn column_path(name: &str) -> ColumnPath {
    ColumnPath::new(vec![name.to_string()])
}

/// Delta encoding without a dictionary: far smaller for slowly changing integers.
fn delta(props: WriterPropertiesBuilder, name: &str) -> WriterPropertiesBuilder {
    props
        .set_column_dictionary_enabled(column_path(name), false)
        .set_column_encoding(column_path(name), Encoding::DELTA_BINARY_PACKED)
}

pub fn write(path: &Path, t: &RefTable) -> anyhow::Result<()> {
    let rows = t.rows();
    if t.columns.iter().any(|c| c.len() != rows) || t.names.len() != t.columns.len() {
        bail!("columns of different lengths for {}", path.display());
    }
    let mut props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(22)?))
        .set_writer_version(WriterVersion::PARQUET_2_0)
        .set_key_value_metadata(Some(
            t.meta
                .iter()
                .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
                .collect(),
        ));
    let mut fields = Vec::with_capacity(t.columns.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(t.columns.len());
    for (name, col) in t.names.iter().zip(&t.columns) {
        match col {
            RefColumn::Text(v) => {
                fields.push(Field::new(name, DataType::Utf8, true));
                arrays.push(Arc::new(StringArray::from(v.clone())));
            }
            RefColumn::Decimal { scale, values } => {
                let narrow = values
                    .iter()
                    .flatten()
                    .all(|r| r.unsigned_abs() < 10u128.pow(18));
                let precision = if narrow { 18 } else { 38 };
                let scale = i8::try_from(*scale).context("more than 127 decimals")?;
                let array = Decimal128Array::from(values.clone())
                    .with_precision_and_scale(precision, scale)?;
                fields.push(Field::new(
                    name,
                    DataType::Decimal128(precision, scale),
                    true,
                ));
                arrays.push(Arc::new(array));
                if narrow {
                    // INT64 storage; the wide case is a byte array, where delta is invalid.
                    props = delta(props, name);
                }
            }
            RefColumn::Int(v) => {
                fields.push(Field::new(name, DataType::Int32, true));
                arrays.push(Arc::new(Int32Array::from(v.clone())));
                props = delta(props, name);
            }
        }
    }
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema.clone(), arrays)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let part = path.with_extension("parquet.part");
    let written = (|| -> anyhow::Result<()> {
        let mut w = ArrowWriter::try_new(File::create(&part)?, schema, Some(props.build()))?;
        w.write(&batch)?;
        w.close()?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e.context(format!("writing {}", path.display())));
    }
    std::fs::rename(&part, path).with_context(|| format!("replacing {}", path.display()))
}

fn meta_of(builder: &ParquetRecordBatchReaderBuilder<File>) -> BTreeMap<String, String> {
    builder
        .metadata()
        .file_metadata()
        .key_value_metadata()
        .map(|kv| {
            kv.iter()
                .filter_map(|k| Some((k.key.clone(), k.value.clone()?)))
                .collect()
        })
        .unwrap_or_default()
}

pub fn read_meta(path: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(meta_of(&ParquetRecordBatchReaderBuilder::try_new(file)?))
}

pub fn read(path: &Path) -> anyhow::Result<RefTable> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let meta = meta_of(&builder);
    let schema = builder.schema().clone();
    let mut t = RefTable {
        names: schema.fields().iter().map(|f| f.name().clone()).collect(),
        columns: Vec::new(),
        meta,
    };
    for f in schema.fields() {
        t.columns.push(match f.data_type() {
            DataType::Utf8 => RefColumn::Text(Vec::new()),
            DataType::Decimal128(_, s) => RefColumn::Decimal {
                scale: u32::try_from(*s).context("negative scale")?,
                values: Vec::new(),
            },
            DataType::Int32 => RefColumn::Int(Vec::new()),
            other => bail!("{}: unexpected column type {other}", path.display()),
        });
    }
    for batch in builder.build()? {
        let batch = batch?;
        for (i, col) in t.columns.iter_mut().enumerate() {
            let a = batch.column(i);
            match col {
                RefColumn::Text(v) => {
                    let a = a
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .context("text column")?;
                    v.extend(a.iter().map(|x| x.map(str::to_string)));
                }
                RefColumn::Decimal { values, .. } => {
                    let a = a
                        .as_any()
                        .downcast_ref::<Decimal128Array>()
                        .context("decimal column")?;
                    values.extend(a.iter());
                }
                RefColumn::Int(v) => {
                    let a = a
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .context("int column")?;
                    v.extend(a.iter());
                }
            }
        }
    }
    Ok(t)
}

pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Mark a file as checked now without rewriting it (a 304 answer).
pub fn touch(path: &Path) -> std::io::Result<()> {
    File::options()
        .write(true)
        .open(path)?
        .set_modified(SystemTime::now())
}

/// Bytes on disk; missing files count 0.
pub fn size_of(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
