// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Typed workflow errors. Every failure carries the file, the field, a
//! stable code, a message, and a corrective action. Diagnostics never
//! contain credential values: this crate only handles identifiers and
//! digests.

use std::fmt::Display;

/// Stable machine-readable error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// The Pkl definition is malformed or violates a validation rule.
    InvalidDefinition,
    /// A required input (file, selection, repository state) is missing.
    MissingInput,
    /// The definition requests something the runtime cannot support.
    UnsupportedCapability,
    /// The current authorization does not cover the request.
    AuthorizationDenied,
    /// Bounded evaluation hit its deadline or memory cap.
    EvaluationLimit,
    /// A stored plan or input failed digest verification.
    PlanDigestMismatch,
}

impl ErrorCode {
    /// The wire value used in diagnostics and JSON output.
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

/// One actionable failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Workflow or plan file the diagnostic refers to.
    pub file: String,
    /// Dotted field path, empty when the whole file is at fault.
    pub field: String,
    /// Stable code from [`ErrorCode`].
    pub code: ErrorCode,
    /// Human-readable description.
    pub message: String,
    /// What the operator should do next.
    pub action: String,
}

impl Diagnostic {
    /// Build a diagnostic from its parts.
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

/// One or more diagnostics for a failed workflow operation.
#[derive(Debug, thiserror::Error)]
#[error("{diagnostics:?}")]
#[non_exhaustive]
pub struct WorkflowError {
    /// All diagnostics collected for the failure.
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkflowError {
    /// Wrap a single diagnostic.
    #[must_use]
    pub fn single(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    /// The first diagnostic's code, or `invalid_definition` when empty.
    #[must_use]
    pub fn primary_code(&self) -> ErrorCode {
        self.diagnostics
            .first()
            .map_or(ErrorCode::InvalidDefinition, |found| found.code)
    }
}

/// Fallible workflow result.
pub type WorkflowResult<T> = Result<T, WorkflowError>;
