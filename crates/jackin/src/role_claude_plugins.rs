//! Claude plugin marketplace validation for role-authoring commands.
//!
//! Runtime image builds intentionally keep using Claude Code's own marketplace
//! and plugin installer. This module belongs to `jackin-role`/`jackin role
//! validate`: it proves role manifests are resolvable before a role is
//! published or merged.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use jackin_manifest::{ClaudeMarketplaceConfig, RoleManifest};
use serde::Deserialize;

const OFFICIAL_MARKETPLACE_NAME: &str = "claude-plugins-official";
const OFFICIAL_MARKETPLACE_SOURCE: &str = "anthropics/claude-plugins-official";
const MARKETPLACE_MANIFEST_PATH: &str = ".claude-plugin/marketplace.json";

#[derive(Debug, Deserialize)]
struct MarketplaceManifest {
    name: String,
    plugins: Vec<MarketplacePlugin>,
}

#[derive(Debug, Deserialize)]
struct MarketplacePlugin {
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginRef<'a> {
    plugin: &'a str,
    marketplace: &'a str,
}

trait MarketplaceResolver {
    fn resolve(&self, source: &str) -> anyhow::Result<String>;
}

#[derive(Debug)]
struct GitHubRawMarketplaceResolver;

impl GitHubRawMarketplaceResolver {
    fn raw_url(source: &str) -> anyhow::Result<String> {
        let slug = github_slug(source)?;
        Ok(format!(
            "https://raw.githubusercontent.com/{slug}/HEAD/{MARKETPLACE_MANIFEST_PATH}"
        ))
    }
}

impl MarketplaceResolver for GitHubRawMarketplaceResolver {
    fn resolve(&self, source: &str) -> anyhow::Result<String> {
        let url = Self::raw_url(source)?;
        std::thread::spawn(move || fetch_marketplace_manifest(&url))
            .join()
            .map_err(|_| anyhow::anyhow!("fetching Claude marketplace manifest panicked"))?
    }
}

fn fetch_marketplace_manifest(url: &str) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "jackin-role-validate")
        .send()
        .with_context(|| format!("fetching Claude marketplace manifest from {url}"))?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("fetching Claude marketplace manifest from {url} returned {status}");
    }
    response
        .text()
        .with_context(|| format!("reading Claude marketplace manifest from {url}"))
}

/// Validate that every `[claude].plugins` reference uses
/// `plugin@marketplace`, that each marketplace name resolves to a declared
/// marketplace (or the official built-in marketplace), and that the fetched
/// marketplace manifest publishes the requested plugin.
pub(crate) fn validate_claude_plugin_marketplaces(manifest: &RoleManifest) -> anyhow::Result<()> {
    validate_claude_plugin_marketplaces_with(manifest, &GitHubRawMarketplaceResolver)
}

fn validate_claude_plugin_marketplaces_with(
    manifest: &RoleManifest,
    resolver: &dyn MarketplaceResolver,
) -> anyhow::Result<()> {
    let Some(claude) = &manifest.claude else {
        return Ok(());
    };
    if claude.plugins.is_empty() {
        return Ok(());
    }

    let plugin_refs = claude
        .plugins
        .iter()
        .map(|plugin| parse_plugin_ref(plugin))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut sources_by_name = BTreeMap::from([(
        OFFICIAL_MARKETPLACE_NAME.to_owned(),
        OFFICIAL_MARKETPLACE_SOURCE.to_owned(),
    )]);
    for marketplace in &claude.marketplaces {
        register_marketplace(&mut sources_by_name, marketplace, resolver)?;
    }

    for plugin_ref in plugin_refs {
        let source = sources_by_name
            .get(plugin_ref.marketplace)
            .ok_or_else(|| unknown_marketplace_error(&sources_by_name, &plugin_ref))?;
        let manifest_json = resolver.resolve(source)?;
        let marketplace_manifest: MarketplaceManifest = serde_json::from_str(&manifest_json)
            .with_context(|| format!("parsing Claude marketplace manifest for {source}"))?;
        let published_plugins = marketplace_manifest
            .plugins
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<BTreeSet<_>>();
        if !published_plugins.contains(plugin_ref.plugin) {
            anyhow::bail!(
                "Claude plugin \"{}@{}\" is invalid: marketplace \"{}\" from {} publishes [{}], not \"{}\"",
                plugin_ref.plugin,
                plugin_ref.marketplace,
                marketplace_manifest.name,
                source,
                published_plugins.into_iter().collect::<Vec<_>>().join(", "),
                plugin_ref.plugin
            );
        }
    }

    Ok(())
}

fn register_marketplace(
    sources_by_name: &mut BTreeMap<String, String>,
    marketplace: &ClaudeMarketplaceConfig,
    resolver: &dyn MarketplaceResolver,
) -> anyhow::Result<()> {
    let manifest_json = resolver.resolve(&marketplace.source)?;
    let manifest: MarketplaceManifest =
        serde_json::from_str(&manifest_json).with_context(|| {
            format!(
                "parsing Claude marketplace manifest for {}",
                marketplace.source
            )
        })?;
    if let Some(previous) =
        sources_by_name.insert(manifest.name.clone(), marketplace.source.clone())
        && previous != marketplace.source
    {
        anyhow::bail!(
            "Claude marketplace name \"{}\" is declared by both {} and {}",
            manifest.name,
            previous,
            marketplace.source
        );
    }
    Ok(())
}

fn parse_plugin_ref(raw: &str) -> anyhow::Result<PluginRef<'_>> {
    let Some((plugin, marketplace)) = raw.split_once('@') else {
        anyhow::bail!(
            "Claude plugin \"{raw}\" must use plugin@marketplace format, e.g. code-review@claude-plugins-official"
        );
    };
    if plugin.is_empty() || marketplace.is_empty() || marketplace.contains('@') {
        anyhow::bail!(
            "Claude plugin \"{raw}\" must use plugin@marketplace format, e.g. code-review@claude-plugins-official"
        );
    }
    Ok(PluginRef {
        plugin,
        marketplace,
    })
}

fn unknown_marketplace_error(
    sources_by_name: &BTreeMap<String, String>,
    plugin_ref: &PluginRef<'_>,
) -> anyhow::Error {
    anyhow::anyhow!(
        "Claude plugin \"{}@{}\" references unknown marketplace \"{}\"; declare [[claude.marketplaces]] with a source whose marketplace.json name is \"{}\" or use one of [{}]",
        plugin_ref.plugin,
        plugin_ref.marketplace,
        plugin_ref.marketplace,
        plugin_ref.marketplace,
        sources_by_name
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn github_slug(source: &str) -> anyhow::Result<String> {
    let mut trimmed = source.trim();
    if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        trimmed = rest;
    }
    if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        trimmed = rest;
    }
    if let Some(rest) = trimmed.strip_suffix(".git") {
        trimmed = rest;
    }
    let trimmed = trimmed.trim_end_matches('/');
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
        anyhow::bail!(
            "Claude marketplace source \"{source}\" must be a GitHub owner/repo slug or GitHub URL"
        );
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}

#[cfg(test)]
mod tests;
