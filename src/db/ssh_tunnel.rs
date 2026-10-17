//! Reaching a database through a jump host.
//!
//! Almost every managed Postgres inside a company sits behind a bastion, and
//! without this the answer is "open a tunnel in a terminal first". A connection
//! carrying an [`SshTunnel`] gets a local forward opened for it on demand:
//! Octa binds a loopback port, dials the bastion over SSH, and every byte the
//! database client writes is carried over a `direct-tcpip` channel to the real
//! server.
//!
//! **The connection keeps its real host and port.** Only the socket goes to
//! loopback ([`DbConnection::dial_target`]); `conn.host` stays the database's
//! own name so TLS verification, Entra token audiences and error messages all
//! keep naming the database rather than `127.0.0.1`. Connectors whose client
//! library can separate the two (Postgres via `hostaddr`, MySQL via a TLS
//! hostname override, SQL Server because we hand it the socket) therefore
//! validate certificates exactly as they would without a tunnel.
//!
//! Implemented with `russh` rather than by shelling out to `ssh -L`: a
//! self-contained binary is the point of Octa's packaging, and `ssh` cannot do
//! password authentication without a terminal, which a GUI has not got.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};

pub use crate::ssh::{SshAuth, SshTarget as SshTunnel};

use super::DbConnection;

/// Live tunnels for this process, keyed by everything that would make one
/// different. Editing the bastion settings therefore opens a fresh tunnel
/// rather than silently reusing the old one.
///
/// ponytail: tunnels live until the process exits; nothing counts users or
/// closes an idle one. A session opens a handful, each costing one loopback
/// socket and one task. Refcount them against the connection cache if someone
/// ever leaves Octa open for weeks against dozens of bastions.
fn tunnels() -> &'static Mutex<HashMap<String, u16>> {
    static T: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Identity of a tunnel: change any of this and a new one is opened.
fn tunnel_key(conn: &DbConnection, ssh: &SshTunnel) -> String {
    let auth = match &ssh.auth {
        SshAuth::Agent => "agent".to_string(),
        SshAuth::PrivateKey { path } => format!("key:{path}"),
        SshAuth::Password => "password".to_string(),
    };
    format!(
        "{}|{}@{}:{}|{}|{}:{}",
        conn.id, ssh.username, ssh.host, ssh.port, auth, conn.host, conn.port
    )
}

/// The loopback port `conn` is reachable on, opening the tunnel if needed.
/// `None` when the connection has no bastion configured, which is the common
/// case and costs nothing.
///
/// `ssh_secret` is the keyring entry for this connection's SSH credential: the
/// key passphrase for [`SshAuth::PrivateKey`], the password for
/// [`SshAuth::Password`], and unused for [`SshAuth::Agent`].
pub fn ensure(conn: &DbConnection, ssh_secret: Option<&str>) -> Result<Option<u16>> {
    // Already tunnelled: this is a copy `db::connect` handed to a connector,
    // which may later reconnect for a cancel. Reuse the forward it is holding
    // rather than looking one up, so that path needs no credential at all.
    if let Some(port) = conn.tunnel_port {
        return Ok(Some(port));
    }
    let Some(ssh) = conn.ssh.as_ref() else {
        return Ok(None);
    };
    if ssh.host.trim().is_empty() {
        bail!("the SSH tunnel for connection '{}' has no host", conn.name);
    }
    let key = tunnel_key(conn, ssh);
    if let Some(port) = tunnels().lock().unwrap().get(&key).copied() {
        return Ok(Some(port));
    }
    let port = open(conn, ssh, ssh_secret)?;
    tunnels().lock().unwrap().insert(key, port);
    Ok(Some(port))
}

/// `conn` ready to dial: the same connection with its tunnel open and
/// [`DbConnection::tunnel_port`] filled in, or an unchanged copy when it has no
/// bastion. The one call every path that opens a socket to a database goes
/// through, `db::connect` and the DuckDB ATTACH lanes alike.
pub fn with_tunnel(conn: &DbConnection, ssh_secret: Option<&str>) -> Result<DbConnection> {
    let tunnel_port = ensure(conn, ssh_secret)?;
    Ok(DbConnection {
        tunnel_port,
        ..conn.clone()
    })
}

/// Dial the bastion, authenticate, and start forwarding on a fresh loopback
/// port. Blocks on the SSH handshake, so it runs where the connectors already
/// block: on the shared DB runtime.
fn open(conn: &DbConnection, ssh: &SshTunnel, ssh_secret: Option<&str>) -> Result<u16> {
    let session = crate::ssh::connect(super::runtime(), ssh, ssh_secret)?;

    // Port 0 lets the OS pick a free port, so two tunnels never collide and
    // nothing has to be configured. Bound to loopback only: a forward reachable
    // from the network would hand the whole company a route into the database.
    let target_host = conn.host.trim().to_string();
    let target_port = conn.port;
    let listener = super::runtime()
        .block_on(tokio::net::TcpListener::bind(("127.0.0.1", 0)))
        .context("binding a local port for the SSH tunnel")?;
    let local_port = listener
        .local_addr()
        .context("reading the SSH tunnel's local port")?
        .port();

    // One authenticated session carries every forward; russh multiplexes the
    // channels. `Handle` is not `Clone`, and `channel_open_direct_tcpip` only
    // needs `&self`, so an `Arc` is all the sharing required.
    let session = Arc::new(session);
    super::runtime().spawn(async move {
        loop {
            let Ok((mut socket, peer)) = listener.accept().await else {
                break;
            };
            let session = Arc::clone(&session);
            let host = target_host.clone();
            tokio::spawn(async move {
                let channel = match session
                    .channel_open_direct_tcpip(
                        host,
                        u32::from(target_port),
                        peer.ip().to_string(),
                        u32::from(peer.port()),
                    )
                    .await
                {
                    Ok(c) => c,
                    // The database client sees a closed socket and reports its
                    // own connection error, which is the right layer for it.
                    Err(e) => {
                        tracing::warn!("ssh tunnel: opening a forward failed: {e}");
                        return;
                    }
                };
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
            });
        }
    });

    Ok(local_port)
}

#[cfg(test)]
#[path = "ssh_tunnel_tests.rs"]
mod tests;
