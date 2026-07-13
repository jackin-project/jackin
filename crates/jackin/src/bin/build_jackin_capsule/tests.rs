// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn feature_suffix_is_stable_and_filename_safe() {
    assert_eq!(feature_suffix(&[]), "");
    assert_eq!(
        feature_suffix(&["dhat-heap".to_owned(), "trace/json".to_owned()]),
        "-features-dhat-heap-trace-json"
    );
}

#[test]
fn feature_builds_do_not_overwrite_normal_cache_entry() {
    let cache = PathBuf::from("/tmp/cache");
    let normal = binary_cache_path(&cache, "0.6.0-dev+abc", "arm64", BuildProfile::Release, &[]);
    let dhat = binary_cache_path(
        &cache,
        "0.6.0-dev+abc",
        "arm64",
        BuildProfile::Release,
        &["dhat-heap".to_owned()],
    );

    assert_ne!(normal, dhat);
    assert!(normal.ends_with("jackin-capsule"));
    assert!(dhat.ends_with("jackin-capsule-features-dhat-heap"));
}
