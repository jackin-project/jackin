// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn resolve(values: &[(&str, &str)]) -> Result<Option<OtlpConfig>, OtlpConfigError> {
    let values: HashMap<&str, &str> = values.iter().copied().collect();
    resolve_otlp_config(&|key| values.get(key).map(|value| (*value).to_owned()))
}
