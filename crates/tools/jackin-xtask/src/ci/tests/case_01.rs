use std::{fs, os::unix::fs::symlink, path::Path};

fn repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".github/workflows")).unwrap();
    fs::write(
        temp.path().join(".github/jactionlint.yaml"),
        "profile: correctness\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".github/workflows/ci.yml"),
        "name: CI\non: push\njobs: {}\n",
    )
    .unwrap();
    temp
}

fn args(root: &Path) -> anyhow::Result<Vec<std::ffi::OsString>> {
    super::super::workflow_lint::jactionlint_args(root)
}

fn assert_valid(root: &Path) {
    assert_eq!(args(root).unwrap().len(), 9);
}

#[test]
fn jactionlint_uses_native_repository_mode_without_file_operands() {
    let temp = repository();
    let actual = args(temp.path()).unwrap();
    let expected = [
        "--config-file",
        ".github/jactionlint.yaml",
        "--format",
        "oneline",
        "--strict-exit",
        "--no-online",
        "--shellcheck=shellcheck",
        "--pyflakes=",
        "--no-color",
    ];
    assert_eq!(actual, expected.map(std::ffi::OsString::from));
}

#[test]
fn unrelated_valid_dependabot_and_composite_files_do_not_mask_empty_workflows() {
    for valid_input in ["dependabot", "composite"] {
        let temp = repository();
        fs::remove_file(temp.path().join(".github/workflows/ci.yml")).unwrap();
        match valid_input {
            "dependabot" => fs::write(
                temp.path().join(".github/dependabot.yml"),
                "version: 2\nupdates: []\n",
            )
            .unwrap(),
            "composite" => {
                let action = temp.path().join(".github/actions/build/action.yml");
                fs::create_dir_all(action.parent().unwrap()).unwrap();
                fs::write(
                    action,
                    "name: Build\nruns:\n  using: composite\n  steps: []\n",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let error = args(temp.path()).unwrap_err().to_string();
        assert!(
            error.contains("no top-level workflow YAML files"),
            "{error}"
        );
    }
}

#[test]
fn repository_action_metadata_is_in_custody_and_referenced_excluded_actions_are_checked() {
    let temp = repository();
    let external = temp.path().join("external-action.yml");
    fs::write(
        temp.path().join("action.yml"),
        "name: Root\nruns:\n  using: composite\n  steps:\n    - uses: $/fixtures/hidden\n",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("fixtures/hidden")).unwrap();
    fs::write(
        &external,
        "name: Hidden\nruns:\n  using: composite\n  steps: []\n",
    )
    .unwrap();
    symlink(external, temp.path().join("fixtures/hidden/action.yml")).unwrap();

    let error = args(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("symlink is not allowed in lint inventory"),
        "{error}"
    );
}

#[test]
fn rooted_reusable_workflow_reference_symlink_and_nonregular_inputs_reject() {
    let temp = repository();
    let workflow = temp.path().join(".github/workflows/ci.yml");
    fs::write(
        &workflow,
        "name: CI\non: push\njobs:\n  call:\n    uses: $/fixtures/reusable.yml\n",
    )
    .unwrap();
    let fixtures = temp.path().join("fixtures");
    fs::create_dir_all(&fixtures).unwrap();
    let target = temp.path().join("external.yml");
    fs::write(&target, "name: Reusable\njobs: {}\n").unwrap();
    symlink(&target, fixtures.join("reusable.yml")).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );

    fs::remove_file(fixtures.join("reusable.yml")).unwrap();
    fs::create_dir(fixtures.join("reusable.yml")).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("local workflow input is not a regular file")
    );
}

#[test]
fn dot_and_dollar_slash_select_root_composite_action() {
    let temp = repository();
    fs::write(
        temp.path().join("action.yml"),
        "name: Root\nruns:\n  using: composite\n  steps:\n    - uses: $/fixtures/nested\n",
    )
    .unwrap();
    let nested = temp.path().join("fixtures/nested/action.yml");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::write(
        nested,
        "name: Nested\nruns:\n  using: composite\n  steps:\n    - uses: $/fixtures/leaf\n",
    )
    .unwrap();
    let leaf = temp.path().join("fixtures/leaf/action.yml");
    fs::create_dir_all(leaf.parent().unwrap()).unwrap();
    fs::write(leaf, "name: Leaf\nruns:\n  using: composite\n  steps: []\n").unwrap();
    fs::write(
        temp.path().join(".github/workflows/ci.yml"),
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./\n      - uses: $/\n",
    )
    .unwrap();
    assert_valid(temp.path());
}

#[test]
fn preferred_action_yml_is_the_only_metadata_parsed_when_both_names_exist() {
    let temp = repository();
    let action_dir = temp.path().join(".github/actions/dual");
    fs::create_dir_all(&action_dir).unwrap();
    fs::write(
        action_dir.join("action.yml"),
        "name: Preferred\nruns:\n  using: composite\n  steps: []\n",
    )
    .unwrap();
    fs::write(action_dir.join("action.yaml"), "this: [is: malformed").unwrap();
    assert_valid(temp.path());
}

#[test]
fn unreferenced_skipped_fixtures_are_not_parsed() {
    let temp = repository();
    let fixture = temp.path().join("fixtures/unused/action.yml");
    fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    fs::write(fixture, "this: [is: malformed").unwrap();
    let github_fixture = temp
        .path()
        .join(".github/actions/fixtures/unused/action.yml");
    fs::create_dir_all(github_fixture.parent().unwrap()).unwrap();
    fs::write(github_fixture, "this: [is: malformed").unwrap();
    assert_valid(temp.path());
}

#[test]
fn root_and_outside_composite_metadata_symlinks_are_rejected() {
    for relative in ["action.yml", "tools/release/action.yaml"] {
        let temp = repository();
        let target = temp.path().join("external.yml");
        fs::write(&target, "name: External\n").unwrap();
        let link = temp.path().join(relative);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(target, &link).unwrap();
        let error = args(temp.path()).unwrap_err().to_string();
        assert!(
            error.contains("symlink is not allowed in lint inventory"),
            "{error}"
        );
    }
}

#[test]
fn optional_actions_lock_accepts_regular_file_and_rejects_symlink_or_nonregular_file() {
    let temp = repository();
    let lock = temp.path().join(".github/workflows/actions.lock");
    fs::write(&lock, "{}").unwrap();
    assert_valid(temp.path());

    fs::remove_file(&lock).unwrap();
    let target = temp.path().join("external.lock");
    fs::write(&target, "{}").unwrap();
    symlink(&target, &lock).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );

    fs::remove_file(&lock).unwrap();
    fs::create_dir(&lock).unwrap();
    let error = args(temp.path()).unwrap_err().to_string();
    assert!(error.contains("actions lock is not a regular file"));
}

#[test]
fn missing_or_symlinked_repository_roots_and_config_reject() {
    let temp = tempfile::tempdir().unwrap();
    let error = args(temp.path()).unwrap_err().to_string();
    assert!(error.contains(".github"));

    let temp = repository();
    let config = temp.path().join(".github/jactionlint.yaml");
    let target = temp.path().join("external-config.yaml");
    fs::write(&target, "profile: correctness\n").unwrap();
    fs::remove_file(&config).unwrap();
    symlink(target, config).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("not a regular file")
    );
}

#[test]
fn symlinked_workflow_and_action_inventory_entries_reject() {
    let temp = repository();
    let workflow = temp.path().join(".github/workflows/external.yml");
    let target = temp.path().join("outside.yml");
    fs::write(&target, "name: External\n").unwrap();
    symlink(&target, workflow).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );

    let temp = repository();
    let actions = temp.path().join(".github/actions/build");
    fs::create_dir_all(&actions).unwrap();
    fs::write(temp.path().join("outside.txt"), "external").unwrap();
    symlink(
        temp.path().join("outside.txt"),
        actions.join("unrelated.md"),
    )
    .unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
}

#[test]
fn symlinked_inventory_roots_and_non_directory_action_root_reject() {
    let temp = tempfile::tempdir().unwrap();
    let external = temp.path().join("external");
    fs::create_dir_all(external.join("workflows")).unwrap();
    fs::write(external.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(external.join("workflows/ci.yml"), "name: CI\njobs: {}\n").unwrap();
    symlink(&external, temp.path().join(".github")).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github is not a real directory")
    );

    let temp = repository();
    let workflows = temp.path().join(".github/workflows");
    fs::remove_dir_all(&workflows).unwrap();
    fs::create_dir_all(temp.path().join("external-workflows")).unwrap();
    fs::write(
        temp.path().join("external-workflows/ci.yml"),
        "name: CI\njobs: {}\n",
    )
    .unwrap();
    symlink(temp.path().join("external-workflows"), &workflows).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github/workflows is not a real directory")
    );

    let temp = repository();
    let external_actions = temp.path().join("external-actions");
    fs::create_dir(&external_actions).unwrap();
    symlink(&external_actions, temp.path().join(".github/actions")).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github/actions is not a real directory")
    );

    let temp = repository();
    fs::write(temp.path().join(".github/actions"), "not a directory").unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github/actions is not a real directory")
    );
}

#[test]
fn nonregular_legacy_inventory_names_still_reject_without_becoming_jactionlint_operands() {
    for relative in [
        ".github/workflows/legacy.y.extra.ml",
        ".github/actions/legacy/action.yml",
        ".github/actions/legacy/action.y.extra.ml",
        ".github/dependabot.y.extra.ml",
    ] {
        let temp = repository();
        let path = temp.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir(path).unwrap();
        assert!(args(temp.path()).is_err(), "{}", relative);
    }
    let temp = repository();
    fs::write(temp.path().join(".github/workflows/legacy.y.extra.ml"), "").unwrap();
    assert_eq!(args(temp.path()).unwrap().len(), 9);
}

#[test]
fn dependabot_symlink_rejects() {
    let temp = repository();
    let target = temp.path().join("outside.yml");
    fs::write(&target, "version: 2\n").unwrap();
    symlink(target, temp.path().join(".github/dependabot.yml")).unwrap();
    assert!(
        args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
}

#[test]
fn actions_lock_nonregular_file_rejects() {
    let temp = repository();
    let lock = temp.path().join(".github/workflows/actions.lock");
    fs::create_dir(&lock).unwrap();
    let error = args(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("actions lock is not a regular file"),
        "{error}"
    );
}
