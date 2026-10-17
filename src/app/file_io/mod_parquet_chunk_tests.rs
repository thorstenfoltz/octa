use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use super::load_remaining_parquet_rows;

/// Chunks of a Parquet file with small row groups: `with_offset` skips the
/// groups before the chunk, and together the chunks hold every row once.
#[test]
fn parquet_chunks_skip_to_their_offset() {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use parquet::file::properties::WriterProperties;

    let f = tempfile::NamedTempFile::new().unwrap();
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from_iter_values(0..10))],
    )
    .unwrap();
    let props = WriterProperties::builder()
        .set_max_row_group_row_count(Some(3))
        .build();
    let mut w = ArrowWriter::try_new(f.reopen().unwrap(), schema, Some(props)).unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();

    let chunk = |skip: usize| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let exhausted = Arc::new(AtomicBool::new(false));
        load_remaining_parquet_rows(
            f.path(),
            skip,
            4,
            buffer.clone(),
            Arc::new(AtomicBool::new(false)),
            exhausted.clone(),
        )
        .unwrap();
        let rows = std::mem::take(&mut *buffer.lock().unwrap());
        let ids: Vec<String> = rows.iter().map(|r| r[0].to_string()).collect();
        (ids, exhausted.load(std::sync::atomic::Ordering::Relaxed))
    };
    assert_eq!(
        chunk(2),
        (vec!["2".into(), "3".into(), "4".into(), "5".into()], false)
    );
    assert_eq!(
        chunk(6),
        (vec!["6".into(), "7".into(), "8".into(), "9".into()], false)
    );
    assert_eq!(chunk(10), (vec![], true));
}
