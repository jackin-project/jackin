use super::*;
use serde_json::json;
use tempfile::tempdir;

fn action(root: &Path, name: &str, yaml: &str) -> Result<()> {
    let directory = root.join(".github/actions").join(name);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("action.yml"), yaml)?;
    Ok(())
}

fn composite(steps: &str) -> String {
    format!("runs:\n  using: composite\n  steps:\n{steps}")
}

fn plain_composite() -> String {
    composite("    - run: true\n      shell: bash\n")
}

fn assert_accepted(root: &Path, steps: &[Value]) -> Result<()> {
    assert_steps(root, steps)
}

fn assert_rejected(root: &Path, steps: &[Value]) {
    assert!(assert_steps(root, steps).is_err());
}

#[test]
fn clean_inline_and_repeated_nested_composites_are_accepted() -> Result<()> {
    let root = tempdir()?;
    action(
        root.path(),
        "leaf",
        &composite("    - run: printf clean\n      shell: bash\n"),
    )?;
    action(
        root.path(),
        "branch",
        &composite("    - uses: ./.github/actions/leaf\n    - uses: ./.github/actions/leaf\n"),
    )?;
    assert_accepted(
        root.path(),
        &[
            json!({"run": "true"}),
            json!({"uses": "./.github/actions/branch"}),
        ],
    )
}

#[test]
fn hidden_authority_markers_in_composite_closure_are_rejected() -> Result<()> {
    for step in [
        json!({"env": {"VELNOR_INTERNAL_OP": "resolve-task-execution-v1"}, "run": "true"}),
        json!({"env": {"VELNOR_TASK_EXECUTION_DIGEST": "forged"}, "run": "true"}),
        json!({"with": {"digest": "forged"}, "run": "true"}),
        json!({"run": "echo VELNOR_INTERNAL_OP"}),
        json!({"run": "echo VELNOR_TASK_EXECUTION_DIGEST"}),
        json!({"uses": "example/declared-task-action@v1"}),
        json!({"uses": "actions/upload-artifact@v4"}),
    ] {
        let root = tempdir()?;
        action(
            root.path(),
            "nested",
            &composite(&format!("    - {}\n", serde_json::to_string(&step)?)),
        )?;
        assert_rejected(root.path(), &[json!({"uses": "./.github/actions/nested"})]);
    }
    Ok(())
}

#[test]
fn missing_escaping_malformed_and_non_composite_actions_are_rejected() -> Result<()> {
    let root = tempdir()?;
    assert_rejected(root.path(), &[json!({"uses": "./.github/actions/missing"})]);
    for uses in [
        "./.github/actions/../outside",
        "./.github/actions/nested/child",
        "./.github/actions/",
        "./.github/actions/nested name",
        "./outside",
    ] {
        assert_rejected(root.path(), &[json!({"uses": uses})]);
    }
    action(root.path(), "docker", "runs:\n  using: docker\n")?;
    assert_rejected(root.path(), &[json!({"uses": "./.github/actions/docker"})]);
    action(root.path(), "malformed", "runs: [broken\n")?;
    assert_rejected(
        root.path(),
        &[json!({"uses": "./.github/actions/malformed"})],
    );
    action(root.path(), "missing-steps", "runs:\n  using: composite\n")?;
    assert_rejected(
        root.path(),
        &[json!({"uses": "./.github/actions/missing-steps"})],
    );
    Ok(())
}

#[test]
fn repository_root_action_aliases_are_rejected_for_benign_and_malicious_targets() -> Result<()> {
    let root = tempdir()?;
    action(root.path(), "benign", &plain_composite())?;
    action(
        root.path(),
        "malicious",
        &composite("    - run: echo VELNOR_INTERNAL_OP\n      shell: bash\n"),
    )?;
    assert_rejected(root.path(), &[json!({"uses": "$/.github/actions/benign"})]);
    assert_rejected(
        root.path(),
        &[json!({"uses": "$/.github/actions/malicious"})],
    );
    Ok(())
}

#[test]
fn composite_closure_resource_budgets_accept_boundaries_and_reject_overages() -> Result<()> {
    let depth_root = tempdir()?;
    for index in 0..MAX_COMPOSITE_DEPTH {
        let name = format!("depth-{index:02}");
        let contents = if index + 1 == MAX_COMPOSITE_DEPTH {
            plain_composite()
        } else {
            composite(&format!(
                "    - uses: ./.github/actions/depth-{:02}\n",
                index + 1
            ))
        };
        action(depth_root.path(), &name, &contents)?;
    }
    assert_accepted(
        depth_root.path(),
        &[json!({"uses": "./.github/actions/depth-00"})],
    )?;
    let too_deep = format!("depth-{MAX_COMPOSITE_DEPTH:02}");
    action(depth_root.path(), &too_deep, &plain_composite())?;
    let parent = format!("depth-{:02}", MAX_COMPOSITE_DEPTH - 1);
    fs::write(
        depth_root
            .path()
            .join(".github/actions")
            .join(parent)
            .join("action.yml"),
        composite(&format!("    - uses: ./.github/actions/{too_deep}\n")),
    )?;
    assert_rejected(
        depth_root.path(),
        &[json!({"uses": "./.github/actions/depth-00"})],
    );

    let count_root = tempdir()?;
    let mut uses = Vec::new();
    for index in 0..MAX_COMPOSITE_ACTIONS {
        let name = format!("count-{index:03}");
        action(count_root.path(), &name, &plain_composite())?;
        uses.push(json!({"uses": format!("./.github/actions/{name}")}));
    }
    assert_accepted(count_root.path(), &uses)?;
    let name = format!("count-{MAX_COMPOSITE_ACTIONS:03}");
    action(count_root.path(), &name, &plain_composite())?;
    uses.push(json!({"uses": format!("./.github/actions/{name}")}));
    assert_rejected(count_root.path(), &uses);

    let file_root = tempdir()?;
    let oversized = format!(
        "#{}\n{}",
        "x".repeat(MAX_ACTION_METADATA_BYTES as usize + 1),
        plain_composite()
    );
    action(file_root.path(), "oversized", &oversized)?;
    assert_rejected(
        file_root.path(),
        &[json!({"uses": "./.github/actions/oversized"})],
    );

    let aggregate_root = tempdir()?;
    let body = plain_composite();
    let padded = format!(
        "#{}\n{body}",
        "x".repeat(MAX_ACTION_METADATA_BYTES as usize - body.len() - 2)
    );
    assert_eq!(padded.len() as u64, MAX_ACTION_METADATA_BYTES);
    let mut uses = Vec::new();
    for index in 0..(MAX_TOTAL_METADATA_BYTES / MAX_ACTION_METADATA_BYTES) {
        let name = format!("total-{index:02}");
        action(aggregate_root.path(), &name, &padded)?;
        uses.push(json!({"uses": format!("./.github/actions/{name}")}));
    }
    assert_accepted(aggregate_root.path(), &uses)?;
    let name = "total-over";
    action(aggregate_root.path(), name, &plain_composite())?;
    uses.push(json!({"uses": format!("./.github/actions/{name}")}));
    assert_rejected(aggregate_root.path(), &uses);
    Ok(())
}

#[test]
fn composite_cycles_and_symlinked_paths_are_rejected() -> Result<()> {
    let root = tempdir()?;
    action(
        root.path(),
        "cycle-a",
        &composite("    - uses: ./.github/actions/cycle-b\n"),
    )?;
    action(
        root.path(),
        "cycle-b",
        &composite("    - uses: ./.github/actions/cycle-a\n"),
    )?;
    assert_rejected(root.path(), &[json!({"uses": "./.github/actions/cycle-a"})]);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlink_root = tempdir()?;
        action(
            symlink_root.path(),
            "real",
            &composite("    - run: true\n      shell: bash\n"),
        )?;
        symlink(
            symlink_root.path().join(".github/actions/real"),
            symlink_root.path().join(".github/actions/linked"),
        )?;
        assert_rejected(
            symlink_root.path(),
            &[json!({"uses": "./.github/actions/linked"})],
        );

        let metadata_root = tempdir()?;
        action(metadata_root.path(), "dangling", &plain_composite())?;
        let dangling_dir = metadata_root.path().join(".github/actions/dangling");
        fs::remove_file(dangling_dir.join("action.yml"))?;
        symlink(
            dangling_dir.join("missing.yml"),
            dangling_dir.join("action.yml"),
        )?;
        fs::write(dangling_dir.join("action.yaml"), plain_composite())?;
        assert_rejected(
            metadata_root.path(),
            &[json!({"uses": "./.github/actions/dangling"})],
        );

        let metadata_symlink_root = tempdir()?;
        action(
            metadata_symlink_root.path(),
            "metadata-link",
            &plain_composite(),
        )?;
        let linked_dir = metadata_symlink_root
            .path()
            .join(".github/actions/metadata-link");
        symlink(
            linked_dir.join("action.yml"),
            linked_dir.join("action.yaml"),
        )?;
        assert_rejected(
            metadata_symlink_root.path(),
            &[json!({"uses": "./.github/actions/metadata-link"})],
        );
    }
    Ok(())
}
