// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use futures::stream;
use jackin_core::{Provider, Timestamp};
use jackin_usage_provider_core::Http;
use tokio_util::sync::CancellationToken;

use crate::cache::{Cache, Entry, Good, fingerprint};
use crate::deps::UsageDeps;
use crate::dispatch::{Call, Failure, collect};
use crate::model::{
    AccountUsage, Freshness, LimitWindow, SCHEMA, UnavailableReason, UsageError, UsageOutcome,
    UsageReport,
};
use crate::request::{Refresh, UsageAccount, UsageRequest};
use crate::window::windows;

const FAN_OUT: usize = 4;
const TTL_SECONDS: i64 = 120;
const CLAUDE_TTL_SECONDS: i64 = 300;

#[derive(Debug)]
pub struct HostUsage {
    deps: UsageDeps,
    http: Http,
}

enum Update {
    Keep,
    Set(Entry),
    Remove,
}

struct Settled {
    usage: AccountUsage,
    update: Update,
}

impl HostUsage {
    pub fn new(deps: UsageDeps) -> HostUsage {
        let http = Http::new(deps.endpoints.timeout());
        HostUsage { deps, http }
    }

    pub async fn report(
        &self,
        request: &UsageRequest,
        cancel: &CancellationToken,
    ) -> Result<UsageReport, UsageError> {
        let now = self.deps.clock.now();
        let mut cache = Cache::load(&self.deps).await?;
        let jobs: Vec<(&UsageAccount, Option<Entry>)> = request
            .accounts
            .iter()
            .map(|account| (account, cache.entry(account)))
            .collect();
        let work = stream::iter(jobs)
            .map(|(account, entry)| self.settle(account, entry, request.refresh, now.0, cancel))
            .buffered(FAN_OUT)
            .collect::<Vec<_>>();
        let settled = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(UsageError::Cancelled),
            settled = work => settled,
        };
        let mut accounts = Vec::with_capacity(settled.len());
        for (account, done) in request.accounts.iter().zip(settled) {
            match done.update {
                Update::Keep => {}
                Update::Set(entry) => cache.put(account, entry),
                Update::Remove => cache.remove(account),
            }
            accounts.push(done.usage);
        }
        cache.save(&self.deps, cancel).await?;
        Ok(UsageReport {
            schema: SCHEMA,
            generated_at: now,
            accounts,
        })
    }

    async fn settle(
        &self,
        account: &UsageAccount,
        entry: Option<Entry>,
        refresh: Refresh,
        now: i64,
        cancel: &CancellationToken,
    ) -> Settled {
        let print = fingerprint(account);
        let entry = entry.filter(|entry| entry.fingerprint == print);
        if let Some(held) = entry.as_ref().and_then(|entry| held(entry, now)) {
            return settled(account, held.0, held.1, Update::Keep);
        }
        if let (Refresh::Cached, Some(good)) =
            (refresh, entry.as_ref().and_then(|e| e.good.as_ref()))
            && now < good.fetched_at.saturating_add(ttl(account.provider))
        {
            return settled(
                account,
                Freshness::Fresh,
                UsageOutcome::Windows(good.windows.clone()),
                Update::Keep,
            );
        }
        match self.fetch(account, cancel, now).await {
            Ok(found) => {
                let next = Entry {
                    fingerprint: print,
                    good: Some(Good {
                        fetched_at: now,
                        windows: found.clone(),
                    }),
                    retry_at: None,
                };
                settled(
                    account,
                    Freshness::Fresh,
                    UsageOutcome::Windows(found),
                    Update::Set(next),
                )
            }
            Err(failure) => failed(account, failure, entry, print),
        }
    }

    async fn fetch(
        &self,
        account: &UsageAccount,
        cancel: &CancellationToken,
        now: i64,
    ) -> Result<Vec<LimitWindow>, Failure> {
        let http = match self.deps.endpoints.base(account.provider) {
            Some(base) => &self
                .http
                .with_base(base)
                .map_err(|error| UnavailableReason::Network(error.to_string()))?,
            None => &self.http,
        };
        let call = Call {
            deps: &self.deps,
            account,
            cancel,
            http,
            now,
        };
        Ok(windows(collect(&call).await?)?)
    }
}

fn ttl(provider: Provider) -> i64 {
    match provider {
        Provider::Claude => CLAUDE_TTL_SECONDS,
        _ => TTL_SECONDS,
    }
}

fn held(entry: &Entry, now: i64) -> Option<(Freshness, UsageOutcome)> {
    let retry_at = entry.retry_at.filter(|at| *at > now)?;
    Some(match &entry.good {
        Some(good) => (
            Freshness::Stale {
                since: Timestamp(good.fetched_at),
            },
            UsageOutcome::Windows(good.windows.clone()),
        ),
        None => (
            Freshness::Fresh,
            UsageOutcome::Unavailable(UnavailableReason::RateLimited {
                retry_at: Some(Timestamp(retry_at)),
            }),
        ),
    })
}

fn settled(
    account: &UsageAccount,
    freshness: Freshness,
    outcome: UsageOutcome,
    update: Update,
) -> Settled {
    Settled {
        usage: AccountUsage {
            account: account.name.clone(),
            provider: account.provider,
            freshness,
            outcome,
        },
        update,
    }
}

fn failed(
    account: &UsageAccount,
    failure: Failure,
    entry: Option<Entry>,
    print: String,
) -> Settled {
    let Failure { reason, transient } = failure;
    if !transient {
        return settled(
            account,
            Freshness::Fresh,
            UsageOutcome::Unavailable(reason),
            Update::Remove,
        );
    }
    let retry_at = match &reason {
        UnavailableReason::RateLimited { retry_at } => retry_at.map(|at| at.0),
        _ => None,
    };
    let next = Entry {
        retry_at,
        ..entry.unwrap_or(Entry {
            fingerprint: print,
            good: None,
            retry_at: None,
        })
    };
    if let Some(good) = next.good.clone() {
        return settled(
            account,
            Freshness::Stale {
                since: Timestamp(good.fetched_at),
            },
            UsageOutcome::Windows(good.windows),
            Update::Set(next),
        );
    }
    let update = if next.retry_at.is_some() {
        Update::Set(next)
    } else {
        Update::Keep
    };
    settled(
        account,
        Freshness::Fresh,
        UsageOutcome::Unavailable(reason),
        update,
    )
}
