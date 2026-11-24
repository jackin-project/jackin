// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_kimi as kimi_api;
use jackin_usage_provider_minimax as minimax_api;
use serde_json::{Map, Value, json};

pub(crate) async fn kimi(
    http: &Http,
    region: kimi_api::Region,
    key: &str,
) -> Result<Value, ProviderError> {
    let plan = kimi_api::fetch_plan(http, region, key).await?;
    let mut resources = Map::new();
    for window in plan.windows {
        let id = match window.label.as_str() {
            "5-hour" => "session",
            "weekly" => "weekly",
            "monthly" => "monthlyTotal",
            "monthly code" => "monthlyCode",
            _ => continue,
        };
        resources.insert(id.to_owned(),json!({"kind":"consumption","unit":"percent","limit":100,
            "window_seconds":match id {"session"=>Some(18_000),"weekly"=>Some(604_800),_=>None},
            "used":window.used_percent,"remaining":window.used_percent.map(|p|(100.0-p).max(0.0)), "resets_at":window.resets_at}));
    }
    if let Some(wallet) = plan.booster {
        resources.insert("booster".into(),json!({"kind":"balance","unit":"millionths_of_cent","available":wallet.remaining_millionths_of_cent,
            "limit":wallet.total_millionths_of_cent,"monthly_charge_limit_enabled":wallet.monthly_charge_limit_enabled}));
        for (id, value) in [
            ("monthlyChargeLimit", wallet.monthly_limit),
            ("monthlyChargeUsed", wallet.monthly_used),
        ] {
            if let Some(charge) = value {
                resources.insert(id.into(),json!({"kind":"amount","unit":"minor_currency","exponent":2,"currency":charge.currency,"amount":charge.cents}));
            }
        }
    }
    Ok(
        json!({"resources":resources,"authMethod":"key","region":match region {kimi_api::Region::Mainland=>"cn",kimi_api::Region::Global=>"global"}}),
    )
}

pub(crate) async fn minimax(
    http: &Http,
    region: minimax_api::Region,
    key: &str,
    oauth: bool,
) -> Result<Value, ProviderError> {
    if !oauth && key.starts_with("sk-api-") {
        let balance = minimax_api::fetch_balance(http, region, key).await?;
        let mut resources = Map::new();
        for (label, amount, exponent) in balance.amounts {
            let id = if label == "Available" {
                "totalAvailable".to_owned()
            } else {
                label.to_lowercase().replace(' ', "_")
            };
            resources.insert(format!("balance/{id}"),json!({"label":label,"kind":"balance","unit":"monetary units","exponent":exponent,"available":amount}));
        }
        return Ok(
            json!({"resources":resources,"label":"MiniMax Balance","authMethod":"key","region":match region{minimax_api::Region::China=>"cn",minimax_api::Region::Global=>"global"},"balanceAlertsEnabled":balance.alerts}),
        );
    }
    let models = if oauth {
        minimax_api::fetch_oauth_plan(http, region, key).await?
    } else {
        minimax_api::fetch_plan(http, region, key).await?
    };
    let mut resources = Map::new();
    for model in models {
        for (id, window, boost) in [
            ("interval", model.interval, None),
            ("weekly", model.weekly, model.weekly_boost_permille),
        ] {
            if window.status.is_none()
                && window.total.is_none()
                && window.remaining_percent.is_none()
                && window.starts_at_ms.is_none()
                && window.ends_at_ms.is_none()
                && window.remaining_ms.is_none()
            {
                continue;
            }
            let state = match (model.excluded_from_plan, window.status) {
                (true, _) => "not_in_plan",
                (_, Some(2)) => "exhausted",
                (_, Some(3)) => "unlimited",
                (_, Some(1)) => "limited",
                _ => "unreported",
            };
            let reported_percent = window
                .remaining_percent
                .or_else(|| (window.status == Some(2)).then_some(0.0));
            let effective_percent =
                reported_percent.map(|p| p * boost.unwrap_or(1000) as f64 / 1000.0);
            let quota = json!({"kind":"consumption","model":model.model_name,"window":id,"unit":"count","status":state,
               "limit":window.total,"remaining":window.remaining_count,"remaining_percent":effective_percent,"base_remaining_percent":reported_percent,
               "boost_permille":boost,"used":window.total.zip(window.remaining_count).map(|(n,r)| n-r),
               "starts_at":window.starts_at_ms.map(|t|t/1000),"resets_at":window.ends_at_ms.map(|t|t/1000),
               "remaining_ms":window.remaining_ms});
            resources.insert(format!("{}/{id}", model.model_name), quota);
        }
    }
    Ok(
        json!({"resources":resources,"authMethod":"key","region":match region {minimax_api::Region::China=>"cn",minimax_api::Region::Global=>"global"}}),
    )
}

pub(super) async fn balance(http: &Http, key: &str) -> Result<Value, ProviderError> {
    let balance = jackin_usage_provider_mimo::fetch_balance(http, key).await?;
    let mut resources = Map::new();
    for item in balance.balances {
        for (id, amount) in [
            ("cash", item.cash_cents),
            ("gift", item.gift_cents),
            ("frozen", item.frozen_cents),
            ("creditLimit", item.credit_limit_cents),
            ("creditAvailable", item.available_credit_cents),
            ("totalAvailable", item.total_available_cents),
        ] {
            resources.insert(format!("{}/{id}",item.currency),json!({"kind":"balance","unit":"minor_currency","exponent":2,"currency":item.currency,"available":amount}));
        }
    }
    Ok(json!({"is_available":balance.available,"resources":resources}))
}
