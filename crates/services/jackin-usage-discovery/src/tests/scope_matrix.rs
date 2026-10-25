// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::host_credential_root_matrix;

#[test]
fn credential_root_matrix_lists_each_surface_once() {
    let matrix = host_credential_root_matrix();

    assert!(!matrix.is_empty());
    let mut surfaces: Vec<&str> = matrix.iter().map(|row| row.surface).collect();
    surfaces.sort_unstable();
    let before = surfaces.len();
    surfaces.dedup();
    assert_eq!(surfaces.len(), before);
    assert!(surfaces.contains(&"claude"));
    assert!(surfaces.contains(&"codex"));
}

#[test]
fn claude_row_documents_host_inputs_and_handoff() {
    let matrix = host_credential_root_matrix();
    let claude = matrix
        .iter()
        .find(|row| row.surface == "claude")
        .expect("matrix must document claude");

    assert!(claude.env_vars.contains("ANTHROPIC_API_KEY"));
    assert!(!claude.host_paths.is_empty());
    assert!(!claude.container_handoff.is_empty());
}
