use crate::manifest::ClaudeMarketplaceConfig;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct PluginState<'a> {
    pub(super) marketplaces: &'a [ClaudeMarketplaceConfig],
    pub(super) plugins: &'a [String],
}

#[cfg(test)]
mod tests {
    use crate::config::AuthForwardMode;
    use crate::instance::RoleState;
    use crate::paths::JackinPaths;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn prepares_plugins_json_for_runtime_bootstrap() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official", "feature-dev@claude-plugins-official"]
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = crate::manifest::RoleManifest::load(temp.path()).unwrap();
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert!(state.jackin_dir.is_dir());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state.plugins_json).unwrap()).unwrap();
        assert_eq!(value["marketplaces"], json!([]));
        assert_eq!(
            value["plugins"],
            json!([
                "code-review@claude-plugins-official",
                "feature-dev@claude-plugins-official"
            ])
        );
    }

    #[test]
    fn prepares_plugins_json_with_marketplaces_for_runtime_bootstrap() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = crate::manifest::RoleManifest::load(temp.path()).unwrap();
        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state.plugins_json).unwrap()).unwrap();
        assert_eq!(
            value["marketplaces"],
            json!([
                {
                    "source": "obra/superpowers-marketplace",
                    "sparse": ["plugins", ".claude-plugin"]
                }
            ])
        );
        assert_eq!(
            value["plugins"],
            json!(["superpowers@superpowers-marketplace"])
        );
    }
}
