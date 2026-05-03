use jackin::manifest::{RoleManifest, validate::validate_harness_consistency};
use tempfile::tempdir;

#[test]
fn rejects_supported_harness_without_corresponding_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "codex"]

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let manifest = RoleManifest::load(temp.path()).unwrap();
    let err = validate_harness_consistency(&manifest).unwrap_err();
    assert!(err.to_string().contains("[codex]"));
}

#[test]
fn legacy_manifest_passes_validation() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let manifest = RoleManifest::load(temp.path()).unwrap();
    validate_harness_consistency(&manifest).unwrap();
}

#[test]
fn codex_only_manifest_with_codex_table_passes() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let manifest = RoleManifest::load(temp.path()).unwrap();
    validate_harness_consistency(&manifest).unwrap();
}
