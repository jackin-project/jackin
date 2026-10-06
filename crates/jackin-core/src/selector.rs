// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Role and container selector types used across the CLI, workspace
//! resolution, and launch pipeline to identify the operator's target.
//!
//! The `RoleChoice` trait impl (`impl RoleChoice for RoleSelector`) lives in
//! `jackin-console` (where `RoleChoice` is defined), not here, to satisfy the
//! orphan rule.

use std::fmt;
use thiserror::Error;

use crate::constants::CONTAINER_PREFIX_DASH;

/// Top-level selector: either a role (by org/name) or a bare container name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// Role identified by optional namespace and name.
    Role(RoleSelector),
    /// Bare Docker container name (must use the jackin container prefix).
    Container(String),
}

/// Identifies a role by optional namespace and name (e.g. `chainargos/the-architect`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleSelector {
    /// Optional org/namespace segment before `/`.
    pub namespace: Option<String>,
    /// Role name segment.
    pub name: String,
}

impl fmt::Display for RoleSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(namespace) = &self.namespace {
            write!(f, "{namespace}/{}", self.name)
        } else {
            f.write_str(&self.name)
        }
    }
}

/// Why a selector string could not be parsed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SelectorError {
    /// Input was empty.
    #[error("selector cannot be empty")]
    Empty,
    /// Input failed segment/container-name validation.
    #[error("invalid selector: {0}")]
    Invalid(String),
}

impl RoleSelector {
    /// Construct a role selector from optional namespace and name (already validated).
    pub fn new(namespace: Option<&str>, name: &str) -> Self {
        Self {
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    /// Parse a role selector. Input is lowercased before validation so
    /// `ChainArgos/Agent-Brown` and `chainargos/agent-brown` both produce
    /// the same `RoleSelector`. This matches GitHub's case-insensitive
    /// org/user routing and the Docker constraint that container/image
    /// names must be lowercase. Display names live in the manifest's
    /// `[identity].name` field, so case preservation has its own slot.
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        if input.is_empty() {
            return Err(SelectorError::Empty);
        }

        let normalized = input.to_ascii_lowercase();
        let input = normalized.as_str();

        if !input.contains('/') {
            return (is_valid_role_segment(input) && !is_reserved_builtin_role_name(input))
                .then(|| Self::new(None, input))
                .ok_or_else(|| SelectorError::Invalid(input.to_owned()));
        }

        let mut parts = input.split('/');
        if let (Some(namespace), Some(name), None) = (parts.next(), parts.next(), parts.next())
            && is_valid_role_segment(namespace)
            && is_valid_role_segment(name)
        {
            return Ok(Self::new(Some(namespace), name));
        }

        Err(SelectorError::Invalid(input.to_owned()))
    }

    /// Canonical key string (`name` or `namespace/name`).
    pub fn key(&self) -> String {
        self.to_string()
    }
}

/// Derive the role's canonical runtime slug (used for image-tag and
/// repo-lock-file naming). A namespaced role becomes `namespace_name`;
/// a bare role becomes `name`.
pub fn runtime_slug(selector: &RoleSelector) -> String {
    selector.namespace.as_ref().map_or_else(
        || selector.name.clone(),
        |namespace| format!("{namespace}_{}", selector.name),
    )
}

impl TryFrom<&str> for RoleSelector {
    type Error = SelectorError;

    /// Idiomatic wrapper around [`RoleSelector::parse`]. Exists so callers
    /// that rely on `TryFrom` conversion traits (including generic code and
    /// `try_into()` call sites) can convert a `&str` without having to
    /// reach for the inherent `parse` method.
    fn try_from(input: &str) -> Result<Self, Self::Error> {
        Self::parse(input)
    }
}

impl Selector {
    /// Parse either a container name (jackin prefix) or a role selector.
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        if input.is_empty() {
            return Err(SelectorError::Empty);
        }

        if is_valid_container_name(input) {
            return Ok(Self::Container(input.to_owned()));
        }

        Ok(Self::Role(RoleSelector::parse(input)?))
    }
}

impl TryFrom<&str> for Selector {
    type Error = SelectorError;

    /// Idiomatic wrapper around [`Selector::parse`]. See the analogous impl
    /// on [`RoleSelector`] for rationale.
    fn try_from(input: &str) -> Result<Self, Self::Error> {
        Self::parse(input)
    }
}

fn is_valid_role_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn is_valid_container_name(value: &str) -> bool {
    value
        .strip_prefix(CONTAINER_PREFIX_DASH)
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.chars().all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_'
                })
        })
}

fn is_reserved_builtin_role_name(value: &str) -> bool {
    value.starts_with(CONTAINER_PREFIX_DASH)
}
