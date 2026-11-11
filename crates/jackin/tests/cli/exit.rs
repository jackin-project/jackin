// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;

#[test]
fn every_exit_class_has_its_documented_number() {
    let table = [
        (ExitClass::Ok, 0_u8),
        (ExitClass::Failure, 1),
        (ExitClass::Usage, 2),
        (ExitClass::Refused, 3),
        (ExitClass::Cancelled, 130),
    ];
    for (class, code) in table {
        assert_eq!(class.code(), code, "{class:?}");
    }
}
