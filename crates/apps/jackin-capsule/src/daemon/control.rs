// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Control reply + status capture helpers extracted from the daemon
//! coordinator: `write_status_capture`, `control_reply_for_request`, and the
//! related reply builders.

mod auth;
mod frames;
mod replies;

pub(crate) use auth::{attach_peer_is_authorized, control_request_allowed};
pub(crate) use frames::coalesce_client_frames;
pub use frames::handle_client_frame;
pub(crate) use replies::send_attach_control_response;
pub use replies::{control_reply_for_request, write_status_capture};

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

#[cfg(test)]
mod tests;
