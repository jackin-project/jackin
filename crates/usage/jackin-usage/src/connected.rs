// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_core::{Provider, Secret, SecretBytes};
use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_mimo::{Region, fetch_token_plan};
use serde_json::{Value, json};

use crate::ProviderEndpoints;

mod extensions;
pub(crate) use extensions::{kimi as kimi_snapshot, minimax as minimax_snapshot};
mod mimo_history;
pub(crate) use mimo_history::project as project_daily;
mod openrouter;
pub(crate) mod zai;
use jackin_usage_provider_kimi::Region as KimiRegion;
use jackin_usage_provider_minimax::Region as MiniMaxRegion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyConnection {
    Zai,
    OpenRouter,
    Mimo(Region),
    MimoBalance,
    Kimi(KimiRegion),
    MiniMax(MiniMaxRegion),
}

impl KeyConnection {
    pub const fn provider(self) -> Provider {
        match self {
            Self::Zai => Provider::Zai,
            Self::OpenRouter => Provider::Openrouter,
            Self::Mimo(_) | Self::MimoBalance => Provider::Mimo,
            Self::Kimi(_) => Provider::Kimi,
            Self::MiniMax(_) => Provider::Minimax,
        }
    }

    pub const fn key_page(self) -> &'static str {
        match self {
            Self::Zai => "https://z.ai/manage-apikey/apikey-list",
            Self::OpenRouter => "https://openrouter.ai/settings/management-keys",
            Self::Mimo(_) | Self::MimoBalance => "https://platform.xiaomimimo.com/",
            Self::Kimi(KimiRegion::Global) => "https://www.kimi.ai/code/console",
            Self::Kimi(KimiRegion::Mainland) => "https://www.kimi.com/code/console",
            Self::MiniMax(MiniMaxRegion::China) => {
                "https://platform.minimaxi.com/subscribe/coding-plan"
            }
            Self::MiniMax(MiniMaxRegion::Global) => {
                "https://platform.minimax.io/subscribe/token-plan"
            }
        }
    }

    pub const fn default_label(self) -> &'static str {
        match self {
            Self::Zai => "Z.ai Account",
            Self::OpenRouter => "OpenRouter Account",
            Self::Mimo(_) => "MiMo Token Plan",
            Self::MimoBalance => "MiMo Balance",
            Self::Kimi(_) => "Kimi Code",
            Self::MiniMax(_) => "MiniMax Token Plan",
        }
    }

    pub fn for_provider(provider: Provider, region: Option<&str>) -> Result<Self, ProviderError> {
        match (provider, region) {
            (Provider::Zai, None) => Ok(Self::Zai),
            (Provider::Openrouter, None) => Ok(Self::OpenRouter),
            (Provider::Kimi, None | Some("global")) => Ok(Self::Kimi(KimiRegion::Global)),
            (Provider::Kimi, Some("cn")) => Ok(Self::Kimi(KimiRegion::Mainland)),
            (Provider::Minimax, None | Some("global")) => Ok(Self::MiniMax(MiniMaxRegion::Global)),
            (Provider::Minimax, Some("cn")) => Ok(Self::MiniMax(MiniMaxRegion::China)),
            (Provider::Kimi | Provider::Minimax, _) => {
                Err(ProviderError::Unsupported("select the global or cn region"))
            }
            (Provider::Mimo, None) => Ok(Self::MimoBalance),
            (Provider::Mimo, Some("cn")) => Ok(Self::Mimo(Region::China)),
            (Provider::Mimo, Some("sgp")) => Ok(Self::Mimo(Region::Singapore)),
            (Provider::Mimo, Some("ams")) => Ok(Self::Mimo(Region::Europe)),
            (Provider::Mimo, _) => Err(ProviderError::Unsupported(
                "select the MiMo cluster: cn, sgp or ams",
            )),
            _ => Err(ProviderError::Unsupported(
                "this provider does not use this key connection",
            )),
        }
    }

    pub(crate) const fn region(self) -> Option<&'static str> {
        match self {
            Self::Mimo(Region::China)
            | Self::Kimi(KimiRegion::Mainland)
            | Self::MiniMax(MiniMaxRegion::China) => Some("cn"),
            Self::Kimi(KimiRegion::Global) | Self::MiniMax(MiniMaxRegion::Global) => Some("global"),
            Self::Mimo(Region::Singapore) => Some("sgp"),
            Self::Mimo(Region::Europe) => Some("ams"),
            _ => None,
        }
    }

    pub fn encode(self, key: &Secret) -> Result<SecretBytes, ProviderError> {
        let region = self.region();

        serde_json::to_vec(&(region, key.expose()))
            .map(SecretBytes::new)
            .map_err(|_| ProviderError::Decode("could not encode the connected key"))
    }

    pub fn decode(
        provider: Provider,
        bytes: &SecretBytes,
    ) -> Result<(Self, Secret), ProviderError> {
        let (region, key): (Option<&str>, String) = serde_json::from_slice(bytes.expose())
            .map_err(|_| ProviderError::Decode("could not read the stored key; replace it"))?;
        Ok((Self::for_provider(provider, region)?, Secret::new(key)))
    }

    pub async fn read(
        self,
        endpoints: &ProviderEndpoints,
        key: &Secret,
    ) -> Result<Value, ProviderError> {
        self.read_with_policy(
            endpoints,
            key,
            jackin_core::Timestamp(time::OffsetDateTime::now_utc().unix_timestamp()),
            None,
            false,
        )
        .await
    }

    pub async fn read_with_policy(
        self,
        endpoints: &ProviderEndpoints,
        key: &Secret,
        now: jackin_core::Timestamp,
        previous: Option<&Value>,
        manual: bool,
    ) -> Result<Value, ProviderError> {
        let http = endpoints.http(self.provider())?;
        self.read_using_http(http, key, now, previous, manual).await
    }

    pub(crate) async fn read_using_http(
        self,
        http: Http,
        key: &Secret,
        now: jackin_core::Timestamp,
        previous: Option<&Value>,
        manual: bool,
    ) -> Result<Value, ProviderError> {
        match self {
            Self::Mimo(region) => {
                let plan = fetch_token_plan(&http, region, key.expose()).await?;
                Ok(
                    json!({"region":self.region(),"authMethod":"key","plan":plan.plan_code,"period_start":plan.starts_at,"auto_renew":plan.auto_renew,
                    "claw_enabled":plan.claw_enabled,"resources":{"tokenPlan":{"kind":"consumption",
                    "unit":"credits","limit":plan.quota.total_credits,"used":plan.quota.used_credits,
                    "remaining":plan.quota.remaining_credits,"resets_at":plan.ends_at,"starts_at":plan.starts_at}}}),
                )
            }
            Self::MimoBalance => {
                let mut snapshot = extensions::balance(&http, key.expose()).await?;
                mimo_history::read(&http, key.expose(), now.0, previous, manual, &mut snapshot)
                    .await;
                Ok(snapshot)
            }
            Self::Kimi(region) => extensions::kimi(&http, region, key.expose()).await,
            Self::MiniMax(region) => extensions::minimax(&http, region, key.expose(), false).await,
            Self::Zai => zai::read(&http, key.expose(), now.0, previous, manual).await,
            Self::OpenRouter => {
                openrouter::read(&http, key.expose(), now.0, previous, manual).await
            }
        }
    }
}
