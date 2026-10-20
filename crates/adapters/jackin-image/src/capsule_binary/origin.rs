// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Version text is not remote-fetch authority for local or source builds.

use super::{ReleaseChannel, version_channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeReleaseOrigin<'a> {
    pub(super) channel: ReleaseChannel,
    pub(super) source: &'a str,
}

pub(super) fn compiled(version: &str) -> Option<NativeReleaseOrigin<'static>> {
    parse(
        version,
        option_env!("JACKIN_NATIVE_RELEASE_CHANNEL"),
        option_env!("JACKIN_NATIVE_RELEASE_SOURCE"),
    )
}

pub(super) fn parse<'a>(
    version: &str,
    channel: Option<&str>,
    source: Option<&'a str>,
) -> Option<NativeReleaseOrigin<'a>> {
    let source = source?;
    if source.len() != 40
        || !source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let expected = match channel? {
        "preview" => ReleaseChannel::Preview,
        "stable" => ReleaseChannel::Stable,
        _ => return None,
    };
    if version_channel(version) != expected {
        return None;
    }
    if expected == ReleaseChannel::Preview
        && !version
            .rsplit_once('+')
            .is_some_and(|(_, sha)| source.starts_with(sha))
    {
        return None;
    }
    Some(NativeReleaseOrigin {
        channel: expected,
        source,
    })
}

pub(super) fn cache_path(
    cache: &std::path::Path,
    version: &str,
    arch: &str,
    origin: Option<NativeReleaseOrigin<'_>>,
) -> std::path::PathBuf {
    let Some(origin) = origin else {
        return super::cached_binary_path(cache, version, arch);
    };
    let channel = match origin.channel {
        ReleaseChannel::Preview => "preview",
        ReleaseChannel::Stable => "stable",
        ReleaseChannel::LocalOnly => unreachable!("native origins have a validated channel"),
    };
    cache
        .join("jackin-capsule")
        .join("native")
        .join(channel)
        .join(origin.source)
        .join(version.replace('+', "_"))
        .join(format!("linux-{arch}"))
        .join("jackin-capsule")
}
