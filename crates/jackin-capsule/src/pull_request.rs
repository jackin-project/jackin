//! Pull-request context snapshots shared by daemon, title, and TUI rendering.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub is_draft: bool,
    pub checks: Option<PullRequestChecks>,
}

impl PullRequestInfo {
    pub fn number_label(&self) -> String {
        format!("#{}", self.number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullRequestChecks {
    passing: usize,
    failing: usize,
    pending: usize,
    skipped: usize,
    cancelled: usize,
    total: usize,
}

impl PullRequestChecks {
    /// Build a check rollup from `gh pr checks` bucket strings.
    /// Unknown buckets count toward `skipped` so the
    /// `passing + failing + pending + skipped + cancelled == total`
    /// invariant always holds; renderers can trust the counters.
    pub fn from_buckets<I, S>(buckets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut checks = Self::default();
        for bucket in buckets {
            checks.total += 1;
            match bucket.as_ref() {
                "pass" => checks.passing += 1,
                "fail" => checks.failing += 1,
                "pending" => checks.pending += 1,
                "skipping" => checks.skipped += 1,
                "cancel" => checks.cancelled += 1,
                _ => checks.skipped += 1,
            }
        }
        debug_assert_eq!(
            checks.passing + checks.failing + checks.pending + checks.skipped + checks.cancelled,
            checks.total,
            "PullRequestChecks counters must sum to total"
        );
        checks
    }

    #[cfg(test)]
    pub fn passing(&self) -> usize {
        self.passing
    }
    #[cfg(test)]
    pub fn failing(&self) -> usize {
        self.failing
    }
    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.pending
    }
    #[cfg(test)]
    pub fn skipped(&self) -> usize {
        self.skipped
    }
    #[cfg(test)]
    pub fn cancelled(&self) -> usize {
        self.cancelled
    }
    #[cfg(test)]
    pub fn total(&self) -> usize {
        self.total
    }

    pub fn summary(&self) -> String {
        if self.total == 0 {
            return "(none)".to_string();
        }
        if self.failing > 0 {
            return format!(
                "failing ({} fail, {} pass, {} pending)",
                self.failing, self.passing, self.pending
            );
        }
        if self.cancelled > 0 {
            return format!(
                "cancelled ({} cancel, {} pass, {} pending)",
                self.cancelled, self.passing, self.pending
            );
        }
        if self.pending > 0 {
            return format!("pending ({} pending, {} pass)", self.pending, self.passing);
        }
        if self.passing == self.total || self.passing + self.skipped == self.total {
            return format!("passing ({}/{})", self.passing, self.total);
        }
        format!(
            "{} pass, {} skip, {} total",
            self.passing, self.skipped, self.total
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PullRequestChecks;

    #[test]
    fn pull_request_checks_from_buckets_keeps_sum_equals_total_for_known_inputs() {
        let checks = PullRequestChecks::from_buckets([
            "pass", "pass", "fail", "pending", "skipping", "cancel",
        ]);
        assert_eq!(checks.total(), 6);
        assert_eq!(checks.passing(), 2);
        assert_eq!(checks.failing(), 1);
        assert_eq!(checks.pending(), 1);
        assert_eq!(checks.skipped(), 1);
        assert_eq!(checks.cancelled(), 1);
    }

    #[test]
    fn pull_request_checks_from_buckets_routes_unknown_into_skipped() {
        let checks = PullRequestChecks::from_buckets(["pass", "unknown-bucket", "another-bucket"]);
        assert_eq!(checks.total(), 3);
        assert_eq!(checks.passing(), 1);
        assert_eq!(checks.skipped(), 2, "unknown buckets fall into skipped");
        assert_eq!(
            checks.passing()
                + checks.failing()
                + checks.pending()
                + checks.skipped()
                + checks.cancelled(),
            checks.total(),
            "counters must always sum to total"
        );
    }

    #[test]
    fn pull_request_checks_from_buckets_empty_yields_zero_total() {
        let checks = PullRequestChecks::from_buckets(std::iter::empty::<&str>());
        assert_eq!(checks.total(), 0);
        assert_eq!(checks.summary(), "(none)");
    }
}
