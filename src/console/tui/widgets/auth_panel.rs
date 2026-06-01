//! Root bindings for the console-local auth panel component.

use crate::operator_env::{EnvValue, OpRef};

pub type AuthForm = jackin_console::tui::components::auth_panel::AuthForm<EnvValue>;

pub use jackin_console::tui::components::auth_panel::{CredentialInput, required_height};
pub(crate) use jackin_console::tui::components::auth_panel::mode_str;
pub use jackin_console::tui::components::auth_panel::render_form;

impl jackin_console::tui::components::auth_panel::AuthCredentialRef for OpRef {
    fn path(&self) -> &str {
        &self.path
    }

    fn is_empty(&self) -> bool {
        self.op.is_empty() || self.path.is_empty()
    }
}

impl jackin_console::tui::components::auth_panel::AuthCredential for EnvValue {
    type Ref = OpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}
