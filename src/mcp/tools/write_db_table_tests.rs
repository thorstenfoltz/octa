use super::*;

#[test]
fn write_refused_without_allow_writes() {
    let conn = octa::db::DbConnection {
        id: "db-1".into(),
        name: "wh".into(),
        engine: octa::db::DbEngine::Postgres,
        host: "localhost".into(),
        port: 5432,
        database: "app".into(),
        username: "u".into(),
        auth: octa::db::DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    };
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], false, 0);
    let p = Params {
        connection: "wh".into(),
        schema: "public".into(),
        table: "t".into(),
        mode: None,
        path: Some("whatever.csv".into()),
        open_tab: None,
        catalog: None,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("write"), "{err}");
}

#[test]
fn bad_mode_errors_before_any_read() {
    let conn = octa::db::DbConnection {
        id: "db-1".into(),
        name: "wh".into(),
        engine: octa::db::DbEngine::Postgres,
        host: "localhost".into(),
        port: 5432,
        database: "app".into(),
        username: "u".into(),
        auth: octa::db::DbAuth::Password,
        allow_writes: true,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    };
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], false, 0);
    let p = Params {
        connection: "wh".into(),
        schema: "public".into(),
        table: "t".into(),
        mode: Some("upsert".into()),
        path: Some("whatever.csv".into()),
        open_tab: None,
        catalog: None,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("upsert"), "{err}");
}
