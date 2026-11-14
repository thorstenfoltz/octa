//! MATLAB `.mat` files, read-only. Every variable is a table.
//!
//! Two layouts share one model ([`Var`]) and one table builder:
//!
//! - **Version 5** (also what MATLAB's `-v6` and `-v7` write): a 128-byte
//!   header, then one data element per variable, each possibly zlib
//!   compressed. Parsed here by hand; the zlib crate is already in the tree.
//! - **Version 7.3**: an HDF5 file behind a 512-byte header, read through
//!   `hdf5-reader`. Each variable carries a `MATLAB_class` attribute, matrices
//!   are stored transposed, text in a cell lives in `#refs#` behind object
//!   references.
//!
//! What a variable becomes: a number or true/false vector one column named
//! after the variable, a matrix columns `c1..cn`, text one row per text row,
//! a cell of single values one column (or `c1..cn`), a struct whose fields are
//! equally long one column per field, a struct array of single values one row
//! per element. Whole-valued doubles (MATLAB's default for every number)
//! become whole numbers, a NaN an empty cell.
//!
//! ponytail: MATLAB `table`/`string`/`datetime` objects, class instances,
//! sparse and complex matrices, function handles and arrays of more than two
//! dimensions are listed but refuse to open, each with the reason. Version 4
//! files are refused. The whole file is read into memory.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::{FormatReader, TableInfo};

pub struct MatReader;

impl FormatReader for MatReader {
    fn name(&self) -> &str {
        "MATLAB"
    }

    fn extensions(&self) -> &[&str] {
        &["mat"]
    }

    /// The first variable that reads as a table; with none, why the first
    /// one does not.
    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let names = variable_names(path)?;
        let mut first_error = None;
        for name in &names {
            match self.read_table(path, name) {
                Ok(table) => return Ok(table),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        Err(first_error.unwrap_or_else(|| anyhow!("the MAT-file holds no variables")))
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        Ok(Some(
            variable_names(path)?
                .into_iter()
                .map(|name| TableInfo {
                    name,
                    schema: None,
                    columns: Vec::new(),
                    row_count: None,
                })
                .collect(),
        ))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let var = match layout(path)? {
            Layout::V5(bytes) => v5::variable(&bytes, table)?,
            Layout::Hdf5 => v73::variable(path, table)?,
        };
        let mut t = to_table(table, var).with_context(|| format!("variable `{table}`"))?;
        t.source_path = Some(path.to_string_lossy().to_string());
        t.format_name = Some("MATLAB".to_string());
        Ok(t)
    }
}

enum Layout {
    V5(Vec<u8>),
    Hdf5,
}

fn layout(path: &Path) -> Result<Layout> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading MAT-file {}", path.display()))?;
    if bytes.len() >= 520 && bytes[512..520] == *b"\x89HDF\r\n\x1a\n" {
        return Ok(Layout::Hdf5);
    }
    // The header's text is free; the byte-order marker at 126 is what every
    // version 5 writer fills in.
    if bytes.len() < 128 || !matches!(&bytes[126..128], b"IM" | b"MI") {
        bail!("not a MATLAB version 5 or 7.3 MAT-file (version 4 files are not read)");
    }
    Ok(Layout::V5(bytes))
}

fn variable_names(path: &Path) -> Result<Vec<String>> {
    match layout(path)? {
        Layout::V5(bytes) => v5::names(&bytes),
        Layout::Hdf5 => v73::names(path),
    }
}

/// One MATLAB variable before it becomes a table. Values are column-major,
/// as MATLAB stores them, with MATLAB's dimensions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Var {
    /// Numbers or true/false, already as cells (NaN is `Null`).
    Num {
        dims: Vec<usize>,
        cells: Vec<CellValue>,
    },
    /// Character codes.
    Char {
        dims: Vec<usize>,
        codes: Vec<u32>,
    },
    Cell {
        dims: Vec<usize>,
        items: Vec<Var>,
    },
    /// `values[element * fields.len() + field]`.
    Struct {
        dims: Vec<usize>,
        fields: Vec<String>,
        values: Vec<Var>,
    },
    /// Not read, and why.
    Other(String),
}

fn float_cell(v: f64) -> CellValue {
    if v.is_nan() {
        CellValue::Null
    } else {
        CellValue::Float(v)
    }
}

fn u64_cell(v: u64) -> CellValue {
    i64::try_from(v).map_or(CellValue::Float(v as f64), CellValue::Int)
}

/// Rows and columns of a two-dimensional array.
fn rows_cols(dims: &[usize]) -> Result<(usize, usize)> {
    match dims {
        [] => Ok((0, 0)),
        [n] => Ok((*n, 1)),
        [r, c] => Ok((*r, *c)),
        more => bail!("it has {} dimensions; only two are read", more.len()),
    }
}

/// The text rows of a char array, trailing padding removed. MATLAB keeps
/// text as UTF-16 units, so a character past U+FFFF is a pair of them.
fn char_rows(dims: &[usize], codes: &[u32]) -> Result<Vec<String>> {
    let (rows, cols) = rows_cols(dims)?;
    Ok((0..rows)
        .map(|r| {
            let row: Vec<u32> = (0..cols)
                .filter_map(|c| codes.get(c * rows + r).copied())
                .collect();
            let s: String = match row
                .iter()
                .map(|c| u16::try_from(*c))
                .collect::<Result<Vec<u16>, _>>()
            {
                Ok(units) => String::from_utf16_lossy(&units),
                Err(_) => row
                    .iter()
                    .map(|c| char::from_u32(*c).unwrap_or('\u{FFFD}'))
                    .collect(),
            };
            s.trim_end_matches([' ', '\0']).to_string()
        })
        .collect())
}

/// One value, if the variable holds no more than one.
fn single(var: &Var) -> Option<CellValue> {
    match var {
        Var::Num { cells, .. } if cells.len() <= 1 => {
            Some(cells.first().cloned().unwrap_or(CellValue::Null))
        }
        Var::Char { dims, codes } => match char_rows(dims, codes).ok()?.as_slice() {
            [] => Some(CellValue::Null),
            [one] => Some(CellValue::String(one.clone())),
            _ => None,
        },
        Var::Cell { items, .. } if items.len() == 1 => single(&items[0]),
        _ => None,
    }
}

/// The columns a variable gives: one named `name` for a vector, text or a
/// list, `c1..cn` (or `name_c1..`) for a matrix.
fn columns_of(name: &str, var: &Var, top: bool) -> Result<Vec<(String, Vec<CellValue>)>> {
    let matrix = |rows: usize, cols: usize, cells: Vec<CellValue>| {
        if rows <= 1 || cols <= 1 {
            return vec![(name.to_string(), cells)];
        }
        (0..cols)
            .map(|c| {
                let col_name = if top {
                    format!("c{}", c + 1)
                } else {
                    format!("{name}_c{}", c + 1)
                };
                (col_name, cells[c * rows..(c + 1) * rows].to_vec())
            })
            .collect()
    };
    match var {
        Var::Num { dims, cells } => {
            let (rows, cols) = rows_cols(dims)?;
            Ok(matrix(rows, cols, cells.clone()))
        }
        Var::Char { dims, codes } => Ok(vec![(
            name.to_string(),
            char_rows(dims, codes)?
                .into_iter()
                .map(CellValue::String)
                .collect(),
        )]),
        Var::Cell { dims, items } => {
            let (rows, cols) = rows_cols(dims)?;
            let cells = items
                .iter()
                .map(|item| {
                    single(item).ok_or_else(|| {
                        anyhow!("`{name}` is a cell holding arrays, only single values are read")
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(matrix(rows, cols, cells))
        }
        Var::Struct { .. } => bail!("`{name}` is a struct inside a struct, which is not read"),
        Var::Other(why) => bail!("`{name}` is {why}, which is not read"),
    }
}

pub(crate) fn to_table(name: &str, var: Var) -> Result<DataTable> {
    let columns: Vec<(String, Vec<CellValue>)> = match &var {
        Var::Other(why) => bail!("it is {why}, which is not read"),
        Var::Struct {
            dims,
            fields,
            values,
        } if dims.iter().product::<usize>() == 1 => {
            let mut out = Vec::new();
            for (field, value) in fields.iter().zip(values) {
                out.extend(columns_of(field, value, false)?);
            }
            if let Some((first, rest)) = out.split_first()
                && let Some((other, _)) = rest.iter().find(|(_, c)| c.len() != first.1.len())
            {
                bail!(
                    "its fields are not equally long (`{}` has {} values, `{other}` {})",
                    first.0,
                    first.1.len(),
                    rest.iter()
                        .find(|(n, _)| n == other)
                        .map_or(0, |(_, c)| c.len())
                );
            }
            out
        }
        Var::Struct { fields, values, .. } => {
            // A struct array: one row per element, every field one value.
            let mut cols: Vec<(String, Vec<CellValue>)> =
                fields.iter().map(|f| (f.clone(), Vec::new())).collect();
            for (i, value) in values.iter().enumerate() {
                let f = i % fields.len().max(1);
                let cell = single(value).ok_or_else(|| {
                    anyhow!(
                        "field `{}` of element {} holds more than one value",
                        fields[f],
                        i / fields.len() + 1
                    )
                })?;
                cols[f].1.push(cell);
            }
            cols
        }
        other => columns_of(name, other, true)?,
    };
    let rows = columns.first().map_or(0, |(_, c)| c.len());
    let mut table = DataTable::empty();
    let mut cells_by_col = Vec::with_capacity(columns.len());
    for (col_name, cells) in columns {
        let (data_type, cells) = typed(cells);
        table.columns.push(ColumnInfo {
            name: col_name,
            data_type: data_type.into(),
        });
        cells_by_col.push(cells);
    }
    table.rows = (0..rows)
        .map(|r| cells_by_col.iter().map(|c| c[r].clone()).collect())
        .collect();
    Ok(table)
}

/// A column's type, read off its cells: whole-valued numbers become whole
/// numbers (MATLAB stores every number as a double), mixed cells text.
fn typed(cells: Vec<CellValue>) -> (&'static str, Vec<CellValue>) {
    let filled = || cells.iter().filter(|c| !matches!(c, CellValue::Null));
    if filled().all(|c| matches!(c, CellValue::Bool(_))) && filled().next().is_some() {
        return ("Boolean", cells);
    }
    if filled().all(|c| matches!(c, CellValue::Int(_) | CellValue::Float(_))) {
        let whole = filled().all(|c| match c {
            CellValue::Float(f) => f.fract() == 0.0 && f.abs() < 9.0e15,
            _ => true,
        });
        let convert = |c: CellValue| match c {
            CellValue::Float(f) if whole => CellValue::Int(f as i64),
            CellValue::Int(i) if !whole => CellValue::Float(i as f64),
            other => other,
        };
        let ty = if whole { "Int64" } else { "Float64" };
        return (ty, cells.into_iter().map(convert).collect());
    }
    if filled().all(|c| matches!(c, CellValue::String(_))) {
        return ("Utf8", cells);
    }
    let text = |c: CellValue| match c {
        CellValue::Null => CellValue::Null,
        other => CellValue::String(other.to_string()),
    };
    ("Utf8", cells.into_iter().map(text).collect())
}

/// Version 5: data elements after a 128-byte header.
mod v5 {
    use super::*;

    const MI_INT8: u32 = 1;
    const MI_UINT8: u32 = 2;
    const MI_INT16: u32 = 3;
    const MI_UINT16: u32 = 4;
    const MI_INT32: u32 = 5;
    const MI_UINT32: u32 = 6;
    const MI_SINGLE: u32 = 7;
    const MI_DOUBLE: u32 = 9;
    const MI_INT64: u32 = 12;
    const MI_UINT64: u32 = 13;
    pub(super) const MI_MATRIX: u32 = 14;
    pub(super) const MI_COMPRESSED: u32 = 15;
    const MI_UTF8: u32 = 16;
    const MI_UTF16: u32 = 17;
    const MI_UTF32: u32 = 18;

    #[derive(Clone, Copy)]
    pub(super) struct Order {
        pub little: bool,
    }

    impl Order {
        fn of(bytes: &[u8]) -> Result<Self> {
            match &bytes[126..128] {
                b"IM" => Ok(Self { little: true }),
                b"MI" => Ok(Self { little: false }),
                _ => bail!("the MAT-file header names no byte order"),
            }
        }

        fn u32(self, b: &[u8], at: usize) -> Result<u32> {
            let raw: [u8; 4] = b
                .get(at..at + 4)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| anyhow!("the MAT-file ends inside a data element"))?;
            Ok(if self.little {
                u32::from_le_bytes(raw)
            } else {
                u32::from_be_bytes(raw)
            })
        }
    }

    /// The element at `pos`: its type, its bytes and where the next one
    /// starts (elements are padded to 8 bytes; a small element is 8 bytes).
    pub(super) fn element(b: &[u8], pos: usize, o: Order) -> Result<(u32, &[u8], usize)> {
        let first = o.u32(b, pos)?;
        if first >> 16 != 0 {
            let n = (first >> 16) as usize;
            let data = b
                .get(pos + 4..pos + 4 + n.min(4))
                .ok_or_else(|| anyhow!("the MAT-file ends inside a data element"))?;
            return Ok((first & 0xffff, data, pos + 8));
        }
        let n = o.u32(b, pos + 4)? as usize;
        let data = b
            .get(pos + 8..pos + 8 + n)
            .ok_or_else(|| anyhow!("the MAT-file ends inside a data element"))?;
        Ok((first, data, pos + 8 + n.div_ceil(8) * 8))
    }

    /// A top-level element: its type and bytes.
    type Element<'a> = (u32, &'a [u8]);

    /// The top-level elements. Not padded at this level, so a compressed
    /// element's length is exact.
    fn top_level(bytes: &[u8]) -> Result<(Order, Vec<Element<'_>>)> {
        let o = Order::of(bytes)?;
        let mut out = Vec::new();
        let mut pos = 128;
        while pos + 8 <= bytes.len() {
            let ty = o.u32(bytes, pos)?;
            let n = o.u32(bytes, pos + 4)? as usize;
            let data = bytes
                .get(pos + 8..pos + 8 + n)
                .ok_or_else(|| anyhow!("the MAT-file is cut short"))?;
            out.push((ty, data));
            pos += 8 + n;
            if ty != MI_COMPRESSED {
                pos = pos.div_ceil(8) * 8;
            }
        }
        Ok((o, out))
    }

    /// `limit` bytes of a compressed element, at most the decompression cap.
    fn inflate(data: &[u8], limit: Option<u64>) -> Result<Vec<u8>> {
        let cap = crate::formats::compression::max_decompressed();
        let take = limit.unwrap_or(cap).min(cap);
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(data)
            .take(take.saturating_add(1))
            .read_to_end(&mut out)
            .context("decompressing a MAT-file variable")?;
        if limit.is_none() && out.len() as u64 > cap {
            bail!("a variable unpacks to more than the decompression limit in Settings");
        }
        Ok(out)
    }

    /// The name inside a matrix element's bytes.
    fn matrix_name(m: &[u8], o: Order) -> Result<String> {
        let (_, _, pos) = element(m, 0, o)?;
        let (_, _, pos) = element(m, pos, o)?;
        let (_, name, _) = element(m, pos, o)?;
        Ok(String::from_utf8_lossy(name).into_owned())
    }

    /// The matrix element a top-level element holds, unpacked when needed;
    /// `prefix` unpacks only enough to read its name.
    fn matrix_bytes(ty: u32, data: &[u8], o: Order, prefix: bool) -> Result<Option<Vec<u8>>> {
        match ty {
            MI_MATRIX => Ok(Some(data.to_vec())),
            MI_COMPRESSED => {
                // A prefix holds only the start of the element, enough for
                // its name.
                let raw = inflate(data, prefix.then_some(4096))?;
                if raw.len() < 8 || o.u32(&raw, 0)? != MI_MATRIX {
                    return Ok(None);
                }
                let end = (8 + o.u32(&raw, 4)? as usize).min(raw.len());
                Ok(Some(raw[8..end].to_vec()))
            }
            _ => Ok(None),
        }
    }

    /// The name of a top-level element, if it holds a matrix: read from an
    /// unpacked prefix, or from the whole element when the prefix is too
    /// short for it.
    fn element_name(ty: u32, data: &[u8], o: Order) -> Result<Option<String>> {
        let Some(head) = matrix_bytes(ty, data, o, true)? else {
            return Ok(None);
        };
        match matrix_name(&head, o) {
            Ok(name) => Ok(Some(name)),
            Err(_) => matrix_bytes(ty, data, o, false)?
                .map(|m| matrix_name(&m, o))
                .transpose(),
        }
    }

    /// The variables' names. MATLAB writes the data of objects (tables,
    /// strings) as one more matrix without a name; that is no variable.
    pub(super) fn names(bytes: &[u8]) -> Result<Vec<String>> {
        let (o, elements) = top_level(bytes)?;
        let mut names = Vec::new();
        for (ty, data) in elements {
            if let Some(name) = element_name(ty, data, o)?.filter(|n| !n.is_empty()) {
                names.push(name);
            }
        }
        Ok(names)
    }

    pub(super) fn variable(bytes: &[u8], name: &str) -> Result<Var> {
        let (o, elements) = top_level(bytes)?;
        for (ty, data) in elements {
            if element_name(ty, data, o)?.as_deref() != Some(name) {
                continue;
            }
            let m = matrix_bytes(ty, data, o, false)?.unwrap_or_default();
            return matrix(&m, o).map(|(_, var)| var);
        }
        bail!("the MAT-file has no variable `{name}`")
    }

    fn numbers(ty: u32, d: &[u8], o: Order) -> Result<Vec<CellValue>> {
        macro_rules! each {
            ($t:ty, $f:expr) => {{
                const N: usize = std::mem::size_of::<$t>();
                d.chunks_exact(N)
                    .map(|c| {
                        let a: [u8; N] = c.try_into().expect("exact chunk");
                        let v = if o.little {
                            <$t>::from_le_bytes(a)
                        } else {
                            <$t>::from_be_bytes(a)
                        };
                        $f(v)
                    })
                    .collect()
            }};
        }
        Ok(match ty {
            MI_INT8 => each!(i8, |v| CellValue::Int(v as i64)),
            MI_UINT8 | MI_UTF8 => each!(u8, |v| CellValue::Int(v as i64)),
            MI_INT16 => each!(i16, |v| CellValue::Int(v as i64)),
            MI_UINT16 | MI_UTF16 => each!(u16, |v| CellValue::Int(v as i64)),
            MI_INT32 => each!(i32, |v| CellValue::Int(v as i64)),
            MI_UINT32 | MI_UTF32 => each!(u32, |v| CellValue::Int(v as i64)),
            MI_INT64 => each!(i64, CellValue::Int),
            MI_UINT64 => each!(u64, u64_cell),
            MI_SINGLE => each!(f32, |v| float_cell(v as f64)),
            MI_DOUBLE => each!(f64, float_cell),
            other => bail!("a number element of unknown type {other}"),
        })
    }

    /// A matrix element: its name and what it holds.
    pub(super) fn matrix(m: &[u8], o: Order) -> Result<(String, Var)> {
        if m.is_empty() {
            // An empty matrix may be written with no contents at all.
            return Ok((
                String::new(),
                Var::Num {
                    dims: vec![0, 0],
                    cells: Vec::new(),
                },
            ));
        }
        let (_, flags, pos) = element(m, 0, o)?;
        let flags = o.u32(flags, 0)?;
        let class = flags & 0xff;
        let complex = flags & 0x800 != 0;
        let logical = flags & 0x200 != 0;
        let (_, dims_raw, pos) = element(m, pos, o)?;
        let dims: Vec<usize> = (0..dims_raw.len() / 4)
            .map(|i| o.u32(dims_raw, i * 4).map(|v| v as usize))
            .collect::<Result<_>>()?;
        let (_, name, mut pos) = element(m, pos, o)?;
        let name = String::from_utf8_lossy(name).into_owned();
        let count: usize = dims.iter().product();
        let var = match class {
            1 => {
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let (ty, sub, next) = element(m, pos, o)?;
                    if ty != MI_MATRIX {
                        bail!("a cell holds an element of type {ty}");
                    }
                    items.push(matrix(sub, o)?.1);
                    pos = next;
                }
                Var::Cell { dims, items }
            }
            2 => {
                let (_, len, pos2) = element(m, pos, o)?;
                let len = o.u32(len, 0)? as usize;
                let (_, raw, mut pos3) = element(m, pos2, o)?;
                let fields: Vec<String> = raw
                    .chunks(len.max(1))
                    .map(|c| {
                        let end = c.iter().position(|b| *b == 0).unwrap_or(c.len());
                        String::from_utf8_lossy(&c[..end]).into_owned()
                    })
                    .collect();
                let mut values = Vec::with_capacity(count * fields.len());
                for _ in 0..count * fields.len() {
                    let (ty, sub, next) = element(m, pos3, o)?;
                    if ty != MI_MATRIX {
                        bail!("a struct field holds an element of type {ty}");
                    }
                    values.push(matrix(sub, o)?.1);
                    pos3 = next;
                }
                Var::Struct {
                    dims,
                    fields,
                    values,
                }
            }
            4 => {
                let (ty, data, _) = element(m, pos, o)?;
                let codes = if ty == MI_UTF8 {
                    String::from_utf8_lossy(data)
                        .chars()
                        .map(|c| c as u32)
                        .collect()
                } else {
                    numbers(ty, data, o)?
                        .into_iter()
                        .map(|c| match c {
                            CellValue::Int(i) => i as u32,
                            _ => 0xFFFD,
                        })
                        .collect()
                };
                Var::Char { dims, codes }
            }
            6..=15 if complex => Var::Other("a matrix of complex numbers".into()),
            6..=15 => {
                let (ty, data, _) = element(m, pos, o)?;
                let mut cells = numbers(ty, data, o)?;
                if logical {
                    for c in &mut cells {
                        *c = CellValue::Bool(!matches!(c, CellValue::Int(0)));
                    }
                }
                Var::Num { dims, cells }
            }
            5 => Var::Other("a sparse matrix".into()),
            3 => Var::Other("an object of a MATLAB class".into()),
            16 => Var::Other("a function handle".into()),
            17 => Var::Other("a MATLAB object such as a table, string or datetime".into()),
            other => Var::Other(format!("an array of class {other}")),
        };
        Ok((name, var))
    }
}

/// Version 7.3: HDF5 with MATLAB's attributes.
mod v73 {
    use super::*;
    use hdf5_reader::group::Group;
    use hdf5_reader::{Dataset, Datatype, Hdf5File};

    fn open(path: &Path) -> Result<Hdf5File> {
        let options = hdf5_reader::OpenOptions::default()
            .with_max_chunk_bytes(hdf5_reader::HDF5_MAX_CHUNK_BYTES);
        Ok(Hdf5File::open_with_options(path, options)?)
    }

    pub(super) fn names(path: &Path) -> Result<Vec<String>> {
        let file = open(path)?;
        let (groups, datasets) = file.root_group()?.member_names()?;
        Ok(datasets
            .into_iter()
            .chain(groups)
            .filter(|n| !n.starts_with('#'))
            .collect())
    }

    fn class_of<E>(attr: std::result::Result<hdf5_reader::Attribute, E>) -> String {
        attr.ok()
            .and_then(|a| a.read_string().ok())
            .unwrap_or_default()
    }

    pub(super) fn variable(path: &Path, name: &str) -> Result<Var> {
        let file = open(path)?;
        let root = file.root_group()?;
        let refs = Refs::of(&root);
        if let Ok(ds) = root.dataset(name) {
            return dataset(&ds, &refs);
        }
        let group = root
            .group(name)
            .map_err(|_| anyhow!("the MAT-file has no variable `{name}`"))?;
        match class_of(group.attribute("MATLAB_class")).as_str() {
            "struct" => {
                let mut fields = Vec::new();
                let mut values = Vec::new();
                for ds in group.datasets()? {
                    fields.push(ds.name().rsplit('/').next().unwrap_or("").to_string());
                    values.push(dataset(&ds, &refs)?);
                }
                for g in group.groups()? {
                    fields.push(g.name().rsplit('/').next().unwrap_or("").to_string());
                    values.push(Var::Other("a struct inside a struct".into()));
                }
                let order = field_order(&group);
                let mut pairs: Vec<(String, Var)> = fields.into_iter().zip(values).collect();
                pairs.sort_by_key(|(f, _)| order.iter().position(|o| o == f).unwrap_or(usize::MAX));
                let (fields, values) = pairs.into_iter().unzip();
                Ok(Var::Struct {
                    dims: vec![1, 1],
                    fields,
                    values,
                })
            }
            "" => Ok(Var::Other("a group without a MATLAB class".into())),
            other => Ok(Var::Other(format!("a MATLAB {other} object"))),
        }
    }

    /// The order MATLAB lists a struct's fields in (`MATLAB_fields`), empty
    /// when the attribute cannot be read: the members' order is used then.
    fn field_order(group: &Group) -> Vec<String> {
        let Ok(attr) = group.attribute("MATLAB_fields") else {
            return Vec::new();
        };
        // One variable-length list of single characters per field: a length,
        // then a global heap address and index.
        let size = 4 + group.offset_size() as usize + 4;
        attr.raw_data
            .chunks_exact(size)
            .filter_map(|r| group.resolve_vlen_reference_bytes(r, 1).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .collect()
    }

    /// The datasets under `#refs#`, by address, for cells.
    struct Refs {
        offset_size: u8,
        by_address: Vec<(u64, Dataset)>,
    }

    impl Refs {
        fn of(root: &Group) -> Self {
            let by_address = root
                .group("#refs#")
                .ok()
                .and_then(|g| g.datasets().ok())
                .map(|list| list.into_iter().map(|d| (d.address(), d)).collect())
                .unwrap_or_default();
            Self {
                offset_size: root.offset_size(),
                by_address,
            }
        }
    }

    fn dims_of(ds: &Dataset) -> Vec<usize> {
        // HDF5 lists dimensions slowest first; MATLAB's are the reverse.
        let mut dims: Vec<usize> = ds.shape().iter().map(|d| *d as usize).collect();
        dims.reverse();
        dims
    }

    fn dataset(ds: &Dataset, refs: &Refs) -> Result<Var> {
        let class = class_of(ds.attribute("MATLAB_class"));
        let empty = ds
            .attribute("MATLAB_empty")
            .ok()
            .and_then(|a| a.read_as_f64().ok())
            .is_some_and(|v| v != 0.0);
        if empty {
            return Ok(match class.as_str() {
                "char" => Var::Char {
                    dims: vec![0, 0],
                    codes: Vec::new(),
                },
                _ => Var::Num {
                    dims: vec![0, 0],
                    cells: Vec::new(),
                },
            });
        }
        if ds.attribute("MATLAB_sparse").is_ok() {
            return Ok(Var::Other("a sparse matrix".into()));
        }
        let dims = dims_of(ds);
        Ok(match class.as_str() {
            "double" | "single" | "int8" | "uint8" | "int16" | "uint16" | "int32" | "uint32"
            | "int64" | "uint64" => match ds.dtype() {
                Datatype::Compound { .. } => Var::Other("a matrix of complex numbers".into()),
                _ => Var::Num {
                    dims,
                    cells: numbers(ds)?,
                },
            },
            "logical" => Var::Num {
                dims,
                cells: numbers(ds)?
                    .into_iter()
                    .map(|c| CellValue::Bool(!matches!(c, CellValue::Int(0))))
                    .collect(),
            },
            "char" => Var::Char {
                dims,
                codes: numbers(ds)?
                    .into_iter()
                    .map(|c| match c {
                        CellValue::Int(i) => i as u32,
                        _ => 0xFFFD,
                    })
                    .collect(),
            },
            "cell" => {
                let raw = ds.read_raw_bytes()?;
                let addresses =
                    hdf5_reader::reference::read_object_references(&raw, refs.offset_size)?;
                let items = addresses
                    .iter()
                    .map(
                        |a| match refs.by_address.iter().find(|(addr, _)| addr == a) {
                            Some((_, target)) => dataset(target, refs),
                            None => Ok(Var::Other("a reference Octa cannot follow".into())),
                        },
                    )
                    .collect::<Result<Vec<_>>>()?;
                Var::Cell { dims, items }
            }
            "" => Var::Other("a dataset without a MATLAB class".into()),
            other => Var::Other(format!("a MATLAB {other} object")),
        })
    }

    fn numbers(ds: &Dataset) -> Result<Vec<CellValue>> {
        macro_rules! read {
            ($t:ty, $f:expr) => {
                ds.read_array::<$t>()?.iter().map(|v| $f(*v)).collect()
            };
        }
        Ok(match ds.dtype() {
            Datatype::FloatingPoint { size: 4, .. } => read!(f32, |v: f32| float_cell(v as f64)),
            Datatype::FloatingPoint { .. } => read!(f64, float_cell),
            Datatype::FixedPoint { size, signed, .. } => match (size, signed) {
                (1, true) => read!(i8, |v: i8| CellValue::Int(v as i64)),
                (1, false) => read!(u8, |v: u8| CellValue::Int(v as i64)),
                (2, true) => read!(i16, |v: i16| CellValue::Int(v as i64)),
                (2, false) => read!(u16, |v: u16| CellValue::Int(v as i64)),
                (4, true) => read!(i32, |v: i32| CellValue::Int(v as i64)),
                (4, false) => read!(u32, |v: u32| CellValue::Int(v as i64)),
                (8, false) => read!(u64, u64_cell),
                _ => read!(i64, CellValue::Int),
            },
            other => bail!("numbers stored as {other:?}"),
        })
    }
}

#[cfg(test)]
#[path = "mat_reader_tests.rs"]
mod tests;
