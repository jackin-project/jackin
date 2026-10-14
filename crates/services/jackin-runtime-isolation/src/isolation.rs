// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod finalize {
    pub use jackin_isolation::finalize::*;
}
pub mod git_inspect {
    pub use jackin_isolation::git_inspect::*;
}

pub mod branch {
    pub use jackin_isolation::branch::*;
}
pub mod cleanup {
    pub use jackin_isolation::cleanup::*;
}
pub mod materialize {
    pub use jackin_isolation::materialize::*;
}
pub mod state {
    pub use jackin_isolation::state::*;
}
pub mod safe_remove {
    pub use jackin_isolation::safe_remove::*;
}

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;

#[cfg(test)]
mod tests;
