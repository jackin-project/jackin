// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Cursor` usage snapshot: personal allowance vs Enterprise Admin reporting.
//!
//! Two scopes, never mixed (see `ref-contracts-C.md` §2):
//!
//! * Personal (selected account token): `DashboardService` Connect RPC on
//!   `api2.cursor.sh` (`GetCurrentPeriodUsage`, `GetPlanInfo`,
//!   `GetCreditGrantsBalance`, `GetSandUsageStatus`) plus `cursor.com` session
//!   REST (`/api/usage?user=`, `/api/usage-summary`, `/api/auth/stripe`).
//!   A personal key never implies Enterprise reporting access.
//! * Enterprise Admin (`api.cursor.com/teams/spend|filtered-usage-events`):
//!   separate [`CursorEnterpriseScope`] with explicit admin auth; the personal
//!   snapshot never touches admin hosts. The events API is hourly aggregated
//!   and is never polled at the overview interval.
//!
//! Money discipline: actual charged amounts, estimated model cost, included
//! quota, and credit grants are separate buckets. Estimates never back a
//! Spend-slot headline.

mod auth;
mod events;
mod period;
mod plan;
mod requests;
mod rest;
mod sand;
mod snapshot;
mod summary;
mod teams;

#[cfg(test)]
use super::*;

#[cfg(test)]
pub(crate) use auth::CURSOR_DEFAULT_DASHBOARD_BASE;
pub(crate) use auth::CURSOR_SESSION_BASE;
pub(crate) use auth::{
    CursorAuth, cursor_auth_from_value, cursor_auth_path, cursor_cli_identity_from_value,
    cursor_dashboard_base, cursor_dashboard_url_with_base, cursor_default_base,
    cursor_identity_from_cli_config, cursor_user_id_from_token, load_cursor_auth,
    load_cursor_cli_identity,
};
pub(crate) use events::{
    CursorUsageEvents, cursor_events_buckets, fetch_cursor_usage_events, parse_cursor_usage_events,
};
pub(crate) use period::{
    CursorPeriodUsage, cursor_dashboard_post, cursor_needs_request_fallback, cursor_period_buckets,
    fetch_cursor_period_usage, parse_cursor_period_usage,
};
pub(crate) use plan::{
    cursor_credits_bucket, fetch_cursor_credit_grants, fetch_cursor_plan_info,
    parse_cursor_credit_grants, parse_cursor_plan_info,
};
pub(crate) use requests::{
    CursorRequestUsage, cursor_request_bucket, fetch_cursor_request_usage,
    parse_cursor_request_usage,
};
pub(crate) use rest::{cursor_rest_get, cursor_session_cookie};
pub(crate) use sand::{
    CursorSandUsage, cursor_sand_bucket, fetch_cursor_sand_usage, parse_cursor_sand_usage,
};
pub(crate) use snapshot::{
    cursor_enterprise_snapshot, cursor_profile_snapshot, cursor_snapshot, cursor_snapshot_with_auth,
};
pub(crate) use summary::{
    CursorUsageSummary, cursor_summary_buckets, fetch_cursor_stripe_balance,
    fetch_cursor_usage_summary, parse_cursor_stripe_balance, parse_cursor_usage_summary,
};
pub(crate) use teams::{
    CursorEnterpriseScope, CursorMemberSpend, CursorTeamSpend, cursor_team_spend_buckets,
    cursor_teams_events_url, cursor_teams_spend_url, fetch_cursor_team_spend,
    parse_cursor_team_spend,
};

#[cfg(test)]
mod tests;
