// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn delta_subtracts_snapshot_totals() {
    let before = HeapSnapshot {
        total_blocks: 5,
        total_bytes: 50,
    };
    let after = HeapSnapshot {
        total_blocks: 8,
        total_bytes: 80,
    };
    let delta = HeapDelta {
        blocks: after.total_blocks - before.total_blocks,
        bytes: after.total_bytes - before.total_bytes,
    };

    assert_eq!(
        delta,
        HeapDelta {
            blocks: 3,
            bytes: 30
        }
    );
}
