//! AWS Signature Version 4 request signing, for the Athena connector.
//!
//! Athena is three HTTP calls per query (start, poll, fetch), so shelling out
//! to the `aws` CLI the way [`super::auth`] does for RDS tokens would spawn
//! three processes per query. Signing is a couple of HMACs, so it happens
//! here instead, on `hmac` + `sha2`, both already in the tree.
//!
//! Deliberately narrow: it signs the shape Athena sends, which is a POST to
//! `/` with a JSON body and no query string. Everything the general algorithm
//! allows but Athena never uses (URI normalisation, multi-value headers,
//! query-string signing, chunked payloads) is left out rather than written
//! untested.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// The credentials a signed request needs: from the environment, a profile,
/// or the Identity Center role [`super::auth`] already mints.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Set for temporary credentials (SSO, assumed roles), empty otherwise.
    pub session_token: String,
}

/// One request to sign. `headers` are the ones that must be covered by the
/// signature besides `host` and `x-amz-date`; Athena passes `content-type`
/// and `x-amz-target`.
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub host: &'a str,
    pub headers: &'a [(&'a str, String)],
    pub payload: &'a [u8],
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// The `YYYYMMDDTHHMMSSZ` stamp a signature is bound to.
pub fn amz_date_now() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// Sign `req` and return the headers to send: `Authorization`, `X-Amz-Date`,
/// and `X-Amz-Security-Token` when the credentials are temporary. The caller
/// sends these alongside the headers it passed in.
///
/// `amz_date` is a parameter rather than read from the clock so the signature
/// is reproducible in a test; [`amz_date_now`] is what callers pass.
pub fn sign(
    creds: &Credentials,
    region: &str,
    service: &str,
    req: &Request<'_>,
    amz_date: &str,
) -> Vec<(String, String)> {
    let date = &amz_date[..8];
    let scope = format!("{date}/{region}/{service}/aws4_request");

    // Canonical headers: host and x-amz-date always, plus the caller's, each
    // lower-cased and sorted by name. The security token is signed when it is
    // sent, which is what AWS expects for temporary credentials.
    let mut canon: Vec<(String, String)> = vec![
        ("host".to_string(), req.host.to_string()),
        ("x-amz-date".to_string(), amz_date.to_string()),
    ];
    for (name, value) in req.headers {
        canon.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    if !creds.session_token.is_empty() {
        canon.push((
            "x-amz-security-token".to_string(),
            creds.session_token.clone(),
        ));
    }
    canon.sort_by(|a, b| a.0.cmp(&b.0));
    let signed_headers = canon
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers: String = canon.iter().map(|(n, v)| format!("{n}:{v}\n")).collect();

    let payload_hash = sha256_hex(req.payload);
    let canonical_request = format!(
        "{}\n{}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        req.method, req.path
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac_sha256(format!("AWS4{}", creds.secret_access_key).as_bytes(), date);
    let k_region = hmac_sha256(&k_date, region);
    let k_service = hmac_sha256(&k_region, service);
    let k_signing = hmac_sha256(&k_service, "aws4_request");
    let signature = hex(&hmac_sha256(&k_signing, &string_to_sign));

    let mut out = vec![
        (
            "Authorization".to_string(),
            format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, \
                 Signature={signature}",
                creds.access_key_id
            ),
        ),
        ("X-Amz-Date".to_string(), amz_date.to_string()),
    ];
    if !creds.session_token.is_empty() {
        out.push((
            "X-Amz-Security-Token".to_string(),
            creds.session_token.clone(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_creds() -> Credentials {
        Credentials {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: String::new(),
        }
    }

    /// AWS publishes a signing test suite; `post-header-key-case` is the shape
    /// Athena sends (POST /, no query, one extra header). The expected
    /// signature here was produced independently by botocore against the same
    /// inputs, so this test fails if the algorithm drifts rather than merely
    /// staying self-consistent.
    #[test]
    fn matches_the_published_signature() {
        let headers = [("content-type", "application/x-amz-json-1.1".to_string())];
        let req = Request {
            method: "POST",
            path: "/",
            host: "example.amazonaws.com",
            headers: &headers,
            payload: b"{}",
        };
        let out = sign(
            &example_creds(),
            "us-east-1",
            "service",
            &req,
            "20150830T123600Z",
        );
        let auth = &out[0].1;
        assert!(
            auth.starts_with(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
                 SignedHeaders=content-type;host;x-amz-date, Signature="
            ),
            "{auth}"
        );
        assert!(
            auth.ends_with("64417f0d3b55d13dfbc567ed0a9705f2f3975886d43edd7d9683bbe8fdfcf85b"),
            "signature was {auth}"
        );
        assert_eq!(
            out[1],
            ("X-Amz-Date".to_string(), "20150830T123600Z".into())
        );
    }

    /// Temporary credentials must sign the session token, not merely send it:
    /// AWS rejects a signature whose SignedHeaders omit a header that is
    /// present.
    #[test]
    fn session_token_is_signed_and_sent() {
        let mut creds = example_creds();
        creds.session_token = "FQoGZXIvYXdzEXAMPLE".into();
        let req = Request {
            method: "POST",
            path: "/",
            host: "athena.eu-central-1.amazonaws.com",
            headers: &[],
            payload: b"{}",
        };
        let out = sign(&creds, "eu-central-1", "athena", &req, "20260830T120000Z");
        assert!(
            out[0]
                .1
                .contains("SignedHeaders=host;x-amz-date;x-amz-security-token")
        );
        assert!(
            out[0]
                .1
                .ends_with("df5a5267fe9ebe7601822e6b16d268fa30119fb203b3502d4e076d083e590c6a"),
            "signature was {}",
            out[0].1
        );
        assert_eq!(out[2].0, "X-Amz-Security-Token");
    }
}
