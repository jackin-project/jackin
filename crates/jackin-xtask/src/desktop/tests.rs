use super::{
    APP_EXECUTABLE, BUNDLE_ID, BUNDLE_NAME, MIN_OS, app_info_plist, minos_newer_than_14,
    validate_build, validate_version,
};

#[test]
fn version_accepts_dotted_numeric() {
    validate_version("0.6.0").unwrap();
    validate_version("1").unwrap();
    validate_version("10.20.30").unwrap();
}

#[test]
fn version_rejects_semver_prerelease_and_empty() {
    assert!(validate_version("").is_err());
    assert!(validate_version("0.6.0-dev").is_err());
    assert!(validate_version("v0.6.0").is_err());
    assert!(validate_version("0..1").is_err());
}

#[test]
fn build_accepts_numeric_only() {
    validate_build("1").unwrap();
    validate_build("42").unwrap();
    assert!(validate_build("").is_err());
    assert!(validate_build("1a").is_err());
}

#[test]
fn minos_compare_rejects_newer_than_14() {
    assert!(!minos_newer_than_14("14.0"));
    assert!(!minos_newer_than_14("14.0.0"));
    assert!(!minos_newer_than_14("13.5"));
    assert!(minos_newer_than_14("14.1"));
    assert!(minos_newer_than_14("15.0"));
}

#[test]
fn info_plist_embeds_identity_and_versions() {
    let plist = app_info_plist("0.6.0", "1");
    assert!(plist.contains(BUNDLE_ID));
    assert!(plist.contains(BUNDLE_NAME));
    assert!(plist.contains(APP_EXECUTABLE));
    assert!(plist.contains(MIN_OS));
    assert!(plist.contains("<string>0.6.0</string>"));
    assert!(plist.contains("<string>1</string>"));
    assert!(plist.contains("<key>LSUIElement</key>"));
}
