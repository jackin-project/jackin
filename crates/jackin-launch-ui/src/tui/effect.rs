// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch cockpit effect vocabulary.
//!
//! Launch update is currently pure and emits no side effects. Keep this
//! module as the typed effect boundary so future work grows here instead of
//! adding service calls to update or view code.

pub type LaunchEffect = termrock::runtime::NoEffect;
