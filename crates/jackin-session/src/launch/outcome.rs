// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::GatewayError;

pub(super) fn outcome_unknown(error: &GatewayError) -> bool {
    match error {
        GatewayError::Unreachable { .. }
        | GatewayError::SandboxUnreachable { .. }
        | GatewayError::Tls { .. }
        | GatewayError::Timeout { .. }
        | GatewayError::Cancelled { .. }
        | GatewayError::Process(_)
        | GatewayError::Transfer { .. }
        | GatewayError::DownloadTooLarge { .. }
        | GatewayError::NotRegularFile { .. }
        | GatewayError::OutputTooLarge { .. }
        | GatewayError::HostKeyMismatch
        | GatewayError::TerminalRefused { .. }
        | GatewayError::Protocol { .. } => true,
        GatewayError::Unauthenticated
        | GatewayError::PermissionDenied { .. }
        | GatewayError::NotFound { .. }
        | GatewayError::AlreadyExists { .. }
        | GatewayError::NotReady { .. }
        | GatewayError::MainProcessFailed { .. }
        | GatewayError::FailedPrecondition { .. }
        | GatewayError::ResourceExhausted { .. }
        | GatewayError::InvalidArgument { .. }
        | GatewayError::UnsafePath { .. } => false,
    }
}
