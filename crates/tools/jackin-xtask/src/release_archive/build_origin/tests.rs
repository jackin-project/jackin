// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn validated_native_origin_projects_exact_cargo_environment() {
    for (channel, expected, version) in [
        (
            PackageReleaseChannel::Preview,
            "preview",
            "0.6.4-preview.1+abcdef0",
        ),
        (PackageReleaseChannel::Stable, "stable", "0.6.4"),
    ] {
        let mut command = crate::cmd::command("/bin/sh");
        command.args(["-c", "printf '%s\n' \"$JACKIN_VERSION_OVERRIDE\" \"$JACKIN_NATIVE_RELEASE_CHANNEL\" \"$JACKIN_NATIVE_RELEASE_SOURCE\""]);
        BuildVersion {
            version,
            origin: Some(NativeBuildOrigin {
                channel,
                source: "abcdef01234567890123456789012345678901234",
            }),
        }
        .apply(&mut command);
        let bytes = crate::cmd::output(&mut command).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            format!("{version}\n{expected}\nabcdef01234567890123456789012345678901234\n")
        );
    }
}
