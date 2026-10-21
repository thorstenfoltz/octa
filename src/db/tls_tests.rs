use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

use super::*;
use crate::db::postgres::PostgresConnector;

fn conn(engine: DbEngine, host: &str, port: u16, tls: DbTls) -> DbConnection {
    DbConnection {
        id: "db-tls".into(),
        name: "tls".into(),
        engine,
        host: host.into(),
        port,
        database: "app".into(),
        username: "octa".into(),
        auth: DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls,
        tunnel_port: None,
    }
}

/// A private CA and a server certificate it signed for the names a
/// CloudNativePG cluster's certificate carries: never `localhost`.
struct Pki {
    ca_pem: String,
    server: rustls::pki_types::CertificateDer<'static>,
    key: Vec<u8>,
}

fn pki() -> Pki {
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca.distinguished_name
        .push(rcgen::DnType::CommonName, "octa test cluster CA");
    let ca = ca.self_signed(&ca_key).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let server = rcgen::CertificateParams::new(vec!["db-rw".into(), "db-rw.test.svc".into()])
        .unwrap()
        .signed_by(&key, &ca, &ca_key)
        .unwrap();
    Pki {
        ca_pem: ca.pem(),
        server: server.der().clone(),
        key: key.serialize_der(),
    }
}

fn ca_file(pki: &Pki) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(pki.ca_pem.as_bytes()).unwrap();
    f
}

/// What the fake server saw from the client.
#[derive(Debug, PartialEq)]
enum Seen {
    /// No SSLRequest: the client spoke in the clear.
    Plain,
    /// The TLS handshake completed: the client accepted the certificate.
    Tls,
    /// The client broke the handshake off: it refused the certificate.
    Refused,
}

/// One connection of a PostgreSQL server that answers the SSLRequest with
/// `S` and completes the handshake, then hangs up. Enough to tell whether the
/// client encrypted and whether it accepted the certificate.
fn fake_postgres(pki: &Pki) -> (u16, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![pki.server.clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pki.key.clone())),
            )
            .unwrap(),
    );
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        sock.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut first = [0u8; 8];
        if sock.read_exact(&mut first).is_err() {
            return;
        }
        // SSLRequest: length 8, code 80877103.
        if first != [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f] {
            let _ = tx.send(Seen::Plain);
            return;
        }
        sock.write_all(b"S").unwrap();
        let mut tls = rustls::ServerConnection::new(config).unwrap();
        let seen = loop {
            match tls.complete_io(&mut sock) {
                Ok(_) if !tls.is_handshaking() => break Seen::Tls,
                Ok((0, 0)) | Err(_) => break Seen::Refused,
                Ok(_) => {}
            }
        };
        let _ = tx.send(seen);
    });
    (port, rx)
}

/// Connect to a fresh fake server with `tls` and return what it saw plus the
/// client's error text (the fake always hangs up, so there always is one).
fn attempt(pki: &Pki, host: &str, tls: DbTls) -> (Seen, String) {
    let (port, seen) = fake_postgres(pki);
    let c = conn(DbEngine::Postgres, host, port, tls);
    let err = PostgresConnector::connect(&c, Some("pw"))
        .err()
        .expect("the fake server never completes a login");
    let seen = seen
        .recv_timeout(Duration::from_secs(20))
        .expect("the fake server saw no connection");
    (seen, format!("{err:#}"))
}

fn mode(mode: TlsMode) -> DbTls {
    DbTls {
        mode,
        ..DbTls::default()
    }
}

fn with_ca(mode: TlsMode, ca: &tempfile::NamedTempFile) -> DbTls {
    DbTls {
        mode,
        ca_file: Some(ca.path().to_string_lossy().into_owned()),
        connect_via: None,
    }
}

#[test]
fn prefer_and_require_encrypt_without_checking_the_certificate() {
    // The issue: a private CA and a name that is not `localhost`. psql and
    // DBeaver connect; Octa refused.
    let pki = pki();
    assert_eq!(
        attempt(&pki, "localhost", mode(TlsMode::Prefer)).0,
        Seen::Tls
    );
    assert_eq!(
        attempt(&pki, "localhost", mode(TlsMode::Require)).0,
        Seen::Tls
    );
}

#[test]
fn disable_never_asks_for_tls() {
    let pki = pki();
    assert_eq!(
        attempt(&pki, "localhost", mode(TlsMode::Disable)).0,
        Seen::Plain
    );
}

#[test]
fn verify_full_refuses_an_unknown_ca_and_says_where_to_fix_it() {
    let pki = pki();
    let (seen, err) = attempt(&pki, "localhost", mode(TlsMode::VerifyFull));
    assert_eq!(seen, Seen::Refused);
    assert!(err.contains("invalid peer certificate"), "{err}");
    assert!(err.contains("Settings -> Databases"), "{err}");
}

#[test]
fn verify_full_with_the_ca_still_checks_the_name() {
    let pki = pki();
    let ca = ca_file(&pki);
    let (seen, err) = attempt(&pki, "localhost", with_ca(TlsMode::VerifyFull, &ca));
    assert_eq!(seen, Seen::Refused);
    assert!(err.contains("not valid for name"), "{err}");
}

#[test]
fn connect_via_lets_verify_full_pass_through_a_port_forward() {
    // Host is the name on the certificate, the socket goes to 127.0.0.1:
    // `kubectl port-forward` with the strictest mode. Also the SSH tunnel's
    // path (`hostaddr`), which failed with "invalid number of ports" while
    // the port was set twice.
    let pki = pki();
    let ca = ca_file(&pki);
    let tls = DbTls {
        connect_via: Some("127.0.0.1".into()),
        ..with_ca(TlsMode::VerifyFull, &ca)
    };
    assert_eq!(attempt(&pki, "db-rw.test.svc", tls).0, Seen::Tls);
}

#[test]
fn verify_ca_checks_the_chain_but_not_the_name() {
    let pki = pki();
    let ca = ca_file(&pki);
    assert_eq!(
        attempt(&pki, "localhost", with_ca(TlsMode::VerifyCa, &ca)).0,
        Seen::Tls
    );
    // Without the CA file the chain is unknown, so it refuses.
    assert_eq!(
        attempt(&pki, "localhost", mode(TlsMode::VerifyCa)).0,
        Seen::Refused
    );
}

#[test]
fn a_ca_file_without_certificates_is_an_error_before_connecting() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(b"not a certificate\n").unwrap();
    let tls = DbTls {
        mode: TlsMode::VerifyFull,
        ca_file: Some(f.path().to_string_lossy().into_owned()),
        connect_via: None,
    };
    let err = client_config(&tls, TlsMode::VerifyFull).unwrap_err();
    assert!(
        format!("{err:#}").contains("holds no PEM certificate"),
        "{err:#}"
    );
}

#[test]
fn modes_read_and_print_in_each_engines_words() {
    for m in TlsMode::ALL {
        assert_eq!(TlsMode::parse(m.label(DbEngine::Postgres)), Some(m));
        assert_eq!(TlsMode::parse(m.label(DbEngine::MySql)), Some(m));
    }
    assert_eq!(TlsMode::parse(" Verify_Full "), Some(TlsMode::VerifyFull));
    assert_eq!(TlsMode::parse("allow"), None);
    assert_eq!(
        TlsMode::VerifyFull.label(DbEngine::MySql),
        "verify_identity"
    );
    assert_eq!(TlsMode::VerifyFull.label(DbEngine::Redshift), "verify-full");
    assert!(!TlsMode::offered(DbEngine::MySql).contains(&TlsMode::VerifyCa));
    assert_eq!(TlsMode::offered(DbEngine::Postgres), &TlsMode::ALL);
}

#[test]
fn an_older_settings_file_gets_prefer() {
    // A connection saved before the field existed has no `tls` table.
    let tls: DbTls = toml::from_str("").unwrap();
    assert_eq!(tls, DbTls::default());
    assert_eq!(tls.mode, TlsMode::Prefer);
    let tls: DbTls = toml::from_str("mode = \"verify-full\"\nconnect_via = \"127.0.0.1\"").unwrap();
    assert_eq!(tls.mode, TlsMode::VerifyFull);
    assert_eq!(tls.connect_via.as_deref(), Some("127.0.0.1"));
}

#[test]
fn a_token_never_travels_unencrypted() {
    let mut c = conn(DbEngine::Postgres, "h", 5432, mode(TlsMode::Prefer));
    assert_eq!(effective_mode(&c), TlsMode::Prefer);
    c.auth = DbAuth::AzureAd;
    assert_eq!(effective_mode(&c), TlsMode::Require);
    c.tls.mode = TlsMode::Disable;
    assert_eq!(effective_mode(&c), TlsMode::Require);
    c.tls.mode = TlsMode::VerifyFull;
    assert_eq!(effective_mode(&c), TlsMode::VerifyFull);
}

#[test]
fn connect_via_changes_only_the_address_dialled() {
    let mut c = conn(
        DbEngine::Postgres,
        "db-rw.test.svc",
        55432,
        DbTls::default(),
    );
    assert_eq!(c.dial_target(), ("db-rw.test.svc".to_string(), 55432));
    c.tls.connect_via = Some("127.0.0.1".into());
    assert_eq!(c.dial_target(), ("127.0.0.1".to_string(), 55432));
    // A live SSH tunnel decides where the socket goes.
    c.tunnel_port = Some(40000);
    assert_eq!(c.dial_target(), ("127.0.0.1".to_string(), 40000));
}

#[test]
fn mysql_options_follow_the_mode() {
    let mut c = conn(
        DbEngine::MySql,
        "db.example.com",
        3306,
        mode(TlsMode::Disable),
    );
    assert!(mysql_ssl_opts(&c, TlsMode::Disable).is_none());

    let ssl = mysql_ssl_opts(&c, TlsMode::Prefer).unwrap();
    assert!(ssl.accept_invalid_certs());
    assert!(ssl.tls_hostname_override().is_none());

    let ssl = mysql_ssl_opts(&c, TlsMode::VerifyFull).unwrap();
    assert!(!ssl.accept_invalid_certs());
    assert!(!ssl.skip_domain_validation());

    c.tls.ca_file = Some("/etc/octa/ca.pem".into());
    c.tls.connect_via = Some("127.0.0.1".into());
    let ssl = mysql_ssl_opts(&c, TlsMode::VerifyFull).unwrap();
    assert_eq!(ssl.root_certs().len(), 1);
    // A mode that checks nothing does not read the file, so a stale path left
    // in the field cannot break the connection (PostgreSQL ignores it too).
    let unchecked = mysql_ssl_opts(&c, TlsMode::Require).unwrap();
    assert!(unchecked.root_certs().is_empty());
    // The socket goes to 127.0.0.1; the certificate is checked for Host.
    assert_eq!(ssl.tls_hostname_override(), Some("db.example.com"));
}

#[test]
fn only_a_refused_certificate_gets_the_hint() {
    let refused = anyhow::anyhow!("invalid peer certificate: UnknownIssuer").context("connecting");
    assert!(format!("{:#}", certificate_hint(refused)).contains("SSL mode"));
    let other = anyhow::anyhow!("password authentication failed").context("connecting");
    assert!(!format!("{:#}", certificate_hint(other)).contains("SSL mode"));
}

#[test]
fn the_ca_file_is_a_path_on_this_computer_with_home_expanded() {
    // psql users keep it at ~/.postgresql/root.crt; the drivers take paths
    // literally, so `~/` has to be expanded before them.
    let tls = DbTls {
        ca_file: Some(" ~/.postgresql/root.crt ".into()),
        ..DbTls::default()
    };
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .expect("a home directory");
    assert_eq!(
        tls.ca_path(),
        Some(std::path::PathBuf::from(home).join(".postgresql/root.crt"))
    );
    let empty = DbTls {
        ca_file: Some("  ".into()),
        ..DbTls::default()
    };
    assert_eq!(empty.ca_path(), None);
}
