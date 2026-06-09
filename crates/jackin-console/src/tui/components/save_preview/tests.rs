//! Tests for `save_preview`.
use super::workspace_create_display_name;

#[test]
fn workspace_create_display_name_uses_pending_or_visible_fallback() {
    assert_eq!(workspace_create_display_name(Some("demo")), "demo");
    assert_eq!(workspace_create_display_name(None), "(unnamed)");
}
