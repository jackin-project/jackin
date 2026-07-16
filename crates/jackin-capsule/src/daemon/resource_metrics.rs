// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{fs, io, time::Instant};

use jackin_telemetry::{
    Attr, ResultTelemetryExt, Value, counter, gauge,
    metric::{PROCESS_CPU_TIME, PROCESS_UPTIME},
    schema::{attrs, enums::ErrorType},
};
use tokio::task::JoinHandle;

use super::Multiplexer;

const LINUX_USER_HZ: u64 = 100;

#[derive(Clone, Copy, Debug)]
struct ResourceSample {
    at: Instant,
    cpu: CpuJiffies,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CpuJiffies {
    user: u64,
    system: u64,
}

#[derive(Clone, Copy, Debug)]
struct ResourceObservation {
    uptime_seconds: f64,
    user_cpu_seconds: u64,
    system_cpu_seconds: u64,
}

#[derive(Debug)]
pub(crate) struct ResourceMetricsSampler {
    started_at: Instant,
    previous: Option<ResourceSample>,
    user_remainder: u64,
    system_remainder: u64,
    pending: Option<JoinHandle<io::Result<ResourceSample>>>,
}

impl Default for ResourceMetricsSampler {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            previous: None,
            user_remainder: 0,
            system_remainder: 0,
            pending: None,
        }
    }
}

impl ResourceMetricsSampler {
    fn sample() -> io::Result<ResourceSample> {
        Ok(ResourceSample {
            at: Instant::now(),
            cpu: read_cpu_jiffies()?,
        })
    }

    fn start(&mut self) {
        if self.pending.is_some() {
            return;
        }
        self.pending = Some(jackin_telemetry::spawn::joined_blocking(Self::sample));
    }

    async fn poll(&mut self) -> Option<Result<ResourceObservation, ErrorType>> {
        let task = self.pending.as_ref()?;
        if !task.is_finished() {
            return None;
        }
        let task = self.pending.take()?;
        let sample = match task.await {
            Ok(Ok(sample)) => sample,
            Ok(Err(_)) => return Some(Err(ErrorType::IoError)),
            Err(_) => return Some(Err(ErrorType::Panic)),
        };
        let previous = self.previous.replace(sample);
        let user_delta = previous.map_or(sample.cpu.user, |previous| {
            sample.cpu.user.saturating_sub(previous.cpu.user)
        });
        let system_delta = previous.map_or(sample.cpu.system, |previous| {
            sample.cpu.system.saturating_sub(previous.cpu.system)
        });
        let user_total = self.user_remainder.saturating_add(user_delta);
        let system_total = self.system_remainder.saturating_add(system_delta);
        let ticks_per_second = clock_ticks_per_second();
        self.user_remainder = user_total % ticks_per_second;
        self.system_remainder = system_total % ticks_per_second;
        Some(Ok(ResourceObservation {
            uptime_seconds: sample.at.duration_since(self.started_at).as_secs_f64(),
            user_cpu_seconds: user_total / ticks_per_second,
            system_cpu_seconds: system_total / ticks_per_second,
        }))
    }
}

impl Multiplexer {
    pub(super) async fn record_resource_metrics(&mut self) {
        match self.resource_metrics.poll().await {
            Some(Ok(observation)) => {
                gauge(&PROCESS_UPTIME)
                    .record(observation.uptime_seconds, &[])
                    .unwrap_or(());
                let user = [Attr {
                    key: attrs::std_attrs::CPU_MODE,
                    value: Value::Str("user"),
                }];
                let system = [Attr {
                    key: attrs::std_attrs::CPU_MODE,
                    value: Value::Str("system"),
                }];
                counter(&PROCESS_CPU_TIME)
                    .add(observation.user_cpu_seconds, &user)
                    .unwrap_or(());
                counter(&PROCESS_CPU_TIME)
                    .add(observation.system_cpu_seconds, &system)
                    .unwrap_or(());
            }
            Some(Err(error_type)) => {
                Err::<(), _>(())
                    .record_telemetry_error(error_type)
                    .unwrap_or(());
            }
            None => {}
        }
        self.resource_metrics.start();
    }
}

fn read_cpu_jiffies() -> io::Result<CpuJiffies> {
    parse_stat_cpu_jiffies(&fs::read_to_string("/proc/self/stat")?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "CPU time is unavailable"))
}

const fn clock_ticks_per_second() -> u64 {
    // Linux containers use USER_HZ=100 for `/proc/<pid>/stat` CPU accounting
    // on the platforms jackin-capsule targets. Deltas retain sub-second
    // remainders before export as cumulative seconds.
    LINUX_USER_HZ
}

fn parse_stat_cpu_jiffies(stat: &str) -> Option<CpuJiffies> {
    let end_comm = stat.rfind(')')?;
    // Fields after `comm` are 1-based in proc(5); utime is field 14 and stime 15,
    // i.e. indices 11 and 12 of the post-`)` whitespace split. Walk the iterator
    // once instead of materializing every field into a vec.
    let mut fields = stat[end_comm + 1..].split_whitespace();
    let utime = fields.nth(11)?.parse::<u64>().ok()?;
    let stime = fields.next()?.parse::<u64>().ok()?;
    Some(CpuJiffies {
        user: utime,
        system: stime,
    })
}

#[cfg(test)]
mod tests;
