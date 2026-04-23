//! Env policy model: reserved runtime env vars and interpolation parsing.
//!
//! This module is the single source of truth for:
//!
//! * The names and default values of environment variables reserved by the
//!   jackin runtime — manifests may not declare these, and the runtime
//!   silently skips them if a resolved set contains them anyway.
//! * Parsing `${env.VAR_NAME}` interpolation references out of manifest
//!   strings.  Both manifest validation and runtime env resolution consume
//!   this helper so that they agree on what constitutes a reference.
//!
//! Previously these definitions lived in two places (`manifest::RESERVED_RUNTIME_ENV_VARS`
//! and `runtime::RUNTIME_OWNED_ENV_VARS`) with the runtime list being a
//! subset of the manifest list plus two inline `JACKIN_*` checks.  The list
//! here is the union — identical in membership to the previous manifest
//! constant — and the runtime now consults it through
//! [`is_reserved`] instead of maintaining its own.

/// Env var injected by jackin into every agent container so that child
/// processes can detect they are running inside a jackin-managed runtime.
pub const JACKIN_RUNTIME_ENV_NAME: &str = "JACKIN_CLAUDE_ENV";

/// Value set for [`JACKIN_RUNTIME_ENV_NAME`].  Manifests that try to declare
/// `JACKIN_CLAUDE_ENV` are rejected at validation time because the value is
/// fixed by jackin.
pub const JACKIN_RUNTIME_ENV_VALUE: &str = "jackin";

/// Env var that carries the `DinD` hostname into the agent container.
///
/// In-container tooling reaches the sibling docker-in-docker daemon through
/// this hostname.  The value is runtime-generated (derived from the container
/// name) and manifests may not override it.
pub const JACKIN_DIND_HOSTNAME_ENV_NAME: &str = "JACKIN_DIND_HOSTNAME";

/// Environment variables reserved by the jackin runtime.
///
/// Each entry is `(name, default)`.  `Some(value)` indicates a fixed value
/// assigned by jackin (currently only [`JACKIN_RUNTIME_ENV_NAME`]); `None`
/// indicates a runtime-generated value (hostname, Docker TLS paths, ...).
///
/// Manifests that declare any of these names fail validation, and the
/// runtime filter in `runtime::launch` skips them via [`is_reserved`] so
/// that a resolved env set cannot smuggle a value past validation.
pub(crate) const RESERVED_RUNTIME_ENV_VARS: &[(&str, Option<&str>)] = &[
    (JACKIN_RUNTIME_ENV_NAME, Some(JACKIN_RUNTIME_ENV_VALUE)),
    (JACKIN_DIND_HOSTNAME_ENV_NAME, None),
    // Docker TLS vars injected by jackin — must not be overridden by manifests.
    ("DOCKER_HOST", None),
    ("DOCKER_TLS_VERIFY", None),
    ("DOCKER_CERT_PATH", None),
];

/// Returns `true` if `name` appears in [`RESERVED_RUNTIME_ENV_VARS`].
///
/// Used by the runtime launch path to filter reserved names out of
/// resolved env sets before constructing `docker run -e` flags.
pub fn is_reserved(name: &str) -> bool {
    RESERVED_RUNTIME_ENV_VARS
        .iter()
        .any(|(reserved, _)| *reserved == name)
}

/// Extract env var names from `${env.VAR_NAME}` interpolation placeholders.
///
/// Returns the var name portion (after `env.`) for each match.  Non-`env.`
/// references like `${other.FOO}` are ignored — only the `env` namespace is
/// recognised for interpolation.
///
/// The scanning logic here mirrors `env_resolver::interpolate` — both parse
/// `${...}` the same way so that validation and runtime resolution agree on
/// what constitutes a reference.
pub(crate) fn extract_interpolation_refs(s: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find('}') {
            let ref_expr = &after_open[..end];
            if let Some(var_name) = ref_expr.strip_prefix("env.") {
                refs.push(var_name);
            }
            rest = &after_open[end + 1..];
        } else {
            break;
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_runtime_env_vars_covers_every_previously_reserved_name() {
        // Each name previously in manifest::RESERVED_RUNTIME_ENV_VARS
        // AND in runtime's old RUNTIME_OWNED_ENV_VARS must be present.
        let names: Vec<&str> = RESERVED_RUNTIME_ENV_VARS.iter().map(|(n, _)| *n).collect();
        for sentinel in &[
            "JACKIN_CLAUDE_ENV",    // was manifest JACKIN_RUNTIME_ENV_NAME value
            "JACKIN_DIND_HOSTNAME", // was manifest JACKIN_DIND_HOSTNAME_ENV_NAME value
            "DOCKER_HOST",
            "DOCKER_TLS_VERIFY",
            "DOCKER_CERT_PATH",
        ] {
            assert!(
                names.contains(sentinel),
                "reserved env list must include {sentinel} for previous manifest/runtime coverage"
            );
        }
    }

    #[test]
    fn is_reserved_accepts_all_sentinel_names() {
        for sentinel in &[
            "JACKIN_CLAUDE_ENV",
            "JACKIN_DIND_HOSTNAME",
            "DOCKER_HOST",
            "DOCKER_TLS_VERIFY",
            "DOCKER_CERT_PATH",
        ] {
            assert!(
                is_reserved(sentinel),
                "{sentinel} must be recognized as reserved"
            );
        }
    }

    #[test]
    fn is_reserved_rejects_user_names() {
        assert!(!is_reserved("MY_USER_VAR"));
        assert!(!is_reserved("PATH"));
        assert!(!is_reserved(""));
    }

    #[test]
    fn extract_interpolation_refs_finds_single_ref() {
        assert_eq!(
            extract_interpolation_refs("Branch for ${env.PROJECT}:"),
            vec!["PROJECT"]
        );
    }

    #[test]
    fn extract_interpolation_refs_finds_multiple_refs() {
        assert_eq!(
            extract_interpolation_refs("${env.TEAM}/${env.PROJECT}"),
            vec!["TEAM", "PROJECT"]
        );
    }

    #[test]
    fn extract_interpolation_refs_returns_empty_for_no_refs() {
        assert!(extract_interpolation_refs("plain text").is_empty());
    }

    #[test]
    fn extract_interpolation_refs_ignores_non_env_namespace() {
        assert!(extract_interpolation_refs("${other.FOO}").is_empty());
        assert!(extract_interpolation_refs("${FOO}").is_empty());
    }

    #[test]
    fn extract_interpolation_refs_returns_empty_name_for_empty_env_ref() {
        assert_eq!(extract_interpolation_refs("${env.}"), vec![""]);
    }

    #[test]
    fn extract_interpolation_refs_handles_unclosed_brace() {
        assert!(extract_interpolation_refs("${env.OPEN").is_empty());
    }
}
