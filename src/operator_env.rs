//! Operator-controlled env resolution: four config layers, three value
//! syntaxes (`op://`, `$NAME` / `${NAME}`, literal), and merging onto
//! the manifest-resolved env at launch.

/// Test seam for the `op` CLI subprocess.
///
/// Production code uses [`OpCli`] which shells out to `op read`; tests
/// use a mock implementation that captures inputs and returns canned
/// responses.
pub trait OpRunner {
    /// Resolve a single `op://...` reference to its secret value.
    fn read(&self, reference: &str) -> anyhow::Result<String>;
}

/// Dispatch a single env value string to the appropriate resolver.
///
/// * `op://...`              → `op_runner.read(value)`
/// * `$NAME` or `${NAME}`    → `host_env(name)`
/// * anything else           → returned verbatim as a literal
///
/// `layer_label` and `var_name` are used only for error messages so
/// operators can locate the offending config line (e.g. `"workspace
/// \"big-monorepo\" env var \"API_TOKEN\""`).
pub fn dispatch_value(
    layer_label: &str,
    var_name: &str,
    value: &str,
    op_runner: &impl OpRunner,
    host_env: impl FnOnce(&str) -> Result<String, std::env::VarError>,
) -> anyhow::Result<String> {
    if value.starts_with("op://") {
        return op_runner.read(value).map_err(|e| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: 1Password reference {value:?} failed: {e}"
            )
        });
    }

    if let Some(host_name) = parse_host_ref(value) {
        return host_env(host_name).map_err(|_| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: host env var {host_name:?} is not set"
            )
        });
    }

    Ok(value.to_string())
}

/// Parse `$NAME` or `${NAME}` and return the name. Returns `None` for
/// any other string (including bare `$`, `${}`, partially braced like
/// `${NAME`, and anything containing whitespace or non-identifier
/// characters after the sigil).
fn parse_host_ref(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix("${")
        && let Some(name) = rest.strip_suffix('}')
        && is_valid_env_name(name)
    {
        return Some(name);
    }

    if let Some(name) = value.strip_prefix('$')
        && !name.is_empty()
        && is_valid_env_name(name)
    {
        return Some(name);
    }

    None
}

/// A valid POSIX-ish env name: ASCII letter or `_`, followed by ASCII
/// alphanumeric or `_`. Empty names are rejected.
fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_literal_value_returns_literal() {
        let out = dispatch_value(
            "global",
            "FOO",
            "plain-literal",
            &TestOpRunner::forbidden(),
            |n| panic!("host env should not be queried for literal; got {n}"),
        )
        .unwrap();
        assert_eq!(out, "plain-literal");
    }

    #[test]
    fn dispatch_host_ref_dollar_name_reads_host_env() {
        let out = dispatch_value(
            "global",
            "MY_VAR",
            "$OPERATOR_HOST_SOURCE",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_SOURCE");
                Ok("from-host".to_string())
            },
        )
        .unwrap();
        assert_eq!(out, "from-host");
    }

    #[test]
    fn dispatch_host_ref_braced_reads_host_env() {
        let out = dispatch_value(
            "global",
            "MY_VAR",
            "${OPERATOR_HOST_SOURCE}",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_SOURCE");
                Ok("braced".to_string())
            },
        )
        .unwrap();
        assert_eq!(out, "braced");
    }

    #[test]
    fn dispatch_host_ref_empty_string_passes_through() {
        // Spec: empty string host-env result is "set but empty" and
        // passes through unchanged (Unix semantics). Differentiates
        // from VarError::NotPresent, which is a hard error.
        let out = dispatch_value(
            "global",
            "MAYBE_EMPTY",
            "$OPERATOR_HOST_EMPTY",
            &TestOpRunner::forbidden(),
            |name| {
                assert_eq!(name, "OPERATOR_HOST_EMPTY");
                Ok(String::new())
            },
        )
        .unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn dispatch_host_ref_missing_returns_clear_error() {
        let err = dispatch_value(
            "workspace \"big-monorepo\"",
            "MY_VAR",
            "$MISSING_HOST_VAR",
            &TestOpRunner::forbidden(),
            |_| Err(std::env::VarError::NotPresent),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("MY_VAR"), "expected var name in error: {msg}");
        assert!(
            msg.contains("MISSING_HOST_VAR"),
            "expected host var name in error: {msg}"
        );
        assert!(
            msg.contains("workspace \"big-monorepo\""),
            "expected layer name in error: {msg}"
        );
    }

    #[test]
    fn dispatch_op_ref_invokes_op_cli() {
        let runner = TestOpRunner::new(Ok("tok-abc".to_string()));
        let out = dispatch_value(
            "agent \"agent-smith\"",
            "API_TOKEN",
            "op://Personal/api/token",
            &runner,
            |_| panic!("host env should not be queried for op:// refs"),
        )
        .unwrap();
        assert_eq!(out, "tok-abc");
        assert_eq!(
            runner.last_ref().as_deref(),
            Some("op://Personal/api/token")
        );
    }

    /// Test seam: an `OpRunner` that captures the last `op read` argument.
    struct TestOpRunner {
        response: std::cell::RefCell<Option<anyhow::Result<String>>>,
        last_ref: std::cell::RefCell<Option<String>>,
    }

    impl TestOpRunner {
        fn new(response: anyhow::Result<String>) -> Self {
            Self {
                response: std::cell::RefCell::new(Some(response)),
                last_ref: std::cell::RefCell::new(None),
            }
        }

        fn forbidden() -> Self {
            Self {
                response: std::cell::RefCell::new(None),
                last_ref: std::cell::RefCell::new(None),
            }
        }

        fn last_ref(&self) -> std::cell::Ref<'_, Option<String>> {
            self.last_ref.borrow()
        }
    }

    impl OpRunner for TestOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            *self.last_ref.borrow_mut() = Some(reference.to_string());
            match self.response.borrow_mut().take() {
                Some(r) => r,
                None => panic!("op CLI should not have been invoked"),
            }
        }
    }
}
