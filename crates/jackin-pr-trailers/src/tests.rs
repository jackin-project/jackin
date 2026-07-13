// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn git_native_trailer_path_dedups_and_orders() -> Result<()> {
    let block = trailer_block_from_messages(vec![
        "feat: a\n\nSigned-off-by: Alice <a@example.com>\nAcked-by: C <c@example.com>".into(),
        "feat: b\n\nCo-authored-by: Bob <b@example.com>\nSigned-off-by: Alice <a@example.com>"
            .into(),
    ])?;

    assert_eq!(
        block,
        "Signed-off-by: Alice <a@example.com>\nCo-authored-by: Bob <b@example.com>\nAcked-by: C <c@example.com>"
    );
    Ok(())
}

#[test]
fn body_file_append_adds_blank_line_before_trailers() -> Result<()> {
    let tmp = tempfile::NamedTempFile::new()?;
    std::fs::write(tmp.path(), "Summary\n\nDetails")?;

    append_trailer_block(
        tmp.path()
            .to_str()
            .ok_or_else(|| anyhow!("temp path is not valid UTF-8"))?,
        "Signed-off-by: Alice <a@example.com>",
    )?;

    let body = std::fs::read_to_string(tmp.path())?;
    assert_eq!(
        body,
        "Summary\n\nDetails\n\nSigned-off-by: Alice <a@example.com>\n"
    );
    Ok(())
}

#[test]
fn fixes_hash_line_is_not_rewritten_as_trailer() -> Result<()> {
    let block = trailer_block_from_messages(vec![
        "fix: issue\n\nFixes #123\nSigned-off-by: Alice <a@example.com>".into(),
    ])?;

    assert_eq!(block, "Signed-off-by: Alice <a@example.com>");
    assert!(!block.contains("Fixes: 123"));
    Ok(())
}

#[test]
fn sync_error_messages_are_distinct() {
    assert_eq!(
        sync_error_message("feature/demo", SyncError::MissingRemote),
        "remote branch origin/feature/demo not found — push the branch first"
    );
    assert_eq!(
        sync_error_message("feature/demo", SyncError::Diverged),
        "local HEAD differs from origin/feature/demo — push your commits first"
    );
}

#[test]
fn nul_log_parser_splits_full_messages() {
    assert_eq!(
        commit_messages_from_nul_log("feat: one\n\nbody\0fix: two\n\nbody\0"),
        vec![
            "feat: one\n\nbody".to_owned(),
            "fix: two\n\nbody".to_owned()
        ]
    );
}
