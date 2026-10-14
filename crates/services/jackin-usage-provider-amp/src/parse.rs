// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AmpRenewal, AmpSubscription, AmpSubscriptionKind, AmpUsage, AmpWorkspaceBalance};
use jackin_usage_provider_core::dollar_amounts;

pub fn parse_amp_usage_output(text: &str) -> Option<AmpUsage> {
    let cleaned = text.replace("**", "");
    let mut usage = AmpUsage::default();
    for line in cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(rest) = line.strip_prefix("Signed in as ") {
            let identity = rest.split(" (").next().unwrap_or(rest).trim();
            if !identity.is_empty() {
                usage.account_label = Some(identity.to_owned());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Amp Free:") {
            if let Some(percent) = parse_amp_daily_percent(rest) {
                usage.daily_remaining_percent = Some(percent);
            }
            continue;
        }
        if line.starts_with("Individual credits:") {
            usage.individual_credits = dollar_amounts(line).first().copied();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Workspace ")
            && let Some(balance) = parse_amp_workspace(rest)
        {
            usage.workspace_balances.push(balance);
            continue;
        }
        if let Some((subscription, renewal, period)) = parse_amp_tier_line(line) {
            usage.subscription = Some(subscription);
            usage.renewal = renewal;
            usage.billing_period = period;
            continue;
        }
        if usage.subscription.is_none()
            && let Some((subscription, renewal)) = parse_amp_legacy_subscription_line(line)
        {
            usage.subscription = Some(subscription);
            usage.renewal = renewal;
        }
    }
    (usage.daily_remaining_percent.is_some()
        || usage.individual_credits.is_some()
        || !usage.workspace_balances.is_empty()
        || usage.subscription.is_some())
    .then_some(usage)
}

pub(crate) fn parse_amp_tier_line(
    line: &str,
) -> Option<(AmpSubscription, Option<AmpRenewal>, Option<String>)> {
    let after_amp = line.strip_prefix("Amp ")?;
    let (plan, segment) = after_amp.split_once(" Tier:")?;
    let plan = plan.trim();
    if plan.is_empty() {
        return None;
    }
    let segment = segment.strip_prefix(" agent usage ")?;
    if !segment.contains("remaining") {
        return None;
    }
    let amounts = dollar_amounts(segment);
    let (remaining, limit) = (*amounts.first()?, *amounts.get(1)?);
    if !remaining.is_finite() || !limit.is_finite() || remaining < 0.0 || limit <= 0.0 {
        return None;
    }
    let (orb_remaining_hours, orb_limit_hours) =
        parse_amp_orb_hours(segment).map_or((None, None), |(r, l)| (Some(r), Some(l)));
    let subscription = AmpSubscription {
        plan: plan.to_owned(),
        kind: AmpSubscriptionKind::Tier {
            agent_remaining: remaining,
            agent_limit: limit,
            orb_remaining_hours,
            orb_limit_hours,
        },
    };
    Some((
        subscription,
        parse_amp_renewal(segment),
        parse_amp_period(segment),
    ))
}

fn parse_amp_orb_hours(segment: &str) -> Option<(f64, f64)> {
    let after = segment.split_once("orb usage ")?.1;
    let (remaining_token, rest) = after.split_once('h')?;
    let rest = rest.strip_prefix(" of ")?;
    let (limit_token, rest) = rest.split_once('h')?;
    if !rest.contains("orb hours") {
        return None;
    }
    let remaining = parse_amp_number(remaining_token)?;
    let limit = parse_amp_number(limit_token)?;
    if !remaining.is_finite() || !limit.is_finite() || remaining < 0.0 || limit <= 0.0 {
        return None;
    }
    Some((remaining, limit))
}

fn parse_amp_renewal(segment: &str) -> Option<AmpRenewal> {
    let after = segment.split_once("resets upon renewal in ")?.1;
    let mut parts = after.split_whitespace();
    let value: u64 = parts.next()?.replace(',', "").parse().ok()?;
    let unit = parts.next()?.to_ascii_lowercase();
    let months = if unit.starts_with("day") {
        false
    } else if unit.starts_with("month") {
        true
    } else {
        return None;
    };
    Some(AmpRenewal { value, months })
}

fn parse_amp_period(segment: &str) -> Option<String> {
    let after = segment.split_once("period ")?.1;
    let start = after.get(..10)?;
    let end = after.get(10..)?.strip_prefix(" to ")?.get(..10)?;
    if !is_amp_period_date(start) || !is_amp_period_date(end) || end <= start {
        return None;
    }
    Some(format!("{start} to {end}"))
}

fn is_amp_period_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn parse_amp_legacy_subscription_line(line: &str) -> Option<(AmpSubscription, Option<AmpRenewal>)> {
    let (plan, segment) = if let Some(rest) = line.strip_prefix("Subscription ") {
        let (plan, segment) = rest.split_once(':')?;
        (plan, segment)
    } else {
        let after_amp = line.strip_prefix("Amp ")?;
        after_amp.split_once(" Subscription:")?
    };
    let plan = plan.trim();
    if plan.is_empty() {
        return None;
    }
    let (agent_part, orb_segment) = segment.split_once("% other usage and ")?;
    let (orb_number, orb_rest) = orb_segment.split_once("% orb usage")?;
    if !orb_rest.contains("remaining") {
        return None;
    }
    let agent = parse_amp_trailing_percent(agent_part)?;
    let orb = parse_amp_trailing_percent(orb_number)?;
    let subscription = AmpSubscription {
        plan: plan.to_owned(),
        kind: AmpSubscriptionKind::Legacy {
            agent_remaining_percent: agent,
            orb_remaining_percent: orb,
        },
    };
    Some((subscription, parse_amp_renewal(segment)))
}

fn parse_amp_trailing_percent(before_percent: &str) -> Option<u8> {
    let token = before_percent.split_whitespace().last()?;
    let percent = parse_amp_number(token)?;
    if !percent.is_finite() {
        return None;
    }
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

fn parse_amp_number(token: &str) -> Option<f64> {
    let cleaned: String = token.chars().filter(|ch| *ch != ',').collect();
    cleaned.trim().parse().ok()
}

fn parse_amp_daily_percent(rest: &str) -> Option<u8> {
    let (value, _) = rest.trim().split_once('%')?;
    let percent: f64 = value.trim().parse().ok()?;
    if !percent.is_finite() {
        return None;
    }
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 below")]
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

fn parse_amp_workspace(rest: &str) -> Option<AmpWorkspaceBalance> {
    let (name, amount_part) = rest.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let remaining = dollar_amounts(amount_part).first().copied()?;
    if !remaining.is_finite() || remaining < 0.0 {
        return None;
    }
    Some(AmpWorkspaceBalance {
        name: name.to_owned(),
        remaining,
    })
}
