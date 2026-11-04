// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jackin_core::{
    BuilderName, FileLock, HostPath, ImageId, ImageTag, ProcessStatus, Sha256, TerminalText,
};
use jackin_image::{
    BuildStage, EngineError, ImageEngine, ImageEvent, ImageInspect, PlannedBuild, ProgressSink,
};
use tokio::sync::Notify;
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
    crafted: BTreeMap<BuildStageKey, ImageInspect>,
    inspects: BTreeMap<ImageId, ImageInspect>,
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
struct Exclusion {
    state: Mutex<ExclusionState>,
    released: Notify,
}

#[derive(Debug, Default)]
struct ExclusionState {
    held: BTreeSet<ImageTag>,
    waiting: BTreeMap<ImageTag, usize>,
}

impl Exclusion {
    fn take(&self, key: &ImageTag) -> bool {
        locked(&self.state).held.insert(key.clone())
    }

    fn waiters(&self, key: &ImageTag) -> usize {
        locked(&self.state).waiting.get(key).copied().unwrap_or(0)
    }
}

fn lock_file(key: &ImageTag) -> Result<HostPath, EngineError> {
    let digest = Sha256::of(key.to_string().as_bytes());
    let path = Path::new("/fake-engine/locks").join(format!("{}.lock", digest.as_str()));
    HostPath::parse(&path).map_err(|_| EngineError::LockTimeout { key: key.clone() })
}

struct Held {
    exclusion: Arc<Exclusion>,
    key: ImageTag,
}

impl Drop for Held {
    fn drop(&mut self) {
        locked(&self.exclusion.state).held.remove(&self.key);
        self.exclusion.released.notify_waiters();
    }
}

struct Waiting<'a> {
    exclusion: &'a Exclusion,
    key: &'a ImageTag,
}

impl<'a> Waiting<'a> {
    fn enter(exclusion: &'a Exclusion, key: &'a ImageTag) -> Waiting<'a> {
        *locked(&exclusion.state)
            .waiting
            .entry(key.clone())
            .or_insert(0) += 1;
        Waiting { exclusion, key }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut state = locked(&self.exclusion.state);
        if let Some(count) = state.waiting.get_mut(self.key) {
            *count -= 1;
            if *count == 0 {
                state.waiting.remove(self.key);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct FakeEngine {
    state: Mutex<EngineState>,
    exclusion: Arc<Exclusion>,
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
    NewId(ImageId),
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

    pub fn waiters(&self, key: &ImageTag) -> usize {
        self.exclusion.waiters(key)
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

    pub fn craft_inspect(&self, stage: BuildStage, inspect: ImageInspect) {
        locked(&self.state)
            .crafted
            .insert(BuildStageKey::of(stage), inspect);
    }

    pub fn script_lines(&self, stage: BuildStage, lines: Vec<String>) {
        locked(&self.state)
            .lines
            .insert(BuildStageKey::of(stage), lines);
    }
}

fn layer_of(text: &str) -> String {
    format!("sha256:{}", Sha256::of(text.as_bytes()).as_str())
}

pub fn construct_layer() -> String {
    layer_of("the fake construct layer")
}

fn honest_inspect(stage: BuildStage, tag: &ImageTag) -> ImageInspect {
    let mut layers = vec![construct_layer()];
    if stage != BuildStage::Construct {
        layers.push(layer_of(tag.as_str()));
    }
    ImageInspect {
        layers,
        env: vec!["PATH=/usr/local/bin:/usr/bin".to_owned()],
        on_build: Vec::new(),
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
        let mut id = match state.pinned.get(&build.tag) {
            Some(id) => id.clone(),
            None => id_of(&build.tag)?,
        };
        let due = state.take(|moment| {
            matches!(moment, EngineMoment::BuildFinished { stage, .. } if *stage == build.stage)
        });
        let mut edit = None;
        let mut after = None;
        if let Some(EngineMoment::BuildFinished { effect, .. }) = due {
            match effect {
                BuildEffect::Retag(retag) => after = Some(retag),
                BuildEffect::NewId(new) => id = new,
                BuildEffect::Cancel => cancel.cancel(),
                BuildEffect::EditHost(found) => edit = Some(found),
            }
        }
        let inspect = state
            .crafted
            .get(&BuildStageKey::of(build.stage))
            .cloned()
            .unwrap_or_else(|| honest_inspect(build.stage, &build.tag));
        state.inspects.insert(id.clone(), inspect);
        state.images.insert(build.tag.clone(), id.clone());
        state.builds.push(BuildRecord {
            stage: build.stage,
            tag: build.tag.clone(),
        });
        if let Some(retag) = after {
            state.retag(retag);
        }
        Ok((id, edit))
    }
}

#[async_trait]
impl ImageEngine for FakeEngine {
    async fn find(
        &self,
        tag: &ImageTag,
        _cancel: &CancellationToken,
    ) -> Result<Option<ImageId>, EngineError> {
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

    async fn inspect(
        &self,
        id: &ImageId,
        _cancel: &CancellationToken,
    ) -> Result<ImageInspect, EngineError> {
        Ok(locked(&self.state)
            .inspects
            .get(id)
            .cloned()
            .unwrap_or_else(|| ImageInspect {
                layers: vec![construct_layer()],
                env: Vec::new(),
                on_build: Vec::new(),
            }))
    }

    async fn check_builder(
        &self,
        builder: &BuilderName,
        _cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
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

    async fn hold(
        &self,
        key: &ImageTag,
        cancel: &CancellationToken,
    ) -> Result<Option<FileLock>, EngineError> {
        let mut waiting = None;
        loop {
            let released = self.exclusion.released.notified();
            let mut released = std::pin::pin!(released);
            released.as_mut().enable();
            if self.exclusion.take(key) {
                return Ok(Some(FileLock::from_guard(
                    lock_file(key)?,
                    Held {
                        exclusion: Arc::clone(&self.exclusion),
                        key: key.clone(),
                    },
                )));
            }
            waiting.get_or_insert_with(|| Waiting::enter(&self.exclusion, key));
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(EngineError::Cancelled { group: None }),
                () = released => {}
            }
        }
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
