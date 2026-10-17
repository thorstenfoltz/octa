use super::*;

fn params() -> Params {
    Params {
        connection: "warehouse".into(),
        schema: "public".into(),
        catalog: None,
        tables: Vec::new(),
        out_dir: PathBuf::from("dump"),
        format: None,
        overwrite: false,
    }
}

#[test]
fn a_read_only_session_refuses_before_any_network() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), true, 0);
    let err = run(&ctx, &params()).unwrap_err().to_string();
    assert!(err.contains("read-only"), "{err}");
}

#[test]
fn an_unknown_format_is_named() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    let p = Params {
        format: Some("xlsx".into()),
        ..params()
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("parquet, csv, tsv or jsonl"), "{err}");
}

#[test]
fn the_summary_counts_each_outcome() {
    let v = summary(&[
        TableOutcome::Written {
            table: "a".into(),
            path: "a.parquet".into(),
            rows: 3,
        },
        TableOutcome::Skipped {
            table: "b".into(),
            path: "b.parquet".into(),
        },
        TableOutcome::Failed {
            table: "c".into(),
            error: "denied".into(),
        },
    ]);
    assert_eq!(
        (
            v["written"].as_u64(),
            v["skipped"].as_u64(),
            v["failed"].as_u64(),
            v["rows"].as_u64()
        ),
        (Some(1), Some(1), Some(1), Some(3))
    );
    assert_eq!(v["items"][2]["error"], "denied");
}
