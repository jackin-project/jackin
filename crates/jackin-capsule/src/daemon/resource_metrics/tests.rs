// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{parse_stat_cpu_jiffies, parse_status_rss_kib};

#[test]
fn parses_status_rss_kib() {
    let status = "Name:\tjackin-capsule\nVmRSS:\t  143720 kB\nThreads:\t1\n";

    assert_eq!(parse_status_rss_kib(status), Some(143_720));
}

#[test]
fn parses_stat_cpu_jiffies_with_spaces_in_comm() {
    let stat = "123 (jackin capsule) S 1 2 3 4 5 6 7 8 9 10 42 58 14 15";

    assert_eq!(parse_stat_cpu_jiffies(stat), Some(100));
}
