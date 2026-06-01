/// Structured parts of an `op://...` reference.
///
/// Syntax: `op://<vault>/<item>/[<section>/]<field>`. Account scope is
/// not encoded in the path; multi-account picks live separately on the
/// selected account. See
/// <https://developer.1password.com/docs/cli/secret-reference-syntax/>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpReferenceParts {
    pub vault: String,
    pub item: String,
    pub section: Option<String>,
    pub field: String,
}

impl OpReferenceParts {
    /// Operator-facing copy-pasteable `op item delete` invocation.
    pub fn manual_delete_hint(&self) -> impl std::fmt::Display + '_ {
        struct Hint<'a> {
            item: &'a str,
            vault: &'a str,
        }
        impl std::fmt::Display for Hint<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "op item delete {} --vault {}", self.item, self.vault)
            }
        }
        Hint {
            item: &self.item,
            vault: &self.vault,
        }
    }
}

#[must_use]
pub fn parse_op_reference(value: &str) -> Option<OpReferenceParts> {
    let path = value.strip_prefix("op://")?;
    let path = path.split('?').next().unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|s| s.is_empty()) {
        return None;
    }
    match parts.as_slice() {
        [vault, item, field] => Some(OpReferenceParts {
            vault: (*vault).to_string(),
            item: (*item).to_string(),
            section: None,
            field: (*field).to_string(),
        }),
        [vault, item, section, field] => Some(OpReferenceParts {
            vault: (*vault).to_string(),
            item: (*item).to_string(),
            section: Some((*section).to_string()),
            field: (*field).to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_op_reference_three_segments() {
        let parts = parse_op_reference("op://Vault/Item/field").unwrap();
        assert_eq!(parts.vault, "Vault");
        assert_eq!(parts.item, "Item");
        assert_eq!(parts.section, None);
        assert_eq!(parts.field, "field");
    }

    #[test]
    fn parse_op_reference_handles_section_in_four_segments() {
        let parts = parse_op_reference("op://Personal/Item/Auth/password").unwrap();
        assert_eq!(parts.vault, "Personal");
        assert_eq!(parts.item, "Item");
        assert_eq!(parts.section, Some("Auth".to_string()));
        assert_eq!(parts.field, "password");
    }

    #[test]
    fn parse_op_reference_strips_query_suffix() {
        let parts = parse_op_reference("op://Vault/Item/token?attribute=otp").unwrap();
        assert_eq!(parts.field, "token");
        assert_eq!(parts.section, None);

        let parts = parse_op_reference("op://Vault/Item/Auth/key?ssh-format=openssh").unwrap();
        assert_eq!(parts.section, Some("Auth".to_string()));
        assert_eq!(parts.field, "key");
    }

    #[test]
    fn parse_op_reference_invalid_segment_count() {
        assert!(parse_op_reference("plain").is_none());
        assert!(parse_op_reference("op://only/two").is_none());
        assert!(parse_op_reference("op://a/b/c/d/e").is_none());
        assert!(parse_op_reference("op://").is_none());
        assert!(parse_op_reference("op:////").is_none());
        assert!(parse_op_reference("op://vault//field").is_none());
    }

    #[test]
    fn op_reference_parts_manual_delete_hint_renders_canonical_cli() {
        let parts = parse_op_reference("op://VAULT_UUID/ITEM_UUID/FIELD").unwrap();
        assert_eq!(
            parts.manual_delete_hint().to_string(),
            "op item delete ITEM_UUID --vault VAULT_UUID",
        );
    }
}
