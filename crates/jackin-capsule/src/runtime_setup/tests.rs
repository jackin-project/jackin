//! Tests for `runtime_setup`.
use super::*;

#[test]
fn container_init_marker_is_container_local() {
    assert_eq!(CONTAINER_INIT_MARKER, "/jackin/state/container-init.done");
}

#[test]
fn git_hook_marker_is_versioned() {
    assert_eq!(
        GIT_HOOK_MARKER,
        "/jackin/state/git-hooks/prepare-commit-msg.v3.done"
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
}

#[test]
fn hook_marker_points_at_capsule_runtime_binary() {
    assert_eq!(CAPSULE_RUNTIME_BIN, "/jackin/runtime/jackin-capsule");
}
