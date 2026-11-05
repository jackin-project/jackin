// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{GroupFault, ImageId, ImageTag, Platform, plain};
use thiserror::Error;

use crate::dockerfile::DockerfileError;
use crate::engine::EngineError;
use crate::inspect::ImageRule;
use crate::plan::BuildStage;

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
    #[error("the {stage} image {image} breaks an image rule: {rule}")]
    Rule {
        stage: BuildStage,
        image: ImageId,
        rule: ImageRule,
    },
    #[error("the tag {tag} was expected to hold {expected} but holds {}", found_text(.found.as_ref()))]
    SourceMoved {
        tag: ImageTag,
        expected: ImageId,
        found: Option<ImageId>,
    },
    #[error(
        "the read_write path {} of the role policy holds a character the image scan cannot take; use only letters, digits and . _ - / in it",
        plain(.path)
    )]
    ReadWritePath { path: String },
    #[error("gave up waiting for another process to finish building {key}")]
    LockTimeout { key: ImageTag },
    #[error("cancelled{}", GroupFault::note(.group.as_ref()))]
    Cancelled { group: Option<GroupFault> },
}

fn found_text(found: Option<&ImageId>) -> String {
    found.map_or_else(|| "no image".to_owned(), ToString::to_string)
}
