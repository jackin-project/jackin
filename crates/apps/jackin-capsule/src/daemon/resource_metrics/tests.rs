// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{CpuJiffies, parse_stat_cpu_jiffies};

#[test]
fn parses_stat_cpu_jiffies_with_spaces_in_comm() {
    let stat = "123 (jackin capsule) S 1 2 3 4 5 6 7 8 9 10 42 58 14 15";

    assert_eq!(
        parse_stat_cpu_jiffies(stat),
        Some(CpuJiffies {
            user: 42,
            system: 58,
        })
    );
}
