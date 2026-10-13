// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MiniMax` usage response types and validation.

use super::super::QuotaBucketView;
use super::{
    MiniMaxBalanceResponse, MiniMaxRegion, MiniMaxWindow, minimax_bucket, minimax_is_general_model,
};
use serde::Deserialize;

/// A successful `MiniMax` fetch: the decoded product payload plus the region
/// and host label that actually served it.
#[derive(Debug)]
pub(crate) struct MiniMaxFetched {
    pub(crate) usage: MiniMaxUsage,
    pub(crate) region: MiniMaxRegion,
    pub(crate) host_label: String,
}

#[derive(Debug)]
pub(crate) enum MiniMaxUsage {
    TokenPlan(MiniMaxUsageResponse),
    Balance(MiniMaxBalanceResponse),
}

impl MiniMaxFetched {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        match &self.usage {
            MiniMaxUsage::TokenPlan(usage) => usage.buckets(now),
            MiniMaxUsage::Balance(balance) => balance.buckets(self.region),
        }
    }

    pub(crate) fn plan_label(&self) -> Option<String> {
        match &self.usage {
            MiniMaxUsage::TokenPlan(usage) => usage.plan_name(),
            MiniMaxUsage::Balance(_) => Some("PAYG".to_owned()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxUsageResponse {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    pub(crate) data: Option<MiniMaxUsageData>,
    #[serde(rename = "model_remains", default)]
    pub(crate) root_model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiniMaxBaseResponse {
    #[serde(rename = "status_code")]
    pub(crate) status_code: Option<i64>,
    #[serde(rename = "status_msg")]
    pub(crate) status_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxUsageData {
    #[serde(rename = "base_resp")]
    pub(crate) base_resp: Option<MiniMaxBaseResponse>,
    #[serde(rename = "current_subscribe_title")]
    pub(crate) current_subscribe_title: Option<String>,
    #[serde(rename = "plan_name")]
    pub(crate) plan_name: Option<String>,
    #[serde(rename = "combo_title")]
    pub(crate) combo_title: Option<String>,
    #[serde(rename = "current_plan_title")]
    pub(crate) current_plan_title: Option<String>,
    #[serde(rename = "current_combo_card")]
    pub(crate) current_combo_card: Option<MiniMaxComboCard>,
    #[serde(rename = "model_remains", default)]
    pub(crate) model_remains: Vec<MiniMaxModelRemain>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MiniMaxComboCard {
    pub(crate) title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MiniMaxModelRemain {
    #[serde(rename = "model_name")]
    pub(crate) model_name: Option<String>,
    #[serde(rename = "current_interval_total_count")]
    pub(crate) current_interval_total_count: Option<i64>,
    #[serde(rename = "current_interval_usage_count")]
    pub(crate) current_interval_usage_count: Option<i64>,
    #[serde(rename = "current_interval_remaining_percent")]
    pub(crate) current_interval_remaining_percent: Option<f64>,
    #[serde(rename = "current_interval_status")]
    pub(crate) current_interval_status: Option<i64>,
    #[serde(rename = "end_time")]
    pub(crate) end_time: Option<i64>,
    #[serde(rename = "remains_time")]
    pub(crate) remains_time: Option<i64>,
    #[serde(
        rename = "interval_boost_permille",
        alias = "interval_boost_permill",
        alias = "current_interval_boost_permille",
        alias = "current_interval_boost_permill"
    )]
    pub(crate) interval_boost_permille: Option<f64>,
    #[serde(rename = "current_weekly_total_count")]
    pub(crate) current_weekly_total_count: Option<i64>,
    #[serde(rename = "current_weekly_usage_count")]
    pub(crate) current_weekly_usage_count: Option<i64>,
    #[serde(rename = "current_weekly_remaining_percent")]
    pub(crate) current_weekly_remaining_percent: Option<f64>,
    #[serde(rename = "current_weekly_status")]
    pub(crate) current_weekly_status: Option<i64>,
    #[serde(rename = "weekly_end_time")]
    pub(crate) weekly_end_time: Option<i64>,
    #[serde(rename = "weekly_remains_time")]
    pub(crate) weekly_remains_time: Option<i64>,
    #[serde(
        rename = "weekly_boost_permille",
        alias = "weekly_boost_permill",
        alias = "current_weekly_boost_permille",
        alias = "current_weekly_boost_permill"
    )]
    pub(crate) weekly_boost_permille: Option<f64>,
}

impl MiniMaxUsageResponse {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let base = self
            .data
            .as_ref()
            .and_then(|data| data.base_resp.as_ref())
            .or(self.base_resp.as_ref());
        if let Some(status) = base.and_then(|base| base.status_code)
            && status != 0
        {
            return Err(base
                .and_then(|base| base.status_msg.clone())
                .unwrap_or_else(|| format!("status_code {status}")));
        }
        if self.model_remains().is_empty() {
            return Err("missing MiniMax coding plan data".to_owned());
        }
        Ok(())
    }

    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        for remain in self.model_remains() {
            if let Some(bucket) = minimax_bucket(
                remain.model_name.as_deref().unwrap_or("MiniMax model"),
                MiniMaxWindow::Interval,
                remain.current_interval_total_count,
                remain.current_interval_usage_count,
                remain.current_interval_remaining_percent,
                remain.interval_boost_permille,
                remain.current_interval_status,
                remain.end_time,
                remain.remains_time,
                now,
            ) {
                buckets.push(bucket);
            }
            if minimax_is_general_model(remain.model_name.as_deref())
                && let Some(bucket) = minimax_bucket(
                    remain.model_name.as_deref().unwrap_or("MiniMax model"),
                    MiniMaxWindow::Weekly,
                    remain.current_weekly_total_count,
                    remain.current_weekly_usage_count,
                    remain.current_weekly_remaining_percent,
                    remain.weekly_boost_permille,
                    remain.current_weekly_status,
                    remain.weekly_end_time,
                    remain.weekly_remains_time,
                    now,
                )
            {
                buckets.push(bucket);
            }
        }
        buckets
    }

    pub(crate) fn plan_name(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        [
            data.current_subscribe_title.as_deref(),
            data.plan_name.as_deref(),
            data.combo_title.as_deref(),
            data.current_plan_title.as_deref(),
            data.current_combo_card
                .as_ref()
                .and_then(|card| card.title.as_deref()),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
    }

    pub(crate) fn model_remains(&self) -> Vec<&MiniMaxModelRemain> {
        if let Some(data) = &self.data
            && !data.model_remains.is_empty()
        {
            return data.model_remains.iter().collect();
        }
        self.root_model_remains.iter().collect()
    }
}
