// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ImageEvent;

use crate::output::Io;

pub(crate) fn image_event(io: &mut Io, event: &ImageEvent) {
    match event {
        ImageEvent::StageStarted(stage) => io.note(&format!("image {stage}: building")),
        ImageEvent::StageCached(stage) => io.note(&format!("image {stage}: cached")),
        ImageEvent::Line { text, .. } => io.note(text.as_str()),
        ImageEvent::StageFinished(stage) => io.note(&format!("image {stage}: built")),
    }
}
