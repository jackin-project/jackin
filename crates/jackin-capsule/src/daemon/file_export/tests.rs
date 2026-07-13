//! Tests for `file_export` path categories (INV-D20).
use super::requested_export_path_category;
use jackin_core::container_paths;

#[test]
fn export_path_category_matrix_inv_d20() {
    assert_eq!(
        requested_export_path_category(container_paths::RUN_DIR),
        "jackin-run"
    );
    assert_eq!(
        requested_export_path_category(&format!("{}/export.bin", container_paths::RUN_DIR)),
        "jackin-run"
    );
    assert_eq!(
        requested_export_path_category(container_paths::STATE_DIR),
        "jackin-owned"
    );
    assert_eq!(
        requested_export_path_category("/workspace/src/main.rs"),
        "container-absolute"
    );
    assert_eq!(
        requested_export_path_category("relative/path.toml"),
        "container-relative"
    );
    assert_eq!(
        requested_export_path_category("  /tmp/x  "),
        "container-absolute"
    );
}
