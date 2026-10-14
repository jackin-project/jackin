// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::KeyEvent;
use termrock::interaction::{
    InteractionElement, InteractionLayer, InteractionOutcome, InteractionScene, LayerDismissPolicy,
    LayerKind, dispatch_keymap_action,
};
use termrock::keymap::Keymap;

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
