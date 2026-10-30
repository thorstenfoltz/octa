use super::*;

#[test]
fn cells_come_out_of_varcharvalue() {
    assert_eq!(athena_cell(&json!({}), "Int64"), CellValue::Null);
    assert_eq!(
        athena_cell(&json!({"VarCharValue": "42"}), "Int64"),
        CellValue::Int(42)
    );
    assert_eq!(
        athena_cell(&json!({"VarCharValue": "1.5"}), "Float64"),
        CellValue::Float(1.5)
    );
    assert_eq!(
        athena_cell(&json!({"VarCharValue": "true"}), "Boolean"),
        CellValue::Bool(true)
    );
}

/// Athena repeats the column names as the first row of a SELECT result,
/// and counting that as data would put a header row in every table.
#[test]
fn the_repeated_header_row_is_recognised() {
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
    ];
    let header = json!({"Data": [{"VarCharValue": "id"}, {"VarCharValue": "name"}]});
    let data = json!({"Data": [{"VarCharValue": "1"}, {"VarCharValue": "ada"}]});
    assert!(is_header_row(&header, &columns));
    assert!(!is_header_row(&data, &columns));
}

#[test]
fn region_comes_from_auth_then_host() {
    let mut c = DbConnection {
        id: "x".into(),
        name: "a".into(),
        engine: DbEngine::Athena,
        host: "athena.eu-central-1.amazonaws.com".into(),
        port: 443,
        database: "default".into(),
        username: String::new(),
        auth: DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: super::super::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    };
    assert_eq!(region_of(&c).as_deref(), Some("eu-central-1"));
    c.auth = DbAuth::AwsIam {
        region: Some("us-east-1".into()),
        sso_start_url: None,
        sso_region: None,
        sso_account_id: None,
        sso_role: None,
    };
    assert_eq!(region_of(&c).as_deref(), Some("us-east-1"));
}

#[test]
fn time_cells_read_typed() {
    let v = serde_json::json!({"VarCharValue": "14:30:00.250"});
    assert_eq!(
        athena_cell(&v, "Time64(µs)"),
        CellValue::Time(52_200_250_000_000)
    );
}
