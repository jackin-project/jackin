// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use jackin_core::{BuilderName, ImageId, ImageTag, ProcessStatus, Sha256, TerminalText};
use jackin_image::{BuildStage, EngineError, ImageEngine, ImageEvent, PlannedBuild, ProgressSink};
use tokio_util::sync::CancellationToken;

use crate::gateway::{HostEdit, apply_host_edit};
use crate::sync::locked;

#[derive(Debug, Default)]
struct EngineState {
    images: BTreeMap<ImageTag, ImageId>,
    pinned: BTreeMap<ImageTag, ImageId>,
    builds: Vec<BuildRecord>,
    failures: Vec<BuildStage>,
    script: Vec<EngineMoment>,
    lines: BTreeMap<BuildStageKey, Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BuildStageKey(u8);

impl BuildStageKey {
    fn of(stage: BuildStage) -> BuildStageKey {
        BuildStageKey(match stage {
            BuildStage::Construct => 0,
            BuildStage::Role => 1,
            BuildStage::Derived => 2,
        })
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retag {
    pub tag: ImageTag,
    pub to: Option<ImageId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildEffect {
    Retag(Retag),
    Cancel,
    EditHost(HostEdit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineMoment {
    AfterFind {
        tag: ImageTag,
        retag: Retag,
    },
    FindFails(ImageTag),
    CheckBuilderFails,
    BuildFinished {
        stage: BuildStage,
        effect: BuildEffect,
    },
}

impl EngineState {
    fn take(&mut self, due: impl Fn(&EngineMoment) -> bool) -> Option<EngineMoment> {
        let index = self.script.iter().position(due)?;
        Some(self.script.remove(index))
    }

    fn retag(&mut self, retag: Retag) {
        match retag.to {
            Some(id) => {
                self.images.insert(retag.tag, id);
            }
            None => {
                self.images.remove(&retag.tag);
            }
        }
    }
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

    pub fn at(&self, moment: EngineMoment) {
        locked(&self.state).script.push(moment);
    }

    pub fn script_lines(&self, stage: BuildStage, lines: Vec<String>) {
        locked(&self.state)
            .lines
            .insert(BuildStageKey::of(stage), lines);
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

impl FakeEngine {
    fn begin(&self, build: &PlannedBuild) -> Result<Vec<String>, EngineError> {
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
        Ok(state
            .lines
            .get(&BuildStageKey::of(build.stage))
            .cloned()
            .unwrap_or_default())
    }

    fn finish(
        &self,
        build: &PlannedBuild,
        cancel: &CancellationToken,
    ) -> Result<(ImageId, Option<HostEdit>), EngineError> {
        let mut state = locked(&self.state);
        let id = match state.pinned.get(&build.tag) {
            Some(id) => id.clone(),
            None => id_of(&build.tag)?,
        };
        state.images.insert(build.tag.clone(), id.clone());
        state.builds.push(BuildRecord {
            stage: build.stage,
            tag: build.tag.clone(),
        });
        let due = state.take(|moment| {
            matches!(moment, EngineMoment::BuildFinished { stage, .. } if *stage == build.stage)
        });
        let mut edit = None;
        if let Some(EngineMoment::BuildFinished { effect, .. }) = due {
            match effect {
                BuildEffect::Retag(retag) => state.retag(retag),
                BuildEffect::Cancel => cancel.cancel(),
                BuildEffect::EditHost(found) => edit = Some(found),
            }
        }
        Ok((id, edit))
    }
}

#[async_trait]
impl ImageEngine for FakeEngine {
    async fn find(&self, tag: &ImageTag) -> Result<Option<ImageId>, EngineError> {
        let mut state = locked(&self.state);
        let fails = state
            .take(|moment| matches!(moment, EngineMoment::FindFails(found) if found == tag))
            .is_some();
        if fails {
            return Err(EngineError::DigestUnreadable {
                tag: tag.clone(),
                output: TerminalText::sanitize(b"the fake engine was told to fail this find"),
            });
        }
        let answer = state.images.get(tag).cloned();
        let due = state.take(
            |moment| matches!(moment, EngineMoment::AfterFind { tag: found, .. } if found == tag),
        );
        if let Some(EngineMoment::AfterFind { retag, .. }) = due {
            state.retag(retag);
        }
        Ok(answer)
    }

    async fn check_builder(&self, builder: &BuilderName) -> Result<(), EngineError> {
        let fails = locked(&self.state)
            .take(|moment| matches!(moment, EngineMoment::CheckBuilderFails))
            .is_some();
        if fails {
            return Err(EngineError::BuilderUnreadable {
                builder: builder.clone(),
                status: ProcessStatus::Exited(1),
                output: TerminalText::sanitize(
                    b"the fake engine was told to fail the builder check",
                ),
            });
        }
        Ok(())
    }

    async fn build(
        &self,
        build: &PlannedBuild,
        progress: &ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError> {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled { group: None });
        }
        for line in self.begin(build)? {
            drop(
                progress
                    .send(ImageEvent::Line {
                        stage: build.stage,
                        text: TerminalText::sanitize(line.as_bytes()),
                    })
                    .await,
            );
        }
        let (id, edit) = self.finish(build, cancel)?;
        if let Some(edit) = edit {
            apply_host_edit(&edit)
                .await
                .map_err(|error| EngineError::BuildFailed {
                    stage: build.stage,
                    status: ProcessStatus::Exited(1),
                    tail: TerminalText::sanitize(error.to_string().as_bytes()),
                })?;
        }
        Ok(id)
    }
}
