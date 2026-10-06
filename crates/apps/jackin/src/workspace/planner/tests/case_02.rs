// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn plan_collapse_error_message_mentions_both_paths() {
    let mounts = vec![mk("/a/b", "/a/b", true), mk("/a", "/a", false)];
    let err = plan_collapse(&mounts, &[1]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("/a"));
    assert!(msg.contains("/a/b"));
    assert!(msg.contains("readonly"));
}

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
