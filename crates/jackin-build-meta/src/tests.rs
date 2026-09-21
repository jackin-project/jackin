use super::local_version_override;

/// Prerelease marker the workspace version must carry on a development tree.
///
/// Every workspace manifest moves in lockstep (`chore(release): finalize …`
/// is the mirror image of a dev-cycle bump), so this crate's own
/// `CARGO_PKG_VERSION` stands for the whole workspace. Local and CI-main
/// builds embed it into `JACKIN_VERSION`, and the capsule channel classifier
/// routes `-dev`/`-preview.` builds at the rolling `preview` tag — a bare
/// version on a dev commit makes every local `cargo run` hit unpublished
/// `vX.Y.Z` capsule assets (404). Release prep strips the marker from all
/// manifests and flips this to `None` (asserting a fully bare version) for
/// the tagged commit; opening the next dev cycle restores `Some("-dev")`.
/// See CONTRIBUTING.md ("Local builds outside CI…").
const EXPECTED_PRERELEASE_MARKER: Option<&str> = Some("-dev");

#[test]
fn local_build_defaults_to_package_version() {
    assert_eq!(
        local_version_override(None, false, "0.6.0-dev").as_deref(),
        Some("0.6.0-dev")
    );
}

#[test]
fn ci_build_keeps_git_stamp_path() {
    assert_eq!(local_version_override(None, true, "0.6.0-dev"), None);
}

#[test]
fn explicit_override_wins_in_ci_and_local_builds() {
    assert_eq!(
        local_version_override(Some("custom".to_owned()), false, "0.6.0-dev").as_deref(),
        Some("custom")
    );
    assert_eq!(
        local_version_override(Some("custom".to_owned()), true, "0.6.0-dev").as_deref(),
        Some("custom")
    );
}

#[test]
fn local_build_preserves_channel_markers_verbatim() {
    // The capsule channel classifier keys off `-dev` / `-preview.` in the
    // embedded version; the local path must never normalize markers away.
    for cargo_version in [
        "0.6.5-dev",
        "0.6.5-dev+bf7df07",
        "0.6.5-preview.411+bf7df07",
        "0.6.5",
    ] {
        assert_eq!(
            local_version_override(None, false, cargo_version).as_deref(),
            Some(cargo_version),
            "local build must embed CARGO_PKG_VERSION verbatim"
        );
    }
}

#[test]
fn workspace_version_carries_expected_channel_marker() {
    let cargo_version = env!("CARGO_PKG_VERSION");
    match EXPECTED_PRERELEASE_MARKER {
        Some(marker) => assert!(
            cargo_version.contains(marker),
            "workspace version {cargo_version:?} lost its `{marker}` marker: local builds would \
             embed a stable-looking JACKIN_VERSION and route capsule downloads at the versioned \
             tag. Restore `-dev` (dev work) or flip EXPECTED_PRERELEASE_MARKER for release prep; \
             see CONTRIBUTING.md."
        ),
        None => assert!(
            !cargo_version.contains("-dev") && !cargo_version.contains("-preview."),
            "release version {cargo_version:?} still carries a prerelease marker; release prep \
             must strip it from every workspace manifest."
        ),
    }
    assert_eq!(
        local_version_override(None, false, cargo_version).as_deref(),
        Some(cargo_version)
    );
}
