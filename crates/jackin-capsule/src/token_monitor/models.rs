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

    fn fetch_from_api(
        &mut self,
        provider: &str,
        env_key: &str,
        url: &str,
        build_req: impl FnOnce(ureq::Request, &str) -> ureq::Request,
        filter: impl Fn(&str) -> bool,
    ) {
        let api_key = std::env::var(env_key).unwrap_or_default();
        if api_key.is_empty() {
            return;
        }
        let req = ureq::get(url);
        let Ok(resp) = build_req(req, &api_key).call() else { return };
        let Ok(body) = resp.into_string() else { return };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) else { return };
        if let Some(arr) = val.get("data").and_then(|d| d.as_array()) {
            let new: Vec<ModelEntry> = arr
                .iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    if !filter(&id) {
                        return None;
                    }
                    let display = m
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    Some(ModelEntry {
                        provider: provider.into(),
                        model_id: id,
                        display_name: display,
                    })
                })
                .collect();
            if !new.is_empty() {
                self.entries.retain(|e| e.provider != provider);
                self.entries.extend(new);
            }
        }
    }

    fn fetch_anthropic(&mut self) {
        self.fetch_from_api(
            "claude",
            "ANTHROPIC_API_KEY",
            "https://api.anthropic.com/v1/models",
            |req, key| req.set("x-api-key", key).set("anthropic-version", "2023-06-01"),
            |_| true,
        );
    }

    fn fetch_openai(&mut self) {
        self.fetch_from_api(
            "codex",
            "OPENAI_API_KEY",
            "https://api.openai.com/v1/models",
            |req, key| req.set("Authorization", &format!("Bearer {key}")),
            // Only coding-relevant models.
            |id| {
                id.starts_with("gpt-4")
                    || id.starts_with("o1")
                    || id.starts_with("o3")
                    || id.starts_with("o4")
            },
        );
    }

    fn fetch_moonshot(&mut self) {
        self.fetch_from_api(
            "kimi",
            "KIMI_CODE_API_KEY",
            "https://api.moonshot.ai/v1/models",
            |req, key| req.set("Authorization", &format!("Bearer {key}")),
            |_| true,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_falls_back_to_embedded_list_on_error() {
        let catalog = ModelCatalog::new();
        let models = catalog.available_models("claude");
        assert!(!models.is_empty(), "should have embedded fallback for claude");
        assert!(models.iter().any(|m| m.model_id.contains("sonnet")));
    }

    #[test]
    fn model_catalog_uses_cached_result_within_ttl() {
        let mut catalog = ModelCatalog::new();
        catalog.entries.push(ModelEntry {
            provider: "claude".to_string(),
            model_id: "claude-test-model".to_string(),
            display_name: "Test Model".to_string(),
        });
        catalog.fetched_at = Some(Instant::now());
        assert!(!catalog.needs_refresh());
        let models = catalog.available_models("claude");
        assert!(models.iter().any(|m| m.model_id == "claude-test-model"));
    }

    #[test]
    fn model_catalog_parses_model_entries_correctly() {
        let mut catalog = ModelCatalog::new();
        catalog.entries.push(ModelEntry {
            provider: "claude".to_string(),
            model_id: "claude-opus-4-8-20251101".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
        });
        catalog.entries.push(ModelEntry {
            provider: "claude".to_string(),
            model_id: "claude-sonnet-4-6-20251101".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
        });
        let models = catalog.available_models("claude");
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.display_name == "Claude Opus 4.8"));
    }
}
