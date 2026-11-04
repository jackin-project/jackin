// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod adr008;
mod cancel;
mod cap_fs;
mod crash;
mod credentials;
mod delivery;
mod docker;
mod docker_fence;
mod docker_support;
mod events;
mod exists;
mod failure_text;
mod fence;
mod hooks;
mod hostile_git;
mod hostile_launch;
mod image_handoff;
mod modes;
mod phases;
mod proofs;
mod provider;
mod recreate;
mod supervisor;
mod support;
mod upload_timeout;

#[path = "../support/trap.rs"]
mod trap;
