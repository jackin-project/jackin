// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::time::Duration;

const REASON_MAX: usize = 300;
const NO_REASON: &str = "no reason recorded";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StepId {
    J00,
    J01,
    J02,
    J03,
    J04,
    J05,
    J06,
    J07,
    J08,
    J09,
    J10,
    J11,
    J12,
    J13,
    J14,
    J15,
    J16,
    J17,
}

impl StepId {
    pub const ALL: [StepId; 18] = [
        StepId::J00,
        StepId::J01,
        StepId::J02,
        StepId::J03,
        StepId::J04,
        StepId::J05,
        StepId::J06,
        StepId::J07,
        StepId::J08,
        StepId::J09,
        StepId::J10,
        StepId::J11,
        StepId::J12,
        StepId::J13,
        StepId::J14,
        StepId::J15,
        StepId::J16,
        StepId::J17,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        const LABELS: [&str; 18] = [
            "J00", "J01", "J02", "J03", "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11",
            "J12", "J13", "J14", "J15", "J16", "J17",
        ];
        LABELS[self as usize]
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

pub const STEP_IDS: [&str; 14] = [
    "J00", "J04", "J05", "J06", "J07", "J08", "J09", "J10", "J11", "J12", "J13", "J14", "J15",
    "J16",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    status: Status,
    reason: String,
}

impl Verdict {
    #[must_use]
    pub fn pass(evidence: impl Into<String>) -> Verdict {
        Verdict {
            status: Status::Pass,
            reason: evidence.into(),
        }
    }

    #[must_use]
    pub fn fail(reason: impl Into<String>) -> Verdict {
        Verdict {
            status: Status::Fail,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn skip(reason: impl Into<String>) -> Verdict {
        Verdict {
            status: Status::Skip,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub fn is_pass(&self) -> bool {
        self.status == Status::Pass
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineId {
    Step(StepId),
    Clean,
}

impl LineId {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LineId::Step(id) => id.label(),
            LineId::Clean => "CLEAN",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepLine {
    pub id: LineId,
    pub verdict: Verdict,
    pub elapsed: Duration,
}

#[must_use]
pub(crate) fn one_line(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len().min(REASON_MAX * 4));
    let mut chars = reason.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            skip_escape(&mut chars);
            out.push(' ');
        } else if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.chars().take(REASON_MAX).collect()
}

fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            for c in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    break;
                }
            }
        }
        Some(']') => {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\u{7}' {
                    break;
                }
                if c == '\u{1b}' {
                    chars.next();
                    break;
                }
            }
        }
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

#[must_use]
pub fn render(line: &StepLine) -> String {
    let reason = one_line(line.verdict.reason());
    let reason = if reason.is_empty() && line.verdict.status() != Status::Pass {
        NO_REASON.to_owned()
    } else if reason.is_empty() {
        "ok".to_owned()
    } else {
        reason
    };
    format!(
        "{} {} {:>7.1}s {}",
        line.id.label(),
        line.verdict.status().word(),
        line.elapsed.as_secs_f64(),
        reason
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    Pass,
    Failed,
    Setup,
    SkipOnly,
    Interrupted,
}

impl ExitStatus {
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            ExitStatus::Pass => 0,
            ExitStatus::Failed => 1,
            ExitStatus::Setup => 2,
            ExitStatus::SkipOnly => 3,
            ExitStatus::Interrupted => 130,
        }
    }

    #[must_use]
    pub fn of(lines: &[StepLine], interrupted: bool) -> ExitStatus {
        if interrupted {
            return ExitStatus::Interrupted;
        }
        if lines.is_empty() {
            return ExitStatus::Setup;
        }
        let any = |status| lines.iter().any(|l| l.verdict.status() == status);
        if any(Status::Fail) {
            ExitStatus::Failed
        } else if any(Status::Skip) {
            ExitStatus::SkipOnly
        } else {
            ExitStatus::Pass
        }
    }
}
