//! Construct tool selection: Mise follows the root project minimum.

use std::fs;

use anyhow::{Context, Result, bail};

pub(super) fn mise_minimum() -> Result<String> {
    let source = fs::read_to_string("mise.toml").context("read root Mise configuration")?;
    parse_mise_minimum(&source)
}

pub(super) fn parse_mise_minimum(source: &str) -> Result<String> {
    let config: toml::Value = toml::from_str(source).context("parse root Mise configuration")?;
    let version = config
        .get("min_version")
        .and_then(toml::Value::as_str)
        .context("root Mise min_version is required for construct builds")?;
    let components: Vec<_> = version.split('.').collect();
    if components.len() != 3
        || components
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("construct requires an exact numeric Mise min_version");
    }
    Ok(version.to_owned())
}

pub(super) fn env_value(key: &str) -> String {
    let Ok(content) = fs::read_to_string("docker/construct/versions.env") else {
        return String::new();
    };
    content
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
        .unwrap_or_default()
}
