use octa::db::pushdown::row_estimate::RowCount;

#[test]
fn the_question_counts_pairs_from_the_tab_columns() {
    let widths = [3, 2];
    let lines = super::question_lines(
        &["orders".to_string(), "customers".to_string()],
        &widths,
        &[
            RowCount {
                rows: 10,
                estimate: false,
            },
            RowCount {
                rows: 5,
                estimate: true,
            },
        ],
    );
    assert_eq!(lines.len(), 4);
    assert!(lines[2].contains('6'), "{}", lines[2]);
}
