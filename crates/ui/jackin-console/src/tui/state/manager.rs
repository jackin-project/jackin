// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ManagerState` list, instance, refresh, and upkeep methods.
mod adapters;
mod apply;
mod construct;
mod instances;
mod op_commit;
mod pending;
mod recovery;
mod refresh;
mod rows;
mod scroll;
mod sessions;
#[cfg(test)]
mod tests;
mod upkeep;
pub(crate) use recovery::record_manager_recovery;
