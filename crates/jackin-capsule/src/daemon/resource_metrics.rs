use std::{fs, time::Instant};

use super::Multiplexer;

const LINUX_USER_HZ: f64 = 100.0;

#[derive(Clone, Copy, Debug)]
struct ResourceSample {
    at: Instant,
    rss_kib: u64,
    cpu_jiffies: u64,
}

#[derive(Debug, Default)]
pub(super) struct ResourceMetricsSampler {
    previous: Option<ResourceSample>,
}

impl ResourceMetricsSampler {
    fn sample() -> Option<ResourceSample> {
        let rss_kib = read_rss_kib()?;
        let cpu_jiffies = read_cpu_jiffies()?;
        Some(ResourceSample {
            at: Instant::now(),
            rss_kib,
            cpu_jiffies,
        })
    }

    fn record(&mut self) -> Option<(ResourceSample, Option<f64>)> {
        let sample = Self::sample()?;
        let cpu_percent = self.previous.and_then(|previous| {
            let elapsed = sample.at.duration_since(previous.at).as_secs_f64();
            if elapsed <= f64::EPSILON {
                return None;
            }
            let delta = sample.cpu_jiffies.saturating_sub(previous.cpu_jiffies);
            Some((delta as f64 / clock_ticks_per_second()) / elapsed * 100.0)
        });
        self.previous = Some(sample);
        Some((sample, cpu_percent))
    }
}

impl Multiplexer {
    pub(super) fn log_resource_metrics(&mut self) {
        if !crate::logging::debug_enabled() {
            return;
        }
        let session_count = self.sessions.len();
        let tab_count = self.tabs.len();
        let visible_panes = self.visible_pane_count();
        let pending_render = self.has_pending_render();
        let Some((sample, cpu_percent)) = self.resource_metrics.record() else {
            crate::cdebug!(
                "resource: sample unavailable sessions={} tabs={} panes={} pending_render={}",
                session_count,
                tab_count,
                visible_panes,
                pending_render
            );
            return;
        };
        let cpu_percent =
            cpu_percent.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}"));
        crate::cdebug!(
            "resource: sessions={} tabs={} panes={} pending_render={} rss_kib={} cpu_jiffies={} cpu_percent_estimate={}",
            session_count,
            tab_count,
            visible_panes,
            pending_render,
            sample.rss_kib,
            sample.cpu_jiffies,
            cpu_percent
        );
    }
}

fn read_rss_kib() -> Option<u64> {
    parse_status_rss_kib(&fs::read_to_string("/proc/self/status").ok()?)
}

fn read_cpu_jiffies() -> Option<u64> {
    parse_stat_cpu_jiffies(&fs::read_to_string("/proc/self/stat").ok()?)
}

fn clock_ticks_per_second() -> f64 {
    // Linux containers use USER_HZ=100 for `/proc/<pid>/stat` CPU accounting
    // on the platforms jackin-capsule targets. The raw `cpu_jiffies` value is
    // logged too, so this debug-only percentage remains an operator aid rather
    // than the source of truth.
    LINUX_USER_HZ
}

fn parse_status_rss_kib(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmRSS:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

fn parse_stat_cpu_jiffies(stat: &str) -> Option<u64> {
    let end_comm = stat.rfind(')')?;
    // Fields after `comm` are 1-based in proc(5); utime is field 14 and stime 15,
    // i.e. indices 11 and 12 of the post-`)` whitespace split. Walk the iterator
    // once instead of materializing every field into a vec.
    let mut fields = stat[end_comm + 1..].split_whitespace();
    let utime = fields.nth(11)?.parse::<u64>().ok()?;
    let stime = fields.next()?.parse::<u64>().ok()?;
    Some(utime.saturating_add(stime))
}

#[cfg(test)]
mod tests {
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
}
