use super::{check_file, has_agents_path_mention, link_targets};

use std::fs;

#[test]
fn extracts_inline_link_target() {
    let targets = link_targets("see [rules](../AGENTS.md) for more");
    assert_eq!(targets, vec!["../AGENTS.md".to_owned()]);
}

#[test]
fn extracts_reference_definition_target() {
    let targets = link_targets("[rules]: ../../AGENTS.md");
    assert_eq!(targets, vec!["../../AGENTS.md".to_owned()]);
}

#[test]
fn ignores_links_inside_inline_code_spans() {
    let targets = link_targets("run `cargo run -- x [y](AGENTS.md)` now");
    assert!(targets.is_empty(), "{targets:?}");
}

#[test]
fn ignores_non_agents_targets() {
    let targets = link_targets("see [design](../../docs/x.mdx) and [src](src/lib.rs)");
    assert_eq!(
        targets,
        vec!["../../docs/x.mdx".to_owned(), "src/lib.rs".to_owned()]
    );
}

#[test]
fn path_mention_detects_other_agents_files() {
    assert!(has_agents_path_mention("see .github/AGENTS.md"));
    assert!(has_agents_path_mention("see ../AGENTS.md for rules"));
    assert!(has_agents_path_mention("see crates/AGENTS.md"));
    assert!(!has_agents_path_mention("# AGENTS.md"));
    assert!(!has_agents_path_mention("CLAUDE.md = symlink to AGENTS.md"));
}

/// A README linking to an AGENTS.md is a violation.
#[test]
fn flags_readme_linking_agents() {
    let temp = tempfile::tempdir().unwrap();
    let readme = temp.path().join("README.md");
    fs::write(&readme, "see [rules](AGENTS.md)\n").unwrap();
    let mut problems = Vec::new();
    check_file(temp.path(), &readme, &mut problems).unwrap();
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("links to `AGENTS.md`"),
        "{}",
        problems[0]
    );
}

/// An AGENTS.md that *mentions* another AGENTS.md (no link) is a violation.
#[test]
fn flags_agents_mentioning_another_agents() {
    let temp = tempfile::tempdir().unwrap();
    let agents = temp.path().join("AGENTS.md");
    fs::write(&agents, "see .github/AGENTS.md for PR rules\n").unwrap();
    let mut problems = Vec::new();
    check_file(temp.path(), &agents, &mut problems).unwrap();
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("mentions another AGENTS.md"),
        "{}",
        problems[0]
    );
}

/// The convention doc crates/AGENTS.md is exempt from the mention check.
#[test]
fn exempts_convention_doc_from_mention_check() {
    let temp = tempfile::tempdir().unwrap();
    let conv = temp.path().join("crates").join("AGENTS.md");
    fs::create_dir_all(conv.parent().unwrap()).unwrap();
    fs::write(
        &conv,
        "every crate has AGENTS.md; see crates/AGENTS.md rules\n",
    )
    .unwrap();
    let mut problems = Vec::new();
    check_file(temp.path(), &conv, &mut problems).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
}

/// A fenced code block (template example) is not flagged.
#[test]
fn ignores_links_inside_code_fence() {
    let temp = tempfile::tempdir().unwrap();
    let agents = temp.path().join("AGENTS.md");
    fs::write(
        &agents,
        "text\n\n```markdown\nWorkspace rules: [../AGENTS.md](../AGENTS.md)\n```\n",
    )
    .unwrap();
    let mut problems = Vec::new();
    check_file(temp.path(), &agents, &mut problems).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
}

/// A plain prose mention of AGENTS.md with no path (self/convention) is allowed.
#[test]
fn allows_bare_agents_mention() {
    let targets = link_targets("the nearest AGENTS.md file wins");
    assert!(targets.is_empty(), "{targets:?}");
    assert!(!has_agents_path_mention("the nearest AGENTS.md file wins"));
}
