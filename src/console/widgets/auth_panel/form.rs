//! Auth-form state and save invariants.
//!
//! State for the workspace Auth-tab edit modal. Rendered by
//! `super::render::render_form`; mounted and driven by
//! `crate::console::manager::input::auth::handle_auth_form_key`.

use crate::console::manager::auth_kind::{AuthKind, AuthMode};
use crate::operator_env::{EnvValue, OpRef, OpRunner};

/// What the user has supplied in the credential block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialInput {
    None,
    Literal(String),
    OpRef(OpRef),
}

/// The form's mutable state. Mode and credential are independently editable;
/// only the [`AuthForm::can_save`] invariant decides whether the parent should
/// allow the Save action.
///
/// `kind` widens beyond the runtime `Agent` enum so the GitHub CLI auth
/// kind (no `Agent` peer, agent-neutral by design) can drive the same
/// form widget.
#[derive(Debug)]
pub struct AuthForm {
    pub kind: AuthKind,
    pub mode: Option<AuthMode>,
    pub credential: CredentialInput,
}

/// Output of a successful commit. The parent uses these fields to write the
/// kind block at the chosen layer and the env-var entry under the role's env
/// block.
#[derive(Debug, Clone)]
pub struct AuthFormOutcome {
    pub mode: AuthMode,
    pub env_var_name: Option<&'static str>,
    pub env_value: Option<EnvValue>,
}

impl AuthForm {
    pub const fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            mode: None,
            credential: CredentialInput::None,
        }
    }

    /// Pre-populate the form from an existing row's mode and credential.
    /// Used when [edit] is pressed on a row that already has values.
    pub fn from_existing(kind: AuthKind, mode: AuthMode, credential: Option<EnvValue>) -> Self {
        let credential = match credential {
            None => CredentialInput::None,
            Some(EnvValue::Plain(s)) => CredentialInput::Literal(s),
            Some(EnvValue::OpRef(r)) => CredentialInput::OpRef(r),
        };
        Self {
            kind,
            mode: Some(mode),
            credential,
        }
    }

    /// Set the mode. If switching to a mode that doesn't need a credential,
    /// clears the credential field automatically.
    pub fn set_mode(&mut self, mode: AuthMode) {
        // Pin the `(kind, mode)` validity invariant in dev/test. The
        // form's `available_modes` filter is the operational guard;
        // this assertion makes a future caller that bypasses
        // `available_modes` fail loudly instead of silently storing
        // an unsupported mode that the persistence-side `to_*`
        // converters would later return `None` for.
        debug_assert!(
            self.kind.supported_modes().contains(&mode),
            "AuthMode::{mode:?} not supported by AuthKind::{:?}",
            self.kind,
        );
        self.mode = Some(mode);
        if !mode_requires_credential(self.kind, mode) {
            self.credential = CredentialInput::None;
        }
    }

    pub fn set_literal(&mut self, s: String) {
        self.credential = CredentialInput::Literal(s);
    }

    pub fn set_op_ref(&mut self, r: OpRef) {
        self.credential = CredentialInput::OpRef(r);
    }

    /// Attempts to commit an [`OpRef`]. Calls `runner.read(&candidate.op)`;
    /// only persists the reference if the read succeeds. On failure, the
    /// form's credential state is left unchanged so a broken reference
    /// never reaches disk.
    pub fn try_commit_op_ref<R: OpRunner + ?Sized>(
        &mut self,
        runner: &R,
        candidate: OpRef,
    ) -> anyhow::Result<()> {
        let _ = runner.read(&candidate.op)?;
        self.set_op_ref(candidate);
        Ok(())
    }

    pub fn clear_credential(&mut self) {
        self.credential = CredentialInput::None;
    }

    /// Whether the credential input block should be shown.
    pub const fn shows_credential_block(&self) -> bool {
        matches!(self.mode, Some(m) if mode_requires_credential(self.kind, m))
    }

    /// Modes the user can pick. Each `AuthKind` exposes only the modes
    /// it supports — Codex omits `OAuthToken`, Github trades `api_key`
    /// / `oauth_token` for `token`.
    pub const fn available_modes(&self) -> &'static [AuthMode] {
        self.kind.supported_modes()
    }

    /// Save invariant: mode is committed AND, if needed, a non-empty credential.
    pub const fn can_save(&self) -> bool {
        let Some(mode) = self.mode else { return false };
        if !mode_requires_credential(self.kind, mode) {
            return true;
        }
        match &self.credential {
            CredentialInput::None => false,
            CredentialInput::Literal(s) => !s.is_empty(),
            // Both fields must be non-empty: an empty `OpRef { op: "",
            // path: "" }` reachable via partial picker round-trips or
            // programmatic state injection would otherwise pass the
            // save gate and write a broken reference into
            // `[workspaces.X.roles.Y.env]`. The launcher would only
            // surface "credential is unset" after the corrupt config
            // had already landed on disk.
            CredentialInput::OpRef(r) => !r.op.is_empty() && !r.path.is_empty(),
        }
    }

    /// Build the outcome for the parent to persist. Returns None if `!can_save`.
    pub fn commit(&self) -> Option<AuthFormOutcome> {
        if !self.can_save() {
            return None;
        }
        let mode = self.mode?;
        let env_var_name = self.kind.required_env_var(mode);
        let env_value = match &self.credential {
            CredentialInput::None => None,
            CredentialInput::Literal(s) => Some(EnvValue::Plain(s.clone())),
            CredentialInput::OpRef(r) => Some(EnvValue::OpRef(r.clone())),
        };
        Some(AuthFormOutcome {
            mode,
            env_var_name,
            env_value,
        })
    }
}

const fn mode_requires_credential(kind: AuthKind, mode: AuthMode) -> bool {
    kind.required_env_var(mode).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    fn dummy_op_ref() -> OpRef {
        OpRef {
            op: "op://uuid/test".into(),
            path: "Test/api/key".into(),
        }
    }

    #[test]
    fn save_disabled_when_mode_unset() {
        let f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        assert!(!f.can_save(), "no mode picked");
    }

    #[test]
    fn save_enabled_for_sync() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::Sync);
        assert!(f.can_save(), "sync needs no credential");
    }

    #[test]
    fn save_enabled_for_ignore() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::Ignore);
        assert!(f.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_without_credential() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        assert!(!f.can_save(), "api_key requires credential");
    }

    #[test]
    fn save_enabled_for_api_key_with_literal() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        assert!(f.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_with_empty_literal() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal(String::new());
        assert!(!f.can_save());
    }

    #[test]
    fn save_enabled_for_api_key_with_op_ref() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        f.set_op_ref(dummy_op_ref());
        assert!(f.can_save());
    }

    /// Empty `OpRef` (seeded by the credential-source toggle before
    /// the operator has picked a vault item) must NOT pass the save
    /// gate. Persisting such a reference into the env block would
    /// only blow up at launch time with "credential is unset", long
    /// after the broken config had been written to disk.
    #[test]
    fn save_disabled_for_api_key_with_empty_op_ref() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        // Both fields empty: rejected.
        f.set_op_ref(OpRef {
            op: String::new(),
            path: String::new(),
        });
        assert!(!f.can_save());
        // Empty `op` alone: rejected.
        f.set_op_ref(OpRef {
            op: String::new(),
            path: "Test/api/key".into(),
        });
        assert!(!f.can_save());
        // Empty `path` alone: rejected.
        f.set_op_ref(OpRef {
            op: "op://uuid/test".into(),
            path: String::new(),
        });
        assert!(!f.can_save());
    }

    #[test]
    fn mode_switch_to_sync_collapses_credential_block() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        assert!(f.shows_credential_block());
        f.set_mode(AuthMode::Sync);
        assert!(!f.shows_credential_block());
        // Credential is cleared on mode change-to-no-credential.
        assert_eq!(f.credential, CredentialInput::None);
    }

    #[test]
    fn codex_form_does_not_offer_oauth_token() {
        let f = AuthForm::new(AuthKind::Agent(Agent::Codex));
        let modes = f.available_modes();
        assert!(!modes.contains(&AuthMode::OAuthToken));
    }

    #[test]
    fn amp_form_does_not_offer_oauth_token() {
        let f = AuthForm::new(AuthKind::Agent(Agent::Amp));
        let modes = f.available_modes();
        assert!(!modes.contains(&AuthMode::OAuthToken));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_claude_api_key() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.mode, AuthMode::ApiKey);
        assert_eq!(outcome.env_var_name, Some("ANTHROPIC_API_KEY"));
        assert!(matches!(outcome.env_value, Some(EnvValue::Plain(ref s)) if s == "sk-ant-test"));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_claude_oauth_token() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::OAuthToken);
        f.set_op_ref(dummy_op_ref());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.env_var_name, Some("CLAUDE_CODE_OAUTH_TOKEN"));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_codex_api_key() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Codex));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal("sk-test".into());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.env_var_name, Some("OPENAI_API_KEY"));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_amp_api_key() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Amp));
        f.set_mode(AuthMode::ApiKey);
        f.set_literal("sgamp-test".into());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.env_var_name, Some("AMP_API_KEY"));
    }

    #[test]
    fn save_returns_none_for_sync_with_no_credential() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::Sync);
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.mode, AuthMode::Sync);
        assert_eq!(outcome.env_var_name, None);
        assert!(outcome.env_value.is_none());
    }

    #[test]
    fn from_existing_pre_populates_literal_credential() {
        let f = AuthForm::from_existing(
            AuthKind::Agent(Agent::Claude),
            AuthMode::ApiKey,
            Some(EnvValue::Plain("sk-ant-existing".into())),
        );
        assert_eq!(f.mode, Some(AuthMode::ApiKey));
        assert_eq!(
            f.credential,
            CredentialInput::Literal("sk-ant-existing".into())
        );
        assert!(f.can_save());
    }

    #[test]
    fn from_existing_pre_populates_op_ref_credential() {
        let r = dummy_op_ref();
        let f = AuthForm::from_existing(
            AuthKind::Agent(Agent::Claude),
            AuthMode::OAuthToken,
            Some(EnvValue::OpRef(r.clone())),
        );
        assert_eq!(f.mode, Some(AuthMode::OAuthToken));
        assert_eq!(f.credential, CredentialInput::OpRef(r));
    }

    /// The GitHub kind's mode picker is exactly `sync` / `token` /
    /// `ignore` — no `api_key`, no `oauth_token`.
    #[test]
    fn github_form_offers_sync_token_ignore_only() {
        let f = AuthForm::new(AuthKind::Github);
        let modes = f.available_modes();
        assert_eq!(modes, &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore]);
    }

    /// GitHub `token` mode requires a `GH_TOKEN` credential. Save must
    /// stay disabled until the operator picks one (literal or `OpRef`).
    #[test]
    fn github_token_mode_requires_credential_to_save() {
        let mut f = AuthForm::new(AuthKind::Github);
        f.set_mode(AuthMode::Token);
        assert!(!f.can_save(), "token mode must require GH_TOKEN");
        f.set_literal("ghp_xxxx".into());
        assert!(f.can_save());
    }

    /// GitHub `sync` and `ignore` save without a credential, mirroring
    /// Claude / Codex `sync` / `ignore`.
    #[test]
    fn github_sync_and_ignore_save_without_credential() {
        let mut f = AuthForm::new(AuthKind::Github);
        f.set_mode(AuthMode::Sync);
        assert!(f.can_save());
        f.set_mode(AuthMode::Ignore);
        assert!(f.can_save());
    }

    /// `commit()` for GitHub `token` mode emits `GH_TOKEN` as the env
    /// var name and the literal credential as the env value.
    #[test]
    fn github_token_commit_emits_gh_token_env_var() {
        let mut f = AuthForm::new(AuthKind::Github);
        f.set_mode(AuthMode::Token);
        f.set_literal("ghp_xxxx".into());
        let outcome = f.commit().expect("can_save → Some");
        assert_eq!(outcome.mode, AuthMode::Token);
        assert_eq!(outcome.env_var_name, Some("GH_TOKEN"));
        assert!(matches!(outcome.env_value, Some(EnvValue::Plain(ref s)) if s == "ghp_xxxx"));
    }

    struct FailRunner;
    impl OpRunner for FailRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("vault gone"))
        }
    }

    struct GoodRunner;
    impl OpRunner for GoodRunner {
        fn read(&self, _r: &str) -> anyhow::Result<String> {
            Ok("sk-ant-real".into())
        }
    }

    #[test]
    fn op_picker_failed_read_blocks_commit() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        let attempted = OpRef {
            op: "op://uuid/missing".into(),
            path: "Vault/Missing/field".into(),
        };
        let result = f.try_commit_op_ref(&FailRunner, attempted);
        assert!(result.is_err(), "failed op read must not commit");
        assert!(
            !f.can_save(),
            "form must not be commitable after failed read"
        );
        assert_eq!(f.credential, CredentialInput::None);
    }

    #[test]
    fn op_picker_successful_read_persists_op_ref() {
        let mut f = AuthForm::new(AuthKind::Agent(Agent::Claude));
        f.set_mode(AuthMode::ApiKey);
        let r = OpRef {
            op: "op://uuid/anthropic".into(),
            path: "Work/Anthropic/api-key".into(),
        };
        let result = f.try_commit_op_ref(&GoodRunner, r.clone());
        assert!(result.is_ok());
        assert!(f.can_save());
        let outcome = f.commit().unwrap();
        assert!(matches!(outcome.env_value, Some(EnvValue::OpRef(ref got)) if got == &r));
    }
}
