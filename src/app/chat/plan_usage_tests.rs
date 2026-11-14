use super::*;
use chrono::TimeZone;

// As Claude Code 2.1.322 printed it on 2026-11-11.
const CLAUDE_EVENT: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1794891600,"rateLimitType":"seven_day","utilization":0.59,"unifiedWindows":{"five_hour":{"utilization":0.47,"resetsAt":1794439200,"status":"allowed"},"seven_day":{"utilization":0.59,"resetsAt":1794891600,"status":"allowed"}},"overageEnabled":false},"uuid":"u","session_id":"s"}"#;

// As codex-cli 0.167.1 answered `account/rateLimits/read` on a free plan.
const CODEX_FREE: &str = r#"{"ordinaryUsageAllowed":true,"rateLimits":{"limitId":"codex","primary":{"usedPercent":2,"windowDurationMins":43200,"resetsAt":1796986851},"secondary":null,"credits":{"hasCredits":false,"unlimited":false,"balance":null},"planType":"free","rateLimitReachedType":null}}"#;

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn claude_reports_the_session_and_the_week() {
    let windows = parse_claude_limits(&json(CLAUDE_EVENT));
    assert_eq!(
        windows,
        [
            UsageWindow {
                minutes: 300,
                used_percent: 47.0,
                resets_at: Some(1794439200)
            },
            UsageWindow {
                minutes: 10_080,
                used_percent: 59.0,
                resets_at: Some(1794891600)
            },
        ]
    );
    // Any other line carries no limits.
    assert!(parse_claude_limits(&json(r#"{"type":"result","usage":{}}"#)).is_empty());
}

#[test]
fn claude_names_its_plan_in_auth_status() {
    let status = r#"{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "pro"}"#;
    assert_eq!(parse_claude_status(status).as_deref(), Some("Pro"));
    assert_eq!(parse_claude_status(r#"{"loggedIn": false}"#), None);
    assert_eq!(parse_claude_status("Not logged in"), None);
}

#[test]
fn codex_reports_whatever_windows_the_plan_has() {
    let (plan, windows) = parse_codex(&json(CODEX_FREE));
    assert_eq!(plan.as_deref(), Some("Free"));
    assert_eq!(
        windows,
        [UsageWindow {
            minutes: 43_200,
            used_percent: 2.0,
            resets_at: Some(1796986851)
        }]
    );

    // A paid plan: a session and a week, shortest first whatever the order.
    let paid = r#"{"rateLimits":{"primary":{"usedPercent":61.5,"windowDurationMins":10080,"resetsAt":20},"secondary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":10},"planType":"self_serve_business_prolite"}}"#;
    let (plan, windows) = parse_codex(&json(paid));
    assert_eq!(plan.as_deref(), Some("Self serve business prolite"));
    assert_eq!(
        windows.iter().map(|w| w.minutes).collect::<Vec<_>>(),
        [300, 10_080]
    );

    assert_eq!(parse_codex(&json("{}")), (None, Vec::new()));
}

#[test]
fn the_line_names_plan_windows_and_when_each_resets() {
    let now = chrono::Local
        .with_ymd_and_hms(2026, 11, 11, 10, 0, 0)
        .unwrap();
    let at = |h: i64| Some((now + chrono::Duration::hours(h)).timestamp());
    let usage = PlanUsage {
        plan: Some("Pro".into()),
        windows: vec![
            UsageWindow {
                minutes: 300,
                used_percent: 47.4,
                resets_at: at(4),
            },
            UsageWindow {
                minutes: 10_080,
                used_percent: 59.0,
                resets_at: at(100),
            },
            UsageWindow {
                minutes: 43_200,
                used_percent: 2.0,
                resets_at: None,
            },
        ],
        ..Default::default()
    };
    let line = line(&usage, now).unwrap();
    assert_eq!(
        line,
        "Pro   Session 47% (resets 14:00)   Week 59% (resets 2026-11-15 14:00)   30 days 2%"
    );
}

#[test]
fn nothing_known_is_no_line_and_a_plan_alone_is_just_the_plan() {
    let now = chrono::Local::now();
    assert_eq!(line(&PlanUsage::default(), now), None);
    let plan_only = PlanUsage {
        plan: Some("Pro".into()),
        ..Default::default()
    };
    assert_eq!(line(&plan_only, now).as_deref(), Some("Pro"));
}

#[test]
fn only_the_programs_that_report_are_ever_asked() {
    use ChatProviderKind as K;
    assert!(reported_by(K::ClaudeCli) && reported_by(K::CodexCli));
    assert!(!reported_by(K::CopilotCli) && !reported_by(K::Anthropic));
    // Asking a kind that reports nothing starts nothing and stores nothing.
    ensure(K::CopilotCli, "octa-no-such-program-xyz");
    assert_eq!(get(K::CopilotCli), PlanUsage::default());
}

/// Asks the real programs (no model call, nothing is spent):
/// `OCTA_TEST_PLAN_USAGE=1 cargo test --bin octa plan_usage -- --ignored`.
#[test]
#[ignore = "needs signed-in Claude Code and Codex CLIs"]
fn the_real_programs_name_a_plan() {
    if std::env::var("OCTA_TEST_PLAN_USAGE").is_err() {
        return;
    }
    assert!(claude_plan("claude").is_some(), "claude auth status");
    let (plan, windows) = codex_limits("codex").expect("codex app-server answered");
    assert!(plan.is_some());
    assert!(!windows.is_empty());
}

/// The whole way for Claude, as the panel does it on first show: nothing but
/// `ensure`, and the line holds plan and limits without the user asking
/// anything.
/// `OCTA_TEST_CLAUDE_CLI=1 cargo test --bin octa plan_usage -- --ignored`.
#[test]
#[ignore = "needs a signed-in Claude Code CLI and spends plan allowance"]
fn showing_a_claude_profile_fills_the_whole_line() {
    if std::env::var("OCTA_TEST_CLAUDE_CLI").is_err() {
        return;
    }
    let kind = ChatProviderKind::ClaudeCli;
    ensure(kind, "claude");
    let started = std::time::Instant::now();
    while get(kind).pending && started.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let usage = get(kind);
    let line = line(&usage, chrono::Local::now());
    assert!(usage.plan.is_some(), "{usage:?}");
    assert_eq!(usage.windows.len(), 2, "{usage:?}");
    println!("LINE: {line:?}");
}
