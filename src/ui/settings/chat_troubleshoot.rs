//! Turn a failed chat connection test into one plain-language sentence about
//! what to fix.
//!
//! The provider's own error is always shown verbatim, but "HTTP 404: No
//! endpoints found for deepseek/deepseek-v4-pro" does not tell a user which of
//! the four fields on the form is wrong. This maps the handful of failures that
//! actually happen onto the field that causes them.
//!
//! Deliberately a substring match rather than per-provider error parsing: the
//! five backends word their errors differently and keep changing them, and a
//! hint that is occasionally too general costs nothing next to the real message.
//! Returns an i18n key so the hint translates like the rest of the dialog.

use super::ChatProviderKind;

/// The hint for a failed test, as an i18n key. `None` when nothing in the
/// message is recognisable (the raw error is shown on its own).
pub fn hint_for(kind: ChatProviderKind, error: &str) -> Option<&'static str> {
    let e = error.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| e.contains(n));

    // Parameter rejections first: they carry an HTTP 400 that the generic
    // "check the model name" branch would otherwise claim.
    if has(&["temperature"]) {
        return Some("chat.tr_temperature");
    }
    if has(&["reasoning", "thinking", "budget_tokens", "effort"]) {
        return Some("chat.tr_reasoning");
    }
    if has(&[
        "401",
        "403",
        "unauthorized",
        "authentication",
        "api key",
        "api-key",
        "invalid key",
    ]) {
        return Some("chat.tr_auth");
    }
    if has(&[
        "429",
        "rate limit",
        "quota",
        "insufficient",
        "credit",
        "billing",
        "payment",
    ]) {
        return Some("chat.tr_quota");
    }
    // Anything that smells like "we never reached the server".
    if has(&[
        "request failed",
        "connection refused",
        "connection reset",
        "timed out",
        "timeout",
        "dns",
        "no such host",
        "failed to lookup",
        "certificate",
    ]) {
        return Some(match kind {
            ChatProviderKind::Ollama => "chat.tr_ollama_down",
            _ => "chat.tr_unreachable",
        });
    }
    if has(&[
        "404",
        "not found",
        "no endpoints",
        "model_not_found",
        "does not exist",
        "unknown model",
        "invalid model",
        "no allowed providers",
    ]) {
        return Some("chat.tr_model");
    }
    // A gateway that is not actually OpenAI-shaped fails in too many ways to
    // pattern-match; point at the two fields that are wrong most often.
    match kind {
        ChatProviderKind::OpenAiCompatible => Some("chat.tr_compat"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
}
