//! Model catalog: available models per provider.
//!
//! Queries each provider's model listing API and caches the result.
//! Exposes available models for the console agent picker.

/// A single model entry in the catalog.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub provider: String,
    pub model_id: String,
    pub display_name: String,
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
