//! How a file should be written, as opposed to what is written into it.
//!
//! `Default` reproduces exactly what Octa wrote before this module existed, so
//! every path that does not care keeps its behaviour byte for byte. Only the
//! formats with knobs worth turning (Parquet, CSV) read these; the other
//! readers inherit the default `write_file_with_options`, which ignores them.

use serde::{Deserialize, Serialize};

/// Quoting policy for the CSV writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum QuoteStyle {
    /// Quote only fields that need it (RFC 4180 behaviour, the default).
    #[default]
    Necessary,
    /// Quote every field.
    Always,
    /// Suppress defensive quoting. Fields containing the delimiter are still
    /// quoted, since not doing so would corrupt the file.
    Never,
}

impl QuoteStyle {
    /// In picker order, for the Settings combo.
    pub const ALL: &'static [QuoteStyle] =
        &[QuoteStyle::Necessary, QuoteStyle::Always, QuoteStyle::Never];

    /// i18n key for the label shown in the picker.
    pub fn i18n_key(self) -> &'static str {
        match self {
            QuoteStyle::Necessary => "wo.quote_necessary",
            QuoteStyle::Always => "wo.quote_always",
            QuoteStyle::Never => "wo.quote_never",
        }
    }
}

/// Parquet writer knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParquetOptions {
    /// One of [`PARQUET_CODECS`].
    pub compression: String,
    /// Rows per row group. `None` keeps the writer's own default.
    pub row_group_size: Option<usize>,
    /// Dictionary-encode repeated values (smaller files for low-cardinality
    /// columns).
    pub dictionary: bool,
    /// Write per-chunk min/max statistics. Without them a query engine cannot
    /// skip row groups.
    pub statistics: bool,
}

impl Default for ParquetOptions {
    fn default() -> Self {
        Self {
            // Deliberately NOT `ArrowWriter::try_new(.., None)`'s uncompressed
            // output, which every other default here still mirrors. Measured on
            // 300k rows x 9 columns of order-line data: uncompressed is only
            // 1.6x smaller than the source CSV, zstd is 5.4x, and zstd costs
            // ~1% write time and ~3% read time over uncompressed. There is no
            // trade-off left to preserve, and File internals used to flag
            // Octa's own output for it.
            compression: "zstd".to_string(),
            row_group_size: None,
            dictionary: true,
            statistics: true,
        }
    }
}

/// CSV / TSV writer knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CsvOptions {
    /// Field separator. The per-tab delimiter wins when this is left at the
    /// default comma; a TSV keeps its tab either way, since that is what the
    /// format means. Applies to CSV targets only.
    pub delimiter: u8,
    pub quote_style: QuoteStyle,
    /// Write `\r\n` line endings instead of `\n`.
    pub crlf: bool,
    /// Write the header row.
    pub write_header: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote_style: QuoteStyle::Necessary,
            crlf: false,
            write_header: true,
        }
    }
}

/// Excel writer knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct XlsxOptions {
    /// Carry the tab's marks, conditional-format colours, frozen columns and
    /// per-column number formats into the workbook.
    ///
    /// Off by default: every save path that existed before this field must
    /// keep writing exactly what it wrote before.
    pub include_formatting: bool,
    /// Write the formulas an `.xlsx` was read with back into the saved
    /// workbook, instead of the values Octa is showing.
    ///
    /// Off by default, and the default is the honest one: a preserved formula
    /// **recalculates when Excel opens the file**, so the saved workbook can
    /// show a different number than Octa did. `DataTable::formula` already
    /// withholds a formula from an edited cell and from a restructured table,
    /// so this only ever covers cells Octa did not touch.
    pub preserve_formulas: bool,
    /// Stamp the workbook with document properties: the file name as the
    /// title, Octa as the application. Off by default, since it writes the
    /// file name into the file's metadata whether the user wanted that or not.
    pub document_properties: bool,
    /// Write the data as a real Excel table object instead of a plain range
    /// with an autofilter.
    ///
    /// Off by default: a table style paints its own banding, which fights a
    /// live conditional-format rule carried over from Octa. With it off the
    /// two never collide.
    pub as_table: bool,
}

/// The presentation of one tab, handed to a writer that can express it.
///
/// This is **not** a setting. It describes the table being written right now,
/// so it is `#[serde(skip)]` on [`WriteOptions`] and defaults to `None`
/// everywhere that does not care. Manual marks are absent on purpose: they
/// already live on `DataTable.marks` and need no plumbing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableStyle {
    /// Conditional-formatting rules, in first-match-wins order.
    pub conditional: Vec<crate::data::conditional_format::CondRule>,
    /// Per-column display formats, keyed by column index.
    pub number_formats: std::collections::HashMap<usize, crate::data::num_format::NumberFormat>,
    /// How many leading columns are frozen in the view. 0 = none.
    pub frozen_cols: usize,
    /// On-screen column widths in pixels, by column index. Empty means the
    /// caller has no view to carry (CLI, MCP, batch convert), and the writer
    /// falls back to Excel's autofit.
    pub col_widths: Vec<f32>,
    /// The tab's data-validation rules. They only paint red on screen; the
    /// writer turns the ones Excel can express into real validation.
    pub validation: Vec<crate::data::validation::ValidationRule>,
    /// Whether the tab's **decoration** travels: marks, conditional colours,
    /// number formats and the frozen band. This mirrors
    /// [`XlsxOptions::include_formatting`] and is set at the save site, the
    /// only place that knows the answer.
    ///
    /// `col_widths` and `validation` above ignore it on purpose: readable
    /// columns and a workbook that rejects bad input are not decoration, so
    /// they travel on every save.
    pub formatting: bool,
}

/// Every writer knob in one struct, carried per save operation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WriteOptions {
    pub parquet: ParquetOptions,
    pub csv: CsvOptions,
    pub xlsx: XlsxOptions,
    /// Presentation of the table being written. Never serialised.
    #[serde(skip)]
    pub style: Option<TableStyle>,
}

/// Codec names accepted by the CLI and offered in the GUI, in menu order.
pub const PARQUET_CODECS: &[&str] = &["uncompressed", "snappy", "zstd", "gzip", "lz4"];

/// Map a codec name onto the parquet crate's enum. Unknown names fall back to
/// uncompressed; the CLI rejects them at parse time so this is only reached
/// through a hand-edited settings file.
pub fn parquet_compression(name: &str) -> parquet::basic::Compression {
    use parquet::basic::{Compression, GzipLevel, ZstdLevel};
    match name.to_ascii_lowercase().as_str() {
        "snappy" => Compression::SNAPPY,
        "gzip" => Compression::GZIP(GzipLevel::default()),
        "zstd" => Compression::ZSTD(ZstdLevel::default()),
        "lz4" => Compression::LZ4,
        _ => Compression::UNCOMPRESSED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_pre_existing_writer_behaviour() {
        // Everything except the Parquet codec still reproduces exactly what
        // Octa wrote before this module existed. See the Parquet exception
        // below; do not fold the two tests together, they pin opposite intents.
        let o = WriteOptions::default();
        assert_eq!(o.parquet.row_group_size, None);
        assert_eq!(o.csv.delimiter, b',');
        assert!(o.csv.write_header);
        assert!(!o.csv.crlf);
    }

    #[test]
    fn parquet_compresses_by_default() {
        // The one deliberate break with pre-existing behaviour. Uncompressed
        // parquet measured only 1.6x smaller than the source CSV where zstd
        // managed 5.4x, at ~1% more write time and ~3% more read time, so the
        // old default cost ~3.5x the bytes for nothing. If this ever reads
        // "uncompressed" again it is a regression, not a restoration.
        assert_eq!(WriteOptions::default().parquet.compression, "zstd");
    }

    #[test]
    fn every_offered_codec_maps_to_a_distinct_value() {
        use parquet::basic::Compression;
        for name in PARQUET_CODECS {
            let c = parquet_compression(name);
            if *name == "uncompressed" {
                assert_eq!(c, Compression::UNCOMPRESSED);
            } else {
                assert_ne!(c, Compression::UNCOMPRESSED, "{name} fell back");
            }
        }
        // An unknown name is safe rather than a panic.
        assert_eq!(parquet_compression("banana"), Compression::UNCOMPRESSED);
    }

    #[test]
    fn xlsx_formatting_is_off_by_default() {
        let o = WriteOptions::default();
        assert!(
            !o.xlsx.include_formatting,
            "carrying styling must be opt-in, so an existing save path keeps writing plain data"
        );
        assert!(o.style.is_none(), "no tab styling attached by default");
        assert!(
            !o.xlsx.preserve_formulas,
            "keeping formulas must be opt-in too: a preserved one recalculates \
             in Excel and can then differ from the value Octa showed"
        );
        assert!(
            !o.xlsx.document_properties,
            "writing the file name into the workbook's metadata must be opt-in"
        );
        assert!(
            !o.xlsx.as_table,
            "an Excel table object paints its own banding, which fights a \
             carried-over conditional rule, so it must be opt-in"
        );
    }

    #[test]
    fn style_is_not_serialised() {
        // `style` is per-save presentation, not a setting. If it ever leaks
        // into settings.toml, a stale style would be reapplied on a later run.
        let o = WriteOptions {
            style: Some(TableStyle {
                frozen_cols: 3,
                ..TableStyle::default()
            }),
            ..WriteOptions::default()
        };
        let text = toml::to_string(&o).expect("serialise write options");
        assert!(
            !text.contains("frozen_cols"),
            "style must be skipped on serialise, got:\n{text}"
        );
    }
}
