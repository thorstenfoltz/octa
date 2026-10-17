use super::*;

#[test]
fn header_row_becomes_the_columns() {
    let t = parse_tables(
        "<table><tr><th>id</th><th>name</th></tr>\
             <tr><td>1</td><td>ada</td></tr></table>",
    );
    assert_eq!(t.len(), 1);
    let dt = to_data_table(&t[0]);
    let names: Vec<&str> = dt.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert_eq!(dt.row_count(), 1);
    assert_eq!(dt.rows[0][1], CellValue::String("ada".into()));
}

/// Without a header row the columns are numbered and no data is eaten.
#[test]
fn a_table_without_th_keeps_every_row() {
    let dt = to_data_table(&parse_tables("<table><tr><td>1</td><td>2</td></tr></table>")[0]);
    assert_eq!(dt.columns[0].name, "Column 1");
    assert_eq!(dt.row_count(), 1);
}

/// A spanning cell is repeated into every position it covers: a grid with
/// holes cannot be sorted or filtered.
#[test]
fn spans_expand_into_repeated_cells() {
    let dt = to_data_table(
        &parse_tables(
            "<table>\
                 <tr><td colspan=\"2\">wide</td><td>x</td></tr>\
                 <tr><td rowspan=\"2\">tall</td><td>a</td><td>b</td></tr>\
                 <tr><td>c</td><td>d</td></tr></table>",
        )[0],
    );
    assert_eq!(dt.rows[0][0], CellValue::String("wide".into()));
    assert_eq!(dt.rows[0][1], CellValue::String("wide".into()));
    assert_eq!(dt.rows[1][0], CellValue::String("tall".into()));
    // The third row starts with the carried "tall", then its own cells.
    assert_eq!(dt.rows[2][0], CellValue::String("tall".into()));
    assert_eq!(dt.rows[2][1], CellValue::String("c".into()));
    assert_eq!(dt.rows[2][2], CellValue::String("d".into()));
}

/// A nested table is listed in its own right *and* its text stays in the
/// cell that holds it, so neither reading of the page is lost.
#[test]
fn nested_tables_are_listed_and_stay_in_their_cell() {
    let tables = parse_tables(
        "<table><tr><td>outer <table><tr><td>inner</td></tr></table></td></tr></table>",
    );
    assert_eq!(tables.len(), 2);
    assert_eq!(
        to_data_table(&tables[0]).rows[0][0],
        CellValue::String("outer inner".into())
    );
    assert_eq!(
        to_data_table(&tables[1]).rows[0][0],
        CellValue::String("inner".into())
    );
}

/// The classic layout page: a table whose only job is to position another
/// one. Listing it would bury the real table's data in a single cell.
#[test]
fn a_pure_layout_wrapper_is_dropped() {
    let tables = parse_tables(
        "<table class=\"layout\"><tr><td>\
             <table><tr><th>id</th></tr><tr><td>1</td></tr></table>\
             </td></tr></table>",
    );
    assert_eq!(tables.len(), 1);
    let dt = to_data_table(&tables[0]);
    assert_eq!(dt.columns[0].name, "id");
    assert_eq!(dt.row_count(), 1);
}

/// Real pages are tag soup; the parser is the browser's, so an unclosed
/// row or a missing `<tbody>` is not an error.
#[test]
fn unclosed_markup_still_parses() {
    let dt = to_data_table(&parse_tables("<table><tr><td>a<tr><td>b</table>")[0]);
    assert_eq!(dt.row_count(), 2);
    assert_eq!(dt.rows[1][0], CellValue::String("b".into()));
}

/// A long caption is cut on a word boundary: the tab title is the whole
/// identity of the table in the picker.
#[test]
fn long_captions_are_cut_between_words() {
    let name = shorten("List of countries and inhabited territories by total population");
    assert_eq!(name, "List of countries and inhabited territories by total");
    assert_eq!(shorten("Short one"), "Short one");
}

#[test]
fn names_come_from_captions_and_stay_unique() {
    let tables = parse_tables(
        "<table><caption>Population</caption><tr><td>1</td></tr></table>\
             <table><caption>Population</caption><tr><td>2</td></tr></table>\
             <table><tr><td>3</td></tr></table>",
    );
    assert_eq!(
        table_names(&tables),
        vec!["Population", "Population (2)", "Table 3"]
    );
}

/// Script and style are code that happens to be in the markup, and a
/// `<br>` separates words rather than joining them.
#[test]
fn cell_text_is_the_text_a_reader_sees() {
    let dt = to_data_table(
        &parse_tables(
            "<table><tr><td>a<br>b<script>var x=1;</script></td>\
                 <td>  spaced   out </td></tr></table>",
        )[0],
    );
    assert_eq!(dt.rows[0][0], CellValue::String("a b".into()));
    assert_eq!(dt.rows[0][1], CellValue::String("spaced out".into()));
}
