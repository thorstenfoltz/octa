//! Pairwise correlation matrix over a table's numeric columns. Backs the MCP
//! `correlation` tool. Pure (no IO), so it is unit-testable and reusable.
//!
//! Pearson measures linear association; Spearman measures monotonic
//! association by correlating the value ranks. For each column pair only the
//! rows where *both* values are present and finite are used; a pair with fewer
//! than two such rows, or with zero variance in either column, yields `None`.

use super::{CellValue, ColumnInfo, DataTable};

/// Correlation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrMethod {
    Pearson,
    Spearman,
}

impl CorrMethod {
    /// Parse a case-insensitive method name. Defaults handled by the caller.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pearson" => Some(CorrMethod::Pearson),
            "spearman" => Some(CorrMethod::Spearman),
            _ => None,
        }
    }
}

/// Result of [`correlation_matrix`]: the numeric column names and a square
/// matrix of correlation coefficients (`None` where undefined).
#[derive(Debug, Clone)]
pub struct CorrMatrix {
    pub columns: Vec<String>,
    /// `matrix[i][j]` is the correlation of column `i` with column `j`.
    pub matrix: Vec<Vec<Option<f64>>>,
}

/// Number of non-null cells sampled when deciding whether a column is numeric.
const TYPE_SAMPLE: usize = 100;

fn cell_to_f64(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Null => None,
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        other => {
            let s = other.to_string();
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        }
    }
}

/// True when the column reads as numeric: either a numeric Arrow type name, or
/// a sampled run of non-null cells that mostly parse as numbers.
fn is_numeric_column(table: &DataTable, col: usize) -> bool {
    let ty = table.columns[col].data_type.to_ascii_lowercase();
    if ["int", "float", "double", "decimal", "real", "uint"]
        .iter()
        .any(|k| ty.contains(k))
    {
        return true;
    }
    let mut seen = 0usize;
    let mut numeric = 0usize;
    for r in 0..table.row_count() {
        if seen >= TYPE_SAMPLE {
            break;
        }
        match table.get(r, col) {
            Some(CellValue::Null) | None => continue,
            Some(v) => {
                seen += 1;
                if cell_to_f64(v).is_some() {
                    numeric += 1;
                }
            }
        }
    }
    seen > 0 && numeric * 100 >= seen * 90
}

/// Compute the correlation matrix over every numeric column.
pub fn correlation_matrix(table: &DataTable, method: CorrMethod) -> CorrMatrix {
    let cols: Vec<usize> = (0..table.col_count())
        .filter(|&c| is_numeric_column(table, c))
        .collect();
    let names: Vec<String> = cols
        .iter()
        .map(|&c| table.columns[c].name.clone())
        .collect();

    // Extract each numeric column as a Vec<Option<f64>> once.
    let data: Vec<Vec<Option<f64>>> = cols
        .iter()
        .map(|&c| {
            (0..table.row_count())
                .map(|r| table.get(r, c).and_then(cell_to_f64))
                .collect()
        })
        .collect();

    let n = cols.len();
    let mut matrix = vec![vec![None; n]; n];
    for i in 0..n {
        matrix[i][i] = correlate(&data[i], &data[i], method);
        for j in (i + 1)..n {
            let r = correlate(&data[i], &data[j], method);
            matrix[i][j] = r;
            matrix[j][i] = r;
        }
    }

    CorrMatrix {
        columns: names,
        matrix,
    }
}

/// Correlate two aligned columns over rows where both are present.
fn correlate(a: &[Option<f64>], b: &[Option<f64>], method: CorrMethod) -> Option<f64> {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (x, y) in a.iter().zip(b.iter()) {
        if let (Some(x), Some(y)) = (x, y)
            && x.is_finite()
            && y.is_finite()
        {
            xs.push(*x);
            ys.push(*y);
        }
    }
    if xs.len() < 2 {
        return None;
    }
    match method {
        CorrMethod::Pearson => pearson(&xs, &ys),
        CorrMethod::Spearman => {
            let rx = ranks(&xs);
            let ry = ranks(&ys);
            pearson(&rx, &ry)
        }
    }
}

fn pearson(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    if var_x <= 0.0 || var_y <= 0.0 {
        return None;
    }
    let r = cov / (var_x.sqrt() * var_y.sqrt());
    Some(r.clamp(-1.0, 1.0))
}

/// Fractional ranks (1-based), averaging ties.
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..values.len()).collect();
    idx.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = vec![0.0; values.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i + 1;
        while j < idx.len() && values[idx[j]] == values[idx[i]] {
            j += 1;
        }
        // Ranks i+1..=j averaged (1-based).
        let avg = ((i + 1 + j) as f64) / 2.0;
        for &k in &idx[i..j] {
            out[k] = avg;
        }
        i = j;
    }
    out
}

/// Render a [`CorrMatrix`] as a [`DataTable`]: a leading `variable` text column
/// then one Float64 column per variable. `None` coefficients become Null cells
/// so the table view renders them blank.
pub fn matrix_to_table(m: &CorrMatrix) -> DataTable {
    let mut table = DataTable::empty();
    table.columns.push(ColumnInfo {
        name: "variable".to_string(),
        data_type: "Utf8".to_string(),
    });
    for name in &m.columns {
        table.columns.push(ColumnInfo {
            name: name.clone(),
            data_type: "Float64".to_string(),
        });
    }
    for (i, name) in m.columns.iter().enumerate() {
        let mut row = Vec::with_capacity(m.columns.len() + 1);
        row.push(CellValue::String(name.clone()));
        for j in 0..m.columns.len() {
            match m.matrix[i][j] {
                Some(v) => row.push(CellValue::Float(v)),
                None => row.push(CellValue::Null),
            }
        }
        table.rows.push(row);
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    fn numeric_table() -> DataTable {
        let mut t = DataTable::empty();
        for n in ["x", "y", "label"] {
            t.columns.push(ColumnInfo {
                name: n.to_string(),
                data_type: if n == "label" { "Utf8" } else { "Float64" }.to_string(),
            });
        }
        // y = 2x (perfect positive correlation); label is non-numeric.
        for i in 1..=5 {
            t.rows.push(vec![
                CellValue::Float(i as f64),
                CellValue::Float((i * 2) as f64),
                CellValue::String(format!("r{i}")),
            ]);
        }
        t
    }

    #[test]
    fn perfect_positive_pearson() {
        let t = numeric_table();
        let m = correlation_matrix(&t, CorrMethod::Pearson);
        // Only x and y are numeric.
        assert_eq!(m.columns, vec!["x", "y"]);
        assert!((m.matrix[0][1].unwrap() - 1.0).abs() < 1e-9);
        assert!((m.matrix[0][0].unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn perfect_negative() {
        let mut t = DataTable::empty();
        for n in ["a", "b"] {
            t.columns.push(ColumnInfo {
                name: n.to_string(),
                data_type: "Int64".to_string(),
            });
        }
        for i in 0..4 {
            t.rows.push(vec![CellValue::Int(i), CellValue::Int(10 - i)]);
        }
        let m = correlation_matrix(&t, CorrMethod::Pearson);
        assert!((m.matrix[0][1].unwrap() + 1.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_monotonic() {
        // y = x^3 is monotonic but not linear: Spearman = 1, Pearson < 1.
        let mut t = DataTable::empty();
        for n in ["x", "y"] {
            t.columns.push(ColumnInfo {
                name: n.to_string(),
                data_type: "Float64".to_string(),
            });
        }
        for i in 1..=6 {
            let x = i as f64;
            t.rows
                .push(vec![CellValue::Float(x), CellValue::Float(x.powi(3))]);
        }
        let sp = correlation_matrix(&t, CorrMethod::Spearman);
        assert!((sp.matrix[0][1].unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn matrix_to_table_shapes_correctly() {
        let m = CorrMatrix {
            columns: vec!["x".into(), "y".into()],
            matrix: vec![vec![Some(1.0), Some(0.5)], vec![Some(0.5), Some(1.0)]],
        };
        let t = matrix_to_table(&m);
        assert_eq!(t.col_count(), 3); // variable + x + y
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.columns[0].name, "variable");
        assert_eq!(t.columns[1].name, "x");
    }

    #[test]
    fn zero_variance_is_none() {
        let mut t = DataTable::empty();
        for n in ["a", "b"] {
            t.columns.push(ColumnInfo {
                name: n.to_string(),
                data_type: "Float64".to_string(),
            });
        }
        for _ in 0..3 {
            t.rows
                .push(vec![CellValue::Float(5.0), CellValue::Float(1.0)]);
        }
        let m = correlation_matrix(&t, CorrMethod::Pearson);
        assert_eq!(m.matrix[0][1], None);
    }
}
