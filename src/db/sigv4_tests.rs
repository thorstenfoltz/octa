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
