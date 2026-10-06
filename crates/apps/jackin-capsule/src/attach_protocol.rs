// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Client attach/detach lifecycle for the capsule multiplexer.

mod client;
mod handshake;
mod messages;
mod shutdown;
mod spawn;

#[cfg(test)]
use crate::protocol::attach::ClientFrame;

#[cfg(test)]
pub(crate) use client::handle_attach_client;
pub(crate) use client::{detach_client, handle_attach_client_with_handshake};
#[cfg(test)]
pub(crate) use handshake::perform_control_handshake;
pub(crate) use handshake::perform_handshake;
pub(crate) use messages::{
    AttachHandshake, AttachResponseCompletion, ControlReply, ControlRequest, ControlResponse,
    RPC_ERROR,
};
pub(crate) use shutdown::{detach_attached_task, drain_and_exit, drain_and_exit_with_reason};
#[cfg(test)]
pub(crate) use spawn::initial_spawn_request;
pub(crate) use spawn::{initial_spawn_requests, spawn_request_label};

#[cfg(test)]
mod tests;
