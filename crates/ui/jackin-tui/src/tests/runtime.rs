// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::runtime::{Dirty, UpdateResult};

#[test]
fn clean_update_reports_no_work() {
    let update: UpdateResult<()> = UpdateResult::clean();
    assert!(!update.is_dirty());
    assert_eq!(update.dirty(), Dirty::Clean);
    assert!(update.effects().is_empty());
}

#[test]
fn redraw_update_reports_dirty_without_effects() {
    let update: UpdateResult<()> = UpdateResult::redraw();
    assert!(update.is_dirty());
    assert!(update.effects().is_empty());
}

#[test]
fn effect_update_is_dirty_and_carries_effect() {
    let update = UpdateResult::with_effect("reload");
    assert!(update.is_dirty());
    assert_eq!(update.effects(), ["reload"]);
}

#[test]
fn merged_updates_union_dirt_and_effects() {
    let merged = UpdateResult::with_effect("one").merge(UpdateResult::<&str>::clean());
    assert!(merged.is_dirty());
    assert_eq!(merged.effects(), ["one"]);
    let merged = UpdateResult::<&str>::clean().merge(UpdateResult::clean());
    assert!(!merged.is_dirty());
}

#[test]
fn dirty_merge_is_redraw_when_either_side_is_dirty() {
    assert_eq!(Dirty::Clean.merge(Dirty::Clean), Dirty::Clean);
    assert_eq!(Dirty::Redraw.merge(Dirty::Clean), Dirty::Redraw);
    assert_eq!(Dirty::Clean.merge(Dirty::Redraw), Dirty::Redraw);
}
