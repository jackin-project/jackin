// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential slot layout and directory mapping.

use crate::InstanceAuthBinding;

/// Container-visible layout for one provisioned slot, derived from the
/// legacy per-agent dirs plus the slot suffix.
#[derive(Debug)]
pub struct SlotLayout {
    pub suffix: Option<String>,
    pub home_rel: String,
    pub store_rel: String,
    pub folder_target: String,
}

/// Sanitize a binding key into a directory-name suffix: keep
/// alphanumerics plus `-_.`, fold anything else (notably `@` in
/// synthesized `{account}@{agent}` keys) to `-`.
pub(crate) fn sanitize_slot_suffix(key: &str) -> String {
    let sanitized: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "slot".to_owned()
    } else {
        sanitized
    }
}

/// Apply a slot suffix to a store dir name (`claude` →
/// `claude-<suffix>`); `None` keeps the legacy name.
pub fn slot_store_rel(store: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(suffix) => format!("{store}-{suffix}"),
        None => store.to_owned(),
    }
}

/// Apply a slot suffix to the last component of a home-relative dir
/// (`.local/share/amp` → `.local/share/amp-<suffix>`); `None` keeps
/// the legacy path.
#[must_use]
pub fn slot_home_rel(rel: &str, suffix: Option<&str>) -> String {
    let Some(suffix) = suffix else {
        return rel.to_owned();
    };
    match rel.rsplit_once('/') {
        Some((parent, leaf)) => format!("{parent}/{leaf}-{suffix}"),
        None => format!("{rel}-{suffix}"),
    }
}

pub fn xdg_root_agent(agent: jackin_core::Agent) -> bool {
    use jackin_core::FolderVarKind;

    matches!(
        agent.runtime().state_paths().folder_env_var,
        Some(var) if matches!(var.kind, FolderVarKind::XdgRoot)
    )
}

pub fn xdg_cache_rel(agent: jackin_core::Agent, suffix: Option<&str>) -> Option<String> {
    xdg_root_agent(agent).then(|| slot_home_rel(&format!(".cache/{}", agent.slug()), suffix))
}

/// Slot home rel + folder-var target for `agent`, honoring the
/// folder-var kind. `Dir` agents point at their config home; `Parent`
/// agents (`GEMINI_CLI_HOME`) point at a unique parent whose
/// `credential_dir` child is the home; `XdgRoot` agents point at the
/// XDG data root (the parent of the credential directory), matching the
/// client's `XDG_DATA_HOME + <agent>/` lookup. Agents without a folder var
/// resolve the plain home; the target is unused because they admit one slot.
pub fn slot_home_and_target(
    agent: jackin_core::Agent,
    home_rel: &str,
    suffix: Option<&str>,
) -> (String, String) {
    use jackin_core::FolderVarKind;
    let kind = agent
        .runtime()
        .state_paths()
        .folder_env_var
        .map(|var| var.kind);
    match (kind, suffix) {
        (Some(FolderVarKind::Parent), Some(suffix)) => {
            let stem = home_rel.split('/').next().unwrap_or(home_rel);
            (
                format!("{stem}-{suffix}/{home_rel}"),
                format!("/home/agent/{stem}-{suffix}"),
            )
        }
        (Some(FolderVarKind::Parent), None) => (home_rel.to_owned(), "/home/agent".to_owned()),
        (Some(FolderVarKind::XdgRoot), _) => {
            // The client appends its own durable subdirectory (`amp/` or
            // `opencode/`) to XDG_DATA_HOME. Exporting only `.local` would
            // leave the runtime variable and the mounted `.local/share/...`
            // tree describing different paths.
            let root = home_rel
                .rsplit_once('/')
                .map_or(home_rel, |(parent, _)| parent);
            (
                slot_home_rel(home_rel, suffix),
                format!("/home/agent/{root}"),
            )
        }
        _ => {
            let home_rel = slot_home_rel(home_rel, suffix);
            let folder_target = format!("/home/agent/{home_rel}");
            (home_rel, folder_target)
        }
    }
}

/// Per-binding slot suffixes in binding order: the first binding per
/// agent keeps the legacy layout (`None`); later same-agent bindings
/// get their sanitized key as suffix. Shared by foreground prepare and
/// background prewarm so shared keys land in identical dirs.
/// Sanitized keys that collide (`a@b` vs `a-b`) get a numeric tail so
/// two slots never share a directory.
pub fn slot_suffixes(bindings: &[InstanceAuthBinding]) -> Vec<Option<String>> {
    let mut primaried: std::collections::HashSet<jackin_core::Agent> =
        std::collections::HashSet::new();
    let mut used: std::collections::HashSet<(jackin_core::Agent, String)> =
        std::collections::HashSet::new();
    bindings
        .iter()
        .map(|binding| {
            if primaried.insert(binding.agent) {
                return None;
            }
            let base = sanitize_slot_suffix(&binding.key);
            let mut candidate = base.clone();
            let mut tail = 2;
            while !used.insert((binding.agent, candidate.clone())) {
                candidate = format!("{base}-{tail}");
                tail += 1;
            }
            Some(candidate)
        })
        .collect()
}

pub fn slot_layout(
    agent: jackin_core::Agent,
    store: &str,
    home_rel: &str,
    suffix: Option<&str>,
) -> SlotLayout {
    let (home_rel, folder_target) = slot_home_and_target(agent, home_rel, suffix);
    SlotLayout {
        folder_target,
        suffix: suffix.map(str::to_owned),
        home_rel,
        store_rel: slot_store_rel(store, suffix),
    }
}

/// Legacy store dir + home rel per agent, the single source the
/// provision, skip, and ignore-check paths derive slot dirs from.
pub fn agent_slot_dirs(agent: jackin_core::Agent) -> (&'static str, &'static str) {
    match agent {
        jackin_core::Agent::Claude => ("claude", ".claude"),
        jackin_core::Agent::Codex => ("codex", ".codex"),
        jackin_core::Agent::Amp => ("amp", ".local/share/amp"),
        jackin_core::Agent::Kimi => ("kimi-code", ".kimi-code"),
        jackin_core::Agent::Opencode => ("opencode", ".local/share/opencode"),
        jackin_core::Agent::Grok => ("grok", ".grok"),
        jackin_core::Agent::Antigravity => ("antigravity", ".gemini/antigravity-cli"),
        jackin_core::Agent::Gemini => ("gemini", ".gemini"),
        jackin_core::Agent::Cursor => ("cursor", ".cursor"),
        jackin_core::Agent::Muse => ("muse", ".config/muse"),
        jackin_core::Agent::Omp => ("omp", ".omp"),
        jackin_core::Agent::Hermes => ("hermes", ".hermes"),
    }
}
