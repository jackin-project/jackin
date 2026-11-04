// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{BuilderName, FileLock, ImageId, ImageTag, TerminalText};
use tokio_util::sync::CancellationToken;

use crate::engine::{EngineError, ImageEngine};
use crate::error::ImageError;
use crate::inspect::{ImageInspect, verify_image};
use crate::plan::{BuildStage, ImagePlan, PlannedBuild};
use crate::request::{ImageRequest, Rebuild};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageSource {
    Cache,
    Built,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltImage {
    pub tag: ImageTag,
    pub id: ImageId,
    pub source: ImageSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageEvent {
    StageStarted(BuildStage),
    StageCached(BuildStage),
    Line {
        stage: BuildStage,
        text: TerminalText,
    },
    StageFinished(BuildStage),
}

pub type ProgressSink = tokio::sync::mpsc::Sender<ImageEvent>;

fn from_engine(error: EngineError) -> ImageError {
    match error {
        EngineError::Cancelled { group } => ImageError::Cancelled { group },
        EngineError::LockTimeout { key } => ImageError::LockTimeout { key },
        other => ImageError::Engine(other),
    }
}

struct Pipeline<'a> {
    engine: &'a dyn ImageEngine,
    builder: &'a BuilderName,
    progress: &'a ProgressSink,
    cancel: &'a CancellationToken,
    builder_checked: bool,
}

impl Pipeline<'_> {
    fn live(&self) -> Result<(), ImageError> {
        if self.cancel.is_cancelled() {
            Err(ImageError::Cancelled { group: None })
        } else {
            Ok(())
        }
    }

    async fn emit(&self, event: ImageEvent) {
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => {}
            sent = self.progress.send(event) => drop(sent),
        }
    }

    async fn find(&self, tag: &ImageTag) -> Result<Option<ImageId>, ImageError> {
        self.live()?;
        self.engine
            .find(tag, self.cancel)
            .await
            .map_err(from_engine)
    }

    async fn cached_construct(&self, tag: &ImageTag) -> Result<Option<ImageId>, ImageError> {
        let Some(found) = self.find(tag).await? else {
            return Ok(None);
        };
        self.live()?;
        let attested = self
            .engine
            .attested(tag, self.cancel)
            .await
            .map_err(from_engine)?;
        Ok((attested.as_ref() == Some(&found)).then_some(found))
    }

    async fn attest(&self, tag: &ImageTag, id: &ImageId) -> Result<(), ImageError> {
        self.live()?;
        self.engine
            .attest(tag, id, self.cancel)
            .await
            .map_err(from_engine)
    }

    async fn inspect(&self, id: &ImageId) -> Result<ImageInspect, ImageError> {
        self.live()?;
        self.engine
            .inspect(id, self.cancel)
            .await
            .map_err(from_engine)
    }

    async fn verify(
        &self,
        stage: BuildStage,
        id: &ImageId,
        construct: &ImageInspect,
    ) -> Result<(), ImageError> {
        let found = self.inspect(id).await?;
        verify_image(construct, &found).map_err(|rule| ImageError::Rule {
            stage,
            image: id.clone(),
            rule,
        })
    }

    async fn hold(&self, key: &ImageTag) -> Result<Option<FileLock>, ImageError> {
        self.live()?;
        self.engine
            .hold(key, self.cancel)
            .await
            .map_err(from_engine)
    }

    async fn require(&self, tag: &ImageTag, expected: &ImageId) -> Result<(), ImageError> {
        let found = self.find(tag).await?;
        if found.as_ref() == Some(expected) {
            Ok(())
        } else {
            Err(ImageError::SourceMoved {
                tag: tag.clone(),
                expected: expected.clone(),
                found,
            })
        }
    }

    async fn build(
        &mut self,
        build: &PlannedBuild,
        sources: &[(&ImageTag, &ImageId)],
    ) -> Result<ImageId, ImageError> {
        self.live()?;
        if !self.builder_checked {
            self.engine
                .check_builder(self.builder, self.cancel)
                .await
                .map_err(from_engine)?;
            self.builder_checked = true;
        }
        for (tag, expected) in sources {
            self.require(tag, expected).await?;
        }
        self.emit(ImageEvent::StageStarted(build.stage)).await;
        let id = self
            .engine
            .build(build, self.progress, self.cancel)
            .await
            .map_err(from_engine)?;
        for (tag, expected) in sources {
            self.require(tag, expected).await?;
        }
        self.emit(ImageEvent::StageFinished(build.stage)).await;
        Ok(id)
    }
}

#[derive(Debug)]
pub struct ImageHold {
    _lock: Option<FileLock>,
}

#[derive(Debug)]
pub struct HeldImage {
    pub image: BuiltImage,
    pub hold: ImageHold,
}

pub async fn ensure_image(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> Result<BuiltImage, ImageError> {
    ensure_held(engine, request, progress, cancel)
        .await
        .map(|held| held.image)
}

pub async fn ensure_held(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> Result<HeldImage, ImageError> {
    let plan = ImagePlan::new(request)?;
    let mut pipeline = Pipeline {
        engine,
        builder: &request.builder,
        progress,
        cancel,
        builder_checked: false,
    };
    let construct = plan.construct_build();
    let mut cached = match request.rebuild {
        Rebuild::No => pipeline.cached_construct(&construct.tag).await?,
        Rebuild::Yes => None,
    };
    let mut construct_lock = None;
    if cached.is_none() {
        construct_lock = pipeline.hold(&construct.tag).await?;
        if request.rebuild == Rebuild::No {
            cached = pipeline.cached_construct(&construct.tag).await?;
        }
    }
    let construct_id = if let Some(id) = cached {
        pipeline
            .emit(ImageEvent::StageCached(BuildStage::Construct))
            .await;
        id
    } else {
        let built = pipeline.build(construct, &[]).await?;
        pipeline.attest(&construct.tag, &built).await?;
        built
    };
    drop(construct_lock);
    let final_tag = plan.final_tag(&construct_id);
    let hold = pipeline.hold(&final_tag).await?;
    if request.rebuild == Rebuild::No
        && let Some(id) = pipeline.find(&final_tag).await?
    {
        let construct_image = pipeline.inspect(&construct_id).await?;
        pipeline
            .verify(BuildStage::Derived, &id, &construct_image)
            .await?;
        pipeline
            .emit(ImageEvent::StageCached(BuildStage::Derived))
            .await;
        return Ok(HeldImage {
            image: BuiltImage {
                tag: final_tag,
                id,
                source: ImageSource::Cache,
            },
            hold: ImageHold { _lock: hold },
        });
    }
    let builds = plan.final_builds(&construct_id);
    let _reading = pipeline.hold(&construct.tag).await?;
    let role = pipeline
        .build(&builds.role, &[(&construct.tag, &construct_id)])
        .await?;
    let construct_image = pipeline.inspect(&construct_id).await?;
    pipeline
        .verify(BuildStage::Role, &role, &construct_image)
        .await?;
    let built = pipeline
        .build(
            &builds.derived,
            &[(&builds.role.tag, &role), (&construct.tag, &construct_id)],
        )
        .await?;
    let found = pipeline.find(&builds.derived.tag).await?;
    if found.as_ref() != Some(&built) {
        return Err(ImageError::NotDelivered {
            tag: builds.derived.tag,
            built,
            found,
        });
    }
    pipeline
        .verify(BuildStage::Derived, &built, &construct_image)
        .await?;
    Ok(HeldImage {
        image: BuiltImage {
            tag: builds.derived.tag,
            id: built,
            source: ImageSource::Built,
        },
        hold: ImageHold { _lock: hold },
    })
}
