use crate::cli::{Action, Cli};
use clap::Parser;

fn detect(args: &[&str]) -> Result<Option<Action>, &'static str> {
    let mut argv = vec!["octa"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).unwrap().detect_action()
}

#[test]
fn a_flag_that_belongs_to_another_action_is_refused_not_ignored() {
    // Each of these used to run --head and drop the flag without a word.
    for extra in [
        &["--rules", "r.toml"][..],
        &["--mcp-read-only"],
        &["--mcp-tools", "core"],
        &["--mcp-without", "write"],
        &["--deep"],
        &["--fail-on", "rows:0.1"],
        &["--report-sample", "5"],
        &["--report-sections", "stats"],
        &["--sync-table", "a.b"],
        &["--sync-on", "id"],
        &["--api-path", "/x"],
        &["--partition-layout", "hive"],
        &["--no-recursive"],
        &["--split-rows", "5"],
        &["--split-size", "1MB"],
    ] {
        let mut args = vec!["--head", "f.csv"];
        args.extend_from_slice(extra);
        assert!(detect(&args).is_err(), "{extra:?} beside --head must error");
    }
}

#[test]
fn a_flag_without_any_action_is_refused_too() {
    for args in [
        &["--rules", "r.toml"][..],
        &["--mcp-read-only"],
        &["--deep"],
    ] {
        assert!(detect(args).is_err(), "{args:?} alone must error");
    }
}

#[test]
fn a_flag_beside_its_own_action_still_works() {
    assert!(matches!(
        detect(&["--mcp", "--mcp-read-only"]),
        Ok(Some(Action::Mcp))
    ));
    assert!(matches!(
        detect(&["--check", "f.csv", "--rules", "r.toml"]),
        Ok(Some(Action::Check { .. }))
    ));
    assert!(matches!(
        detect(&["--describe", "f.csv", "--deep"]),
        Ok(Some(Action::Describe { .. }))
    ));
    assert!(matches!(
        detect(&["f.csv", "--split-rows", "5", "--out-dir", "o"]),
        Ok(Some(Action::Partition { .. }))
    ));
    assert!(matches!(
        detect(&[
            "f.csv",
            "--partition-by",
            "c",
            "--split-rows",
            "5",
            "--out-dir",
            "o"
        ]),
        Ok(Some(Action::Partition { .. }))
    ));
}
