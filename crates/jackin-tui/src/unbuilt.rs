// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{Completion, Effect};
use jackin_console::domain::vocabulary::{Unbuilt, Vocabulary};
use jackin_console::world::Msg;

pub(crate) fn fails(effect: &Effect, words: &Vocabulary) -> Option<Completion> {
    let no = |what: Unbuilt| words.unbuilt(what);
    let msg = match effect {
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(no(Unbuilt::RestoreSandbox)),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(no(Unbuilt::PrewarmImage)),
        },
        _ => return None,
    };
    Some(Completion::ready(0, msg))
}
