use super::*;

/// Hand-build a minimal little-endian `.npy` for a 1-D f8 array.
fn npy_1d_f64(values: &[f64]) -> Vec<u8> {
    let header = format!(
        "{{'descr': '<f8', 'fortran_order': False, 'shape': ({},), }}",
        values.len()
    );
    build_npy(&header, |out| {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    })
}

/// Hand-build a 2x2 int32 `.npy`.
fn npy_2x2_i32(values: &[i32; 4]) -> Vec<u8> {
    let header = "{'descr': '<i4', 'fortran_order': False, 'shape': (2, 2), }".to_string();
    build_npy(&header, |out| {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    })
}

fn build_npy(header: &str, body: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    // Magic + version 1.0, then a 2-byte header length, padded so the data
    // starts on a 64-byte boundary (the .npy spec).
    let mut head = header.as_bytes().to_vec();
    let prefix = 10; // magic(6) + version(2) + len(2)
    let unpadded = prefix + head.len() + 1; // +1 for the trailing newline
    let pad = (64 - (unpadded % 64)) % 64;
    head.extend(std::iter::repeat_n(b' ', pad));
    head.push(b'\n');

    let mut out = Vec::new();
    out.extend_from_slice(b"\x93NUMPY");
    out.push(1);
    out.push(0);
    out.extend_from_slice(&(head.len() as u16).to_le_bytes());
    out.extend_from_slice(&head);
    body(&mut out);
    out
}

#[test]
fn reads_1d_float_array() {
    let bytes = npy_1d_f64(&[1.5, 2.5, 3.5]);
    let table = npy_bytes_to_table(&bytes).expect("parse 1-D npy");
    assert_eq!(table.columns.len(), 1);
    assert_eq!(table.columns[0].name, "value");
    assert_eq!(table.columns[0].data_type, "Float64");
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.get(0, 0), Some(&CellValue::Float(1.5)));
    assert_eq!(table.get(2, 0), Some(&CellValue::Float(3.5)));
}

#[test]
fn reads_2d_int_array_into_columns() {
    let bytes = npy_2x2_i32(&[1, 2, 3, 4]);
    let table = npy_bytes_to_table(&bytes).expect("parse 2-D npy");
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].name, "col_0");
    assert_eq!(table.columns[1].name, "col_1");
    assert_eq!(table.columns[0].data_type, "Int32");
    assert_eq!(table.row_count(), 2);
    // Row-major: [[1, 2], [3, 4]].
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(table.get(0, 1), Some(&CellValue::Int(2)));
    assert_eq!(table.get(1, 0), Some(&CellValue::Int(3)));
    assert_eq!(table.get(1, 1), Some(&CellValue::Int(4)));
}
