// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn known_legacy_snapshot() -> RollingReleaseSnapshot {
    RollingReleaseSnapshot {
        source_repository: LEGACY_SOURCE_REPOSITORY.to_owned(),
        tag_name: LEGACY_TAG.to_owned(),
        release_id: LEGACY_RELEASE_ID,
        release_name: LEGACY_RELEASE_NAME.to_owned(),
        release_body: LEGACY_RELEASE_BODY.to_owned(),
        tag_target: LEGACY_TAG_TARGET.to_owned(),
        draft: false,
        prerelease: true,
        assets: known_legacy_assets(),
    }
}
