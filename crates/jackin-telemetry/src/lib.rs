// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Schema authority for jackin❯ OpenTelemetry signals.
//!
//! Architecture invariant: this is a T0 crate with no jackin❯ crate
//! dependencies. Its extension registry is closed, generated from the Weaver
//! sources, and may never define `jackin.*` or `parallax.*` keys.

pub mod schema;
