// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Serialize)]
pub struct OutputEnvelope<T>
where
    T: Serialize,
{
    pub schema_version: &'static str,
    pub data: T,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Self {
        if s == "json" { Self::Json } else { Self::Human }
    }
}

impl<T: Serialize> OutputEnvelope<T> {
    pub const SCHEMA_V1: &'static str = "v1";

    pub const fn v1(data: T) -> Self {
        Self {
            schema_version: Self::SCHEMA_V1,
            data,
        }
    }
}
