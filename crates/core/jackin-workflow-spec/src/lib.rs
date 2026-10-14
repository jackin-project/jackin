// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Typed repair-workflow definitions: frozen Pkl evaluation, immutable
//! plan compilation, and plan digests.
//!
//! The pipeline is [`eval`] (freeze two local Pkl modules, evaluate in a
//! killable helper) → [`rules`] (validate the definition, compile the
//! fixed plan) → [`digest`] (canonical bytes plus SHA-256) → [`store`]
//! (protected input copies, verified reads). All failures are typed
//! [`error`] diagnostics; input bytes come from [`resolve`].

pub mod digest;
pub mod error;
pub mod eval;
pub mod model;
pub mod resolve;
pub mod rules;
pub mod store;
