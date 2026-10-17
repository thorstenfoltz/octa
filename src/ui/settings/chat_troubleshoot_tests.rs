use super::*;

#[test]
fn the_failures_that_actually_happen_map_to_a_field() {
    use ChatProviderKind::*;
    let cases = [
        // The report that started this: Opus 4.8 refusing the parameter.
        (
            Anthropic,
            "HTTP 400: temperature is deprecated for this model",
            Some("chat.tr_temperature"),
        ),
        (
            OpenAi,
            "HTTP 400: Unsupported parameter: 'reasoning.effort'",
            Some("chat.tr_reasoning"),
        ),
        (
            Anthropic,
            "HTTP 401: invalid x-api-key",
            Some("chat.tr_auth"),
        ),
        (
            OpenAiCompatible,
            "HTTP 402: Insufficient credits",
            Some("chat.tr_quota"),
        ),
        (
            OpenAiCompatible,
            "HTTP 404: No endpoints found for qwen/qwen9",
            Some("chat.tr_model"),
        ),
        (
            Ollama,
            "request failed: connection refused",
            Some("chat.tr_ollama_down"),
        ),
        (
            Gemini,
            "request failed: dns error",
            Some("chat.tr_unreachable"),
        ),
        // Unrecognised: a compatible gateway still gets the generic hint,
        // a first-party provider gets none rather than a wrong guess.
        (
            OpenAiCompatible,
            "HTTP 500: upstream boom",
            Some("chat.tr_compat"),
        ),
        (Anthropic, "HTTP 500: overloaded", None),
    ];
    for (kind, err, want) in cases {
        assert_eq!(hint_for(kind, err), want, "{err}");
    }
}
