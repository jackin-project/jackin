// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::display::plain;
use crate::ids::{AttemptId, IdError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SupervisorState {
    Waiting,
    Running { attempt: AttemptId },
    Exited { attempt: AttemptId, code: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct StartRequest(pub AttemptId);

#[derive(Debug, Error)]
pub enum SupervisorStateError {
    #[error("supervisor state is empty")]
    Empty,
    #[error("unknown supervisor state word {}", plain(.0))]
    UnknownWord(String),
    #[error("invalid attempt ID")]
    BadAttempt(IdError),
    #[error("invalid exit code {}", plain(.0))]
    BadExitCode(String),
    #[error("trailing text {}", plain(.0))]
    TrailingText(String),
}

fn attempt_word(word: Option<&str>) -> Result<AttemptId, SupervisorStateError> {
    AttemptId::parse(word.unwrap_or("")).map_err(SupervisorStateError::BadAttempt)
}

fn no_trailing<'a>(rest: impl Iterator<Item = &'a str>) -> Result<(), SupervisorStateError> {
    let extra: Vec<&str> = rest.collect();
    if extra.is_empty() {
        Ok(())
    } else {
        Err(SupervisorStateError::TrailingText(extra.join(" ")))
    }
}

impl SupervisorState {
    pub fn parse(text: &str) -> Result<SupervisorState, SupervisorStateError> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.is_empty() {
            return Err(SupervisorStateError::Empty);
        }
        let mut words = text.split(' ');
        match words.next().unwrap_or("") {
            "waiting" => {
                no_trailing(words)?;
                Ok(SupervisorState::Waiting)
            }
            "running" => {
                let attempt = attempt_word(words.next())?;
                no_trailing(words)?;
                Ok(SupervisorState::Running { attempt })
            }
            "exited" => {
                let attempt = attempt_word(words.next())?;
                let word = words.next().unwrap_or("");
                let code = word
                    .parse::<i32>()
                    .ok()
                    .filter(|code| code.to_string() == word)
                    .ok_or_else(|| SupervisorStateError::BadExitCode(word.to_owned()))?;
                no_trailing(words)?;
                Ok(SupervisorState::Exited { attempt, code })
            }
            other => Err(SupervisorStateError::UnknownWord(other.to_owned())),
        }
    }

    pub fn render(&self) -> String {
        match self {
            SupervisorState::Waiting => "waiting".to_owned(),
            SupervisorState::Running { attempt } => format!("running {attempt}"),
            SupervisorState::Exited { attempt, code } => format!("exited {attempt} {code}"),
        }
    }

    pub fn attempt(&self) -> Option<AttemptId> {
        match self {
            SupervisorState::Waiting => None,
            SupervisorState::Running { attempt } | SupervisorState::Exited { attempt, .. } => {
                Some(*attempt)
            }
        }
    }

    pub fn next_attempt(&self) -> Option<AttemptId> {
        match self {
            SupervisorState::Waiting => Some(AttemptId::FIRST),
            SupervisorState::Running { .. } => None,
            SupervisorState::Exited { attempt, .. } => attempt.next(),
        }
    }
}

impl StartRequest {
    pub fn parse(text: &str) -> Result<StartRequest, SupervisorStateError> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.is_empty() {
            return Err(SupervisorStateError::Empty);
        }
        attempt_word(Some(text)).map(StartRequest)
    }

    pub fn render(&self) -> String {
        self.0.to_string()
    }
}
