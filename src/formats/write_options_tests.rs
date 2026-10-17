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
    assert!(!o.csv.decimal_comma);
    assert!(!o.csv.bom);
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
