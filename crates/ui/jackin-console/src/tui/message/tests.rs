// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn mount_info_refresh_source_plan_dedups_manager_sources() {
    let plan = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::ManagerList {
        current_dir: "/home/user/work".to_owned(),
        workspace_mount_sources: vec!["/home/user/shared".to_owned(), "/home/user/work".to_owned()],
        global_mount_sources: vec![
            "/home/user/global".to_owned(),
            "/home/user/shared".to_owned(),
        ],
    })
    .expect("manager list has sources");
    assert_eq!(plan.target, MountInfoRefreshTarget::ManagerList);
    assert_eq!(
        plan.sources,
        vec![
            "/home/user/global".to_owned(),
            "/home/user/shared".to_owned(),
            "/home/user/work".to_owned(),
        ]
    );
}

#[test]
fn mount_info_refresh_source_plan_keeps_screen_target() {
    let plan = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Editor {
        mount_sources: vec!["/srv/data".to_owned(), "/srv/data".to_owned()],
    })
    .expect("editor has sources");
    assert_eq!(
        plan,
        MountInfoRefreshSourcePlan {
            target: MountInfoRefreshTarget::Editor,
            sources: vec!["/srv/data".to_owned()],
        }
    );
    let plan = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::SettingsMounts {
        mount_sources: vec!["/srv/global".to_owned()],
    })
    .expect("settings mounts have sources");
    assert_eq!(
        plan,
        MountInfoRefreshSourcePlan {
            target: MountInfoRefreshTarget::SettingsMounts,
            sources: vec!["/srv/global".to_owned()],
        }
    );
    assert!(mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Inactive).is_none());
    assert!(
        mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Editor {
            mount_sources: Vec::new(),
        })
        .is_none()
    );
}
