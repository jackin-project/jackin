// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::ffi::OsStrExt;

use super::{os_record, path_record, shell_quote_path};

#[test]
fn path_records_and_shell_quotes_preserve_non_utf8_bytes() {
    let raw_path = b"/tmp/jackin-\xff'checkout";
    let mut record = raw_path.to_vec();
    record.push(b'\n');
    let parsed = path_record(record, false).expect("parse raw Git path record");
    assert_eq!(parsed.as_os_str().as_bytes(), raw_path);

    let quoted = shell_quote_path(&parsed).expect("quote raw checkout path");
    assert_eq!(
        quoted.as_os_str().as_bytes(),
        b"'/tmp/jackin-\xff'\\''checkout'"
    );
    assert_eq!(
        os_record(b"owner-\xff\n")
            .expect("parse raw config value")
            .as_os_str()
            .as_bytes(),
        b"owner-\xff"
    );
}
