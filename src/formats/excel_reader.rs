use crate::data::conditional_format::match_color;
use crate::data::{CellValue, ColumnInfo, DataTable, MarkColor, MarkKey};
use crate::formats::write_options::{TableStyle, WriteOptions};
use crate::formats::xlsx_style::{XlsxRule, map_rule, mark_rgb, num_format_code};
use crate::formats::{FormatReader, TableInfo};
use anyhow::Result;
use calamine::{Data, Reader, Sheets, open_workbook_auto};
use rust_xlsxwriter::{
    Color, ConditionalFormatBlank, ConditionalFormatCell, ConditionalFormatCellRule,
    ConditionalFormatText, ConditionalFormatTextRule, Format, Workbook, Worksheet,
};
use std::path::Path;

pub struct ExcelReader;

impl FormatReader for ExcelReader {
    fn name(&self) -> &str {
        "Excel"
    }

    fn extensions(&self) -> &[&str] {
        &["xlsx", "xls", "xlsm", "xlsb", "xlm"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut workbook = open_workbook_auto(path)?;
        let sheet_names = workbook.sheet_names().to_vec();
        let Some(first) = sheet_names.first() else {
            return Ok(DataTable::empty());
        };
        let first = first.clone();
        read_sheet(&mut workbook, &first, path)
    }

    /// Each worksheet is exposed as a "table" so the app can open multiple
    /// sheets (see [`FormatReader::opens_all_tables`]). Columns are reported
    /// from the header row only - a cheap, header-accurate listing without
    /// scanning every cell for type refinement.
    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let mut workbook = open_workbook_auto(path)?;
        let sheet_names = workbook.sheet_names().to_vec();
        if sheet_names.is_empty() {
            return Ok(None);
        }
        let mut tables = Vec::with_capacity(sheet_names.len());
        for name in &sheet_names {
            let (columns, row_count) = match workbook.worksheet_range(name) {
                Ok(range) => {
                    let mut rows = range.rows();
                    let columns = rows
                        .next()
                        .map(|header| {
                            header
                                .iter()
                                .enumerate()
                                .map(|(i, cell)| ColumnInfo {
                                    name: header_cell_name(cell, i),
                                    data_type: "Utf8".to_string(),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    (columns, range.rows().count().saturating_sub(1))
                }
                Err(_) => (Vec::new(), 0),
            };
            tables.push(TableInfo {
                name: name.clone(),
                schema: None,
                columns,
                row_count: Some(row_count),
            });
        }
        Ok(Some(tables))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let mut workbook = open_workbook_auto(path)?;
        read_sheet(&mut workbook, table, path)
    }

    fn opens_all_tables(&self) -> bool {
        true
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_excel_styled(path, table, None)
    }

    fn write_file_with_options(
        &self,
        path: &Path,
        table: &DataTable,
        opts: &WriteOptions,
    ) -> Result<()> {
        let style = if opts.xlsx.include_formatting {
            opts.style.as_ref()
        } else {
            None
        };
        write_workbook_with(
            path,
            &[("Sheet1".to_string(), table, style)],
            opts.xlsx.preserve_formulas,
        )
    }
}

/// Name a header cell, falling back to `Column{n}` for empty / non-text cells.
fn header_cell_name(cell: &Data, idx: usize) -> String {
    match cell {
        Data::String(s) => s.clone(),
        Data::Float(f) => format!("{}", f),
        Data::Int(i) => format!("{}", i),
        Data::Bool(b) => format!("{}", b),
        _ => format!("Column{}", idx + 1),
    }
}

/// Read a single worksheet by name into a `DataTable`.
fn read_sheet(
    workbook: &mut Sheets<std::io::BufReader<std::fs::File>>,
    sheet: &str,
    path: &Path,
) -> Result<DataTable> {
    let range = workbook
        .worksheet_range(sheet)
        .map_err(|e| anyhow::anyhow!("Failed to read sheet: {}", e))?;
    // The formulas behind the values live in a parallel range. Read it
    // best-effort: a sheet with no formulas is the normal case, and the older
    // `.xls` path does not always carry one. A workbook must not fail to open
    // because its formulas could not be read.
    let formula_range = workbook.worksheet_formula(sheet).ok();

    let mut rows_iter = range.rows();

    // First row = headers
    let header_row = match rows_iter.next() {
        Some(r) => r,
        None => return Ok(DataTable::empty()),
    };

    let columns: Vec<ColumnInfo> = header_row
        .iter()
        .enumerate()
        .map(|(i, cell)| ColumnInfo {
            name: header_cell_name(cell, i),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let col_count = columns.len();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();

    for row in rows_iter {
        let mut cells: Vec<CellValue> = row
            .iter()
            .map(|cell| match cell {
                Data::Empty => CellValue::Null,
                Data::String(s) => CellValue::String(s.clone()),
                Data::Float(f) => CellValue::Float(*f),
                Data::Int(i) => CellValue::Int(*i),
                Data::Bool(b) => CellValue::Bool(*b),
                Data::DateTime(dt) => CellValue::DateTime(format!("{}", dt)),
                Data::DateTimeIso(s) => CellValue::DateTime(s.clone()),
                Data::DurationIso(s) => CellValue::String(s.clone()),
                Data::Error(e) => CellValue::String(format!("#ERR: {:?}", e)),
            })
            .collect();
        // Pad or truncate to match column count
        cells.resize(col_count, CellValue::Null);
        rows.push(cells);
    }

    let formulas = collect_formulas(formula_range.as_ref(), &range, rows.len(), col_count);

    // Refine column types based on data
    let mut final_columns = columns;
    for (col_idx, col) in final_columns.iter_mut().enumerate() {
        let mut has_int = false;
        let mut has_float = false;
        let mut has_bool = false;
        let mut has_datetime = false;
        let mut has_string = false;

        for row in &rows {
            match row.get(col_idx) {
                Some(CellValue::Int(_)) => has_int = true,
                Some(CellValue::Float(_)) => has_float = true,
                Some(CellValue::Bool(_)) => has_bool = true,
                Some(CellValue::DateTime(_)) => has_datetime = true,
                Some(CellValue::String(_)) => has_string = true,
                _ => {}
            }
        }

        col.data_type = if has_string {
            "Utf8".to_string()
        } else if has_datetime {
            "Timestamp(Microsecond, None)".to_string()
        } else if has_float {
            "Float64".to_string()
        } else if has_int {
            "Int64".to_string()
        } else if has_bool {
            "Boolean".to_string()
        } else {
            "Utf8".to_string()
        };
    }

    Ok(DataTable {
        columns: final_columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some("Excel".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
        formulas,
    })
}

/// Map the sheet's formula range onto `(row, col)` keys in the table's own
/// coordinates.
///
/// Three offsets have to line up. The value range and the formula range each
/// start at their own top-left cell of the sheet, and the table's first data
/// row is one below the value range's first row, because that one is the
/// header. Walking `used_cells` rather than every cell keeps this proportional
/// to the number of formulas, not to the size of the sheet.
fn collect_formulas(
    formula_range: Option<&calamine::Range<String>>,
    value_range: &calamine::Range<Data>,
    row_count: usize,
    col_count: usize,
) -> std::collections::HashMap<(usize, usize), String> {
    let mut out = std::collections::HashMap::new();
    let (Some(fr), Some((value_row0, value_col0))) = (formula_range, value_range.start()) else {
        return out;
    };
    let Some((formula_row0, formula_col0)) = fr.start() else {
        return out;
    };
    for (r, c, text) in fr.used_cells() {
        if text.is_empty() {
            continue;
        }
        let sheet_row = formula_row0 as usize + r;
        let sheet_col = formula_col0 as usize + c;
        let (Some(row), Some(col)) = (
            sheet_row.checked_sub(value_row0 as usize + 1),
            sheet_col.checked_sub(value_col0 as usize),
        ) else {
            continue;
        };
        if row < row_count && col < col_count {
            // Stored without one; shown and written back with one, the way a
            // spreadsheet spells it.
            out.insert((row, col), format!("={text}"));
        }
    }
    out
}

/// Resolve the background colour of one cell, in the same precedence the grid
/// uses: an explicit mark beats a conditional colour, and cell beats row beats
/// column. `match_color` is the single source of truth for what a rule means,
/// so a baked colour cannot drift from what the screen shows.
fn cell_fill(
    table: &DataTable,
    style: Option<&TableStyle>,
    baked: &[usize],
    row: usize,
    col: usize,
) -> Option<MarkColor> {
    // `style: None` means the feature is off (or nothing to attach): nothing
    // from the presentation layer travels, including manual marks. Checking
    // marks unconditionally here would let them leak into a plain save, which
    // is exactly the byte-for-byte-unchanged guarantee `write_excel_styled`
    // documents.
    let style = style?;
    table
        .marks
        .get(&MarkKey::Cell(row, col))
        .or_else(|| table.marks.get(&MarkKey::Row(row)))
        .or_else(|| table.marks.get(&MarkKey::Column(col)))
        .copied()
        .or_else(|| {
            if baked.is_empty() {
                return None;
            }
            let text = table
                .get(row, col)
                .map(|c| c.to_string())
                .unwrap_or_default();
            let rules: Vec<_> = baked
                .iter()
                .map(|i| style.conditional[*i].clone())
                .collect();
            match_color(&rules, col, &text)
        })
}

/// Write a workbook, optionally carrying the tab's presentation.
///
/// `style: None` reproduces byte for byte what Octa wrote before this feature
/// existed, which is what `plain_save_is_unchanged_by_the_feature` pins.
/// Write one workbook, one worksheet per entry.
///
/// The single-table `write_file` path is a one-element call into this, so
/// there is one writer rather than two that can drift apart: everything the
/// styling export does (marks, conditional colours, frozen columns, number
/// formats) applies per sheet with no extra code.
pub fn write_workbook(
    path: &Path,
    sheets: &[(String, &DataTable, Option<&TableStyle>)],
) -> Result<()> {
    write_workbook_with(path, sheets, false)
}

/// As [`write_workbook`], with the choice of writing formulas instead of the
/// values they produced.
///
/// A separate entry rather than a parameter on the one above: only the save
/// path that has `WriteOptions` in its hand can answer the question, and every
/// other caller (the CLI's workbook action, the MCP tool, the Workbook dialog)
/// wants today's behaviour, which is values.
pub fn write_workbook_with(
    path: &Path,
    sheets: &[(String, &DataTable, Option<&TableStyle>)],
    preserve_formulas: bool,
) -> Result<()> {
    if sheets.is_empty() {
        anyhow::bail!("a workbook needs at least one sheet");
    }
    let mut workbook = Workbook::new();
    let mut taken: Vec<String> = Vec::new();
    for (name, table, style) in sheets {
        let sheet_name = crate::formats::xlsx_style::sanitize_sheet_name(name, &mut taken);
        let worksheet = workbook.add_worksheet();
        worksheet.set_name(&sheet_name)?;
        write_sheet(worksheet, table, *style, preserve_formulas)?;
    }
    workbook.save(path)?;
    Ok(())
}

fn write_excel_styled(path: &Path, table: &DataTable, style: Option<&TableStyle>) -> Result<()> {
    write_workbook(path, &[("Sheet1".to_string(), table, style)])
}

/// Write one table into an already-created worksheet.
fn write_sheet(
    worksheet: &mut Worksheet,
    table: &DataTable,
    style: Option<&TableStyle>,
    preserve_formulas: bool,
) -> Result<()> {
    // The first rule that cannot live natively in Excel decides a partition,
    // not just its own fate. In Excel a live conditional-format rule always
    // overrides a cell's direct fill, with no notion of Octa's list ordering
    // between the two mechanisms, so baking only the individually
    // unrepresentable rules would let a later live rule repaint a cell an
    // earlier baked rule was meant to win. Baking that rule and every rule
    // after it keeps the on-screen order and the exported file in agreement:
    // - a live rule ahead of every bake wins both on screen (it is earlier in
    //   the list) and in Excel (a live rule overrides the base fill);
    // - a baked rule can then only be overridden in Excel by a live rule that
    //   precedes it, which is the same rule that would have won on screen;
    // - a rule after the first bake must also bake, since staying live could
    //   let it override an earlier baked rule that should have won.
    // The common case, no rule needs baking at all, leaves `bake_from` `None`
    // and every rule below exports as a live Excel rule.
    let bake_from: Option<usize> = style.and_then(|s| {
        s.conditional
            .iter()
            .position(|r| map_rule(r) == XlsxRule::Bake)
    });
    let baked: Vec<usize> = match (style, bake_from) {
        (Some(s), Some(from)) => (from..s.conditional.len()).collect(),
        _ => Vec::new(),
    };

    // One Format per distinct (fill, number-format) pair. A 100k-row coloured
    // table must not allocate 100k formats.
    let mut cache: std::collections::HashMap<(Option<MarkColor>, String), Format> =
        std::collections::HashMap::new();

    for (col_idx, col) in table.columns.iter().enumerate() {
        worksheet.write_string(0, col_idx as u16, &col.name)?;
    }

    for row_idx in 0..table.row_count() {
        let xlsx_row = (row_idx + 1) as u32;
        for col_idx in 0..table.col_count() {
            let Some(cell) = table.get(row_idx, col_idx) else {
                continue;
            };
            let fill = cell_fill(table, style, &baked, row_idx, col_idx);
            let code = style
                .and_then(|s| s.number_formats.get(&col_idx))
                .map(|f| num_format_code(f, true))
                .unwrap_or_default();

            let format = if fill.is_none() && code.is_empty() {
                None
            } else {
                Some(
                    cache
                        .entry((fill, code.clone()))
                        .or_insert_with(|| {
                            let mut f = Format::new();
                            if let Some(c) = fill {
                                f = f.set_background_color(Color::RGB(mark_rgb(c)));
                            }
                            if !code.is_empty() {
                                f = f.set_num_format(&code);
                            }
                            f
                        })
                        .clone(),
                )
            };

            // A formula, when the user asked for one and the cell still has
            // one to give. `DataTable::formula` is the gate: an edited cell and
            // a restructured table both answer `None`, so what lands here is
            // only ever a formula that still means what it says.
            match table
                .formula(row_idx, col_idx)
                .filter(|_| preserve_formulas)
            {
                Some(f) => {
                    // Carry the value Octa is showing as the formula's cached
                    // result. Without it the file says `0` until something
                    // recalculates it, and a tool that reads the workbook
                    // without evaluating formulas would see that zero.
                    let formula = rust_xlsxwriter::Formula::new(f).set_result(cell.to_string());
                    match format.as_ref() {
                        Some(fmt) => {
                            worksheet.write_formula_with_format(
                                xlsx_row,
                                col_idx as u16,
                                formula,
                                fmt,
                            )?;
                        }
                        None => {
                            worksheet.write_formula(xlsx_row, col_idx as u16, formula)?;
                        }
                    }
                }
                None => write_cell(worksheet, xlsx_row, col_idx as u16, cell, format.as_ref())?,
            }
        }
    }

    if let Some(style) = style {
        apply_freeze(worksheet, style)?;
        apply_conditional(worksheet, table, style, bake_from)?;
    }

    Ok(())
}

fn write_cell(
    worksheet: &mut Worksheet,
    row: u32,
    col: u16,
    cell: &CellValue,
    format: Option<&Format>,
) -> Result<()> {
    match (cell, format) {
        (CellValue::Null, _) => {}
        (CellValue::Int(i), Some(f)) => {
            worksheet.write_number_with_format(row, col, *i as f64, f)?;
        }
        (CellValue::Int(i), None) => {
            worksheet.write_number(row, col, *i as f64)?;
        }
        (CellValue::Float(v), Some(f)) => {
            worksheet.write_number_with_format(row, col, *v, f)?;
        }
        (CellValue::Float(v), None) => {
            worksheet.write_number(row, col, *v)?;
        }
        (CellValue::Bool(b), Some(f)) => {
            worksheet.write_boolean_with_format(row, col, *b, f)?;
        }
        (CellValue::Bool(b), None) => {
            worksheet.write_boolean(row, col, *b)?;
        }
        (other, Some(f)) => {
            worksheet.write_string_with_format(row, col, other.to_string(), f)?;
        }
        (other, None) => {
            worksheet.write_string(row, col, other.to_string())?;
        }
    }
    Ok(())
}

/// Freeze the header row, plus the frozen band when the view has one. A
/// spreadsheet whose header scrolls away is not what the user was looking at.
///
/// Guarded on `frozen_cols > 0`: a zero-column freeze split still freezes the
/// header row alone, which does not require this to run at all, and keeping
/// the guard means a table with no frozen columns emits no `<pane>` element
/// it does not need.
fn apply_freeze(worksheet: &mut Worksheet, style: &TableStyle) -> Result<()> {
    if style.frozen_cols > 0 {
        worksheet.set_freeze_panes(1, style.frozen_cols as u16)?;
    }
    Ok(())
}

fn apply_conditional(
    worksheet: &mut Worksheet,
    table: &DataTable,
    style: &TableStyle,
    bake_from: Option<usize>,
) -> Result<()> {
    let last_row = table.row_count() as u32; // header occupies row 0
    let last_col = table.col_count().saturating_sub(1) as u16;
    if last_row == 0 || table.col_count() == 0 {
        return Ok(());
    }

    for (i, rule) in style.conditional.iter().enumerate() {
        if bake_from.is_some_and(|from| i >= from) {
            // Baked instead: see the partition comment in write_excel_styled.
            continue;
        }
        let (first_col, final_col) = match rule.column {
            Some(c) if c < table.col_count() => (c as u16, c as u16),
            Some(_) => continue, // stale index, the column is gone
            None => (0, last_col),
        };
        let fmt = Format::new().set_background_color(Color::RGB(mark_rgb(rule.color)));

        match map_rule(rule) {
            // Unreachable: bake_from is the position of the first Bake rule,
            // so nothing before it maps to Bake. Kept so the match stays
            // exhaustive over XlsxRule.
            XlsxRule::Bake => continue,
            XlsxRule::Blank { inverted } => {
                let mut cf = ConditionalFormatBlank::new();
                if inverted {
                    cf = cf.invert();
                }
                worksheet.add_conditional_format(
                    1,
                    first_col,
                    last_row,
                    final_col,
                    &cf.set_format(fmt),
                )?;
            }
            XlsxRule::Text { contains, needle } => {
                let cf = ConditionalFormatText::new()
                    .set_rule(if contains {
                        ConditionalFormatTextRule::Contains(needle)
                    } else {
                        ConditionalFormatTextRule::DoesNotContain(needle)
                    })
                    .set_format(fmt);
                worksheet.add_conditional_format(1, first_col, last_row, final_col, &cf)?;
            }
            XlsxRule::Cell { op, numeric, text } => {
                let cf = match numeric {
                    Some(n) => ConditionalFormatCell::new().set_rule(numeric_rule(op, n)),
                    None => ConditionalFormatCell::new().set_rule(text_rule(op, text)),
                };
                worksheet.add_conditional_format(
                    1,
                    first_col,
                    last_row,
                    final_col,
                    &cf.set_format(fmt),
                )?;
            }
        }
    }
    Ok(())
}

fn numeric_rule(
    op: crate::data::conditional_format::CondOp,
    n: f64,
) -> ConditionalFormatCellRule<f64> {
    use crate::data::conditional_format::CondOp as O;
    match op {
        O::Eq => ConditionalFormatCellRule::EqualTo(n),
        O::Ne => ConditionalFormatCellRule::NotEqualTo(n),
        O::Gt => ConditionalFormatCellRule::GreaterThan(n),
        O::Ge => ConditionalFormatCellRule::GreaterThanOrEqualTo(n),
        O::Lt => ConditionalFormatCellRule::LessThan(n),
        O::Le => ConditionalFormatCellRule::LessThanOrEqualTo(n),
        // map_rule never routes these here.
        O::Contains | O::NotContains | O::Empty | O::NotEmpty => {
            ConditionalFormatCellRule::EqualTo(n)
        }
    }
}

fn text_rule(
    op: crate::data::conditional_format::CondOp,
    text: String,
) -> ConditionalFormatCellRule<String> {
    use crate::data::conditional_format::CondOp as O;
    match op {
        O::Ne => ConditionalFormatCellRule::NotEqualTo(text),
        // map_rule only routes Eq / Ne here.
        _ => ConditionalFormatCellRule::EqualTo(text),
    }
}
