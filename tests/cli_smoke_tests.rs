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
