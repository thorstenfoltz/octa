//! End-to-end tests for the `octa` CLI: they spawn the real binary and
//! assert on its stdout / stderr / exit code. Everything else in `tests/`
//! calls the library functions directly, so this file is what catches
//! regressions in the layer above them - flag parsing, `detect_action`'s
//! companion-flag checks, the shared `-f / --format` writer, and the exit
//! codes CI pipelines depend on.
//!
//! Deliberately shallow: one assertion per action, on the first stdout
//! line. The engines are covered in depth by their own test files; the
//! point here is that every action is still reachable through the binary
//! and still prints the table it promises.
//!
//! Actions not covered: `--db-*` (need a live server, see
//! `db_live_tests.rs`), the cloud actions (need credentials), `--mcp`
//! (see `mcp_smoke_tests.rs`), and `--dedupe` / `--anonymize`, which have
//! their own dedicated files.

use std::io::Write;
use std::process::Command;

/// A temp directory holding the shared CSV fixtures. Kept alive for the
/// duration of a test; dropping it removes the directory.
struct Fx {
    dir: tempfile::TempDir,
}

struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Out {
    fn ok(&self, what: &str) {
        assert_eq!(
            self.code,
            Some(0),
            "`{what}` exited {:?}\nstdout:\n{}\nstderr:\n{}",
            self.code,
            self.stdout,
            self.stderr
        );
    }

    fn first_line(&self) -> &str {
        self.stdout.lines().next().unwrap_or("")
    }
}

impl Fx {
    /// `a.csv` and `b.csv` share a schema and overlap on `id`, so the
    /// diff / join / union actions all have something to report. `c.csv`
    /// drifts (drops `amount`, adds `extra`) for the schema-validation
    /// failure case.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "a.csv",
            "id,city,amount\n1,Tokyo,10\n2,Helsinki,20\n3,Tokyo,\n",
        );
        write(
            dir.path(),
            "b.csv",
            "id,city,amount\n1,Tokyo,10\n2,Helsinki,99\n4,Cologne,40\n",
        );
        write(dir.path(), "c.csv", "id,city,extra\n1,Tokyo,9\n");
        Self { dir }
    }

    fn path(&self, name: &str) -> String {
        self.dir.path().join(name).to_string_lossy().into_owned()
    }

    /// `OCTA_CONFIG_DIR` points at the temp dir so a run never reads (or
    /// writes) the developer's real `settings.toml`. It is honoured on
    /// every platform, unlike `XDG_CONFIG_HOME`.
    fn run(&self, args: &[&str]) -> Out {
        let out = Command::new(env!("CARGO_BIN_EXE_octa"))
            .args(args)
            .env("OCTA_CONFIG_DIR", self.dir.path().join("config"))
            .current_dir(self.dir.path())
            .output()
            .unwrap_or_else(|e| panic!("failed to run octa {args:?}: {e}"));
        Out {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Render `a.csv`'s schema as JSON Schema via the CLI itself, so the
    /// `--validate-schema` cases below double as a round-trip check:
    /// what `--export-schema` emits, `--validate-schema` must accept.
    fn json_schema_of_a(&self) -> String {
        let out = self.run(&["--export-schema", &self.path("a.csv"), "-t", "json-schema"]);
        out.ok("--export-schema -t json-schema");
        let path = self.path("a.schema.json");
        std::fs::write(&path, out.stdout.as_bytes()).unwrap();
        path
    }
}

fn write(dir: &std::path::Path, name: &str, body: &str) {
    let mut f = std::fs::File::create(dir.join(name)).unwrap();
    f.write_all(body.as_bytes()).unwrap();
}

/// Every file-based action runs through the binary and prints the table
/// header it documents. This is the breadth pass: if a new action lands
/// without an arm in `dispatch`, or a flag is renamed, it fails here.
#[test]
fn every_file_action_runs_and_prints_its_header() {
    let fx = Fx::new();
    let a = fx.path("a.csv");
    let b = fx.path("b.csv");
    let schema = fx.json_schema_of_a();

    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["--schema", &a], "name\ttype"),
        (vec!["--head", &a, "-n", "2"], "id\tcity\tamount"),
        (vec!["--tail", &a, "-n", "2"], "id\tcity\tamount"),
        (
            vec!["--sample", &a, "-n", "2", "--seed", "1"],
            "id\tcity\tamount",
        ),
        (vec!["--describe", &a], "field\tvalue"),
        (
            vec!["--unique-columns", &a],
            "scope\tcolumns\tdistinct_count\tnull_count\tis_unique",
        ),
        (
            vec!["--detect-pii", &a],
            "column\tkind\tconfidence\tby_name\tvalue_match",
        ),
        (
            vec![
                "--sql",
                &a,
                "-q",
                "SELECT city, sum(amount) AS total FROM data GROUP BY city",
            ],
            "city\ttotal",
        ),
        (
            vec!["--export-schema", &a, "-t", "postgres"],
            "-- Generated by octa - Postgres dialect",
        ),
        (
            vec!["--compare-schemas", &a, &b],
            "status\tcolumn\ttype_a\ttype_b",
        ),
        (vec!["--diff", &a, &b], "status\tid\tcity\tamount"),
        (
            vec!["--diff", &a, &b, "--diff-mode", "ordered"],
            "status\tchanged_columns\tid\tcity\tamount",
        ),
        (
            vec!["--diff", &a, &b, "--diff-mode", "join", "--diff-on", "id"],
            "status\tchanged_columns\tid\tcity\tamount",
        ),
        (vec!["--union", &a, "--union-file", &b], "id\tcity\tamount"),
        (
            vec!["--join", &a, "--join-file", &b, "--join-on", "id"],
            "id\tcity\tamount\tcity\tamount",
        ),
        (vec!["--outliers", &a], "row\tcolumn\tvalue"),
        (vec!["--impute", "amount=mean", &a], "id\tcity\tamount"),
        (
            vec!["--validate-schema", &a, "--expect-schema", &schema],
            "status\tcolumn\tactual_type\texpected_type",
        ),
    ];

    for (args, expect_first_line) in cases {
        let label = args.join(" ");
        let out = fx.run(&args);
        out.ok(&label);
        assert_eq!(
            out.first_line(),
            expect_first_line,
            "`octa {label}` printed an unexpected first line\nfull stdout:\n{}",
            out.stdout
        );
    }
}

/// The CI contract: exit 0 when the schemas match, exit 1 when they
/// drift, with the findings on stdout. Documented in `--help` and in
/// CLAUDE.md as the reason `validate_schema::run` returns its own
/// `ExitCode` instead of going through the normal Result mapping.
#[test]
fn validate_schema_exit_code_signals_drift() {
    let fx = Fx::new();
    let schema = fx.json_schema_of_a();

    let matching = fx.run(&[
        "--validate-schema",
        &fx.path("a.csv"),
        "--expect-schema",
        &schema,
    ]);
    assert_eq!(
        matching.code,
        Some(0),
        "a file matching its own exported schema must exit 0\nstdout:\n{}\nstderr:\n{}",
        matching.stdout,
        matching.stderr
    );
    // Only the header: a clean match reports no findings.
    assert_eq!(matching.stdout.lines().count(), 1, "{}", matching.stdout);

    let drifted = fx.run(&[
        "--validate-schema",
        &fx.path("c.csv"),
        "--expect-schema",
        &schema,
    ]);
    assert_eq!(
        drifted.code,
        Some(1),
        "schema drift must exit 1 so CI can gate on it\nstdout:\n{}\nstderr:\n{}",
        drifted.stdout,
        drifted.stderr
    );
    // `c.csv` adds `extra` and drops `amount`; both must be reported.
    assert!(
        drifted.stdout.contains("unexpected\textra"),
        "missing the added-column finding:\n{}",
        drifted.stdout
    );
    assert!(
        drifted.stdout.contains("missing\tamount"),
        "missing the dropped-column finding:\n{}",
        drifted.stdout
    );
}

/// `-f / --format` is global and routes through `cli::output`. Pinning
/// all three here means a change to the writer cannot pass unnoticed
/// just because every other test reads the default TSV.
#[test]
fn format_flag_switches_the_output_writer() {
    let fx = Fx::new();
    let a = fx.path("a.csv");

    let tsv = fx.run(&["--head", &a, "-n", "1"]);
    tsv.ok("--head (default format)");
    assert_eq!(tsv.stdout, "id\tcity\tamount\n1\tTokyo\t10\n");

    let csv = fx.run(&["-f", "csv", "--head", &a, "-n", "1"]);
    csv.ok("--head -f csv");
    assert_eq!(csv.stdout, "id,city,amount\n1,Tokyo,10\n");

    let json = fx.run(&["-f", "json", "--head", &a, "-n", "1"]);
    json.ok("--head -f json");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], serde_json::json!(1));
    assert_eq!(rows[0]["city"], serde_json::json!("Tokyo"));
}

/// `--convert` and `--partition-by` write files instead of printing a
/// table, so they need their own assertions.
#[test]
fn file_writing_actions_produce_their_output_files() {
    let fx = Fx::new();
    let a = fx.path("a.csv");

    let converted = fx.path("out.json");
    let out = fx.run(&["--convert", &a, &converted]);
    out.ok("--convert");
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&converted).unwrap()).unwrap();
    assert_eq!(rows.len(), 3, "converted file should hold every source row");
    // The summary goes to stderr so stdout stays pipeable.
    assert!(
        out.stdout.is_empty(),
        "stdout should stay clean: {}",
        out.stdout
    );
    assert!(out.stderr.contains("out.json"), "{}", out.stderr);

    let parts = fx.path("parts");
    let out = fx.run(&["--partition-by", "city", "--out-dir", &parts, &a]);
    out.ok("--partition-by");
    // Two distinct cities in `a.csv`, so two files.
    let mut written: Vec<String> = std::fs::read_dir(&parts)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, vec!["helsinki.csv", "tokyo.csv"]);
}

/// `--rows` is parsed before any action runs, so a bad value must fail
/// fast rather than surface halfway through a read.
#[test]
fn rows_flag_is_validated_before_the_action_runs() {
    let fx = Fx::new();
    let a = fx.path("a.csv");

    let bad = fx.run(&["--rows", "nope", "--head", &a]);
    assert_eq!(bad.code, Some(1), "stdout:\n{}", bad.stdout);
    assert!(bad.stderr.contains("--rows"), "{}", bad.stderr);
    assert!(
        bad.stdout.is_empty(),
        "no rows should be printed: {}",
        bad.stdout
    );

    // `all` and separator-laden numbers are both accepted.
    for value in ["all", "10,000"] {
        let out = fx.run(&["--rows", value, "--head", &a, "-n", "1"]);
        out.ok(&format!("--rows {value}"));
        assert_eq!(out.first_line(), "id\tcity\tamount");
    }
}

/// `detect_action` rejects an action whose required companion flag is
/// missing, and clap rejects two action flags at once. Both paths only
/// exist in the binary, so nothing else can cover them.
#[test]
fn missing_companions_and_conflicting_actions_are_rejected() {
    let fx = Fx::new();
    let a = fx.path("a.csv");
    let b = fx.path("b.csv");

    // Missing companion -> our own error, exit 1.
    for (args, needle) in [
        (vec!["--sql", a.as_str()], "--query"),
        (
            vec!["--join", a.as_str(), "--join-file", b.as_str()],
            "--join-on",
        ),
        (vec!["--validate-schema", a.as_str()], "--expect-schema"),
        (vec!["--partition-by", "city", a.as_str()], "--out-dir"),
    ] {
        let out = fx.run(&args);
        assert_eq!(
            out.code,
            Some(1),
            "`octa {}` should fail with exit 1\nstderr:\n{}",
            args.join(" "),
            out.stderr
        );
        assert!(
            out.stderr.contains(needle),
            "`octa {}` should mention `{needle}`, got:\n{}",
            args.join(" "),
            out.stderr
        );
    }

    // Two action flags -> clap's mutually-exclusive group rejects it
    // during parsing, before `detect_action` is ever reached.
    let clash = fx.run(&["--schema", &a, "--head", &b]);
    assert_ne!(clash.code, Some(0), "stdout:\n{}", clash.stdout);
    assert!(
        clash.stderr.contains("cannot be used with"),
        "expected a clap conflict error, got:\n{}",
        clash.stderr
    );
}

/// `--resample` and `--rolling` build DuckDB SQL from flags and print a table.
#[test]
fn timeseries_actions_bucket_and_roll() {
    let fx = Fx::new();
    write(
        fx.dir.path(),
        "ts.csv",
        "day,amount\n2024-01-05,10\n2024-01-20,5\n2024-02-02,7\n",
    );
    let ts = fx.path("ts.csv");

    let out = fx.run(&[
        "--resample",
        "day",
        "--interval",
        "month",
        "--agg",
        "sum",
        "--value-cols",
        "amount",
        &ts,
    ]);
    out.ok("--resample");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines[0].contains("bucket"), "header: {}", lines[0]);
    assert_eq!(lines.len(), 3, "header plus two months: {}", out.stdout);
    // The positional FILE is this action's input, not a stray argument.
    assert!(
        !out.stderr.contains("ignoring trailing files"),
        "stderr: {}",
        out.stderr
    );

    let out = fx.run(&[
        "--rolling",
        "amount",
        "--window",
        "2",
        "--order-by",
        "day",
        "--agg",
        "mean",
        &ts,
    ]);
    out.ok("--rolling");
    assert!(
        out.first_line().contains("amount_rolling_2"),
        "header: {}",
        out.stdout
    );

    // A missing companion flag is an error, not a silent default.
    let out = fx.run(&["--rolling", "amount", &ts]);
    assert_eq!(out.code, Some(1), "--rolling without --order-by must fail");
    assert!(out.stderr.contains("--order-by"), "{}", out.stderr);
}

/// `--batch-convert` converts N positional files into --out-dir and exits 1
/// when any single item failed, so a script can gate on it.
#[test]
fn batch_convert_writes_every_input_and_reports_failures() {
    let fx = Fx::new();
    let out = fx.path("batch_out");

    let out_run = fx.run(&[
        "--batch-convert",
        "--to",
        "json",
        "--out-dir",
        &out,
        &fx.path("a.csv"),
        &fx.path("b.csv"),
    ]);
    out_run.ok("--batch-convert");
    assert!(std::path::Path::new(&out).join("a.json").exists());
    assert!(std::path::Path::new(&out).join("b.json").exists());
    // Bare listing: input TAB output TAB status, headerless.
    let lines: Vec<&str> = out_run.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "one line per input: {}", out_run.stdout);
    assert!(lines[0].ends_with("\tdone"), "{}", lines[0]);

    // A missing input fails that item and the run, without stopping the good one.
    let out2 = fx.path("batch_out2");
    let bad = fx.run(&[
        "--batch-convert",
        "--to",
        "json",
        "--out-dir",
        &out2,
        &fx.path("nope.csv"),
        &fx.path("a.csv"),
    ]);
    assert_eq!(
        bad.code,
        Some(1),
        "a failed item must exit 1: {}",
        bad.stdout
    );
    assert!(
        std::path::Path::new(&out2).join("a.json").exists(),
        "the good input must still convert"
    );

    // --out-dir is required.
    let missing = fx.run(&["--batch-convert", "--to", "json", &fx.path("a.csv")]);
    assert_eq!(missing.code, Some(1));
    assert!(missing.stderr.contains("--out-dir"), "{}", missing.stderr);
}

/// `--describe --deep` adds the file's physical layout: the facts as
/// `key<tab>value` lines, then the per-column-per-row-group table. The
/// fixture is produced by the binary itself, so this also pins that a
/// converted Parquet file is inspectable.
#[test]
fn describe_deep_reports_the_physical_layout() {
    let fx = Fx::new();
    let src = fx.path("a.csv");
    let parquet = fx.path("a.parquet");
    fx.run(&["--convert", &src, &parquet]).ok("--convert");

    // `--deep` follows the file, like every other companion flag: clap would
    // otherwise read it as the value of `--describe`.
    let out = fx.run(&["--describe", &parquet, "--deep"]);
    out.ok("--describe --deep");
    assert!(
        out.stdout.contains("row_groups"),
        "missing row_groups in:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("created_by"),
        "missing created_by in:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("row_group\tcolumn"),
        "missing the chunk table header in:\n{}",
        out.stdout
    );
}

/// Without `--deep` the output is unchanged, so existing scripts that
/// parse `--describe` keep working.
#[test]
fn describe_without_deep_has_no_internals() {
    let fx = Fx::new();
    let out = fx.run(&["--describe", &fx.path("a.csv")]);
    out.ok("--describe");
    assert!(!out.stdout.contains("row_group"), "{}", out.stdout);
}

/// Write options reach the file: `--compression zstd` must actually
/// produce a zstd-compressed Parquet, verified by reading the codec back
/// out of the file's own metadata.
#[test]
fn convert_honours_the_compression_flag() {
    let fx = Fx::new();
    let out = fx.path("compressed.parquet");
    let res = fx.run(&[
        "--convert",
        &fx.path("a.csv"),
        &out,
        "--compression",
        "zstd",
    ]);
    res.ok("--convert --compression zstd");

    let internals = octa::data::file_internals::inspect(std::path::Path::new(&out)).unwrap();
    let codec = internals
        .chunks
        .get(0, 3)
        .map(|c| c.to_string())
        .unwrap_or_default();
    assert!(codec.contains("ZSTD"), "got {codec}");
}

/// An unknown codec is refused at parse time, naming the valid set,
/// rather than silently falling back mid-write.
#[test]
fn convert_rejects_an_unknown_codec() {
    let fx = Fx::new();
    let res = fx.run(&[
        "--convert",
        &fx.path("a.csv"),
        &fx.path("out.parquet"),
        "--compression",
        "banana",
    ]);
    assert_ne!(res.code, Some(0), "stdout: {}", res.stdout);
    assert!(
        res.stderr.contains("snappy"),
        "the error should list the valid codecs: {}",
        res.stderr
    );
}

/// `--row-group-size` splits the output into the requested groups.
#[test]
fn convert_honours_the_row_group_size_flag() {
    let fx = Fx::new();
    let out = fx.path("grouped.parquet");
    fx.run(&[
        "--convert",
        &fx.path("a.csv"),
        &out,
        "--row-group-size",
        "2",
    ])
    .ok("--convert --row-group-size 2");

    let internals = octa::data::file_internals::inspect(std::path::Path::new(&out)).unwrap();
    let facts: std::collections::HashMap<String, String> =
        internals.facts.iter().cloned().collect();
    // Three rows, two per group: two groups.
    assert_eq!(facts.get("row_groups").map(String::as_str), Some("2"));
}

/// `--diff` can take a live database table as its B side. Without a
/// server the interesting assertion is the failure mode: the error must
/// name the connection that could not be found, not complain about a
/// missing second file.
#[test]
fn diff_db_reports_an_unknown_connection() {
    let fx = Fx::new();
    let res = fx.run(&[
        "--diff",
        &fx.path("a.csv"),
        "--diff-db",
        "no_such_connection",
        "--diff-db-table",
        "public.orders",
        "--diff-mode",
        "join",
        "--diff-on",
        "id",
    ]);
    assert_ne!(res.code, Some(0), "stdout: {}", res.stdout);
    assert!(
        res.stderr.contains("no_such_connection"),
        "the error should name the connection: {}",
        res.stderr
    );
}

/// `--diff-db` without its table companion is refused up front.
#[test]
fn diff_db_requires_a_table() {
    let fx = Fx::new();
    let res = fx.run(&["--diff", &fx.path("a.csv"), "--diff-db", "conn"]);
    assert_ne!(res.code, Some(0));
    assert!(
        res.stderr.contains("--diff-db-table"),
        "the error should name the missing flag: {}",
        res.stderr
    );
}

/// A folder whose files agree exits 0; one odd file exits 1. This is the
/// contract a CI step depends on, so it is pinned rather than assumed.
#[test]
fn schema_drift_exit_codes() {
    let fx = Fx::new();
    // Own subfolder: Fx's own c.csv drifts on purpose, so scanning the
    // fixture root could never produce the exit-0 case.
    let scan = fx.dir.path().join("drift_clean");
    std::fs::create_dir(&scan).unwrap();
    write(&scan, "a.csv", "id,amount\n1,2.5\n");
    write(&scan, "b.csv", "id,amount\n2,3.5\n");
    let dir = scan.to_str().unwrap();

    let clean = fx.run(&["--schema-drift", dir]);
    assert_eq!(
        clean.code,
        Some(0),
        "matching schemas must exit 0\nstdout:\n{}\nstderr:\n{}",
        clean.stdout,
        clean.stderr
    );
    assert!(
        clean.stdout.contains("status"),
        "expected a report table, got:\n{}",
        clean.stdout
    );

    write(&scan, "c.csv", "id,other\n3,x\n");
    let drifted = fx.run(&["--schema-drift", dir]);
    assert_eq!(
        drifted.code,
        Some(1),
        "drift must exit 1 for CI gating\nstdout:\n{}\nstderr:\n{}",
        drifted.stdout,
        drifted.stderr
    );
}

/// Case folding is a flag, not the default.
/// `--harmonise-schema` writes harmonised copies and leaves the originals be.
#[test]
fn harmonise_schema_writes_copies_and_keeps_originals() {
    let fx = Fx::new();
    let scan = fx.dir.path().join("harm_in");
    let out = fx.dir.path().join("harm_out");
    std::fs::create_dir(&scan).unwrap();
    write(&scan, "a.csv", "id,name\n1,alice\n");
    write(&scan, "b.csv", "id,name\n2,bob\n");
    // Missing `name`, so it needs harmonising.
    write(&scan, "c.csv", "id\n3\n");
    let before = std::fs::read_to_string(scan.join("c.csv")).unwrap();

    let r = fx.run(&[
        "--harmonise-schema",
        scan.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        r.code,
        Some(0),
        "nothing should be refused\nstdout:\n{}\nstderr:\n{}",
        r.stdout,
        r.stderr
    );
    assert!(
        r.stdout
            .lines()
            .next()
            .unwrap_or_default()
            .contains("input"),
        "expected a report table, got:\n{}",
        r.stdout
    );
    let fixed = std::fs::read_to_string(out.join("c.csv")).unwrap();
    assert!(
        fixed.starts_with("id,name"),
        "the missing column should have been added: {fixed:?}"
    );
    assert_eq!(
        std::fs::read_to_string(scan.join("c.csv")).unwrap(),
        before,
        "the original must be byte-identical"
    );
}

/// A file whose values will not survive the cast is refused, and that is an
/// exit-1 outcome so CI can gate on it.
#[test]
fn harmonise_schema_exits_one_when_a_file_is_refused() {
    let fx = Fx::new();
    let scan = fx.dir.path().join("harm_bad");
    let out = fx.dir.path().join("harm_bad_out");
    std::fs::create_dir(&scan).unwrap();
    write(&scan, "good.csv", "id\n1\n");
    write(&scan, "bad.csv", "id\nnot-a-number\n");

    let r = fx.run(&[
        "--harmonise-schema",
        scan.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--target-file",
        scan.join("good.csv").to_str().unwrap(),
    ]);
    assert_eq!(
        r.code,
        Some(1),
        "a refused file must exit 1\nstdout:\n{}\nstderr:\n{}",
        r.stdout,
        r.stderr
    );
    assert!(
        !out.join("bad.csv").exists(),
        "a refused file must leave nothing behind"
    );
}

/// --out-dir is what makes this non-destructive, so its absence is an error
/// rather than a silent default.
#[test]
fn harmonise_schema_requires_an_out_dir() {
    let fx = Fx::new();
    let r = fx.run(&["--harmonise-schema", fx.dir.path().to_str().unwrap()]);
    assert_ne!(r.code, Some(0), "stdout:\n{}", r.stdout);
    assert!(
        r.stderr.contains("--out-dir"),
        "the error should name the missing flag: {}",
        r.stderr
    );
}

#[test]
fn schema_drift_ignore_case_flag() {
    let fx = Fx::new();
    let scan = fx.dir.path().join("drift_case");
    std::fs::create_dir(&scan).unwrap();
    write(&scan, "a.csv", "Amount\n1\n");
    write(&scan, "b.csv", "amount\n2\n");
    let dir = scan.to_str().unwrap();

    let strict = fx.run(&["--schema-drift", dir]);
    assert_eq!(
        strict.code,
        Some(1),
        "differing case is drift by default\nstderr:\n{}",
        strict.stderr
    );

    let folded = fx.run(&["--schema-drift", dir, "--ignore-case"]);
    assert_eq!(
        folded.code,
        Some(0),
        "--ignore-case must collapse them\nstderr:\n{}",
        folded.stderr
    );
}

#[test]
fn report_writes_a_self_contained_html_file() {
    let fx = Fx::new();
    let out = fx.dir.path().join("report.html");
    let out_arg = out.to_str().unwrap().to_string();

    let result = fx.run(&["--report", &out_arg, &fx.path("a.csv")]);
    assert_eq!(
        result.code,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let html = std::fs::read_to_string(&out).expect("report was not written");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("amount"));
    // Nothing may be fetched. An inline SVG's xmlns is a namespace, not a
    // fetch, so assert on remote references rather than the bare substring.
    for attr in ["src=\"http", "href=\"http", "url(http", "@import"] {
        assert!(!html.contains(attr), "found a remote reference ({attr})");
    }
}

/// The input is positional, so it must be in main.rs's allowlist or the binary
/// warns about ignoring its own argument.
#[test]
fn report_does_not_warn_about_its_own_input() {
    let fx = Fx::new();
    let out = fx.dir.path().join("r.html");
    let out_arg = out.to_str().unwrap().to_string();

    let result = fx.run(&["--report", &out_arg, &fx.path("a.csv")]);
    assert!(
        !result.stderr.contains("ignoring trailing files"),
        "the positional input was treated as a stray file:\n{}",
        result.stderr
    );
}

/// An unknown section name is a typo, not a silent no-op.
#[test]
fn report_rejects_an_unknown_section() {
    let fx = Fx::new();
    let out = fx.dir.path().join("r.html");
    let out_arg = out.to_str().unwrap().to_string();

    let result = fx.run(&[
        "--report",
        &out_arg,
        &fx.path("a.csv"),
        "--report-sections",
        "stats,nonsense",
    ]);
    assert_eq!(result.code, Some(1), "stderr:\n{}", result.stderr);
    assert!(
        result.stderr.contains("nonsense"),
        "the bad name must be quoted back:\n{}",
        result.stderr
    );
}

#[test]
fn fuzzy_join_matches_spelling_variants() {
    let fx = Fx::new();
    write(fx.dir.path(), "crm.csv", "customer\nMueller GmbH\n");
    write(fx.dir.path(), "sales.csv", "account\nMueller Gmbh.\n");

    let out = fx.run(&[
        "--fuzzy-join",
        "--fuzzy-join-file",
        &fx.path("sales.csv"),
        "--fuzzy-on",
        "customer=account",
        &fx.path("crm.csv"),
    ]);
    assert_eq!(
        out.code,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stdout.contains("match_score_1"),
        "expected the score column:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("Mueller Gmbh."),
        "expected the matched partner:\n{}",
        out.stdout
    );
}

/// A column name that does not exist is named back, not silently ignored.
#[test]
fn fuzzy_join_rejects_an_unknown_column() {
    let fx = Fx::new();
    write(fx.dir.path(), "crm.csv", "customer\nMueller GmbH\n");
    write(fx.dir.path(), "sales.csv", "account\nMueller Gmbh.\n");

    let out = fx.run(&[
        "--fuzzy-join",
        "--fuzzy-join-file",
        &fx.path("sales.csv"),
        "--fuzzy-on",
        "customer=nope",
        &fx.path("crm.csv"),
    ]);
    assert_eq!(out.code, Some(1));
    assert!(
        out.stderr.contains("nope"),
        "the missing column must be named:\n{}",
        out.stderr
    );
}

/// The CLI reads the saved write options, so a `--convert` and a GUI Save on
/// the same machine produce the same file. A flag still beats the settings
/// file. Without this the two surfaces silently disagree about compression.
#[test]
fn convert_honours_write_options_from_settings() {
    let fx = Fx::new();
    let config = fx.dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    // `#[serde(default)]` on AppSettings fills everything else in.
    write(
        &config,
        "settings.toml",
        "[write_options.parquet]\ncompression = \"snappy\"\n",
    );

    fx.run(&[
        "--convert",
        &fx.path("a.csv"),
        &fx.path("from_settings.parquet"),
    ])
    .ok("--convert with settings");
    let deep = fx.run(&["--describe", &fx.path("from_settings.parquet"), "--deep"]);
    deep.ok("--describe --deep");
    assert!(
        deep.stdout.contains("SNAPPY"),
        "the settings codec must reach the writer:\n{}",
        deep.stdout
    );

    fx.run(&[
        "--convert",
        &fx.path("a.csv"),
        &fx.path("from_flag.parquet"),
        "--compression",
        "uncompressed",
    ])
    .ok("--convert with flag");
    let deep = fx.run(&["--describe", &fx.path("from_flag.parquet"), "--deep"]);
    deep.ok("--describe --deep");
    assert!(
        deep.stdout.contains("UNCOMPRESSED") && !deep.stdout.contains("SNAPPY"),
        "the flag must override the settings file:\n{}",
        deep.stdout
    );
}

#[test]
fn sync_sql_needs_its_companion_flags() {
    let fx = Fx::new();

    // Missing --sync-on: the plan cannot address rows without key columns,
    // so this must fail loudly rather than diff positionally.
    let out = fx.run(&[
        "--sync-sql",
        &fx.path("a.csv"),
        "--db",
        "nope",
        "--sync-table",
        "public.orders",
    ]);
    assert_ne!(out.code, Some(0), "stdout:\n{}", out.stdout);
    assert!(
        out.stderr.contains("--sync-on"),
        "stderr should name the missing flag, got:\n{}",
        out.stderr
    );

    // Missing --sync-table: nothing says which server table to compare with.
    let out = fx.run(&[
        "--sync-sql",
        &fx.path("a.csv"),
        "--db",
        "nope",
        "--sync-on",
        "id",
    ]);
    assert_ne!(out.code, Some(0));
    assert!(
        out.stderr.contains("--sync-table"),
        "stderr should name the missing flag, got:\n{}",
        out.stderr
    );
}

#[test]
fn to_workbook_writes_one_sheet_per_input() {
    let fx = Fx::new();
    let out = fx.path("book.xlsx");

    let run = fx.run(&["--to-workbook", &out, &fx.path("a.csv"), &fx.path("b.csv")]);
    run.ok("--to-workbook");
    assert!(
        std::path::Path::new(&out).exists(),
        "workbook was not written\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    // The listing names each sheet so a script can check what it produced.
    assert!(run.stdout.contains('a'), "stdout was: {}", run.stdout);
}

#[test]
fn to_workbook_needs_more_than_one_input() {
    let fx = Fx::new();
    let out = fx.path("single.xlsx");
    let run = fx.run(&["--to-workbook", &out, &fx.path("a.csv")]);
    assert_ne!(run.code, Some(0));
    assert!(
        run.stderr.contains("--convert"),
        "a single input should point at --convert, got:\n{}",
        run.stderr
    );
}

#[test]
fn reads_a_table_from_stdin() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_octa"))
        .args(["--schema", "-"])
        .env("OCTA_CONFIG_DIR", dir.path().join("config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn octa");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"name,age\nada,36\ngrace,45\n")
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "exited {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    // Sniffed as CSV even though a pipe has no file name to go by.
    assert!(stdout.contains("name"), "stdout was: {stdout}");
    assert!(stdout.contains("age"), "stdout was: {stdout}");
}

#[test]
fn empty_stdin_is_an_error_not_an_empty_table() {
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_octa"))
        .args(["--schema", "-"])
        .env("OCTA_CONFIG_DIR", dir.path().join("config"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn octa")
        .wait_with_output()
        .expect("wait");
    assert_ne!(child.status.code(), Some(0));
    let err = String::from_utf8_lossy(&child.stderr);
    assert!(err.contains("stdin"), "stderr was: {err}");
}

#[test]
fn convert_to_stdout_needs_an_explicit_format() {
    let fx = Fx::new();
    // Nothing in `-` says what format to write, so --to is required.
    let run = fx.run(&["--convert", &fx.path("a.csv"), "-"]);
    assert_ne!(run.code, Some(0));
    assert!(
        run.stderr.contains("--to"),
        "stderr should name --to, got:\n{}",
        run.stderr
    );
}

#[test]
fn convert_writes_to_stdout() {
    let fx = Fx::new();
    let run = fx.run(&["--convert", &fx.path("a.csv"), "-", "--to", "json"]);
    run.ok("--convert to stdout");
    assert!(
        run.stdout.trim_start().starts_with('['),
        "expected JSON on stdout, got:\n{}",
        run.stdout
    );
}

#[test]
fn partition_hive_layout_writes_key_equals_value_directories() {
    let fx = Fx::new();
    let out = fx.path("hive_out");

    let run = fx.run(&[
        "--partition-by",
        "city",
        "--out-dir",
        &out,
        "--partition-layout",
        "hive",
        &fx.path("a.csv"),
    ]);
    run.ok("--partition-by --partition-layout hive");

    let root = std::path::Path::new(&out);
    assert!(
        root.join("city=Tokyo").join("data.csv").exists(),
        "missing Tokyo"
    );
    assert!(
        root.join("city=Helsinki").join("data.csv").exists(),
        "missing Helsinki"
    );
    // The listing still names every file it wrote.
    assert!(
        run.stdout.contains("city=Tokyo"),
        "stdout was: {}",
        run.stdout
    );

    // Case is preserved, unlike the flat layout's sanitised stems: the value
    // has to be recoverable from the directory name for a reader to use it as
    // a partition column.
    assert!(
        !root.join("city=tokyo").is_dir(),
        "hive values must keep their case"
    );

    // The point of the layout: Octa reads the folder back as one table with
    // `city` restored as a column from the directory names. Without this the
    // feature is just a different filing scheme.
    use octa::formats::lakehouse_reader::{LakehouseKind, PartsFamily, read_dir_report};
    let (back, skipped) =
        read_dir_report(root, LakehouseKind::Parts(PartsFamily::Delimited)).expect("read back");
    assert!(skipped.is_empty(), "skipped: {skipped:?}");
    assert_eq!(back.row_count(), 3, "all rows should come back");
    assert!(
        back.columns.iter().any(|c| c.name == "city"),
        "partition column not restored: {:?}",
        back.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn partition_defaults_to_flat_files() {
    let fx = Fx::new();
    let out = fx.path("flat_out");
    let run = fx.run(&[
        "--partition-by",
        "city",
        "--out-dir",
        &out,
        &fx.path("a.csv"),
    ]);
    run.ok("--partition-by");
    let root = std::path::Path::new(&out);
    // Flat stems go through `sanitize_sql_name`, which lowercases.
    assert!(root.join("tokyo.csv").exists(), "default layout changed");
    assert!(!root.join("city=Tokyo").is_dir(), "should not be Hive");
}

#[test]
fn partition_rejects_an_unknown_layout() {
    let fx = Fx::new();
    let run = fx.run(&[
        "--partition-by",
        "city",
        "--out-dir",
        &fx.path("x"),
        "--partition-layout",
        "nested",
        &fx.path("a.csv"),
    ]);
    assert_ne!(run.code, Some(0));
    assert!(
        run.stderr.contains("flat") && run.stderr.contains("hive"),
        "the error should name both valid words, got:\n{}",
        run.stderr
    );
}

#[test]
fn drift_report_exits_one_when_a_threshold_is_breached() {
    let fx = Fx::new();
    // `a.csv` has one missing amount of three, `b.csv` has none, so the null
    // rate moves far enough to breach a five percent gate.
    let run = fx.run(&[
        "--drift-report",
        &fx.path("a.csv"),
        &fx.path("b.csv"),
        "--fail-on",
        "null_rate:0.05",
    ]);
    assert_eq!(
        run.code,
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("null_rate"),
        "the report should name the metric, got:\n{}",
        run.stdout
    );
}

#[test]
fn drift_report_exits_zero_without_thresholds() {
    let fx = Fx::new();
    let run = fx.run(&["--drift-report", &fx.path("a.csv"), &fx.path("b.csv")]);
    run.ok("--drift-report without --fail-on is informational only");
}

/// `--check` is a CI gate, so its exit code is the contract: 1 when a rule
/// fails, 1 when a rule cannot even run, 0 only when everything passed.
#[test]
fn check_exits_one_on_a_violation() {
    let fx = Fx::new();
    std::fs::write(fx.path("d.csv"), "order_id\n1\n1\n").unwrap();
    std::fs::write(
        fx.path("q.toml"),
        "[[rule]]\ncolumn = \"order_id\"\nkind = \"unique\"\n",
    )
    .unwrap();

    let run = fx.run(&["--check", &fx.path("d.csv"), "--rules", &fx.path("q.toml")]);
    assert_eq!(run.code, Some(1), "stderr:\n{}", run.stderr);
    assert!(
        run.stdout.contains("order_id"),
        "stdout was:\n{}",
        run.stdout
    );
}

#[test]
fn check_exits_zero_when_clean() {
    let fx = Fx::new();
    std::fs::write(fx.path("d.csv"), "order_id\n1\n2\n").unwrap();
    std::fs::write(
        fx.path("q.toml"),
        "[[rule]]\ncolumn = \"order_id\"\nkind = \"unique\"\n",
    )
    .unwrap();

    let run = fx.run(&["--check", &fx.path("d.csv"), "--rules", &fx.path("q.toml")]);
    run.ok("--check on clean data");
}

#[test]
fn check_exits_one_on_an_unknown_column() {
    let fx = Fx::new();
    std::fs::write(fx.path("d.csv"), "order_id\n1\n").unwrap();
    std::fs::write(
        fx.path("q.toml"),
        "[[rule]]\ncolumn = \"missing\"\nkind = \"unique\"\n",
    )
    .unwrap();

    let run = fx.run(&["--check", &fx.path("d.csv"), "--rules", &fx.path("q.toml")]);
    assert_eq!(
        run.code,
        Some(1),
        "a rule that cannot run must fail the gate; stderr:\n{}",
        run.stderr
    );
}

#[test]
fn relationships_names_a_shared_key() {
    let fx = Fx::new();
    let sub = fx.dir.path().join("rel");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("orders.csv"), "customer_id\n1\n2\n").unwrap();
    std::fs::write(sub.join("customers.csv"), "id\n1\n2\n").unwrap();

    let run = fx.run(&["--relationships", &sub.to_string_lossy()]);
    run.ok("--relationships");
    assert!(
        run.stdout.contains("customer_id"),
        "stdout was:\n{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("orphans"),
        "stdout was:\n{}",
        run.stdout
    );
}

#[test]
fn stream_mode_reads_schema_without_the_row_cap() {
    let fx = Fx::new();
    let mut s = String::from("id,name\n");
    for i in 0..50_000 {
        s.push_str(&format!("{i},row{i}\n"));
    }
    std::fs::write(fx.path("big.csv"), s).unwrap();

    let run = fx.run(&[
        "--sql",
        &fx.path("big.csv"),
        "--query",
        "SELECT count(*) AS n FROM data",
        "--stream",
        "--rows",
        "10",
    ]);
    run.ok("--sql --stream");
    assert!(
        run.stdout.contains("50000"),
        "the whole file must be counted even with a tiny row cap; stdout was:\n{}",
        run.stdout
    );
}

#[test]
fn stream_warns_when_the_action_cannot_use_it() {
    let fx = Fx::new();
    let run = fx.run(&["--schema", &fx.path("a.csv"), "--stream"]);
    run.ok("--schema --stream");
    assert!(
        run.stderr.contains("--stream"),
        "an ignored flag must say so; stderr was:\n{}",
        run.stderr
    );
}
