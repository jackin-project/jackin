// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth-related trait impls on `ConsoleModal`. Each impl carries the full
//! 22-type-parameter list and the same where-clause bundle — moved here
//! during the Ledger 2B decomposition so the modal enum stays a thin
//! coordinator and the per-trait dispatch lives next to the trait it
//! implements.
mod credential_source;
mod focus_status;
mod op_picker;
mod plain_text;
mod predicates;
mod source_browser;
