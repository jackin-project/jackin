// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule wire protocol: message types exchanged between the host client and
//! the in-container daemon.
//!
//! Not responsible for: transport setup, authentication, or PTY management.
//!
//! Key invariant: the first byte of any incoming connection disambiguates
//! protocol variant — `0x00` is the control channel (4-byte BE length prefix);
//! `0x01..=0xFF` is the attach channel (tag-plus-length binary framing).
pub mod attach;
pub mod control;

pub use control::AgentState;
