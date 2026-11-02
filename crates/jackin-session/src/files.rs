// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ChatgptAccountId, ModelName, SandboxPath, Timestamp};
use serde_json::json;

const ID_TOKEN: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6ImNvZGV4Iiwic3ViIjoib3BlbnNoZWxsLWFwcC1zZXJ2ZXIifQ.placeholder";
const AUTH_MODE: &str = "chatgptAuthTokens";
const SECONDS_PER_DAY: i64 = 86_400;

pub(crate) fn render_codex_config(model: Option<&ModelName>, workdir: &SandboxPath) -> String {
    let mut text = String::new();
    if let Some(model) = model {
        text.push_str(&format!("model = \"{}\"\n\n", model.as_str()));
    }
    text.push_str(&format!(
        "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
        workdir.as_str()
    ));
    text
}

pub(crate) fn render_auth_json(
    access_token: &str,
    account_id: &ChatgptAccountId,
    now: Timestamp,
) -> String {
    json!({
        "auth_mode": AUTH_MODE,
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": ID_TOKEN,
            "access_token": access_token,
            "refresh_token": "",
            "account_id": account_id.as_str(),
        },
        "last_refresh": rfc3339(now),
    })
    .to_string()
}

fn rfc3339(now: Timestamp) -> String {
    let days = now.0.div_euclid(SECONDS_PER_DAY);
    let seconds = now.0.rem_euclid(SECONDS_PER_DAY);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60
    )
}
