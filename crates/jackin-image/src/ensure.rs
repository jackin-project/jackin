// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{BuilderName, ImageId, ImageTag, TerminalText};
use tokio_util::sync::CancellationToken;

use crate::engine::{EngineError, ImageEngine};
use crate::error::ImageError;
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
        EngineError::Cancelled => ImageError::Cancelled,
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
            Err(ImageError::Cancelled)
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
        self.engine.find(tag).await.map_err(from_engine)
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
        source: Option<(&ImageTag, &ImageId)>,
    ) -> Result<ImageId, ImageError> {
        self.live()?;
        if !self.builder_checked {
            self.engine
                .check_builder(self.builder)
                .await
                .map_err(from_engine)?;
            self.builder_checked = true;
        }
        if let Some((tag, expected)) = source {
            self.require(tag, expected).await?;
        }
        self.emit(ImageEvent::StageStarted(build.stage)).await;
        let id = self
            .engine
            .build(build, self.progress, self.cancel)
            .await
            .map_err(from_engine)?;
        if let Some((tag, expected)) = source {
            self.require(tag, expected).await?;
        }
        self.emit(ImageEvent::StageFinished(build.stage)).await;
        Ok(id)
    }
}

pub async fn ensure_image(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> Result<BuiltImage, ImageError> {
    let plan = ImagePlan::new(request)?;
    let mut pipeline = Pipeline {
        engine,
        builder: &request.builder,
        progress,
        cancel,
        builder_checked: false,
    };
    let construct = plan.construct_build();
    let cached = match request.rebuild {
        Rebuild::No => pipeline.find(&construct.tag).await?,
        Rebuild::Yes => None,
    };
    let construct_id = if let Some(id) = cached {
        pipeline
            .emit(ImageEvent::StageCached(BuildStage::Construct))
            .await;
        id
    } else {
        pipeline.build(construct, None).await?
    };
    if request.rebuild == Rebuild::No {
        let tag = plan.final_tag(&construct_id);
        if let Some(id) = pipeline.find(&tag).await? {
            pipeline
                .emit(ImageEvent::StageCached(BuildStage::Derived))
                .await;
            return Ok(BuiltImage {
                tag,
                id,
                source: ImageSource::Cache,
            });
        }
    }
    let builds = plan.final_builds(&construct_id);
    let role = pipeline
        .build(&builds.role, Some((&construct.tag, &construct_id)))
        .await?;
    let built = pipeline
        .build(&builds.derived, Some((&builds.role.tag, &role)))
        .await?;
    let found = pipeline.find(&builds.derived.tag).await?;
    if found.as_ref() != Some(&built) {
        return Err(ImageError::NotDelivered {
            tag: builds.derived.tag,
            built,
            found,
        });
    }
    Ok(BuiltImage {
        tag: builds.derived.tag,
        id: built,
        source: ImageSource::Built,
    })
}
