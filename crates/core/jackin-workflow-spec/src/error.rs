// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidDefinition,

    MissingInput,

    UnsupportedCapability,

    AuthorizationDenied,

    EvaluationLimit,

    PlanDigestMismatch,
}

impl ErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDefinition => "invalid_definition",
            Self::MissingInput => "missing_input",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::AuthorizationDenied => "authorization_denied",
            Self::EvaluationLimit => "evaluation_limit",
            Self::PlanDigestMismatch => "plan_digest_mismatch",
        }
    }
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,

    pub field: String,

    pub code: ErrorCode,

    pub message: String,

    pub action: String,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        file: impl Into<String>,
        field: impl Into<String>,
        code: ErrorCode,
        message: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            field: field.into(),
            code,
            message: message.into(),
            action: action.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{diagnostics:?}")]
#[non_exhaustive]
pub struct WorkflowError {
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkflowError {
    #[must_use]
    pub fn single(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    #[must_use]
    pub fn primary_code(&self) -> ErrorCode {
        self.diagnostics
            .first()
            .map_or(ErrorCode::InvalidDefinition, |found| found.code)
    }
}

pub type WorkflowResult<T> = Result<T, WorkflowError>;
