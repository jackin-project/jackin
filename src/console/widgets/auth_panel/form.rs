//! Auth-form state and save invariants.
//!
//! State-only — rendering and op-picker integration land in Task 18.
//! The form is mounted by the workspace-manager TUI in Task 19.

use crate::agent::Agent;
use crate::config::AuthForwardMode;
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
#[derive(Debug)]
pub struct AuthForm {
    pub agent: Agent,
    pub mode: Option<AuthForwardMode>,
    pub credential: CredentialInput,
}

/// Output of a successful commit. The parent uses these fields to write the
/// agent block at the chosen layer and the env-var entry under the role's env
/// block.
#[derive(Debug, Clone)]
pub struct AuthFormOutcome {
    pub mode: AuthForwardMode,
    pub env_var_name: Option<&'static str>,
    pub env_value: Option<EnvValue>,
}

impl AuthForm {
    pub const fn new(agent: Agent) -> Self {
        Self {
            agent,
            mode: None,
            credential: CredentialInput::None,
        }
    }

    /// Pre-populate the form from an existing row's mode and credential.
    /// Used when [edit] is pressed on a row that already has values.
    pub fn from_existing(
        agent: Agent,
        mode: AuthForwardMode,
        credential: Option<EnvValue>,
    ) -> Self {
        let credential = match credential {
            None => CredentialInput::None,
            Some(EnvValue::Plain(s)) => CredentialInput::Literal(s),
            Some(EnvValue::OpRef(r)) => CredentialInput::OpRef(r),
        };
        Self {
            agent,
            mode: Some(mode),
            credential,
        }
    }

    /// Set the mode. If switching to a mode that doesn't need a credential,
    /// clears the credential field automatically.
    pub fn set_mode(&mut self, mode: AuthForwardMode) {
        self.mode = Some(mode);
        if !mode_requires_credential(self.agent, mode) {
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
        matches!(self.mode, Some(m) if mode_requires_credential(self.agent, m))
    }

    /// Modes the user can pick. Codex omits `OAuthToken` (parser-rejected by Task 6).
    pub const fn available_modes(&self) -> &'static [AuthForwardMode] {
        self.agent.supported_modes()
    }

    /// Save invariant: mode is committed AND, if needed, a non-empty credential.
    pub const fn can_save(&self) -> bool {
        let Some(mode) = self.mode else { return false };
        if !mode_requires_credential(self.agent, mode) {
            return true;
        }
        match &self.credential {
            CredentialInput::None => false,
            CredentialInput::Literal(s) => !s.is_empty(),
            // Both fields must be non-empty: the empty `OpRef { op: "",
            // path: "" }` seeded by the credential-source toggle would
            // otherwise pass the save gate and write a broken reference
            // into `[workspaces.X.roles.Y.env]`. The launcher would then
            // fail with "credential is unset" only after the operator
            // had already committed the corrupt config to disk.
            CredentialInput::OpRef(r) => !r.op.is_empty() && !r.path.is_empty(),
        }
    }

    /// Build the outcome for the parent to persist. Returns None if `!can_save`.
    pub fn commit(&self) -> Option<AuthFormOutcome> {
        if !self.can_save() {
            return None;
        }
        let mode = self.mode?;
        let env_var_name = self.agent.required_env_var(mode);
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

const fn mode_requires_credential(agent: Agent, mode: AuthForwardMode) -> bool {
    agent.required_env_var(mode).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_op_ref() -> OpRef {
        OpRef {
            op: "op://uuid/test".into(),
            path: "Test/api/key".into(),
        }
    }

    #[test]
    fn save_disabled_when_mode_unset() {
        let f = AuthForm::new(Agent::Claude);
        assert!(!f.can_save(), "no mode picked");
    }

    #[test]
    fn save_enabled_for_sync() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::Sync);
        assert!(f.can_save(), "sync needs no credential");
    }

    #[test]
    fn save_enabled_for_ignore() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::Ignore);
        assert!(f.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_without_credential() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        assert!(!f.can_save(), "api_key requires credential");
    }

    #[test]
    fn save_enabled_for_api_key_with_literal() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        assert!(f.can_save());
    }

    #[test]
    fn save_disabled_for_api_key_with_empty_literal() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        f.set_literal(String::new());
        assert!(!f.can_save());
    }

    #[test]
    fn save_enabled_for_api_key_with_op_ref() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
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
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        // Both fields empty (the toggle-radio seed): rejected.
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
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        assert!(f.shows_credential_block());
        f.set_mode(AuthForwardMode::Sync);
        assert!(!f.shows_credential_block());
        // Credential is cleared on mode change-to-no-credential.
        assert_eq!(f.credential, CredentialInput::None);
    }

    #[test]
    fn codex_form_does_not_offer_oauth_token() {
        let f = AuthForm::new(Agent::Codex);
        let modes = f.available_modes();
        assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_claude_api_key() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
        f.set_literal("sk-ant-test".into());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.mode, AuthForwardMode::ApiKey);
        assert_eq!(outcome.env_var_name, Some("ANTHROPIC_API_KEY"));
        assert!(matches!(outcome.env_value, Some(EnvValue::Plain(ref s)) if s == "sk-ant-test"));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_claude_oauth_token() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::OAuthToken);
        f.set_op_ref(dummy_op_ref());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.env_var_name, Some("CLAUDE_CODE_OAUTH_TOKEN"));
    }

    #[test]
    fn save_emits_correct_env_var_name_for_codex_api_key() {
        let mut f = AuthForm::new(Agent::Codex);
        f.set_mode(AuthForwardMode::ApiKey);
        f.set_literal("sk-test".into());
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.env_var_name, Some("OPENAI_API_KEY"));
    }

    #[test]
    fn save_returns_none_for_sync_with_no_credential() {
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::Sync);
        let outcome = f.commit().unwrap();
        assert_eq!(outcome.mode, AuthForwardMode::Sync);
        assert_eq!(outcome.env_var_name, None);
        assert!(outcome.env_value.is_none());
    }

    #[test]
    fn from_existing_pre_populates_literal_credential() {
        let f = AuthForm::from_existing(
            Agent::Claude,
            AuthForwardMode::ApiKey,
            Some(EnvValue::Plain("sk-ant-existing".into())),
        );
        assert_eq!(f.mode, Some(AuthForwardMode::ApiKey));
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
            Agent::Claude,
            AuthForwardMode::OAuthToken,
            Some(EnvValue::OpRef(r.clone())),
        );
        assert_eq!(f.mode, Some(AuthForwardMode::OAuthToken));
        assert_eq!(f.credential, CredentialInput::OpRef(r));
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
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
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
        let mut f = AuthForm::new(Agent::Claude);
        f.set_mode(AuthForwardMode::ApiKey);
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
