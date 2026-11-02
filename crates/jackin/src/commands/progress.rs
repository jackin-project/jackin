// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ImageEvent;
use jackin_session::{LaunchEvent, LaunchStep, SkipReason, StepResult};

use crate::output::Io;

pub(crate) fn image_event(io: &mut Io, event: &ImageEvent) {
    match event {
        ImageEvent::StageStarted(stage) => io.note(&format!("image {stage}: building")),
        ImageEvent::StageCached(stage) => io.note(&format!("image {stage}: cached")),
        ImageEvent::Line { text, .. } => io.note(text.as_str()),
        ImageEvent::StageFinished(stage) => io.note(&format!("image {stage}: built")),
    }
}

fn step_name(step: LaunchStep) -> String {
    format!("{step:?}").to_lowercase()
}

fn skip_text(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::ImageCached => "the image is cached",
        SkipReason::ProviderPresent => "the provider exists",
        SkipReason::SandboxExists => "the sandbox exists",
        SkipReason::AlreadyUploaded => "the project is uploaded",
        SkipReason::SetupAlreadyDone => "setup already ran",
        SkipReason::AlreadyRunning => "Codex is running",
    }
}

pub(crate) fn launch_event(io: &mut Io, event: &LaunchEvent) {
    match event {
        LaunchEvent::Warning(_) => {}
        LaunchEvent::StepStarted(step) => io.note(&format!("{}...", step_name(*step))),
        LaunchEvent::StepFinished { step, result } => match result {
            StepResult::Done => io.note(&format!("{}: done", step_name(*step))),
            StepResult::Skipped(reason) => io.note(&format!(
                "{}: skipped, {}",
                step_name(*step),
                skip_text(*reason)
            )),
        },
        LaunchEvent::Image(event) => image_event(io, event),
        LaunchEvent::SandboxKnown(name) => io.note(&format!("sandbox {name}")),
    }
}
