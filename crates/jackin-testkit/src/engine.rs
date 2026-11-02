// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use jackin_core::{BuilderName, ImageId, ImageTag, ProcessStatus, Sha256, TerminalText};
use jackin_image::{BuildStage, EngineError, ImageEngine, PlannedBuild, ProgressSink};
use tokio_util::sync::CancellationToken;

use crate::sync::locked;

#[derive(Debug, Default)]
struct EngineState {
    images: BTreeMap<ImageTag, ImageId>,
    pinned: BTreeMap<ImageTag, ImageId>,
    builds: Vec<BuildRecord>,
    failures: Vec<BuildStage>,
}

#[derive(Debug, Default)]
pub struct FakeEngine {
    state: Mutex<EngineState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRecord {
    pub stage: BuildStage,
    pub tag: ImageTag,
}

impl FakeEngine {
    pub fn new() -> FakeEngine {
        FakeEngine::default()
    }

    pub fn images(&self) -> BTreeMap<ImageTag, ImageId> {
        locked(&self.state).images.clone()
    }

    pub fn builds(&self) -> Vec<BuildRecord> {
        locked(&self.state).builds.clone()
    }

    pub fn pin_id(&self, tag: &ImageTag, id: ImageId) {
        let mut state = locked(&self.state);
        state.pinned.insert(tag.clone(), id.clone());
        state.images.insert(tag.clone(), id);
    }

    pub fn fail_next(&self, stage: BuildStage) {
        locked(&self.state).failures.push(stage);
    }
}

fn id_of(tag: &ImageTag) -> Result<ImageId, EngineError> {
    let digest = Sha256::of(tag.as_str().as_bytes());
    ImageId::parse(&format!("sha256:{}", digest.as_str())).map_err(|error| {
        EngineError::DigestUnreadable {
            tag: tag.clone(),
            output: TerminalText::sanitize(error.to_string().as_bytes()),
        }
    })
}

#[async_trait]
impl ImageEngine for FakeEngine {
    async fn find(&self, tag: &ImageTag) -> Result<Option<ImageId>, EngineError> {
        Ok(locked(&self.state).images.get(tag).cloned())
    }

    async fn check_builder(&self, _builder: &BuilderName) -> Result<(), EngineError> {
        Ok(())
    }

    async fn build(
        &self,
        build: &PlannedBuild,
        _progress: &ProgressSink,
        _cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError> {
        let mut state = locked(&self.state);
        if let Some(index) = state
            .failures
            .iter()
            .position(|stage| *stage == build.stage)
        {
            state.failures.remove(index);
            return Err(EngineError::BuildFailed {
                stage: build.stage,
                status: ProcessStatus::Exited(1),
                tail: TerminalText::sanitize(b"the fake engine was told to fail this build"),
            });
        }
        let id = match state.pinned.get(&build.tag) {
            Some(id) => id.clone(),
            None => id_of(&build.tag)?,
        };
        state.images.insert(build.tag.clone(), id.clone());
        state.builds.push(BuildRecord {
            stage: build.stage,
            tag: build.tag.clone(),
        });
        Ok(id)
    }
}
