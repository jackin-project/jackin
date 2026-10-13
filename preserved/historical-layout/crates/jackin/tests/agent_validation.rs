use jackin_manifest::validate::validate_agent_consistency;
use tempfile::tempdir;

#[test]
fn rejects_supported_agent_without_corresponding_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    // Now that load() runs validate_agent_consistency, the load itself
    // fails — operator no longer has to remember to call validate
    // separately.
    let err = jackin_manifest::load_role_manifest(temp.path()).unwrap_err();
    assert!(err.to_string().contains("[codex]"));
}

#[test]
fn legacy_manifest_passes_validation() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    validate_agent_consistency(&manifest).unwrap();
}

#[test]
fn codex_only_manifest_with_codex_table_passes() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    validate_agent_consistency(&manifest).unwrap();
}

#[test]
fn amp_only_manifest_with_amp_table_passes() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    validate_agent_consistency(&manifest).unwrap();
}
