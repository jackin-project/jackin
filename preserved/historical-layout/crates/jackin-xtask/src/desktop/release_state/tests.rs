// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn missing_release() {
    let state = compute_state("1.2.3", DEFAULT_REPO, false, &[], None, None);
    assert!(!state.release_exists);
    assert!(!state.app_file_assets_complete);
    assert!(!state.complete);
    assert_eq!(state.asset, desktop_asset_name("1.2.3"));
}

#[test]
fn complete_release_formula_cask() {
    let version = "1.2.3";
    let asset = desktop_asset_name(version);
    let names = vec![
        asset.clone(),
        format!("{asset}.sha256"),
        format!("{asset}.bundle"),
        format!("{asset}.sbom.json"),
    ];
    let formula = r#"  version "1.2.3""#;
    let cask = format!(
        r#"  version "1.2.3"
  sha256 "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  url "https://github.com/{DEFAULT_REPO}/releases/download/v1.2.3/{asset}"
"#
    );
    let state = compute_state(
        version,
        DEFAULT_REPO,
        true,
        &names,
        Some(formula),
        Some(&cask),
    );
    assert!(state.release_exists);
    assert!(state.app_file_assets_complete);
    assert!(state.formula_complete);
    assert!(state.cask_complete);
    assert!(state.complete);
    // idempotent pure computation
    let again = compute_state(
        version,
        DEFAULT_REPO,
        true,
        &names,
        Some(formula),
        Some(&cask),
    );
    assert_eq!(state, again);
}

#[test]
fn partial_assets_incomplete() {
    let names = vec![desktop_asset_name("9.9.9")];
    let state = compute_state("9.9.9", DEFAULT_REPO, true, &names, None, None);
    assert!(state.release_exists);
    assert!(!state.app_file_assets_complete);
    assert!(!state.complete);
}

#[test]
fn render_key_value_lines() {
    let state = compute_state("0.1.0", DEFAULT_REPO, false, &[], None, None);
    let rendered = state.render();
    assert!(rendered.contains("release_exists=false\n"));
    assert!(rendered.contains("complete=false\n"));
    assert!(rendered.contains(&format!("asset={}\n", desktop_asset_name("0.1.0"))));
}
