//! Fuzz pure env resolution + reserved-name validation — never panic.
#![no_main]
use jackin_config::AppConfig;
use jackin_core::env_model::is_reserved;
use jackin_core::manifest::EnvVarDecl;
use jackin_env::{EnvPrompter, PromptResult, resolve_env, resolve_env_with_overrides, validate_reserved_names};
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;

struct NoPrompt;
impl EnvPrompter for NoPrompt {
    fn prompt_text(
        &self,
        _title: &str,
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        Ok(PromptResult::Skipped)
    }
    fn prompt_select(
        &self,
        _title: &str,
        _options: &[String],
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        Ok(PromptResult::Skipped)
    }
}

fn keys_from_bytes(data: &[u8]) -> Vec<String> {
    // Split on 0x00 into up to 8 keys of printable-ish bytes.
    data.split(|&b| b == 0)
        .take(8)
        .map(|chunk| {
            let s: String = chunk
                .iter()
                .map(|b| {
                    let c = (b % 64) + 32;
                    char::from(c.min(126))
                })
                .collect();
            if s.is_empty() { "K".into() } else { s }
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let keys = keys_from_bytes(data);
    let mut decls: BTreeMap<String, EnvVarDecl> = BTreeMap::new();
    for (i, k) in keys.iter().enumerate() {
        let val = data.get(i).map(|b| format!("v{b}")).unwrap_or_else(|| "v".into());
        decls.insert(
            k.clone(),
            EnvVarDecl {
                default_value: Some(val),
                ..Default::default()
            },
        );
    }
    drop(resolve_env(&decls, &NoPrompt));
    let overrides: BTreeMap<String, String> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), format!("o{i}")))
        .collect();
    drop(resolve_env_with_overrides(&decls, &NoPrompt, &overrides));

    let mut config = AppConfig::default();
    for k in &keys {
        config.env.insert(k.clone(), "x".into());
    }
    let reserved_present = keys.iter().any(|k| is_reserved(k));
    match validate_reserved_names(&config) {
        Ok(()) => assert!(!reserved_present, "reserved key should fail validate"),
        Err(_) => {
            // consistent rejection when reserved present, or other validation error
            if reserved_present {
                // ok
            }
        }
    }
});
