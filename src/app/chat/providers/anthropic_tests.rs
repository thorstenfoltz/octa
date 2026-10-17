use super::*;

fn cfg_with_reasoning(reasoning: Option<&str>, max_tokens: Option<usize>) -> ProviderConfig {
    ProviderConfig {
        model: "claude-opus-4-8".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: Some(0.0),
        max_tokens,
        reasoning: reasoning.map(str::to_string),
        verbosity: None,
        pro_mode: false,
    }
}

#[test]
fn a_models_api_response_yields_the_models_ceiling() {
    // The number an "unlimited" profile ends up sending. 128k on the
    // current generation, and the request would 400 if we sent more.
    assert_eq!(
        parse_model_max_tokens(
            r#"{"id":"claude-opus-5","type":"model","max_tokens":128000,
                    "max_input_tokens":1000000}"#
        ),
        Some(128_000)
    );
}

#[test]
fn an_unusable_models_api_answer_falls_back() {
    // Null, zero, absent and unparseable all mean "the API did not say",
    // which must not become a max_tokens of 0 - that request would fail.
    for body in [
        r#"{"id":"m","max_tokens":null}"#,
        r#"{"id":"m","max_tokens":0}"#,
        r#"{"id":"m"}"#,
        "not json",
        r#"{"error":{"type":"not_found_error","message":"model not found"}}"#,
    ] {
        assert_eq!(parse_model_max_tokens(body), None, "for {body}");
    }
}

#[test]
fn a_profile_with_its_own_cap_is_left_alone() {
    // Only "unlimited" consults the Models API; a number the user typed is
    // theirs, and must never trigger a network call.
    let cfg = cfg_with_reasoning(None, Some(4096));
    let resolved = resolve_max_tokens(&cfg);
    assert!(matches!(resolved, Cow::Borrowed(_)));
    assert_eq!(resolved.max_tokens, Some(4096));
}

#[test]
fn an_effort_word_becomes_adaptive_thinking() {
    // Claude Opus 4.7 and later reject `thinking.budget_tokens` outright;
    // effort in output_config is the control they do accept. A word must
    // therefore produce output_config and NOT a thinking block, and must
    // leave temperature and max_tokens alone.
    for level in ["low", "medium", "high", "xhigh", "max"] {
        let body = build_body(
            &cfg_with_reasoning(Some(level), Some(4096)),
            "sys",
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(body["output_config"]["effort"], json!(level));
        assert!(body.get("thinking").is_none());
        assert_eq!(body["temperature"], json!(0.0));
        assert_eq!(body["max_tokens"], json!(4096));
    }
}

#[test]
fn an_unknown_effort_word_is_still_sent() {
    // The accepted set changes with every model; a level we have never
    // heard of must reach the API rather than being refused locally.
    let body = build_body(&cfg_with_reasoning(Some("ultra"), None), "sys", &[], &[]).unwrap();
    assert_eq!(body["output_config"]["effort"], json!("ultra"));
}

#[test]
fn a_budget_below_the_minimum_is_refused_locally() {
    // 1024 is the documented floor; 0 and negatives were already invalid.
    for bad in ["0", "-5", "512"] {
        let err = build_body(&cfg_with_reasoning(Some(bad), None), "sys", &[], &[]).unwrap_err();
        assert!(err.contains("1024"), "{bad}: {err}");
        // and it points at the control current models actually take
        assert!(err.contains("effort"), "{bad}: {err}");
    }
}

#[test]
fn no_reasoning_leaves_the_request_untouched() {
    let body = build_body(&cfg_with_reasoning(None, Some(4096)), "sys", &[], &[]).unwrap();
    assert!(body.get("thinking").is_none());
    assert!(body.get("output_config").is_none());
    assert_eq!(body["temperature"], json!(0.0));
    assert_eq!(body["max_tokens"], json!(4096));
}

#[test]
fn thinking_forces_temperature_one_and_room_above_the_budget() {
    // Anthropic rejects thinking with temperature != 1, and requires
    // max_tokens to exceed the budget. A profile with temperature 0 and a
    // budget larger than its cap must still produce a valid request.
    let body = build_body(
        &cfg_with_reasoning(Some("8000"), Some(4096)),
        "sys",
        &[],
        &[],
    )
    .unwrap();

    assert_eq!(body["thinking"]["type"], json!("enabled"));
    assert_eq!(body["thinking"]["budget_tokens"], json!(8000));
    assert_eq!(body["temperature"], json!(1.0));
    assert!(body["max_tokens"].as_u64().unwrap() > 8000);
}

#[test]
fn no_temperature_means_no_temperature_field() {
    // Claude Opus 4.7 and newer answer 400 when `temperature` is present at
    // all, so a profile with the field cleared must send a body without it -
    // including the thinking path, which otherwise pins it to 1.
    let mut cfg = cfg_with_reasoning(None, Some(4096));
    cfg.temperature = None;
    let body = build_body(&cfg, "sys", &[], &[]).unwrap();
    assert!(body.get("temperature").is_none());

    cfg.reasoning = Some("8000".into());
    let body = build_body(&cfg, "sys", &[], &[]).unwrap();
    assert!(body.get("temperature").is_none());
    assert_eq!(body["thinking"]["budget_tokens"], json!(8000));
}

#[test]
fn thinking_keeps_a_generous_max_tokens() {
    let body = build_body(
        &cfg_with_reasoning(Some("1024"), Some(32_000)),
        "sys",
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(body["max_tokens"], json!(32_000));
}

#[test]
fn blank_reasoning_is_thinking_off() {
    for blank in [None, Some(""), Some("   ")] {
        let body = build_body(&cfg_with_reasoning(blank, None), "sys", &[], &[]).unwrap();
        assert!(body.get("thinking").is_none());
        assert!(body.get("output_config").is_none());
    }
}

/// The cache marker sits on the system block, which in Anthropic's fixed
/// prefix order (tools, system, messages) also covers the tool payload -
/// the part that repeats unchanged on every request and dwarfs the rest.
#[test]
fn the_system_block_carries_the_cache_marker() {
    let tools = [ToolDef {
        name: "read_table".into(),
        description: "read it".into(),
        input_schema: json!({ "type": "object", "properties": {} }),
    }];
    let body = build_body(&cfg_with_reasoning(None, None), "sys", &[], &tools).unwrap();
    assert_eq!(body["system"][0]["text"], json!("sys"));
    assert_eq!(
        body["system"][0]["cache_control"]["type"],
        json!("ephemeral")
    );
    // The tools ride in front of it, uncached in their own right.
    assert_eq!(body["tools"][0]["name"], json!("read_table"));
    assert!(body["tools"][0].get("cache_control").is_none());
}

/// The second marker rides the last message, so a long conversation stops
/// re-reading its own history at full price. Only the last one gets it.
#[test]
fn the_last_message_carries_the_rolling_marker() {
    let msgs = [
        Message {
            role: Role::User,
            blocks: vec![ContentBlock::Text {
                text: "first".into(),
            }],
        },
        Message {
            role: Role::User,
            blocks: vec![ContentBlock::Text {
                text: "second".into(),
            }],
        },
    ];
    let body = build_body(&cfg_with_reasoning(None, None), "sys", &msgs, &[]).unwrap();
    assert!(
        body["messages"][0]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert_eq!(
        body["messages"][1]["content"][0]["cache_control"]["type"],
        json!("ephemeral")
    );
}

/// No system prompt means no system field at all, marker or not.
#[test]
fn an_empty_system_prompt_sends_no_system_field() {
    let body = build_body(&cfg_with_reasoning(None, None), "", &[], &[]).unwrap();
    assert!(body.get("system").is_none());
}
