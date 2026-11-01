// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ImageId, ImageTag, TerminalText};

use crate::plan::BuildStage;

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
