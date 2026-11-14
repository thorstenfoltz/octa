use super::*;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

/// A loopback HTTP stub serving `replies` (status line tail, extra headers,
/// body) in order. Returns what it saw: per request the request line, the
/// Authorization header and the body.
fn stub(
    replies: Vec<(&'static str, &'static str, String)>,
) -> (String, std::thread::JoinHandle<Vec<[String; 3]>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, headers, body) in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            let (mut auth, mut len) = (String::new(), 0usize);
            loop {
                let mut l = String::new();
                if reader.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" {
                    break;
                }
                let lower = l.to_ascii_lowercase();
                if lower.starts_with("authorization:") {
                    auth = l.trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut sent = vec![0u8; len];
            let _ = reader.read_exact(&mut sent);
            seen.push([
                first.trim().to_string(),
                auth,
                String::from_utf8_lossy(&sent).into_owned(),
            ]);
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
        seen
    });
    (base, handle)
}

fn config(base: String) -> JevConfig {
    JevConfig {
        base,
        key: "sk-test".into(),
        model: DEFAULT_MODEL.into(),
        parallel: 1,
    }
}

#[test]
fn a_request_posts_the_body_with_the_bearer_key() {
    let answer = json!({"answers": {"q": {"noul": 0.9}}});
    let (base, seen) = stub(vec![("200 OK", "", answer.to_string())]);
    let body = json!({"model": "jev-latest", "state": "hi", "questions": {}});

    let got = config(base).sender()(&body).unwrap();

    assert_eq!(got, answer);
    let seen = seen.join().unwrap();
    assert_eq!(seen[0][0], "POST /v1/systemone HTTP/1.1");
    assert_eq!(
        seen[0][1].to_ascii_lowercase(),
        "authorization: bearer sk-test"
    );
    assert_eq!(serde_json::from_str::<Value>(&seen[0][2]).unwrap(), body);
}

#[test]
fn a_rate_limit_is_retried_after_the_pause_the_server_names() {
    let (base, seen) = stub(vec![
        (
            "429 Too Many Requests",
            "Retry-After: 0\r\n",
            r#"{"detail":"slow down"}"#.to_string(),
        ),
        ("200 OK", "", r#"{"answers":{}}"#.to_string()),
    ]);
    let got = config(base).sender()(&json!({})).unwrap();
    assert_eq!(got, json!({"answers": {}}));
    assert_eq!(seen.join().unwrap().len(), 2);
}

#[test]
fn retry_pauses_grow_and_are_capped() {
    assert_eq!(retry_pause(0, None), Duration::from_secs(1));
    assert_eq!(retry_pause(3, None), Duration::from_secs(8));
    assert_eq!(retry_pause(0, Some(5)), Duration::from_secs(5));
    assert_eq!(retry_pause(0, Some(3600)), Duration::from_secs(30));
}

#[test]
fn an_error_carries_the_servers_reason() {
    let e = decode(401, r#"{"detail":"Invalid API key"}"#).unwrap_err();
    let text = format!("{e:#}");
    assert!(text.contains("HTTP 401"), "{text}");
    assert!(text.contains("Invalid API key"), "{text}");

    // A validation failure lists its problems.
    let e = decode(
        422,
        r#"{"detail":[{"loc":["body","questions"],"msg":"Field required","type":"missing"}]}"#,
    )
    .unwrap_err();
    assert!(format!("{e:#}").contains("Field required"));

    // Not JSON at all: the first bytes are shown.
    let e = decode(502, "<html>Bad Gateway</html>").unwrap_err();
    assert!(format!("{e:#}").contains("Bad Gateway"));
    assert!(decode(200, "not json").is_err());
}

#[test]
fn models_are_listed_whatever_the_envelope() {
    let want = vec!["jev-latest".to_string(), "jev-preview".to_string()];
    let entries = json!([{"name": "jev-latest"}, {"name": "jev-preview"}]);
    assert_eq!(model_names(&entries), want);
    assert_eq!(model_names(&json!({"models": entries})), want);
    assert_eq!(model_names(&json!({"data": entries})), want);
    assert!(model_names(&json!({"detail": "no"})).is_empty());

    let (base, seen) = stub(vec![("200 OK", "", entries.to_string())]);
    assert_eq!(config(base).list_models().unwrap(), want);
    assert_eq!(seen.join().unwrap()[0][0], "GET /v1/models HTTP/1.1");
}

#[test]
fn off_by_default_and_the_switch_comes_before_the_key() {
    let settings = AppSettings::default();
    assert!(!settings.jev.enabled);
    // Unless this machine's environment switches it on, the default refuses,
    // and the refusal is about the switch whether or not a key exists.
    if env(ENABLED_ENV).is_none() {
        assert_eq!(JevConfig::check(&settings).unwrap_err(), Missing::Switch);
        let text = JevConfig::resolve(&settings).unwrap_err().to_string();
        assert!(text.contains("United States"), "{text}");
        assert!(text.contains(ENABLED_ENV), "{text}");
    }
}

#[test]
fn settings_without_a_jev_table_still_load() {
    let s: JevSettings = toml::from_str("").unwrap();
    assert_eq!(s, JevSettings::default());
    let s: JevSettings = toml::from_str("enabled = true").unwrap();
    assert!(s.enabled);
    assert_eq!(s.model, DEFAULT_MODEL);
    assert_eq!(s.parallel, DEFAULT_PARALLEL);
}
