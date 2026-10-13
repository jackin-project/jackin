// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn admitted_present_refuses_replaced_target() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("sockets");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::rename(&target, root.join("moved")).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("moved").join("owned")).unwrap(),
        "owned"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}
