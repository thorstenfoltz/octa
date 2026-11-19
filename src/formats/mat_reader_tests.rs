//! Version 5 files built by hand, for the edges the scipy and hdf5storage
//! fixtures (`tests/mat_tests.rs`) do not reach.

use super::*;

/// Builds version 5 elements the way MATLAB does: four bytes or less as a
/// small element, everything else padded to eight bytes.
struct Builder {
    big: bool,
}

impl Builder {
    fn u32(&self, v: u32) -> [u8; 4] {
        if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    }

    fn el(&self, ty: u32, data: &[u8]) -> Vec<u8> {
        if !data.is_empty() && data.len() <= 4 {
            let mut out = self.u32((data.len() as u32) << 16 | ty).to_vec();
            out.extend_from_slice(data);
            out.resize(8, 0);
            return out;
        }
        let mut out = self.u32(ty).to_vec();
        out.extend(self.u32(data.len() as u32));
        out.extend_from_slice(data);
        out.resize(8 + data.len().div_ceil(8) * 8, 0);
        out
    }

    fn doubles(&self, values: &[f64]) -> Vec<u8> {
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|v| {
                if self.big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            })
            .collect();
        self.el(9, &bytes)
    }

    fn matrix(&self, class: u32, dims: &[u32], name: &str, body: &[u8]) -> Vec<u8> {
        let mut flags = self.u32(class).to_vec();
        flags.extend(self.u32(0));
        let mut m = self.el(6, &flags);
        let dims: Vec<u8> = dims.iter().flat_map(|d| self.u32(*d)).collect();
        m.extend(self.el(5, &dims));
        m.extend(self.el(1, name.as_bytes()));
        m.extend_from_slice(body);
        self.el(14, &m)
    }

    fn text(&self, name: &str, s: &str) -> Vec<u8> {
        let codes: Vec<u8> = s
            .encode_utf16()
            .flat_map(|c| {
                if self.big {
                    c.to_be_bytes()
                } else {
                    c.to_le_bytes()
                }
            })
            .collect();
        // MATLAB counts text in UTF-16 units.
        self.matrix(
            4,
            &[1, s.encode_utf16().count() as u32],
            name,
            &self.el(4, &codes),
        )
    }

    fn file(&self, elements: &[Vec<u8>]) -> tempfile::NamedTempFile {
        let mut bytes = b"MATLAB 5.0 MAT-file, written by hand".to_vec();
        bytes.resize(124, b' ');
        bytes.extend(if self.big { [1u8, 0] } else { [0, 1] });
        bytes.extend(if self.big { *b"MI" } else { *b"IM" });
        for e in elements {
            bytes.extend_from_slice(e);
        }
        let file = tempfile::Builder::new().suffix(".mat").tempfile().unwrap();
        std::fs::write(file.path(), bytes).unwrap();
        file
    }
}

fn cells(t: &DataTable, col: usize) -> Vec<String> {
    t.rows.iter().map(|r| r[col].to_string()).collect()
}

#[test]
fn whole_doubles_are_whole_numbers_and_nan_is_empty() {
    let b = Builder { big: false };
    let f = b.file(&[
        b.matrix(6, &[3, 1], "n", &b.doubles(&[1.0, f64::NAN, 3.0])),
        b.matrix(6, &[2, 1], "x", &b.doubles(&[1.5, 2.0])),
    ]);
    let n = MatReader.read_table(f.path(), "n").unwrap();
    assert_eq!(n.columns[0].data_type, "Int64");
    assert_eq!(cells(&n, 0), ["1", "", "3"]);
    let x = MatReader.read_table(f.path(), "x").unwrap();
    assert_eq!(x.columns[0].data_type, "Float64");
    assert_eq!(cells(&x, 0), ["1.5", "2.0"]);
}

#[test]
fn a_big_endian_file_reads_like_a_little_endian_one() {
    for big in [false, true] {
        let b = Builder { big };
        // A 2 x 2 matrix, column-major: [1 3; 2 4].
        let f = b.file(&[
            b.matrix(6, &[2, 2], "m", &b.doubles(&[1.0, 2.0, 3.0, 4.0])),
            b.text("t", "Größe"),
        ]);
        let m = MatReader.read_table(f.path(), "m").unwrap();
        assert_eq!(m.columns[1].name, "c2");
        assert_eq!(cells(&m, 1), ["3", "4"], "big endian: {big}");
        let t = MatReader.read_table(f.path(), "t").unwrap();
        assert_eq!(cells(&t, 0), ["Größe"], "big endian: {big}");
    }
}

#[test]
fn a_struct_whose_fields_differ_in_length_says_which() {
    let b = Builder { big: false };
    let mut body = b.el(5, &b.u32(8));
    let mut names = b"id\0\0\0\0\0\0".to_vec();
    names.extend(b"name\0\0\0\0");
    body.extend(b.el(1, &names));
    body.extend(b.matrix(6, &[3, 1], "", &b.doubles(&[1.0, 2.0, 3.0])));
    body.extend(b.matrix(6, &[2, 1], "", &b.doubles(&[1.0, 2.0])));
    let f = b.file(&[b.matrix(2, &[1, 1], "s", &body)]);
    let err = format!("{:#}", MatReader.read_table(f.path(), "s").unwrap_err());
    assert!(err.contains("not equally long"), "{err}");
    assert!(err.contains("`id` has 3"), "{err}");
}

#[test]
fn a_cell_of_arrays_is_refused_and_a_cell_of_texts_is_a_column() {
    let b = Builder { big: false };
    let texts = [b.text("", "a"), b.text("", "bc")].concat();
    let arrays = [
        b.matrix(6, &[2, 1], "", &b.doubles(&[1.0, 2.0])),
        b.matrix(6, &[2, 1], "", &b.doubles(&[3.0, 4.0])),
    ]
    .concat();
    let f = b.file(&[
        b.matrix(1, &[2, 1], "words", &texts),
        b.matrix(1, &[2, 1], "arrays", &arrays),
    ]);
    let words = MatReader.read_table(f.path(), "words").unwrap();
    assert_eq!(words.columns[0].name, "words");
    assert_eq!(cells(&words, 0), ["a", "bc"]);
    let err = format!(
        "{:#}",
        MatReader.read_table(f.path(), "arrays").unwrap_err()
    );
    assert!(err.contains("cell holding arrays"), "{err}");
}

#[test]
fn an_empty_matrix_is_a_table_without_rows() {
    let b = Builder { big: false };
    let f = b.file(&[b.matrix(6, &[0, 0], "e", &b.el(9, &[]))]);
    let e = MatReader.read_table(f.path(), "e").unwrap();
    assert_eq!((e.row_count(), e.col_count()), (0, 1));
}

#[test]
fn a_version_4_file_is_refused_in_words() {
    let file = tempfile::Builder::new().suffix(".mat").tempfile().unwrap();
    std::fs::write(file.path(), [0u8; 200]).unwrap();
    let err = format!("{:#}", MatReader.read_file(file.path()).unwrap_err());
    assert!(err.contains("version 4"), "{err}");
}

#[test]
fn unnamed_subsystem_data_is_no_variable_and_text_keeps_its_pairs() {
    let b = Builder { big: false };
    let f = b.file(&[
        b.text("t", "Ab\u{1F600}"),
        // What MATLAB appends after objects: a uint8 matrix without a name.
        b.matrix(9, &[1, 1], "", &b.el(2, &[7])),
    ]);
    let names: Vec<String> = MatReader
        .list_tables(f.path())
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["t"]);
    let t = MatReader.read_table(f.path(), "t").unwrap();
    assert_eq!(cells(&t, 0), ["Ab\u{1F600}"]);
}

#[test]
fn the_registry_opens_a_mat_file() {
    let registry = crate::formats::FormatRegistry::new();
    let reader = registry
        .reader_for_path(Path::new("results.mat"))
        .expect("a reader");
    assert_eq!(reader.name(), "MATLAB");
    assert!(!reader.supports_write());
}

/// A damaged file: the dimensions promise more values than the data holds.
/// Reading it used to panic on the column cut.
#[test]
fn a_matrix_holding_fewer_values_than_its_dimensions_is_refused() {
    let b = Builder { big: false };
    let f = b.file(&[b.matrix(6, &[3, 3], "m", &b.doubles(&[1.0, 2.0, 3.0, 4.0]))]);
    let err = format!("{:#}", MatReader.read_table(f.path(), "m").unwrap_err());
    assert!(err.contains("holds 4 values"), "{err}");
}

/// Dimensions are read from the file, so a damaged one must not size an
/// allocation: that aborts the whole program, no error to show.
#[test]
fn dimensions_too_large_to_hold_are_refused_fast() {
    let b = Builder { big: false };
    let huge = u32::MAX;
    let f = b.file(&[
        b.matrix(1, &[huge, huge], "cells", &[]),
        b.matrix(
            2,
            &[huge, huge],
            "structs",
            &[b.el(5, &b.u32(8)), b.el(1, b"a\0\0\0\0\0\0\0")].concat(),
        ),
        b.matrix(6, &[huge, huge, huge], "cube", &b.doubles(&[1.0])),
    ]);
    for name in ["cells", "structs", "cube"] {
        assert!(MatReader.read_table(f.path(), name).is_err(), "{name}");
    }
}
