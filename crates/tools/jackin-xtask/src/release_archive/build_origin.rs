// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;
use velnor_actions_contract::package_release::PackageReleaseChannel;

#[derive(Clone, Copy)]
pub(super) struct NativeBuildOrigin<'a> {
    pub(super) channel: PackageReleaseChannel,
    pub(super) source: &'a str,
}

pub(super) struct BuildVersion<'a> {
    pub(super) version: &'a str,
    pub(super) origin: Option<NativeBuildOrigin<'a>>,
}

impl BuildVersion<'_> {
    pub(super) fn apply(self, command: &mut Command) {
        command.env("JACKIN_VERSION_OVERRIDE", self.version);
        command.env_remove("JACKIN_NATIVE_RELEASE_CHANNEL");
        command.env_remove("JACKIN_NATIVE_RELEASE_SOURCE");
        if let Some(origin) = self.origin {
            command.env(
                "JACKIN_NATIVE_RELEASE_CHANNEL",
                match origin.channel {
                    PackageReleaseChannel::Preview => "preview",
                    PackageReleaseChannel::Stable => "stable",
                },
            );
            command.env("JACKIN_NATIVE_RELEASE_SOURCE", origin.source);
        }
    }
}
