//! Tests for the SSH tunnel's pure parts. The handshake itself needs a real
//! bastion and is a manual check; what is testable here is the bookkeeping that
//! decides *which* tunnel a connection gets and what the database client is
//! told to dial.

use super::*;
use crate::db::{DbAuth, DbEngine};

fn conn_with(ssh: Option<SshTunnel>) -> DbConnection {
    DbConnection {
        id: "db-1".into(),
        name: "prod".into(),
        engine: DbEngine::Postgres,
        host: "db.internal".into(),
        port: 5432,
        database: "app".into(),
        username: "reader".into(),
        auth: DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        ssh,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tunnel_port: None,
    }
}

fn tunnel() -> SshTunnel {
    SshTunnel {
        host: "bastion.example".into(),
        port: 22,
        username: "jump".into(),
        auth: SshAuth::Agent,
        accept_new_host_key: false,
    }
}

#[test]
fn a_connection_without_a_bastion_opens_no_tunnel() {
    // The common case has to cost nothing and touch no network.
    assert_eq!(ensure(&conn_with(None), None).unwrap(), None);
}

#[test]
fn editing_the_bastion_gives_a_different_tunnel() {
    // Every field that changes where the bytes go must change the key,
    // otherwise a user who fixes a typo keeps talking to the old server.
    let base = conn_with(Some(tunnel()));
    let baseline = tunnel_key(&base, base.ssh.as_ref().unwrap());

    let mut other_host = tunnel();
    other_host.host = "bastion2.example".into();
    let mut c = conn_with(Some(other_host));
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);

    let mut other_port = tunnel();
    other_port.port = 2222;
    c = conn_with(Some(other_port));
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);

    let mut other_user = tunnel();
    other_user.username = "someone".into();
    c = conn_with(Some(other_user));
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);

    let mut other_auth = tunnel();
    other_auth.auth = SshAuth::PrivateKey {
        path: "/home/me/.ssh/id_ed25519".into(),
    };
    c = conn_with(Some(other_auth));
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);
}

#[test]
fn retargeting_the_database_gives_a_different_tunnel() {
    // A forward carries one destination. Pointing the connection at another
    // database through the same bastion must not reuse the old forward.
    let base = conn_with(Some(tunnel()));
    let baseline = tunnel_key(&base, base.ssh.as_ref().unwrap());

    let mut c = conn_with(Some(tunnel()));
    c.port = 5433;
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);

    let mut c = conn_with(Some(tunnel()));
    c.host = "replica.internal".into();
    assert_ne!(tunnel_key(&c, c.ssh.as_ref().unwrap()), baseline);
}

#[test]
fn the_same_settings_reuse_one_tunnel() {
    let a = conn_with(Some(tunnel()));
    let b = conn_with(Some(tunnel()));
    assert_eq!(
        tunnel_key(&a, a.ssh.as_ref().unwrap()),
        tunnel_key(&b, b.ssh.as_ref().unwrap())
    );
}

#[test]
fn a_bastion_without_a_host_is_refused_before_dialling() {
    let mut t = tunnel();
    t.host = "  ".into();
    let err = ensure(&conn_with(Some(t)), None).unwrap_err().to_string();
    assert!(err.contains("no host"), "{err}");
}

#[test]
fn without_a_tunnel_the_client_dials_the_database_itself() {
    let c = conn_with(None);
    assert_eq!(c.dial_target(), ("db.internal".to_string(), 5432));
    assert!(!c.is_tunnelled());
}

#[test]
fn with_a_tunnel_only_the_socket_moves_to_loopback() {
    // The point of keeping `host` intact: TLS verification, Entra token
    // audiences and error messages must all still name the database. Only the
    // address the socket is opened on changes.
    let mut c = conn_with(Some(tunnel()));
    c.tunnel_port = Some(54321);
    assert_eq!(c.dial_target(), ("127.0.0.1".to_string(), 54321));
    assert!(c.is_tunnelled());
    assert_eq!(c.host, "db.internal");
    assert_eq!(c.port, 5432);
}

#[test]
fn the_tunnel_port_never_reaches_settings_toml() {
    // It is a runtime detail of one connect, not something to persist: a port
    // written to disk would be stale on the next launch.
    let mut c = conn_with(Some(tunnel()));
    c.tunnel_port = Some(54321);
    let toml = toml::to_string(&c).unwrap();
    assert!(!toml.contains("tunnel_port"), "{toml}");
    assert!(toml.contains("bastion.example"), "{toml}");
}

#[test]
fn a_connection_saved_before_tunnels_existed_still_loads() {
    // Back-compat: the same guarantee `auth` and `allow_writes` already carry.
    let old = r#"
        id = "db-old"
        name = "legacy"
        engine = "Postgres"
        host = "db.internal"
        port = 5432
        database = "app"
        username = "reader"
    "#;
    let c: DbConnection = toml::from_str(old).unwrap();
    assert!(c.ssh.is_none());
    assert!(!c.is_tunnelled());
    assert_eq!(c.dial_target(), ("db.internal".to_string(), 5432));
}

#[test]
fn a_bastion_saved_without_a_port_defaults_to_22() {
    let t: SshTunnel = toml::from_str(
        r#"
        host = "bastion.example"
        username = "jump"
        "#,
    )
    .unwrap();
    assert_eq!(t.port, 22);
    assert_eq!(t.auth, SshAuth::Agent);
    assert!(!t.accept_new_host_key);
}
