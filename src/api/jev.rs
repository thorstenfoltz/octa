//! TypeSafe AI's System One API (the Jev model): the HTTP half of Classify
//! with Jev. The wire format lives in `data::transform::classify`; this file
//! only gets a request body to the server and the answer back.
//!
//! One JSON POST with a Bearer key, written with `ureq` like every other REST
//! call here. No SDK.
//!
//! **Everything sent through here leaves the machine.** TypeSafe hosts in the
//! United States only and names no retention period, so the feature is off
//! until switched on, in Settings or, for a headless install, in
//! `settings.toml` or the environment (see [`JevConfig::resolve`]).

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::rest::read_body_capped;
use crate::ui::settings::AppSettings;
use crate::ui::settings::secrets;

pub const DEFAULT_BASE: &str = "https://api.typesafe.ai";

/// Where a wrong or inappropriate label is reported: TypeSafe serves the
/// model, so only they can act on what it answered. Opened by the Report AI
/// content dialog (a Microsoft Store requirement, see `ai_report.rs`).
pub const REPORT_URL: &str = "https://typesafe.ai/contact";
pub const DEFAULT_MODEL: &str = "jev-latest";
pub const DEFAULT_PARALLEL: usize = 4;
/// The most requests a run sends at once, whatever Settings says. Each is a
/// thread waiting on the network, so the ceiling is the server's rate limit
/// long before it is this machine.
pub const MAX_PARALLEL: usize = 256;

/// Key id in the keyring and the plaintext fallback map.
pub const KEY_ID: &str = "jev";
/// The variable TypeSafe's own SDKs read.
pub const KEY_ENV: &str = "TYPESAFE_API_KEY";
/// Switches the feature on without a Settings window (`1` or `true`).
pub const ENABLED_ENV: &str = "OCTA_JEV_ENABLED";
pub const MODEL_ENV: &str = "OCTA_JEV_MODEL";

/// The `[jev]` table of `settings.toml`. The key is not in here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JevSettings {
    /// Off until the user agrees to send data to TypeSafe.
    pub enabled: bool,
    pub model: String,
    /// Requests sent at once.
    pub parallel: usize,
}

impl Default for JevSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: DEFAULT_MODEL.to_string(),
            parallel: DEFAULT_PARALLEL,
        }
    }
}

fn env(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// What stands between the user and a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// The feature is switched off.
    Switch,
    /// There is no API key.
    Key,
}

impl Missing {
    /// The reason, for a greyed control.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Switch => "greyed.jev_off",
            Self::Key => "greyed.jev_no_key",
        }
    }
}

/// Is the feature switched on, by Settings or the environment?
pub fn enabled(settings: &AppSettings) -> bool {
    settings.jev.enabled
        || env(ENABLED_ENV).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// The API key: `TYPESAFE_API_KEY`, then the keyring, then `settings.toml`.
pub fn api_key(settings: &AppSettings) -> Option<String> {
    secrets::get_key_for(KEY_ID, Some(KEY_ENV), settings)
}

/// Everything a run needs, resolved once.
#[derive(Debug, Clone)]
pub struct JevConfig {
    pub base: String,
    pub key: String,
    pub model: String,
    pub parallel: usize,
}

impl JevConfig {
    /// `Err(Missing)` when the feature is off or has no key. Reads the keyring,
    /// so call it when a run starts, not every frame.
    pub fn check(settings: &AppSettings) -> std::result::Result<Self, Missing> {
        if !enabled(settings) {
            return Err(Missing::Switch);
        }
        let key = api_key(settings).ok_or(Missing::Key)?;
        Ok(Self {
            base: DEFAULT_BASE.to_string(),
            key,
            model: env(MODEL_ENV).unwrap_or_else(|| match settings.jev.model.trim() {
                "" => DEFAULT_MODEL.to_string(),
                m => m.to_string(),
            }),
            parallel: settings.jev.parallel.clamp(1, MAX_PARALLEL),
        })
    }

    /// [`check`](Self::check) for the CLI and the MCP server, where nobody has
    /// a Settings window: the error says what to set and where.
    pub fn resolve(settings: &AppSettings) -> Result<Self> {
        Self::check(settings).map_err(|m| match m {
            Missing::Switch => anyhow::anyhow!(
                "Classify with Jev is switched off. It sends cell data to TypeSafe AI, whose \
                 servers are in the United States only. To allow that, switch it on in Settings \
                 > Classify with Jev, or put `enabled = true` under `[jev]` in settings.toml, or set \
                 {ENABLED_ENV}=1."
            ),
            Missing::Key => anyhow::anyhow!(
                "no TypeSafe API key. Set the {KEY_ENV} environment variable, or enter the key \
                 in Settings > Classify with Jev."
            ),
        })
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            // The key goes to the host named here and nowhere a redirect points.
            .max_redirects(0)
            // One kept connection per worker. ureq keeps 3 per host by default,
            // so every other worker paid a new TLS handshake for each request.
            .max_idle_connections(self.parallel)
            .max_idle_connections_per_host(self.parallel)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into()
    }

    /// The closure `classify` sends each request through. Retries a rate
    /// limit (429) or an overload (529) with a growing pause, as the API asks.
    pub fn sender(&self) -> impl Fn(&Value) -> Result<Value> + Sync + use<> {
        let agent = self.agent();
        let url = format!("{}/v1/systemone", self.base.trim_end_matches('/'));
        let auth = format!("Bearer {}", self.key);
        move |body| {
            let mut attempt = 0u32;
            loop {
                let mut resp = agent
                    .post(&url)
                    .header("Authorization", &auth)
                    .send_json(body)
                    .with_context(|| format!("requesting {url}"))?;
                let status = resp.status().as_u16();
                if matches!(status, 429 | 529) && attempt < MAX_RETRIES {
                    let asked = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok());
                    std::thread::sleep(retry_pause(attempt, asked));
                    attempt += 1;
                    continue;
                }
                return decode(status, &read_body_capped(&mut resp)?);
            }
        }
    }

    /// The model names this key may use (`GET /v1/models`). Also the cheapest
    /// way to find out whether the key works.
    pub fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/models", self.base.trim_end_matches('/'));
        let mut resp = self
            .agent()
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.key))
            .call()
            .with_context(|| format!("requesting {url}"))?;
        let status = resp.status().as_u16();
        let body = decode(status, &read_body_capped(&mut resp)?)?;
        let names = model_names(&body);
        if names.is_empty() {
            anyhow::bail!("TypeSafe listed no models: {body}");
        }
        Ok(names)
    }
}

const MAX_RETRIES: u32 = 4;

/// How long to wait before retry number `attempt` (0-based): what the server
/// asked for, else 1, 2, 4, 8 seconds. Never more than half a minute.
fn retry_pause(attempt: u32, asked_secs: Option<u64>) -> Duration {
    Duration::from_secs(asked_secs.unwrap_or(1 << attempt.min(5)).min(30))
}

/// Turn a status and body into the JSON value, or an error carrying the
/// server's own reason (`detail`, a string or a list of validation problems).
fn decode(status: u16, body: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        if value.is_null() {
            anyhow::bail!(
                "TypeSafe did not return JSON. The first bytes were: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        return Ok(value);
    }
    let detail = match value.get("detail") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| match i.get("msg").and_then(Value::as_str) {
                Some(msg) => msg.to_string(),
                None => i.to_string(),
            })
            .collect::<Vec<_>>()
            .join("; "),
        Some(other) => other.to_string(),
        None => body.chars().take(400).collect(),
    };
    let hint = match status {
        401 => " (the API key is missing or wrong)",
        429 => " (rate limit; lower the parallel requests in Settings > Classify with Jev)",
        529 => " (TypeSafe is overloaded; try again later)",
        _ => "",
    };
    anyhow::bail!("TypeSafe returned HTTP {status}{hint}: {detail}")
}

/// Model names out of a `/v1/models` body. The docs show the entries (`name`,
/// `description`, `release_date`) but not the envelope, so a bare array and an
/// array under `models` or `data` are all read.
fn model_names(body: &Value) -> Vec<String> {
    let list = body
        .as_array()
        .or_else(|| body.get("models").and_then(Value::as_array))
        .or_else(|| body.get("data").and_then(Value::as_array));
    list.into_iter()
        .flatten()
        .filter_map(|m| {
            m.as_str()
                .or_else(|| m.get("name").and_then(Value::as_str))
                .or_else(|| m.get("id").and_then(Value::as_str))
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "jev_tests.rs"]
mod tests;
