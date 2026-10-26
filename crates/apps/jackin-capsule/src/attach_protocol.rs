// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod client;
mod handshake;
mod messages;
mod shutdown;
mod spawn;

pub(crate) use client::{detach_client, handle_attach_client_with_handshake};
pub(crate) use handshake::perform_handshake;
pub(crate) use messages::{
    AttachHandshake, AttachResponseCompletion, ControlReply, ControlRequest, ControlResponse,
    RPC_ERROR,
};
pub(crate) use shutdown::{detach_attached_task, drain_and_exit, drain_and_exit_with_reason};
#[cfg(test)]
pub(crate) use spawn::initial_spawn_request;
pub(crate) use spawn::{initial_spawn_requests, spawn_request_label};
