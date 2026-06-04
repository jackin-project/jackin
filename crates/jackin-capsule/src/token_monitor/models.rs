//! Model catalog: available models per provider.
//!
//! Queries each provider's model listing API and caches the result.
//! Exposes available models for the console agent picker.

use std::time::{Duration, Instant};

/// A single model entry in the catalog.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub provider: String,
    pub model_id: String,
    pub display_name: String,
}

pub struct ModelCatalog {
    entries: Vec<ModelEntry>,
    fetched_at: Option<Instant>,
    ttl: Duration,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            fetched_at: None,
            ttl: Duration::from_secs(24 * 3600),
        }
    }

    /// Return cached entries or embedded fallback if stale/empty.
    pub fn available_models(&self, provider: &str) -> Vec<ModelEntry> {
        let live: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.provider == provider)
            .cloned()
            .collect();
        if live.is_empty() {
            embedded_models(provider)
        } else {
            live
        }
    }

    /// Whether the catalog needs a refresh.
    pub fn needs_refresh(&self) -> bool {
        self.fetched_at
            .map(|t| t.elapsed() > self.ttl)
            .unwrap_or(true)
    }

    /// Fetch fresh model list from a provider's API.
    /// Returns without mutating entries on error (fallback stays active).
    pub fn populate(&mut self, provider: &str) {
        match provider {
            "claude" => self.fetch_anthropic(),
            "codex" => self.fetch_openai(),
            "kimi" => self.fetch_moonshot(),
            _ => {}
        }
        self.fetched_at = Some(Instant::now());
    }

    fn fetch_anthropic(&mut self) {
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            return;
        }
        let Ok(resp) = ureq::get("https://api.anthropic.com/v1/models")
            .set("x-api-key", &api_key)
            .set("anthropic-version", "2023-06-01")
            .call()
        else {
            return;
        };
        let Ok(body) = resp.into_string() else { return };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) else { return };
        if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
            let new: Vec<ModelEntry> = arr
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let display = m
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    Some(ModelEntry {
                        provider: "claude".into(),
                        model_id: id,
                        display_name: display,
                    })
                })
                .collect();
            if !new.is_empty() {
                self.entries.retain(|e| e.provider != "claude");
                self.entries.extend(new);
            }
        }
    }

    fn fetch_openai(&mut self) {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            return;
        }
        let Ok(resp) = ureq::get("https://api.openai.com/v1/models")
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        else {
            return;
        };
        let Ok(body) = resp.into_string() else { return };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) else { return };
        if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
            let new: Vec<ModelEntry> = arr
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    // Only coding-relevant models.
                    if !id.starts_with("gpt-4")
                        && !id.starts_with("o1")
                        && !id.starts_with("o3")
                        && !id.starts_with("o4")
                    {
                        return None;
                    }
                    Some(ModelEntry {
                        provider: "codex".into(),
                        model_id: id.clone(),
                        display_name: id,
                    })
                })
                .collect();
            if !new.is_empty() {
                self.entries.retain(|e| e.provider != "codex");
                self.entries.extend(new);
            }
        }
    }

    fn fetch_moonshot(&mut self) {
        let api_key = std::env::var("KIMI_CODE_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            return;
        }
        let Ok(resp) = ureq::get("https://api.moonshot.ai/v1/models")
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        else {
            return;
        };
        let Ok(body) = resp.into_string() else { return };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) else { return };
        if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
            let new: Vec<ModelEntry> = arr
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    Some(ModelEntry {
                        provider: "kimi".into(),
                        model_id: id.clone(),
                        display_name: id,
                    })
                })
                .collect();
            if !new.is_empty() {
                self.entries.retain(|e| e.provider != "kimi");
                self.entries.extend(new);
            }
        }
    }
}

/// Embedded minimal fallback list when the API is unreachable.
pub fn embedded_models(provider: &str) -> Vec<ModelEntry> {
    match provider {
        "claude" => vec![
            ModelEntry {
                provider: "claude".into(),
                model_id: "claude-opus-4-8-20251101".into(),
                display_name: "Claude Opus 4.8".into(),
            },
            ModelEntry {
                provider: "claude".into(),
                model_id: "claude-sonnet-4-6-20251101".into(),
                display_name: "Claude Sonnet 4.6".into(),
            },
            ModelEntry {
                provider: "claude".into(),
                model_id: "claude-haiku-4-5-20251001".into(),
                display_name: "Claude Haiku 4.5".into(),
            },
        ],
        "codex" => vec![ModelEntry {
            provider: "codex".into(),
            model_id: "codex-mini-latest".into(),
            display_name: "Codex Mini".into(),
        }],
        "kimi" => vec![
            ModelEntry {
                provider: "kimi".into(),
                model_id: "kimi-latest".into(),
                display_name: "Kimi Latest".into(),
            },
            ModelEntry {
                provider: "kimi".into(),
                model_id: "kimi-k2-0711-preview".into(),
                display_name: "Kimi K2".into(),
            },
        ],
        _ => vec![],
    }
}
