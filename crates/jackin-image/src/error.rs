// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{GroupFault, ImageId, ImageTag, Platform};
use thiserror::Error;

use crate::dockerfile::DockerfileError;
use crate::engine::EngineError;

#[derive(Debug, Error)]
pub enum ImageError {
    #[error(transparent)]
    Dockerfile(#[from] DockerfileError),
    #[error("the agent has no install target for {platform}")]
    MissingInstallTarget { platform: Platform },
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("the engine built {built} but holds {} under the tag {tag}", found_text(.found.as_ref()))]
    NotDelivered {
        tag: ImageTag,
        built: ImageId,
        found: Option<ImageId>,
    },
    #[error("the tag {tag} was expected to hold {expected} but holds {}", found_text(.found.as_ref()))]
    SourceMoved {
        tag: ImageTag,
        expected: ImageId,
        found: Option<ImageId>,
    },
    #[error("gave up waiting for another process to finish building {key}")]
    LockTimeout { key: ImageTag },
    #[error("cancelled{}", GroupFault::note(.group.as_ref()))]
    Cancelled { group: Option<GroupFault> },
}

fn found_text(found: Option<&ImageId>) -> String {
    found.map_or_else(|| "no image".to_owned(), ToString::to_string)
}
