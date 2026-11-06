// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_console::world::WallSource;
use jackin_core::Clock;

#[derive(Debug)]
pub struct SystemWall {
    clock: Arc<dyn Clock>,
}

impl SystemWall {
    pub fn new(clock: Arc<dyn Clock>) -> SystemWall {
        SystemWall { clock }
    }
}

impl WallSource for SystemWall {
    fn epoch_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            })
    }

    fn utc_offset_secs(&self) -> i64 {
        i64::from(self.clock.utc_offset_secs())
    }
}
