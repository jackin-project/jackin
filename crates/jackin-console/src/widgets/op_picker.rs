//! Shared 1Password picker modal state enums.

/// Browse-only vs. creation-enabled picker mode.
#[derive(Debug, Clone)]
pub enum OpPickerMode {
    /// Pick an existing field only.
    Browse,
    /// Enable item/field/section creation rows and naming sub-stages.
    Create {
        item_name_default: String,
        field_label_default: String,
    },
}

impl OpPickerMode {
    pub const fn is_create(&self) -> bool {
        matches!(self, Self::Create { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Account,
    Vault,
    Item,
    Section,
    Field,
    NewItemName,
    FieldLabel,
    NewSectionName,
}

#[derive(Debug, Clone)]
pub enum OpLoadState {
    Idle,
    Loading { spinner_tick: u8 },
    Ready,
    Error(OpPickerError),
}

#[derive(Debug, Clone)]
pub enum OpPickerError {
    Fatal(OpPickerFatalState),
    Recoverable { message: String },
}

#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    NotInstalled,
    NotSignedIn,
    NoVaults,
    GenericFatal { message: String },
}

/// A single row in the field-picker display list.
#[derive(Debug, Clone)]
pub enum FieldDisplayRow {
    /// A collapsible section header derived from the `op://` reference.
    SectionHeader { name: String, field_count: usize },
    /// A selectable field row. The index points into the filtered fields.
    Field { field_idx: usize },
    /// `+ New field` creation row.
    NewFieldSentinel,
    /// `+ New section` creation row.
    NewSectionSentinel,
}

/// Multi-account titles lead with the chosen account's email so the
/// operator can see which account they're drilling into; single-account
/// titles omit it.
pub fn breadcrumb_title(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "1Password".to_string(),
        OpPickerStage::Vault => {
            if multi_account {
                account_email.to_string()
            } else {
                "1Password".to_string()
            }
        }
        OpPickerStage::Item
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_string()
            }
        }
        OpPickerStage::Section | OpPickerStage::Field => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name} \u{2192} {item_name}")
            } else {
                format!("{vault_name} \u{2192} {item_name}")
            }
        }
    }
}

/// Classifies by stderr substring because the root picker receives
/// process errors through `anyhow::Error` rather than typed variants.
pub fn classify_probe_error_message(message: impl Into<String>) -> OpPickerError {
    let message = message.into();
    if message.contains("failed to spawn") {
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    } else if message.contains("not signed in")
        || message.contains("not currently signed")
        || message.contains("no accounts")
    {
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    } else {
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumb_omits_pane_type_suffix_multi_account() {
        let title = breadcrumb_title(
            OpPickerStage::Vault,
            true,
            "alice@example.com",
            "ignored",
            "ignored",
        );
        assert_eq!(title, "alice@example.com");
        assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

        let title = breadcrumb_title(
            OpPickerStage::Item,
            true,
            "alice@example.com",
            "Personal",
            "",
        );
        assert_eq!(title, "alice@example.com \u{2192} Personal");
        assert!(!title.contains("Items"));

        let title = breadcrumb_title(
            OpPickerStage::Field,
            true,
            "alice@example.com",
            "Personal",
            "API Keys",
        );
        assert_eq!(
            title,
            "alice@example.com \u{2192} Personal \u{2192} API Keys"
        );
        assert!(!title.contains("Fields"));
    }

    #[test]
    fn breadcrumb_single_account_uses_brand_or_bare_context() {
        let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
        assert_eq!(v, "1Password");

        let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
        assert_eq!(i, "Personal");

        let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
        assert_eq!(f, "Personal \u{2192} API Keys");
    }

    #[test]
    fn breadcrumb_account_pane_is_bare_brand() {
        let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
        assert_eq!(title, "1Password");
    }

    #[test]
    fn probe_error_message_classifies_operator_states() {
        assert!(matches!(
            classify_probe_error_message("failed to spawn op"),
            OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
        ));
        assert!(matches!(
            classify_probe_error_message("not currently signed in"),
            OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
        ));
        assert!(matches!(
            classify_probe_error_message("boom"),
            OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
        ));
    }
}
