use std::fs;

use tempfile::tempdir;

use super::{CiStageArgs, TOOLS, copy_cached_tools, copy_dir_files, run};

#[test]
fn cached_tool_set_is_atomic_and_complete() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    for tool in TOOLS {
        fs::write(source.join(tool), tool).unwrap();
    }

    copy_cached_tools(&source, &destination).unwrap();

    for tool in TOOLS {
        assert_eq!(fs::read_to_string(destination.join(tool)).unwrap(), *tool);
    }
}

#[test]
fn combined_stage_copies_only_files() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(source.join("ignored-directory")).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("jackin-xtask"), "binary").unwrap();

    copy_dir_files(&source, &destination).unwrap();

    assert_eq!(
        fs::read_to_string(destination.join("jackin-xtask")).unwrap(),
        "binary"
    );
    assert!(!destination.join("ignored-directory").exists());
}

#[test]
fn cache_backed_stage_preserves_inputs_in_place() {
    let temp = tempdir().unwrap();
    let tools = temp.path().join("tools");
    let xtask = temp.path().join("xtask");
    let combined = temp.path().join("combined");
    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&xtask).unwrap();
    for tool in TOOLS {
        fs::write(tools.join(tool), tool).unwrap();
    }
    fs::write(xtask.join("jackin-xtask"), "binary").unwrap();
    fs::write(xtask.join("workspace-metadata.json"), "metadata").unwrap();

    run(CiStageArgs {
        xtask_hit: true,
        tools_hit: true,
        cached_xtask: xtask.join("jackin-xtask"),
        cached_tools: tools.clone(),
        built_xtask: temp.path().join("unused"),
        tools_output: tools.clone(),
        xtask_output: xtask.clone(),
        combined_output: combined.clone(),
    })
    .unwrap();

    assert_eq!(
        fs::read_to_string(xtask.join("jackin-xtask")).unwrap(),
        "binary"
    );
    assert_eq!(
        fs::read_to_string(combined.join("workspace-metadata.json")).unwrap(),
        "metadata"
    );
    for tool in TOOLS {
        assert_eq!(fs::read_to_string(combined.join(tool)).unwrap(), *tool);
    }
}
