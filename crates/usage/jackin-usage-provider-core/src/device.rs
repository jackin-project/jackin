// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use thiserror::Error;
use tokio::time::{Instant, sleep, sleep_until};
use tokio_util::sync::CancellationToken;

use crate::ProviderError;

pub enum DevicePoll<T> {
    Pending,
    SlowDown,
    Granted(T),
    Expired,
    Denied,
}

impl<T> std::fmt::Debug for DevicePoll<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pending => "Pending",
            Self::SlowDown => "SlowDown",
            Self::Granted(_) => "Granted(<redacted>)",
            Self::Expired => "Expired",
            Self::Denied => "Denied",
        })
    }
}

pub fn verification_host(url: &reqwest::Url, hosts: &[&str]) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| hosts.contains(&host))
}

#[derive(Debug, Error)]
pub enum DevicePollError {
    #[error("device authorization expired")]
    Expired,
    #[error("device authorization denied")]
    Denied,
    #[error("device authorization cancelled")]
    Cancelled,
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

pub async fn poll_device<T, F>(
    interval: Duration,
    deadline: Instant,
    cancel: &CancellationToken,
    mut poll: impl FnMut() -> F,
) -> Result<T, DevicePollError>
where
    F: Future<Output = Result<DevicePoll<T>, ProviderError>>,
{
    let mut interval = interval.max(Duration::from_millis(1));
    let polling = async {
        loop {
            sleep(interval).await;
            match poll().await? {
                DevicePoll::Pending => {}
                DevicePoll::SlowDown => interval = interval.saturating_add(Duration::from_secs(5)),
                DevicePoll::Expired => return Err(DevicePollError::Expired),
                DevicePoll::Denied => return Err(DevicePollError::Denied),
                DevicePoll::Granted(token) => return Ok(token),
            }
        }
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(DevicePollError::Cancelled),
        () = sleep_until(deadline) => Err(DevicePollError::Expired),
        result = polling => result,
    }
}
