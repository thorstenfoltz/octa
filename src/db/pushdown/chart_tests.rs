use super::super::test_support::{DuckConn, source};
use super::*;

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    }
}

/// Ordered by `g` with the NULL `g` last and by `f`, so the local
/// builder's first-seen order and row order equal the server's ORDER BY.
/// `n` has a NULL; `w` is words; `d` dates.
pub(crate) fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        col("g", "Utf8"),
        col("n", "Int64"),
        col("f", "Float64"),
        col("d", "Date32"),
        col("w", "Utf8"),
    ];
    let g = [
        Some("a"),
        Some("a"),
        Some("b"),
        Some("c"),
        Some("c"),
        Some("c"),
        None,
    ];
    let n = [Some(1), Some(5), None, Some(2), Some(8), Some(3), Some(4)];
    let d = [
        "2024-01-01",
        "2024-01-03",
        "2024-01-10",
        "2024-02-01",
        "2024-02-02",
        "2024-03-15",
        "2024-04-30",
    ];
    t.rows = (0..7)
        .map(|i| {
            vec![
                g[i].map_or(CellValue::Null, |s| CellValue::String(s.into())),
                n[i].map_or(CellValue::Null, CellValue::Int),
                CellValue::Float(0.5 + i as f64),
                CellValue::Date(d[i].into()),
                CellValue::String("word".into()),
            ]
        })
        .collect();
    t
}

pub(crate) fn request(cfg: &ChartConfig, limits: ChartLimits) -> ChartRequest {
    request_for(&table(), cfg, limits)
}

fn request_for(t: &DataTable, cfg: &ChartConfig, limits: ChartLimits) -> ChartRequest {
    ChartRequest {
        key: ChartKey::of(cfg, limits),
        columns: t.columns.clone(),
        reads: column_reads(t),
        tie: Vec::new(),
    }
}

/// One column `v` of the given cells.
fn one_column(ty: &str, cells: Vec<CellValue>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![col("v", ty)];
    t.rows = cells.into_iter().map(|c| vec![c]).collect();
    t
}

pub(crate) fn limits() -> ChartLimits {
    ChartLimits {
        max_points: 1_000,
        max_categories: 200,
    }
}

/// The local builder over every row of [`table`].
pub(crate) fn local(cfg: &ChartConfig) -> Result<ChartPrep, ChartError> {
    let t = table();
    let rows: Vec<usize> = (0..t.row_count()).collect();
    build_chart(&t, &rows, cfg, limits())
}

pub(crate) fn chart_of(s: ServerChart) -> Result<ChartPrep, ChartError> {
    match s {
        ServerChart::Drawn { chart, .. } => chart,
        ServerChart::NotExpressible => panic!("expected a drawn chart"),
    }
}

#[test]
fn columns_are_read_as_the_local_chart_reads_them() {
    let mut t = table();
    t.columns.push(col("s", "Utf8"));
    t.columns.push(col("b", "Boolean"));
    for r in &mut t.rows {
        r.push(CellValue::String("12.5".into()));
        r.push(CellValue::Bool(true));
    }
    use ColumnRead::*;
    assert_eq!(
        column_reads(&t),
        vec![Other, Number, Number, Date, Other, Number, Other]
    );
    // Nothing loaded: the type decides.
    for (ty, read) in [
        ("Int64", Number),
        ("Decimal128(10, 2)", Number),
        ("Date32", Date),
        ("Timestamp(Microsecond, None)", DateTime),
        ("Utf8", Other),
        ("Boolean", Other),
    ] {
        let empty = one_column(ty, vec![CellValue::Null; 3]);
        assert_eq!(column_reads(&empty), vec![read], "{ty}");
    }
}

#[test]
fn cosmetic_settings_leave_the_key_alone() {
    let mut cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(2),
        y_cols: vec![1],
        ..ChartConfig::default()
    };
    let key = ChartKey::of(&cfg, limits());
    cfg.title = "Sales".into();
    cfg.show_grid = false;
    cfg.y_log_scale = true;
    cfg.x_min = Some(1.0);
    cfg.agg = Aggregation::Max; // Line does not aggregate
    assert_eq!(ChartKey::of(&cfg, limits()), key);
    cfg.kind = ChartKind::Bar;
    let bar = ChartKey::of(&cfg, limits());
    cfg.agg = Aggregation::Min;
    assert_ne!(ChartKey::of(&cfg, limits()), bar, "Bar aggregates");
}

#[test]
fn the_server_histogram_is_the_local_one() {
    for (x, bins) in [(1, None), (1, Some(3)), (2, None), (3, None)] {
        let cfg = ChartConfig {
            kind: ChartKind::Histogram,
            x_col: Some(x),
            hist_bins: bins,
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = histogram(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(chart_of(got), local(&cfg), "x {x} bins {bins:?}");
        // SQL Server before 2022 has no LEAST: the clamp is a CASE.
        assert!(
            c.log.last().unwrap().contains("CASE WHEN FLOOR(("),
            "{:?}",
            c.log
        );
    }
}

#[test]
fn a_word_column_is_not_a_number_on_the_server_either() {
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(4),
        ..ChartConfig::default()
    };
    let mut c = DuckConn::new(table());
    let got = histogram(
        &mut c,
        &source(),
        &request(&cfg, limits()),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(chart_of(got), local(&cfg));
    assert!(c.log.is_empty(), "nothing sent for a word column");
}

#[test]
fn the_server_bar_chart_is_the_local_one() {
    for agg in Aggregation::ALL.iter().copied() {
        let cfg = ChartConfig {
            kind: ChartKind::Bar,
            x_col: Some(0),
            y_cols: vec![1, 2, 4],
            agg,
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = bar(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(chart_of(got), local(&cfg), "{agg:?}");
    }
}

#[test]
fn too_many_bars_counts_every_category() {
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![1],
        ..ChartConfig::default()
    };
    let lim = ChartLimits {
        max_points: 1_000,
        max_categories: 2,
    };
    let mut c = DuckConn::new(table());
    let got = bar(
        &mut c,
        &source(),
        &request(&cfg, lim),
        &AtomicBool::new(false),
    )
    .unwrap();
    // a, b, c and the NULL bar: the exact count, not where the local loop stopped.
    assert_eq!(
        chart_of(got),
        Err(ChartError::TooManyCategories { count: 4, cap: 2 })
    );
}

#[test]
fn every_row_fits_so_line_and_scatter_are_the_local_ones() {
    for kind in [ChartKind::Line, ChartKind::Scatter] {
        let cfg = ChartConfig {
            kind,
            x_col: Some(2),
            y_cols: vec![1],
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = sampled(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(chart_of(got), local(&cfg), "{kind:?}");
    }
}

#[test]
fn more_rows_than_points_keeps_every_nth_by_x() {
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(2),
        y_cols: vec![2],
        ..ChartConfig::default()
    };
    let lim = ChartLimits {
        max_points: 3,
        max_categories: 200,
    };
    let mut c = DuckConn::new(table());
    let got = chart_of(
        sampled(
            &mut c,
            &source(),
            &request(&cfg, lim),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
    .unwrap();
    // 7 rows, step ceil(7 / 3) = 3: the 1st, 4th and 7th smallest f.
    let ChartData::Scatter { series, .. } = &got.data else {
        panic!("scatter");
    };
    let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
    assert_eq!(xs, vec![0.5, 3.5, 6.5]);
    assert_eq!((got.used_rows, got.total_rows), (3, 7));
}

#[test]
fn sampling_off_still_caps_a_database_chart() {
    let n = DEFAULT_MAX_POINTS + 1;
    let t = one_column(
        "Float64",
        (0..n).map(|i| CellValue::Float(i as f64)).collect(),
    );
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![0],
        ..ChartConfig::default()
    };
    let lim = ChartLimits {
        max_points: 0,
        max_categories: 200,
    };
    let mut c = DuckConn::new(t.clone());
    let got = chart_of(
        sampled(
            &mut c,
            &source(),
            &request_for(&t, &cfg, lim),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
    .unwrap();
    // Step ceil(25,001 / 25,000) = 2: every other row, not the whole table.
    assert_eq!((got.used_rows, got.total_rows), (n.div_ceil(2), n));
}

#[test]
fn numbers_stored_as_text_are_spaced_in_number_order() {
    let t = one_column(
        "Utf8",
        ["9", "10", "100", "2", "300"]
            .map(|s| CellValue::String(s.into()))
            .to_vec(),
    );
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![0],
        ..ChartConfig::default()
    };
    let lim = ChartLimits {
        max_points: 3,
        max_categories: 200,
    };
    let mut c = DuckConn::new(t.clone());
    let got = chart_of(
        sampled(
            &mut c,
            &source(),
            &request_for(&t, &cfg, lim),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
    .unwrap();
    let ChartData::Scatter { series, .. } = &got.data else {
        panic!("scatter");
    };
    let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
    // Step 2 over 2, 9, 10, 100, 300; text order would keep 10, 2, 9.
    assert_eq!(xs, vec![2.0, 10.0, 300.0]);
}

#[test]
fn a_column_without_any_x_says_so() {
    let t = one_column("Float64", vec![CellValue::Null; 3]);
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![0],
        ..ChartConfig::default()
    };
    let mut c = DuckConn::new(t.clone());
    let got = sampled(
        &mut c,
        &source(),
        &request_for(&t, &cfg, limits()),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        chart_of(got),
        Err(ChartError::XNotNumeric { col: "v".into() })
    );
}

fn close(a: &ChartPrep, b: &ChartPrep) -> bool {
    let (ChartData::Boxes(x), ChartData::Boxes(y)) = (&a.data, &b.data) else {
        return false;
    };
    let near = |p: f64, q: f64| (p - q).abs() < 1e-9;
    x.len() == y.len()
        && x.iter().zip(y).all(|(s, t)| {
            s.name == t.name
                && near(s.lower_whisker, t.lower_whisker)
                && near(s.q1, t.q1)
                && near(s.median, t.median)
                && near(s.q3, t.q3)
                && near(s.upper_whisker, t.upper_whisker)
        })
        && a.total_rows == b.total_rows
}

#[test]
fn the_server_box_chart_is_the_local_one() {
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        y_cols: vec![1, 2, 3, 4],
        ..ChartConfig::default()
    };
    let mut c = DuckConn::new(table());
    let got = chart_of(
        boxes(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(close(&got, &local(&cfg).unwrap()), "{got:?}");
}

/// 100 lies beyond `q3 + 1.5 IQR`: the whisker stops at the largest
/// value inside the fence, as locally, not at the maximum.
#[test]
fn the_whiskers_stop_at_the_fence() {
    let t = one_column(
        "Float64",
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 100.0]
            .map(CellValue::Float)
            .to_vec(),
    );
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        y_cols: vec![0],
        ..ChartConfig::default()
    };
    let mut c = DuckConn::new(t.clone());
    let got = chart_of(
        boxes(
            &mut c,
            &source(),
            &request_for(&t, &cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
    .unwrap();
    let rows: Vec<usize> = (0..t.row_count()).collect();
    let want = build_chart(&t, &rows, &cfg, limits()).unwrap();
    assert!(close(&got, &want), "{got:?}");
    let ChartData::Boxes(b) = &got.data else {
        panic!("boxes");
    };
    assert_eq!(b[0].upper_whisker, 10.0);
}

#[test]
fn mysql_has_no_percentile_so_box_uses_the_loaded_rows() {
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        y_cols: vec![1],
        ..ChartConfig::default()
    };
    let mut src = source();
    src.conn.engine = DbEngine::MySql;
    let mut c = DuckConn::new(table());
    let got = boxes(
        &mut c,
        &src,
        &request(&cfg, limits()),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(got, ServerChart::NotExpressible);
    assert!(c.log.is_empty(), "nothing sent");
}

#[test]
fn run_picks_the_builder_by_kind() {
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(1),
        ..ChartConfig::default()
    };
    let mut c = DuckConn::new(table());
    let got = run(
        &mut c,
        &source(),
        &request(&cfg, limits()),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(chart_of(got), local(&cfg));
}
