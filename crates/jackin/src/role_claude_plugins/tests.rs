use std::collections::BTreeMap;

use jackin_core::manifest::{ClaudeConfig, ManifestDockerConfig};
use jackin_manifest::{ClaudeMarketplaceConfig, RoleManifest};

use super::*;

#[derive(Debug)]
struct FixtureResolver {
    manifests: BTreeMap<&'static str, &'static str>,
}

impl MarketplaceResolver for FixtureResolver {
    fn resolve(&self, source: &str) -> anyhow::Result<String> {
        self.manifests
            .get(source)
            .map(|manifest| (*manifest).to_owned())
            .ok_or_else(|| anyhow::anyhow!("missing fixture for {source}"))
    }
}

fn manifest(plugins: Vec<&str>, marketplaces: Vec<ClaudeMarketplaceConfig>) -> RoleManifest {
    RoleManifest {
        version: "v1alpha5".to_owned(),
        dockerfile: "Dockerfile".to_owned(),
        published_image: None,
        identity: None,
        agents: None,
        claude: Some(ClaudeConfig {
            model: None,
            marketplaces,
            plugins: plugins.into_iter().map(str::to_owned).collect(),
            providers: BTreeMap::new(),
        }),
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        hooks: None,
        env: BTreeMap::new(),
        docker: None::<ManifestDockerConfig>,
    }
}

fn resolver() -> FixtureResolver {
    FixtureResolver {
        manifests: BTreeMap::from([
            (
                OFFICIAL_MARKETPLACE_SOURCE,
                r#"{"name":"claude-plugins-official","plugins":[{"name":"code-review"}]}"#,
            ),
            (
                "tailrocks/tailrocks-marketplace",
                r#"{"name":"tailrocks-marketplace","plugins":[{"name":"tailrocks-skills"}]}"#,
            ),
        ]),
    }
}

#[test]
fn validates_plugin_published_by_declared_marketplace() {
    let manifest = manifest(
        vec!["tailrocks-skills@tailrocks-marketplace"],
        vec![ClaudeMarketplaceConfig {
            source: "tailrocks/tailrocks-marketplace".to_owned(),
            sparse: Vec::new(),
        }],
    );

    validate_claude_plugin_marketplaces_with(&manifest, &resolver()).unwrap();
}

#[test]
fn validates_plugin_published_by_official_marketplace() {
    let manifest = manifest(vec!["code-review@claude-plugins-official"], Vec::new());

    validate_claude_plugin_marketplaces_with(&manifest, &resolver()).unwrap();
}

#[test]
fn rejects_plugin_missing_from_marketplace_manifest() {
    let manifest = manifest(
        vec!["rust-best-practices@tailrocks-marketplace"],
        vec![ClaudeMarketplaceConfig {
            source: "tailrocks/tailrocks-marketplace".to_owned(),
            sparse: Vec::new(),
        }],
    );

    let error = validate_claude_plugin_marketplaces_with(&manifest, &resolver()).unwrap_err();

    assert!(error.to_string().contains("rust-best-practices"));
    assert!(error.to_string().contains("tailrocks-skills"));
}

#[test]
fn rejects_plugin_without_marketplace_suffix() {
    let manifest = manifest(vec!["code-review"], Vec::new());

    let error = validate_claude_plugin_marketplaces_with(&manifest, &resolver()).unwrap_err();

    assert!(error.to_string().contains("plugin@marketplace"));
}

#[test]
fn rejects_unknown_marketplace_name() {
    let manifest = manifest(vec!["tailrocks-skills@tailrocks-marketplace"], Vec::new());

    let error = validate_claude_plugin_marketplaces_with(&manifest, &resolver()).unwrap_err();

    assert!(error.to_string().contains("unknown marketplace"));
    assert!(error.to_string().contains("tailrocks-marketplace"));
}

#[test]
fn normalizes_github_marketplace_sources_to_raw_manifest_urls() {
    assert_eq!(
        GitHubRawMarketplaceResolver::raw_url(
            "https://github.com/tailrocks/tailrocks-marketplace.git"
        )
        .unwrap(),
        "https://raw.githubusercontent.com/tailrocks/tailrocks-marketplace/HEAD/.claude-plugin/marketplace.json"
    );
    assert_eq!(
        GitHubRawMarketplaceResolver::raw_url("tailrocks/tailrocks-marketplace").unwrap(),
        "https://raw.githubusercontent.com/tailrocks/tailrocks-marketplace/HEAD/.claude-plugin/marketplace.json"
    );
}
