use super::*;

fn conn(name: &str, engine: octa::db::DbEngine, allow_writes: bool) -> octa::db::DbConnection {
    octa::db::DbConnection {
        id: format!("db-{name}"),
        name: name.into(),
        engine,
        host: "localhost".into(),
        port: engine.default_port(),
        database: "app".into(),
        username: "u".into(),
        auth: octa::db::DbAuth::Password,
        allow_writes,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    }
}

fn params() -> Params {
    Params {
        source_connection: "src".into(),
        source_schema: "app".into(),
        source_table: "t".into(),
        target_connection: "tgt".into(),
        target_schema: None,
        target_table: None,
        mode: None,
        source_catalog: None,
        target_catalog: None,
    }
}

#[test]
fn read_only_ctx_refuses_before_any_network() {
    let conns = vec![
        conn("src", octa::db::DbEngine::MySql, false),
        conn("tgt", octa::db::DbEngine::Postgres, true),
    ];
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, conns, true, 0);
    let err = run(&ctx, &params()).unwrap_err().to_string();
    assert!(err.contains("disabled for this session"), "{err}");
}

#[test]
fn readonly_target_connection_is_refused() {
    let conns = vec![
        conn("src", octa::db::DbEngine::MySql, false),
        conn("tgt", octa::db::DbEngine::Postgres, false),
    ];
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, conns, false, 0);
    let err = run(&ctx, &params()).unwrap_err().to_string();
    assert!(err.contains("Allow writes"), "{err}");
}
