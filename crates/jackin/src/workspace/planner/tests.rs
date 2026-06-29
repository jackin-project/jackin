//! Tests for `planner`.
use super::*;
use crate::workspace::{MountConfig, WorkspaceConfig, covers};

fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: dst.to_owned(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    }
}

fn workspace(workdir: &str, mounts: Vec<MountConfig>) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: workdir.to_owned(),
        mounts,
        ..Default::default()
    }
}

#[test]
fn plan_create_does_not_insert_workdir_auto_mount() {
    let plan = plan_create(&[mount("/data", "/data")]).unwrap();

    assert_eq!(plan.final_mounts.len(), 1);
    assert_eq!(plan.final_mounts[0].dst, "/data");
    assert!(plan.collapsed.is_empty());
}

#[test]
fn plan_create_preserves_explicit_workdir_mount() {
    let plan = plan_create(&[mount("/custom-src", "/work")]).unwrap();

    assert_eq!(plan.final_mounts.len(), 1);
    assert_eq!(plan.final_mounts[0].src, "/custom-src");
    assert_eq!(plan.final_mounts[0].dst, "/work");
    assert!(plan.collapsed.is_empty());
}

#[test]
fn plan_create_collapses_redundant_children_under_parent() {
    let plan = plan_create(&[mount("/work", "/work"), mount("/work/sub", "/work/sub")]).unwrap();

    assert_eq!(plan.final_mounts.len(), 1);
    assert_eq!(plan.final_mounts[0].dst, "/work");
    assert_eq!(plan.collapsed.len(), 1);
    assert_eq!(plan.collapsed[0].child.dst, "/work/sub");
    assert_eq!(plan.collapsed[0].covered_by.dst, "/work");
}

#[test]
fn plan_edit_classifies_new_parent_as_edit_driven() {
    // Existing: /work/sub. Add: /work (new parent). /work/sub gets
    // subsumed by the new parent — edit-driven.
    let current = workspace("/work", vec![mount("/work/sub", "/work/sub")]);
    let plan = plan_edit(&current, &[mount("/work", "/work")], &[], false).unwrap();

    assert_eq!(plan.edit_driven_collapses.len(), 1);
    assert_eq!(plan.pre_existing_collapses.len(), 0);
    assert_eq!(plan.edit_driven_collapses[0].child.dst, "/work/sub");
    assert!(plan.effective_removals.contains(&"/work/sub".to_owned()));
}

#[test]
fn plan_edit_classifies_untouched_redundancy_as_pre_existing() {
    // Existing config already has /work and /work/sub (redundant).
    // Edit does nothing related to those; collapse reports pre-existing.
    let current = workspace(
        "/work",
        vec![mount("/work", "/work"), mount("/work/sub", "/work/sub")],
    );
    let plan = plan_edit(&current, &[], &[], false).unwrap();

    assert_eq!(plan.edit_driven_collapses.len(), 0);
    assert_eq!(plan.pre_existing_collapses.len(), 1);
    assert_eq!(plan.pre_existing_collapses[0].child.dst, "/work/sub");
}

#[test]
fn plan_edit_applies_remove_destinations_before_upsert() {
    // Remove /work/sub, then add /work. If removes happened *after*
    // upserts, /work would subsume /work/sub and show up as an
    // edit-driven collapse. Because removes come first, nothing collapses.
    let current = workspace("/work", vec![mount("/work/sub", "/work/sub")]);
    let plan = plan_edit(
        &current,
        &[mount("/work", "/work")],
        &["/work/sub".to_owned()],
        false,
    )
    .unwrap();

    assert!(
        plan.edit_driven_collapses.is_empty(),
        "no collapse expected: /work/sub was removed before /work was added"
    );
    assert!(plan.pre_existing_collapses.is_empty());
    assert_eq!(plan.effective_removals, vec!["/work/sub".to_owned()]);
}

#[test]
fn plan_edit_no_workdir_mount_drops_workdir_auto_mount() {
    // Workdir auto-mount is present; also add /work/sub as an upsert.
    // If no_workdir_mount correctly drops /work before collapse, /work/sub
    // has no parent and no collapse fires. If /work survived, /work/sub
    // would collapse under it as edit-driven.
    let current = workspace(
        "/work",
        vec![mount("/work", "/work"), mount("/data", "/data")],
    );
    let plan = plan_edit(&current, &[mount("/work/sub", "/work/sub")], &[], true).unwrap();

    assert!(
        plan.edit_driven_collapses.is_empty(),
        "no_workdir_mount must drop /work before collapse considers it"
    );
    assert!(plan.pre_existing_collapses.is_empty());
}

#[test]
fn plan_edit_effective_removals_stack_on_user_removals() {
    // User asks to remove /extra. Then an edit adds /work that subsumes
    // /work/sub. Both removals should appear in effective_removals.
    let current = workspace(
        "/work",
        vec![mount("/work/sub", "/work/sub"), mount("/extra", "/extra")],
    );
    let plan = plan_edit(
        &current,
        &[mount("/work", "/work")],
        &["/extra".to_owned()],
        false,
    )
    .unwrap();

    assert!(plan.effective_removals.contains(&"/extra".to_owned()));
    assert!(plan.effective_removals.contains(&"/work/sub".to_owned()));
}

#[test]
fn covers_is_false_for_equal_mounts() {
    let a = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let b = a.clone();
    assert!(!covers(&a, &b));
}

#[test]
fn covers_is_true_for_exact_ancestor_with_matching_suffix() {
    let parent = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/a/b".into(),
        dst: "/a/b".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(covers(&parent, &child));
}

#[test]
fn covers_is_true_for_deep_ancestor_with_matching_suffix() {
    let parent = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/a/b/c/d".into(),
        dst: "/a/b/c/d".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(covers(&parent, &child));
}

#[test]
fn covers_is_true_when_src_and_dst_differ_but_offsets_match() {
    let parent = MountConfig {
        src: "/host/root".into(),
        dst: "/container/root".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/host/root/sub".into(),
        dst: "/container/root/sub".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(covers(&parent, &child));
}

#[test]
fn covers_is_false_when_src_nests_but_dst_offsets_differ() {
    let parent = MountConfig {
        src: "/host/root".into(),
        dst: "/container/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/host/root/sub".into(),
        dst: "/container/b/sub".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(!covers(&parent, &child));
}

#[test]
fn covers_is_false_when_src_does_not_nest() {
    let a = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let b = MountConfig {
        src: "/b".into(),
        dst: "/b".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(!covers(&a, &b));
}

#[test]
fn covers_is_false_for_sibling_prefix_match() {
    // `/a-x` is not a child of `/a`, even though "/a-x".starts_with("/a").
    let parent = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/a-x".into(),
        dst: "/a-x".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(!covers(&parent, &child));
}

#[test]
fn covers_normalizes_trailing_slashes() {
    let parent = MountConfig {
        src: "/a/".into(),
        dst: "/a/".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/a/b".into(),
        dst: "/a/b".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(covers(&parent, &child));
}

#[test]
fn covers_handles_different_readonly_flags() {
    // `covers` is purely path-based. Readonly mismatches are caught by plan_collapse.
    let parent = MountConfig {
        src: "/a".into(),
        dst: "/a".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    };
    let child = MountConfig {
        src: "/a/b".into(),
        dst: "/a/b".into(),
        readonly: true,
        isolation: jackin_core::MountIsolation::Shared,
    };
    assert!(covers(&parent, &child));
}

fn mk(src: &str, dst: &str, ro: bool) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: ro,
        isolation: jackin_core::MountIsolation::Shared,
    }
}

#[test]
fn plan_collapse_empty_input_returns_empty_plan() {
    let plan = plan_collapse(&[], &[]).unwrap();
    assert!(plan.kept.is_empty());
    assert!(plan.removed.is_empty());
}

#[test]
fn plan_collapse_preserves_unrelated_mounts() {
    let mounts = vec![mk("/a", "/a", false), mk("/b", "/b", false)];
    let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
    assert_eq!(plan.kept, mounts);
    assert!(plan.removed.is_empty());
}

#[test]
fn plan_collapse_collapses_single_child_under_new_parent() {
    let mounts = vec![
        mk("/a/b", "/a/b", false), // pre-existing child (index 0)
        mk("/a", "/a", false),     // new parent (index 1)
    ];
    let plan = plan_collapse(&mounts, &[1]).unwrap();
    assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
    assert_eq!(plan.removed.len(), 1);
    assert_eq!(plan.removed[0].child, mk("/a/b", "/a/b", false));
    assert_eq!(plan.removed[0].covered_by, mk("/a", "/a", false));
}

#[test]
fn plan_collapse_collapses_multiple_children_under_new_parent() {
    let mounts = vec![
        mk("/a/b", "/a/b", false),
        mk("/a/c", "/a/c", false),
        mk("/a", "/a", false),
    ];
    let plan = plan_collapse(&mounts, &[2]).unwrap();
    assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
    assert_eq!(plan.removed.len(), 2);
}

#[test]
fn plan_collapse_handles_transitive_chain() {
    // A ⊃ B ⊃ C, all present in input; B and C both find A as a parent.
    let mounts = vec![
        mk("/a", "/a", false),
        mk("/a/b", "/a/b", false),
        mk("/a/b/c", "/a/b/c", false),
    ];
    let plan = plan_collapse(&mounts, &[0, 1, 2]).unwrap();
    assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
    assert_eq!(plan.removed.len(), 2);
    // Both B and C are covered by A.
    for rem in &plan.removed {
        assert_eq!(rem.covered_by, mk("/a", "/a", false));
    }
}

#[test]
fn plan_collapse_flags_pre_existing_violation_as_removal_when_nothing_new() {
    // Neither index is in new_indexes — both pre-existing. Library returns
    // Ok with removal; CLI will inspect origin and decide to reject or
    // proceed with --prune.
    let mounts = vec![mk("/a", "/a", false), mk("/a/b", "/a/b", false)];
    let plan = plan_collapse(&mounts, &[]).unwrap();
    assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
    assert_eq!(plan.removed.len(), 1);
}

#[test]
fn plan_collapse_errors_on_readonly_mismatch_rw_parent_ro_child() {
    let mounts = vec![
        mk("/a/b", "/a/b", true), // ro child
        mk("/a", "/a", false),    // rw parent (new)
    ];
    let err = plan_collapse(&mounts, &[1]).unwrap_err();
    match err {
        CollapseError::ReadonlyMismatch { parent, child } => {
            assert_eq!(parent, mk("/a", "/a", false));
            assert_eq!(child, mk("/a/b", "/a/b", true));
        }
        other => panic!("expected ReadonlyMismatch, got {other:?}"),
    }
}

#[test]
fn plan_collapse_errors_on_readonly_mismatch_ro_parent_rw_child() {
    let mounts = vec![
        mk("/a/b", "/a/b", false), // rw child
        mk("/a", "/a", true),      // ro parent (new)
    ];
    let err = plan_collapse(&mounts, &[1]).unwrap_err();
    assert!(matches!(err, CollapseError::ReadonlyMismatch { .. }));
}

#[test]
fn plan_collapse_errors_on_new_child_under_existing_parent() {
    // Parent at index 0 is pre-existing. Child at index 1 is new.
    let mounts = vec![
        mk("/a", "/a", false),     // existing parent
        mk("/a/b", "/a/b", false), // new child
    ];
    let err = plan_collapse(&mounts, &[1]).unwrap_err();
    match err {
        CollapseError::ChildUnderExistingParent { parent, child } => {
            assert_eq!(parent, mk("/a", "/a", false));
            assert_eq!(child, mk("/a/b", "/a/b", false));
        }
        other => panic!("expected ChildUnderExistingParent, got {other:?}"),
    }
}

#[test]
fn plan_collapse_allows_new_child_when_new_parent_is_also_in_same_edit() {
    // Both parent and child introduced in the same edit — not a "child
    // under existing parent" case; child is just redundant and gets
    // collapsed normally.
    let mounts = vec![mk("/a/b", "/a/b", false), mk("/a", "/a", false)];
    let plan = plan_collapse(&mounts, &[0, 1]).unwrap();
    assert_eq!(plan.kept, vec![mk("/a", "/a", false)]);
    assert_eq!(plan.removed.len(), 1);
}

#[test]
fn plan_collapse_error_message_mentions_both_paths() {
    let mounts = vec![mk("/a/b", "/a/b", true), mk("/a", "/a", false)];
    let err = plan_collapse(&mounts, &[1]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("/a"));
    assert!(msg.contains("/a/b"));
    assert!(msg.contains("readonly"));
}

/// Exhaustive check: for any plan produced on a valid (no-error) input,
/// re-planning on `plan.kept` must be a no-op.
#[test]
fn plan_collapse_is_idempotent() {
    let inputs: Vec<Vec<MountConfig>> = vec![
        vec![],
        vec![mk("/a", "/a", false)],
        vec![mk("/a", "/a", false), mk("/b", "/b", false)],
        vec![mk("/a", "/a", false), mk("/a/b", "/a/b", false)],
        vec![
            mk("/a", "/a", false),
            mk("/a/b", "/a/b", false),
            mk("/a/b/c", "/a/b/c", false),
            mk("/x", "/x", true),
        ],
    ];
    for input in inputs {
        let indexes: Vec<usize> = (0..input.len()).collect();
        let plan = plan_collapse(&input, &indexes).unwrap();
        let second = plan_collapse(&plan.kept, &[]).unwrap();
        assert!(
            second.removed.is_empty(),
            "plan.kept should be rule-C compliant, but re-plan removed {} entries",
            second.removed.len(),
        );
        assert_eq!(second.kept, plan.kept);
    }
}

#[test]
fn apply_isolation_overrides_updates_matching_dst() {
    let mut mounts = vec![
        MountConfig {
            src: "/tmp/a".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        },
        MountConfig {
            src: "/tmp/b".into(),
            dst: "/workspace/y".into(),
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        },
    ];
    apply_isolation_overrides(
        &mut mounts,
        &[("/workspace/y".into(), jackin_core::MountIsolation::Worktree)],
    )
    .unwrap();
    assert_eq!(mounts[1].isolation, jackin_core::MountIsolation::Worktree);
    assert_eq!(mounts[0].isolation, jackin_core::MountIsolation::Shared);
}

#[test]
fn apply_isolation_overrides_unknown_dst_errors() {
    let mut mounts = vec![MountConfig {
        src: "/tmp/a".into(),
        dst: "/workspace/x".into(),
        readonly: false,
        isolation: jackin_core::MountIsolation::Shared,
    }];
    let err = apply_isolation_overrides(
        &mut mounts,
        &[("/nope".into(), jackin_core::MountIsolation::Worktree)],
    )
    .unwrap_err();
    assert!(err.to_string().contains("unknown destination `/nope`"));
}

#[test]
fn plan_collapse_result_satisfies_invariant() {
    // After planning, no pair in `kept` covers another pair in `kept`.
    let mounts = vec![
        mk("/a", "/a", false),
        mk("/a/b", "/a/b", false),
        mk("/a/c/d", "/a/c/d", false),
        mk("/x/y", "/x/y", true),
        mk("/x", "/x", true),
    ];
    let indexes: Vec<usize> = (0..mounts.len()).collect();
    let plan = plan_collapse(&mounts, &indexes).unwrap();
    for (i, a) in plan.kept.iter().enumerate() {
        for (j, b) in plan.kept.iter().enumerate() {
            if i != j {
                assert!(
                    !covers(a, b),
                    "invariant violated: {a:?} covers {b:?} in kept set",
                );
            }
        }
    }
}
