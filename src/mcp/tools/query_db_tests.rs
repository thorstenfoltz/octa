use super::*;

fn ctx_with(conn: octa::db::DbConnection, read_only: bool) -> ToolContext {
    ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], read_only, 0)
}

fn conn(allow_writes: bool) -> octa::db::DbConnection {
    octa::db::DbConnection {
        id: "db-1".into(),
        name: "wh".into(),
        engine: octa::db::DbEngine::Postgres,
        host: "localhost".into(),
        port: 5432,
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

#[test]
fn unknown_connection_lists_names() {
    let ctx = ctx_with(conn(false), false);
    let p = Params {
        connection: "nope".into(),
        sql: "SELECT 1".into(),
        limit: None,
        explain: false,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("nope") && err.contains("wh"), "{err}");
}

#[test]
fn mutation_refused_without_allow_writes() {
    let ctx = ctx_with(conn(false), false);
    let p = Params {
        connection: "wh".into(),
        sql: "DELETE FROM t".into(),
        limit: None,
        explain: false,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("write"), "{err}");
}

#[test]
fn read_only_server_refuses_mutation_even_when_conn_allows() {
    let ctx = ctx_with(conn(true), true);
    let p = Params {
        connection: "wh".into(),
        sql: "UPDATE t SET x = 1".into(),
        limit: None,
        explain: false,
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("disabled for this session"), "{err}");
}
