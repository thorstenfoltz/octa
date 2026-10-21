//! Transport encryption for the PostgreSQL-wire connectors (PostgreSQL,
//! Redshift) and MySQL: one SSL mode per connection in libpq's five words, an
//! optional CA file, and an optional address to dial when it is not Host.
//!
//! The default is libpq's `prefer`: encrypt when the server offers it and do
//! not check its certificate, which is what psql, pgJDBC, DBeaver, the `mysql`
//! client and DuckDB's own extensions do. Octa used to check the certificate
//! under `prefer` (issue #87). That refused every server with a private or
//! self-signed certificate (a Kubernetes operator's, Debian's snakeoil, RDS)
//! while a server that offered no encryption at all was let in unchecked, so
//! the check kept out honest servers and no impostor.

use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, RootCertStore, SignatureScheme};
use serde::{Deserialize, Serialize};

use super::{DbAuth, DbConnection, DbEngine};

/// How a connection encrypts, in libpq's words (`sslmode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    /// Never encrypt.
    Disable,
    /// Encrypt when the server offers it; the certificate is not checked.
    #[default]
    Prefer,
    /// Always encrypt; the certificate is not checked.
    Require,
    /// Always encrypt; the certificate must chain to a trusted CA, whatever
    /// name it carries.
    VerifyCa,
    /// Always encrypt; the certificate must chain to a trusted CA and name
    /// the host.
    VerifyFull,
}

impl TlsMode {
    pub const ALL: [TlsMode; 5] = [
        TlsMode::Disable,
        TlsMode::Prefer,
        TlsMode::Require,
        TlsMode::VerifyCa,
        TlsMode::VerifyFull,
    ];

    /// The modes `engine` can honour. mysql_async 0.37 cannot check a chain
    /// without the name: its switch for that looks for `NotValidForName` in
    /// the error text, which rustls 0.23 no longer prints. So MySQL has no
    /// verify_ca; Connect via makes verify_identity work where it would have
    /// been wanted.
    pub fn offered(engine: DbEngine) -> &'static [TlsMode] {
        match engine {
            DbEngine::MySql => &[
                TlsMode::Disable,
                TlsMode::Prefer,
                TlsMode::Require,
                TlsMode::VerifyFull,
            ],
            _ => &Self::ALL,
        }
    }

    /// The engine's own word: libpq's for PostgreSQL and Redshift, the
    /// `mysql` client's for MySQL, in lower case like libpq's (the `mysql`
    /// client takes either case).
    pub fn label(self, engine: DbEngine) -> &'static str {
        let mysql = engine == DbEngine::MySql;
        match self {
            TlsMode::Disable if mysql => "disabled",
            TlsMode::Prefer if mysql => "preferred",
            TlsMode::Require if mysql => "required",
            TlsMode::VerifyCa if mysql => "verify_ca",
            TlsMode::VerifyFull if mysql => "verify_identity",
            TlsMode::Disable => "disable",
            TlsMode::Prefer => "prefer",
            TlsMode::Require => "require",
            TlsMode::VerifyCa => "verify-ca",
            TlsMode::VerifyFull => "verify-full",
        }
    }

    /// Either engine's word, in any case, with `-` or `_`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "disable" | "disabled" => Some(TlsMode::Disable),
            "prefer" | "preferred" => Some(TlsMode::Prefer),
            "require" | "required" => Some(TlsMode::Require),
            "verify-ca" => Some(TlsMode::VerifyCa),
            "verify-full" | "verify-identity" => Some(TlsMode::VerifyFull),
            _ => None,
        }
    }

    /// Whether the server's certificate is checked at all.
    pub fn checks_certificate(self) -> bool {
        matches!(self, TlsMode::VerifyCa | TlsMode::VerifyFull)
    }

    /// i18n key under `[db]` for the plain-words description.
    pub fn i18n_key(self) -> &'static str {
        match self {
            TlsMode::Disable => "ssl_disable",
            TlsMode::Prefer => "ssl_prefer",
            TlsMode::Require => "ssl_require",
            TlsMode::VerifyCa => "ssl_verify_ca",
            TlsMode::VerifyFull => "ssl_verify_full",
        }
    }
}

/// A connection's encryption settings. Read by the PostgreSQL, Redshift and
/// MySQL connectors; Settings and the CLI offer them for those engines only.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DbTls {
    pub mode: TlsMode,
    /// PEM file whose certificates are trusted for this connection, on top
    /// of the system's. Read by verify-ca and verify-full only.
    pub ca_file: Option<String>,
    /// IP address to dial instead of Host, which then only names the
    /// certificate: `127.0.0.1` for a `kubectl port-forward` to a server whose
    /// certificate names its cluster service. libpq's `hostaddr`.
    pub connect_via: Option<String>,
}

impl DbTls {
    /// The CA file as a path on this computer, `~/` expanded, or `None` when
    /// the field is empty.
    pub fn ca_path(&self) -> Option<std::path::PathBuf> {
        self.ca_file
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(crate::ssh::expand_home)
    }
}

/// Whether `engine` reads [`DbTls`]: the PostgreSQL-wire engines and MySQL.
pub fn applies_to(engine: DbEngine) -> bool {
    matches!(
        engine,
        DbEngine::Postgres | DbEngine::Redshift | DbEngine::MySql
    )
}

/// The mode a connect uses: the saved one, except that a token sign-in (AWS,
/// Azure, Google) never travels unencrypted.
pub fn effective_mode(conn: &DbConnection) -> TlsMode {
    match conn.tls.mode {
        TlsMode::Disable | TlsMode::Prefer if conn.auth != DbAuth::Password => TlsMode::Require,
        mode => mode,
    }
}

/// tokio-postgres's SSL mode for `mode`, and the rustls config that does the
/// checking.
pub(crate) fn postgres_tls(
    tls: &DbTls,
    mode: TlsMode,
) -> Result<(tokio_postgres::config::SslMode, rustls::ClientConfig)> {
    use tokio_postgres::config::SslMode;
    let ssl_mode = match mode {
        TlsMode::Disable => SslMode::Disable,
        TlsMode::Prefer => SslMode::Prefer,
        // libpq: the verify modes require TLS, so a server that answers "no
        // encryption" is refused rather than read in the clear.
        TlsMode::Require | TlsMode::VerifyCa | TlsMode::VerifyFull => SslMode::Require,
    };
    Ok((ssl_mode, client_config(tls, mode)?))
}

/// Named explicitly because rustls 0.23 refuses to pick a provider when
/// several are compiled in, and object_store pulls aws-lc-rs alongside our
/// ring.
fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Install ring as the process-wide rustls provider, for the drivers that
/// build their own config with `ClientConfig::builder()` (mysql_async,
/// sqlx-exasol). That call panics while two providers are compiled in and
/// none is installed. Idempotent.
pub(crate) fn install_default_provider() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// The platform's trusted roots, loaded once: reading the native store is
/// slow, and this runs on every connect and every PostgreSQL cancel.
fn native_roots() -> &'static RootCertStore {
    static ROOTS: OnceLock<RootCertStore> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let mut roots = RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        roots
    })
}

/// The rustls config for `mode`: no certificate check, chain only, or chain
/// and name, trusting the system's roots plus `tls.ca_file`.
pub(crate) fn client_config(tls: &DbTls, mode: TlsMode) -> Result<rustls::ClientConfig> {
    let provider = provider();
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default TLS versions");
    if !mode.checks_certificate() {
        return Ok(builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCheck(provider)))
            .with_no_client_auth());
    }
    let mut roots = native_roots().clone();
    if let Some(file) = tls.ca_path() {
        let path = file.display();
        let mut added = 0;
        for cert in CertificateDer::pem_file_iter(&file)
            .with_context(|| format!("reading the CA file {path}"))?
        {
            let cert = cert.with_context(|| format!("reading the CA file {path}"))?;
            roots
                .add(cert)
                .with_context(|| format!("a certificate in the CA file {path}"))?;
            added += 1;
        }
        if added == 0 {
            anyhow::bail!("the CA file {path} holds no PEM certificate");
        }
    }
    let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
        .build()
        .context("building the certificate check")?;
    Ok(match mode {
        TlsMode::VerifyCa => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(ChainOnly(verifier)))
            .with_no_client_auth(),
        _ => builder.with_webpki_verifier(verifier).with_no_client_auth(),
    })
}

/// `prefer` and `require`: any certificate is accepted. The handshake
/// signatures are still checked, so the channel is encrypted to whoever holds
/// the certificate's key; only who that is goes unchecked.
#[derive(Debug)]
struct NoCheck(Arc<CryptoProvider>);

impl ServerCertVerifier for NoCheck {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// `verify-ca`: the chain is checked exactly as `verify-full` checks it, the
/// name is not. rustls checks the chain first and the name last, so a name
/// error means the chain passed.
#[derive(Debug)]
struct ChainOnly(Arc<WebPkiServerVerifier>);

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self
            .0
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
        {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

/// mysql_async's TLS options for `mode`, or `None` for no TLS. mysql_async
/// trusts its built-in Mozilla roots (not the system store) plus the CA file.
/// A saved verify-ca (not offered for MySQL, see [`TlsMode::offered`]) checks
/// the name too: stricter, never weaker.
pub(crate) fn mysql_ssl_opts(conn: &DbConnection, mode: TlsMode) -> Option<mysql_async::SslOpts> {
    if mode == TlsMode::Disable {
        return None;
    }
    let mut ssl = mysql_async::SslOpts::default();
    if !mode.checks_certificate() {
        // Nothing is checked, so the CA file is not read either: mysql_async
        // would load it anyway and fail on a stale path, which PostgreSQL in
        // the same mode ignores.
        ssl = ssl.with_danger_accept_invalid_certs(true);
    } else if let Some(ca) = conn.tls.ca_path() {
        ssl = ssl.with_root_certs(vec![ca.into()]);
    }
    // Through a jump host or Connect via the socket goes elsewhere, but the
    // certificate still belongs to Host. The `danger_` name is about
    // overriding the name in general; here it restores the right one.
    if conn.dial_target().0 != conn.host {
        ssl = ssl.with_danger_tls_hostname_override(Some(conn.host.clone()));
    }
    Some(ssl)
}

/// Point a refused certificate at the setting that answers it. Without this
/// the error names the certificate problem and nothing a user can change.
pub(crate) fn certificate_hint(e: anyhow::Error) -> anyhow::Error {
    if format!("{e:#}").contains("invalid peer certificate") {
        e.context(
            "the server's certificate was not accepted: choose another SSL mode, add the \
             server's CA file, or, when the name does not match, put a name from the \
             certificate in Host and the address you reach it on in Connect via \
             (Settings -> Databases; sslmode=, sslrootcert= and hostaddr= in \
             --add-connection)",
        )
    } else {
        e
    }
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
