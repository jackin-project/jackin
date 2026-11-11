// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::{Command, Output, Stdio};

use super::scenario::{Ran, Scenario};

pub(crate) const IDS: [&str; 19] = [
    "J00", "J01", "J02", "J03", "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11", "J12",
    "J13", "J14", "J15", "J16", "J17", "CLEAN",
];

impl Ran {
    pub(crate) fn expect(&self, case: &str, changed: &[(&str, &str)]) {
        let statuses = self.statuses();
        let mut wrong = Vec::new();
        for id in IDS {
            let wanted = changed
                .iter()
                .find(|(line, _)| *line == id)
                .map_or("PASS", |(_, status)| *status);
            let got = statuses.get(id).map_or("MISSING", String::as_str);
            if got != wanted {
                wrong.push(format!("{id}: wanted {wanted}, got {got}"));
            }
        }
        assert!(
            wrong.is_empty(),
            "{case}: {wrong:?}\n--stdout--\n{}\n--stderr--\n{}",
            self.stdout,
            self.stderr
        );
        let fails = changed.iter().any(|(_, status)| *status == "FAIL");
        let skips = changed.iter().any(|(_, status)| *status == "SKIP");
        let code = if fails {
            1
        } else if skips {
            3
        } else {
            0
        };
        assert_eq!(
            self.status.code(),
            code,
            "{case}: exit code\n{}",
            self.stdout
        );
    }

    pub(crate) fn reason_has(&self, case: &str, id: &str, needle: &str) {
        let reason = self.reason_of(id);
        assert!(
            reason.contains(needle),
            "{case}: the reason of {id} is {reason:?}, which lacks {needle:?}\n{}",
            self.stdout
        );
    }
}

pub(crate) async fn fault(
    case: &str,
    id: &str,
    needle: &str,
    tweak: impl FnOnce(&Scenario),
) -> Ran {
    let scenario = Scenario::new().await;
    tweak(&scenario);
    let ran = scenario.run().await;
    ran.expect(case, &[(id, "FAIL")]);
    ran.reason_has(case, id, needle);
    ran
}

pub(crate) async fn blocking_fault(
    case: &str,
    id: &str,
    needle: &str,
    blocked: &[&str],
    tweak: impl FnOnce(&Scenario),
) -> Ran {
    let scenario = Scenario::new().await;
    tweak(&scenario);
    let ran = scenario.run().await;
    let mut changed: Vec<(&str, &str)> = blocked.iter().map(|line| (*line, "SKIP")).collect();
    changed.push((id, "FAIL"));
    ran.expect(case, &changed);
    ran.reason_has(case, id, needle);
    for line in blocked {
        ran.reason_has(case, line, &format!("blocked by {id} FAIL"));
    }
    ran
}

pub(crate) fn capture(command: &mut Command) -> Output {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn")
        .wait_with_output()
        .expect("wait")
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").expect("write to a string");
        out
    })
}
