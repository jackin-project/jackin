// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{EnvName, plain};
use serde::Deserialize;
use thiserror::Error;

use crate::dockerfile::defers_engine_client_env;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageInspect {
    pub layers: Vec<String>,
    pub env: Vec<String>,
    pub on_build: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawInspect {
    #[serde(rename = "RootFS")]
    root_fs: RawRootFs,
    config: Option<RawConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawRootFs {
    layers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawConfig {
    env: Option<Vec<String>>,
    on_build: Option<Vec<String>>,
}

impl ImageInspect {
    pub fn parse(bytes: &[u8]) -> Result<ImageInspect, serde_json::Error> {
        let raw: RawInspect = serde_json::from_slice(bytes)?;
        let config = raw.config.unwrap_or(RawConfig {
            env: None,
            on_build: None,
        });
        Ok(ImageInspect {
            layers: raw.root_fs.layers.unwrap_or_default(),
            env: config.env.unwrap_or_default(),
            on_build: config.on_build.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ImageRule {
    #[error("the layers of the image do not start with the layers of the construct image")]
    NotBuiltOnConstruct,
    #[error("Config.Env sets {}, which names the Docker engine client", plain(.name))]
    EngineClientEnv { name: String },
    #[error("Config.OnBuild holds {}, which sets or may expand a Docker engine client variable", plain(.trigger))]
    EngineClientOnBuild { trigger: String },
}

fn env_name(entry: &str) -> &str {
    entry.split_once('=').map_or(entry, |(name, _)| name)
}

pub(crate) fn verify_image(
    construct: &ImageInspect,
    image: &ImageInspect,
) -> Result<(), ImageRule> {
    if construct.layers.is_empty() || !image.layers.starts_with(&construct.layers) {
        return Err(ImageRule::NotBuiltOnConstruct);
    }
    if let Some(name) = image
        .env
        .iter()
        .map(|entry| env_name(entry))
        .find(|name| EnvName::is_engine_client(name))
    {
        return Err(ImageRule::EngineClientEnv {
            name: name.to_owned(),
        });
    }
    if let Some(trigger) = image
        .on_build
        .iter()
        .find(|trigger| defers_engine_client_env(trigger))
    {
        return Err(ImageRule::EngineClientOnBuild {
            trigger: trigger.clone(),
        });
    }
    Ok(())
}
