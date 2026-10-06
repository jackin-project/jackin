// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn seed_home_dir_empty_dst_seeds_from_src_and_signals_first_seed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("config.json"), b"{}").unwrap();
    fs::create_dir(&dst).unwrap(); // empty

    let outcome = seed_home_dir(&src, &dst).expect("seed should succeed");
    assert_eq!(outcome, SeedOutcome::FirstSeed, "empty dst → first seed");
    assert!(dst.join("config.json").exists(), "file copied to dst");
}

#[test]
fn seed_home_dir_nonempty_dst_skips_and_signals_already_seeded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("default.json"), b"{}").unwrap();
    fs::create_dir_all(&dst).unwrap();
    // dst has a user file → non-empty
    fs::write(dst.join("user.json"), b"{}").unwrap();

    let outcome = seed_home_dir(&src, &dst).expect("skip should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::AlreadySeeded,
        "non-empty dst → already seeded"
    );
    assert!(
        !dst.join("default.json").exists(),
        "src files not copied into non-empty dst"
    );
}

#[test]
fn seed_home_dir_absent_src_still_signals_first_seed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src-absent");
    let dst = tmp.path().join("dst");
    fs::create_dir(&dst).unwrap(); // empty

    let outcome = seed_home_dir(&src, &dst).expect("no-src seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "absent src + empty dst → still first seed (auth may be copied)"
    );
}

#[test]
fn seed_agent_home_seeds_data_and_paired_config_in_one_transaction() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let cfg_src = tmp.path().join("default/config");
    let data_dst = tmp.path().join("home/data");
    let cfg_dst = tmp.path().join("home/config");
    fs::create_dir_all(&data_src).unwrap();
    fs::create_dir_all(&cfg_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::write(cfg_src.join("settings.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty
    fs::create_dir_all(&cfg_dst).unwrap(); // empty

    let outcome = seed_agent_home(
        data_src.to_str().unwrap(),
        data_dst.to_str().unwrap(),
        Some((cfg_src.to_str().unwrap(), cfg_dst.to_str().unwrap())),
    )
    .expect("seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "empty data root → first seed"
    );
    assert!(data_dst.join("state.json").exists(), "data root seeded");
    assert!(cfg_dst.join("settings.json").exists(), "config root seeded");
}

#[test]
fn seed_agent_home_nonempty_config_root_leaves_both_untouched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let cfg_src = tmp.path().join("default/config");
    let data_dst = tmp.path().join("home/data");
    let cfg_dst = tmp.path().join("home/config");
    fs::create_dir_all(&data_src).unwrap();
    fs::create_dir_all(&cfg_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty data root
    fs::create_dir_all(&cfg_dst).unwrap();
    fs::write(cfg_dst.join("user.json"), b"{}").unwrap(); // durable config content

    let outcome = seed_agent_home(
        data_src.to_str().unwrap(),
        data_dst.to_str().unwrap(),
        Some((cfg_src.to_str().unwrap(), cfg_dst.to_str().unwrap())),
    )
    .expect("skip should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::AlreadySeeded,
        "non-empty config root → treat as durable, no seed/auth"
    );
    assert!(
        !data_dst.join("state.json").exists(),
        "data root left untouched when config root holds durable state"
    );
}

#[test]
fn seed_agent_home_no_config_root_seeds_data_only() {
    // The single-root agents (claude/codex/grok/kimi) call seed_agent_home with
    // config = None; that branch must seed the data root and signal first seed.
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_src = tmp.path().join("default/data");
    let data_dst = tmp.path().join("home/data");
    fs::create_dir_all(&data_src).unwrap();
    fs::write(data_src.join("state.json"), b"{}").unwrap();
    fs::create_dir_all(&data_dst).unwrap(); // empty

    let outcome = seed_agent_home(data_src.to_str().unwrap(), data_dst.to_str().unwrap(), None)
        .expect("seed should succeed");
    assert_eq!(
        outcome,
        SeedOutcome::FirstSeed,
        "empty data root → first seed"
    );
    assert!(data_dst.join("state.json").exists(), "data root seeded");

    // A second call now sees a non-empty data root → already seeded, no re-copy.
    fs::write(data_src.join("new.json"), b"{}").unwrap();
    let again = seed_agent_home(data_src.to_str().unwrap(), data_dst.to_str().unwrap(), None)
        .expect("second call should succeed");
    assert_eq!(
        again,
        SeedOutcome::AlreadySeeded,
        "non-empty data root → skip"
    );
    assert!(
        !data_dst.join("new.json").exists(),
        "second seed must not copy into a non-empty durable home"
    );
}

#[test]
fn git_hook_marker_is_versioned() {
    let state = parse_session_state_dir("/jackin/run/sessions/42/state").unwrap();
    assert_eq!(
        state.join("git-hooks/prepare-commit-msg.v3.done"),
        Path::new("/jackin/run/sessions/42/state/git-hooks/prepare-commit-msg.v3.done")
    );
}

#[test]
fn hook_uses_canonical_agent_trailers() {
    assert_eq!(
        coauthor_trailer_for_agent("claude"),
        Some("Co-authored-by: Claude <noreply@anthropic.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("codex"),
        Some("Co-authored-by: Codex <codex@openai.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("amp"),
        Some("Co-authored-by: Amp <amp@ampcode.com>")
    );
    assert_eq!(
        coauthor_trailer_for_agent("opencode"),
        Some("Co-authored-by: opencode-agent[bot] <opencode-agent[bot]@users.noreply.github.com>")
    );
    assert_eq!(coauthor_trailer_for_agent("kimi"), None);
    assert_eq!(coauthor_trailer_for_agent("grok"), None);
}

#[test]
fn hook_marker_points_at_capsule_runtime_binary() {
    assert_eq!(CAPSULE_RUNTIME_BIN, "/jackin/runtime/jackin-capsule");
}

#[test]
fn enforced_claude_config_keeps_mutable_metadata_inside_directory_mount() {
    let directory = container_paths::CLAUDE_CONFIG_DIR;
    assert_eq!(
        claude_account_path_from(Some(directory)),
        claude_config_dir_from(Some(directory)).join(".claude.json")
    );
    let temporary = tempfile::tempdir().unwrap();
    let metadata = temporary.path().join(".claude.json");
    fs::write(&metadata, b"stale-account").unwrap();
    remove_file_if_exists(&metadata).unwrap();
    let replacement = temporary.path().join(".claude.json.tmp");
    fs::write(&replacement, b"new-account").unwrap();
    fs::rename(&replacement, &metadata).unwrap();
    assert_eq!(fs::read(&metadata).unwrap(), b"new-account");
}
