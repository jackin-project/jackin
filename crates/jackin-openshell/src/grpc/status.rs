// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Phase;
use openshell_sdk::SdkError;
use tonic::{Code, Status};

use super::{Inner, endpoint_authority};
use crate::error::{GatewayError, GatewayOp, Missing};

const SUPERVISOR_RELAY_FAILURE: &str = "supervisor relay failed";

fn violation_field(status: &Status) -> String {
    SdkError::from_status(status.clone())
        .error_details()
        .and_then(|details| {
            details
                .bad_request()
                .and_then(|request| request.field_violations.first())
                .map(|violation| violation.field.clone())
        })
        .unwrap_or_default()
}

impl Inner {
    pub(crate) fn map_status(
        &self,
        op: GatewayOp,
        missing: Option<Missing>,
        status: &Status,
    ) -> GatewayError {
        let message = status.message().to_owned();
        match status.code() {
            Code::NotFound => match missing {
                Some(what) => GatewayError::NotFound { what },
                None => GatewayError::Protocol { message },
            },
            Code::AlreadyExists => match missing {
                Some(what) => GatewayError::AlreadyExists { what },
                None => GatewayError::Protocol { message },
            },
            Code::FailedPrecondition | Code::Aborted => {
                GatewayError::FailedPrecondition { message }
            }
            Code::Unauthenticated => GatewayError::Unauthenticated,
            Code::PermissionDenied => GatewayError::PermissionDenied { what: message },
            Code::ResourceExhausted => GatewayError::ResourceExhausted { what: message },
            Code::InvalidArgument => GatewayError::InvalidArgument {
                field: violation_field(status),
                message,
            },
            Code::DeadlineExceeded => GatewayError::Timeout {
                op,
                after: self.call_timeout,
                group: None,
            },
            Code::Unavailable => match missing {
                Some(Missing::Sandbox(sandbox))
                    if message.starts_with(SUPERVISOR_RELAY_FAILURE) =>
                {
                    GatewayError::SandboxUnreachable { sandbox, message }
                }
                _ => GatewayError::Unreachable {
                    endpoint: endpoint_authority(&self.endpoint),
                    message,
                },
            },
            _ => GatewayError::Protocol { message },
        }
    }

    pub(crate) async fn refine_not_ready(
        &self,
        sandbox: &jackin_core::SandboxName,
        error: GatewayError,
    ) -> GatewayError {
        if !matches!(error, GatewayError::FailedPrecondition { .. }) {
            return error;
        }
        match self.sandbox_phase(sandbox).await {
            Ok(phase) if !phase.accepts_exec() => GatewayError::NotReady { phase },
            Ok(_) | Err(_) => error,
        }
    }

    pub(crate) async fn sandbox_phase(
        &self,
        sandbox: &jackin_core::SandboxName,
    ) -> Result<Phase, GatewayError> {
        self.get_sandbox_view(sandbox).await.map(|view| view.phase)
    }
}
