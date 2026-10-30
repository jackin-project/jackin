// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn sha256_hex_matches_known_vector() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn bundle_digest_ignores_input_order() {
    let left = bundle_manifest_digest([("b.toml", "digest-b"), ("a.toml", "digest-a")]);
    let right = bundle_manifest_digest([("a.toml", "digest-a"), ("b.toml", "digest-b")]);
    assert_eq!(left, right);
}

#[test]
fn bundle_digest_distinguishes_digests() {
    assert_ne!(
        bundle_manifest_digest([("a.toml", "digest-a")]),
        bundle_manifest_digest([("a.toml", "digest-b")])
    );
}

#[test]
fn bundle_digest_distinguishes_paths() {
    assert_ne!(
        bundle_manifest_digest([("a.toml", "digest-a")]),
        bundle_manifest_digest([("b.toml", "digest-a")])
    );
}
