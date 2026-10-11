// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Bridged keymap dispatch helper.

use termrock::input::KeyEvent;
use termrock::interaction::{
    InteractionElement, InteractionLayer, InteractionOutcome, InteractionScene, LayerDismissPolicy,
    LayerKind, dispatch_keymap_action,
};
use termrock::keymap::Keymap;

/// Dispatch a key through the upstream keymap bridge.
///
/// Chord resolution comes from `map` exactly as the retired direct
/// `Keymap::dispatch` calls did; the transient single-element scene
/// advertises every action the keymap carries, so the bridge's availability
/// gate passes precisely what the keymap resolves, and context folding
/// stays at the call sites unchanged. `UiIntent` is not bridged: no console
/// keymap matches the intent granularity 1:1 (research ch04), so the
/// product action enums stay the bridge payload everywhere.
pub(crate) fn bridged_keymap_action<A>(map: &Keymap<A>, key: KeyEvent) -> Option<A>
where
    A: Clone + Copy + PartialEq + 'static,
{
    let mut scene: InteractionScene<(), (), A> = InteractionScene::new();
    scene.ensure_root(InteractionLayer {
        id: (),
        kind: LayerKind::Root,
        owns_input: true,
        esc: LayerDismissPolicy::Ignore,
        outside: LayerDismissPolicy::Ignore,
        focus_return: None,
    });
    let actions = map
        .bindings()
        .iter()
        .map(|binding| *binding.action())
        .collect();
    if scene
        .register(
            InteractionElement::control((), (), ratatui::layout::Rect::default()).actions(actions),
        )
        .is_err()
    {
        return None;
    }
    match dispatch_keymap_action(&scene, map, key) {
        InteractionOutcome::Action { action, .. } => Some(action),
        _ => None,
    }
}

// ── Editor global (fired in both tab-bar and content modes) ──────────────────
