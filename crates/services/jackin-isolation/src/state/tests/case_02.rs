// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn cooperating_mutation_holds_lock_through_read_modify_write() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("jk-a1b2c3d4-role");
    write_records(&state, &[sample_record()]).unwrap();
    let (read_ready, wait_read) = std::sync::mpsc::channel();
    let (continue_write, wait_continue) = std::sync::mpsc::channel();
    let first_state = state.clone();
    let first = std::thread::spawn(move || {
        mutate_records(&first_state, false, |records| {
            read_ready.send(()).unwrap();
            wait_continue.recv().unwrap();
            let mut addition = sample_record();
            addition.mount_dst = "/workspace/first".into();
            records.push(addition);
            true
        })
    });
    wait_read
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let mut second = sample_record();
    second.mount_dst = "/workspace/second".into();
    let error = upsert_record(&state, second.clone()).unwrap_err();
    assert!(error.to_string().contains("state busy"));
    continue_write.send(()).unwrap();
    first.join().unwrap().unwrap();
    upsert_record(&state, second).unwrap();
    let records = read_records(&state).unwrap();
    assert_eq!(records.len(), 3);
    assert!(
        records
            .iter()
            .any(|record| record.mount_dst == "/workspace/first")
    );
    assert!(
        records
            .iter()
            .any(|record| record.mount_dst == "/workspace/second")
    );
}

#[test]
fn reading_fresh_state_directory_creates_no_metadata() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir(temp.path().join(".jackin")).unwrap();
    assert!(read_records(temp.path()).unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(temp.path().join(".jackin"))
            .unwrap()
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn v2_state_reads_succeed_without_write_permission() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    write_records(temp.path(), &[sample_record()]).unwrap();
    let file = isolation_file_path(temp.path());
    let directory = temp.path().join(".jackin");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = read_records(temp.path());
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(result.unwrap(), vec![sample_record()]);
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
}
