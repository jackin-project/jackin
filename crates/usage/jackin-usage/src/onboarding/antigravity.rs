// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AccountRegistry, LoginError, LoginPrompt, OwnedLogin, text_field};
use crate::accounts::CredentialVault;
use jackin_usage_provider_core::ProviderError;
use jackin_usage_provider_gemini::{antigravity_account_email, antigravity_statusline};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn receive_antigravity_statusline(
    path: &Path,
    input: &[u8],
    now: i64,
    vault: Arc<dyn CredentialVault>,
) -> Result<bool, LoginError> {
    let Some(reading) = antigravity_statusline(input, now)? else {
        return Ok(false);
    };
    let Some(mut db) =
        AccountRegistry::open_existing(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE, vault)?
    else {
        return Ok(false);
    };
    Ok(db.receive_statusline(&reading.email, &reading.snapshot.to_string(), now)?)
}

struct Enrollment {
    registry: AccountRegistry,
    ticket: Uuid,
}

impl Drop for Enrollment {
    fn drop(&mut self) {
        let _cleanup = self.registry.cancel_enrollment(self.ticket);
    }
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn launch_command(database: &Path) -> Result<String, LoginError> {
    let binary = std::env::current_exe().map_err(crate::accounts::ConnectionError::from)?;
    let database = database
        .canonicalize()
        .map_err(crate::accounts::ConnectionError::from)?;
    let quote_path = |path: &Path| {
        path.to_str()
            .filter(|s| !s.chars().any(char::is_control))
            .map(shell_quote)
            .ok_or(ProviderError::Unsupported(
                "The Antigravity status line requires printable UTF-8 paths",
            ))
    };
    let command = format!(
        "{} usage antigravity-statusline --database {}",
        quote_path(&binary)?,
        quote_path(&database)?
    );
    let settings = serde_json::json!({
        "statusLine": {"enabled":true, "type":"command", "stack_with_default":true, "command":command},
    });
    Ok(format!(
        "agy --settings {}",
        shell_quote(&settings.to_string())
    ))
}

impl OwnedLogin {
    pub async fn connect_antigravity_statusline(
        &self,
        replace: Option<Uuid>,
        prompts: mpsc::Sender<LoginPrompt>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, LoginError> {
        let value = tokio::select! {
            () = cancel.cancelled() => return Err(LoginError::Cancelled),
            value = text_field(&prompts, "Google account email used in Antigravity", false) => value?,
        };
        let email = antigravity_account_email(&value)?;
        let now = self.now().0;
        let expires = now.saturating_add(600);
        let mut registry =
            AccountRegistry::open_statusline(self.database(), Arc::clone(&self.vault))?;
        let ticket = Uuid::new_v4();
        registry.start_enrollment(ticket, &email, replace, now, expires)?;
        let mut enrollment = Enrollment { registry, ticket };
        let command = launch_command(self.database())?;
        prompts
            .send(LoginPrompt::AntigravityStatusLine { command })
            .await
            .map_err(|_| LoginError::Cancelled)?;
        loop {
            let now = self.now().0;
            if cancel.is_cancelled() {
                return Err(LoginError::Cancelled);
            }
            if now >= expires {
                return Err(ProviderError::Unsupported("Antigravity did not report quota within ten minutes. Connect again and open the displayed command").into());
            }
            if let Some(id) = enrollment
                .registry
                .finish_enrollment(ticket, replace, now)?
            {
                return Ok(id);
            }
            tokio::select! {
                () = cancel.cancelled() => return Err(LoginError::Cancelled),
                () = tokio::time::sleep(Duration::from_millis(400)) => {}
            }
        }
    }
}
