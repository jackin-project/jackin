// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Token, lex_rust};
use crate::repo::{Dependency, DependencyKind, FileKind, Member, Repo};
use crate::violation::{Violation, sorted};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerRule {
    pub member: &'static str,
    pub depends: &'static [&'static str],
    pub dev_depends: &'static [&'static str],
    pub imports: &'static [&'static str],
}

pub const GATEWAY_FAMILIES: &[&str] = &["openshell", "tonic", "russh"];

pub const EXPOSE_ALLOWLIST: &[&str] = &[
    "crates/jackin-core/src/secret.rs",
    "crates/jackin-openshell/src/discovery.rs",
    "crates/jackin-openshell/src/grpc/program.rs",
    "crates/jackin-openshell/src/grpc/provider.rs",
    "crates/jackin-session/src/credentials.rs",
    "crates/jackin-session/src/host.rs",
    "crates/jackin-session/src/op.rs",
    "crates/jackin-testkit/src/gateway.rs",
    "crates/jackin-testkit/src/gateway/exec.rs",
    "crates/jackin-testkit/src/gateway/files.rs",
    "crates/jackin-testkit/src/host/runner.rs",
    "crates/usage/jackin-usage/src/credential.rs",
    "crates/usage/jackin-usage/src/dispatch.rs",
    "crates/xtask/src/e2e/a1.rs",
    "crates/xtask/src/e2e/credential.rs",
    "crates/xtask/src/e2e/redact.rs",
];

pub const TERMINAL_ENGINE_CRATES: &[&str] = &["termpane", "vt100"];

pub const HTTP_CLIENT_CRATE: &str = "reqwest";

pub const HTTP_CLIENT_OWNER: &str = "jackin-usage-provider-core";

const USAGE_PROVIDERS: [&str; 10] = [
    "jackin-usage-provider-claude",
    "jackin-usage-provider-codex",
    "jackin-usage-provider-cursor",
    "jackin-usage-provider-gemini",
    "jackin-usage-provider-grok",
    "jackin-usage-provider-kimi",
    "jackin-usage-provider-minimax",
    "jackin-usage-provider-opencode",
    "jackin-usage-provider-openrouter",
    "jackin-usage-provider-zai",
];

const fn provider(member: &'static str) -> LayerRule {
    LayerRule {
        member,
        depends: &["jackin-usage-provider-core"],
        dev_depends: &[],
        imports: &[],
    }
}

pub const LAYERS: &[LayerRule] = &[
    LayerRule {
        member: "jackin-core",
        depends: &[],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    LayerRule {
        member: "jackin-image",
        depends: &["jackin-core"],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    LayerRule {
        member: "jackin-openshell",
        depends: &["jackin-core"],
        dev_depends: &["jackin-testkit"],
        imports: GATEWAY_FAMILIES,
    },
    LayerRule {
        member: "jackin-session",
        depends: &["jackin-core", "jackin-image", "jackin-openshell"],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    LayerRule {
        member: "jackin-tui",
        depends: &[
            "jackin-console",
            "jackin-core",
            "jackin-image",
            "jackin-openshell",
            "jackin-session",
            "jackin-usage",
        ],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    LayerRule {
        member: "jackin-testkit",
        depends: &[
            "jackin-console",
            "jackin-core",
            "jackin-image",
            "jackin-openshell",
            "jackin-session",
            "jackin-tui",
            "jackin-usage",
        ],
        dev_depends: &[],
        imports: &[],
    },
    LayerRule {
        member: "jackin",
        depends: &[
            "jackin-core",
            "jackin-image",
            "jackin-openshell",
            "jackin-session",
            "jackin-tui",
            "jackin-usage",
        ],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    LayerRule {
        member: "jackin-usage",
        depends: &[
            "jackin-core",
            "jackin-usage-provider-core",
            "jackin-usage-provider-claude",
            "jackin-usage-provider-codex",
            "jackin-usage-provider-cursor",
            "jackin-usage-provider-gemini",
            "jackin-usage-provider-grok",
            "jackin-usage-provider-kimi",
            "jackin-usage-provider-minimax",
            "jackin-usage-provider-opencode",
            "jackin-usage-provider-openrouter",
            "jackin-usage-provider-zai",
        ],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
    provider(USAGE_PROVIDERS[0]),
    provider(USAGE_PROVIDERS[1]),
    provider(USAGE_PROVIDERS[2]),
    provider(USAGE_PROVIDERS[3]),
    provider(USAGE_PROVIDERS[4]),
    provider(USAGE_PROVIDERS[5]),
    provider(USAGE_PROVIDERS[6]),
    provider(USAGE_PROVIDERS[7]),
    provider(USAGE_PROVIDERS[8]),
    provider(USAGE_PROVIDERS[9]),
    LayerRule {
        member: "jackin-usage-provider-core",
        depends: &[],
        dev_depends: &[],
        imports: &[],
    },
    LayerRule {
        member: "jackin-xtask",
        depends: &["jackin-core", "jackin-openshell", "jackin-session"],
        dev_depends: &["jackin-testkit"],
        imports: &[],
    },
];

#[derive(Clone, Copy, Debug, Default)]
pub struct LayersCheck;

impl Check for LayersCheck {
    fn step(&self) -> CiStep {
        CiStep::Layers
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = Vec::new();
        for member in repo.members() {
            let rule = LAYERS.iter().find(|rule| rule.member == member.name());
            match rule {
                Some(rule) => violations.extend(dependency_violations(member, rule)),
                None => violations.push(Violation::at_file(
                    self.step(),
                    member.manifest(),
                    format!(
                        "{} has no row in LAYERS; add one in crates/xtask/src/checks/layers.rs",
                        member.name()
                    ),
                )),
            }
            let imports = rule.map_or(&[][..], |rule| rule.imports);
            violations.extend(import_violations(repo, member, imports));
            violations.extend(http_client_violations(repo, member));
        }
        violations.extend(expose_violations(repo));
        sorted(violations)
    }
}

fn gateway_family(package: &str) -> Option<&'static str> {
    GATEWAY_FAMILIES.iter().copied().find(|family| {
        package
            .strip_prefix(family)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
    })
}

fn is_internal(name: &str) -> bool {
    name == "jackin" || name.starts_with("jackin-")
}

fn verb(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Normal => "depends on",
        DependencyKind::Dev => "dev-depends on",
        DependencyKind::Build => "build-depends on",
    }
}

fn dependency_violations(member: &Member, rule: &LayerRule) -> Vec<Violation> {
    let mut violations = Vec::new();
    for dependency in member.dependencies() {
        if let Some(message) = dependency_problem(member, rule, dependency) {
            violations.push(Violation::at_file(
                CiStep::Layers,
                member.manifest(),
                message,
            ));
        }
    }
    violations
}

fn dependency_problem(
    member: &Member,
    rule: &LayerRule,
    dependency: &Dependency,
) -> Option<String> {
    let name = dependency.name();
    let subject = format!("{} {} {name}", member.name(), verb(dependency.kind()));
    if name == HTTP_CLIENT_CRATE {
        return (member.name() != HTTP_CLIENT_OWNER)
            .then(|| format!("{subject}; only {HTTP_CLIENT_OWNER} may use the HTTP client crate"));
    }
    if TERMINAL_ENGINE_CRATES.contains(&name) {
        return Some(format!(
            "{subject}; only the baseline crates junie-tui and jackin-console may use a terminal engine crate (D27)"
        ));
    }
    if let Some(family) = gateway_family(name) {
        return (!rule.imports.contains(&family))
            .then(|| format!("{subject}; only jackin-openshell may use the gateway crates (D14)"));
    }
    if !is_internal(name) {
        return None;
    }
    if dependency.kind() == DependencyKind::Dev && name == member.name() {
        return None;
    }
    let listed = |list: &[&str]| list.contains(&name);
    let allowed = match dependency.kind() {
        DependencyKind::Dev => listed(rule.dev_depends) || listed(rule.depends),
        DependencyKind::Normal | DependencyKind::Build => listed(rule.depends),
    };
    (!allowed).then(|| format!("{subject}, which its LAYERS row does not allow (D14)"))
}

fn import_violations(repo: &Repo, member: &Member, allowed: &[&str]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for file in repo.files() {
        if file.kind() != FileKind::Rust || !file.path().starts_with(member.dir()) {
            continue;
        }
        let tokens = lex_rust(file.text()).tokens;
        for (index, token) in tokens.iter().enumerate() {
            let Some((family, crate_name)) = gateway_import(&tokens, index) else {
                continue;
            };
            if allowed.contains(&family) {
                continue;
            }
            violations.push(Violation::at_line(
                CiStep::Layers,
                file.path(),
                token.line,
                format!(
                    "{} imports {crate_name}; only jackin-openshell may import the gateway crates (D14)",
                    member.name()
                ),
            ));
        }
    }
    violations
}

fn gateway_import(tokens: &[Token], index: usize) -> Option<(&'static str, String)> {
    let ident = tokens[index].ident()?;
    let family = GATEWAY_FAMILIES.iter().copied().find(|family| {
        ident
            .strip_prefix(family)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))
    })?;
    let followed_by_path = tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 2).is_some_and(|t| t.is_punct(':'));
    let nested = index >= 3
        && tokens[index - 1].is_punct(':')
        && tokens[index - 2].is_punct(':')
        && tokens[index - 3]
            .ident()
            .is_some_and(|previous| previous != "use");
    (followed_by_path && !nested).then(|| (family, ident.replace('_', "-")))
}

fn http_client_violations(repo: &Repo, member: &Member) -> Vec<Violation> {
    if member.name() == HTTP_CLIENT_OWNER {
        return Vec::new();
    }
    let mut violations = Vec::new();
    for file in repo.files() {
        if file.kind() != FileKind::Rust || !file.path().starts_with(member.dir()) {
            continue;
        }
        let tokens = lex_rust(file.text()).tokens;
        for (index, token) in tokens.iter().enumerate() {
            if !names_http_client(&tokens, index) {
                continue;
            }
            violations.push(Violation::at_line(
                CiStep::Layers,
                file.path(),
                token.line,
                format!(
                    "{} imports {HTTP_CLIENT_CRATE}; only {HTTP_CLIENT_OWNER} may import the HTTP client crate",
                    member.name()
                ),
            ));
        }
    }
    violations
}

fn names_http_client(tokens: &[Token], index: usize) -> bool {
    if tokens[index].ident() != Some(HTTP_CLIENT_CRATE) {
        return false;
    }
    let followed_by_path = tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 2).is_some_and(|t| t.is_punct(':'));
    let nested = index >= 3
        && tokens[index - 1].is_punct(':')
        && tokens[index - 2].is_punct(':')
        && tokens[index - 3]
            .ident()
            .is_some_and(|previous| previous != "use");
    let extern_crate = index >= 2
        && tokens[index - 1].ident() == Some("crate")
        && tokens[index - 2].ident() == Some("extern");
    (followed_by_path && !nested) || extern_crate
}

fn expose_violations(repo: &Repo) -> Vec<Violation> {
    let mut violations = Vec::new();
    for file in repo.files() {
        let path = file.path();
        let listed = EXPOSE_ALLOWLIST
            .iter()
            .any(|allowed| path == Path::new(allowed));
        let in_tests = path.components().any(|part| part.as_os_str() == "tests");
        if file.kind() != FileKind::Rust || listed || in_tests {
            continue;
        }
        let tokens = lex_rust(file.text()).tokens;
        for (index, token) in tokens.iter().enumerate() {
            if !calls_expose(&tokens, index) {
                continue;
            }
            violations.push(Violation::at_line(
                CiStep::Layers,
                path,
                token.line,
                "calls Secret::expose outside the allowlist in crates/xtask/src/checks/layers.rs"
                    .to_owned(),
            ));
        }
    }
    violations
}

fn calls_expose(tokens: &[Token], index: usize) -> bool {
    if tokens[index].ident() != Some("expose") {
        return false;
    }
    let opens = tokens.get(index + 1).is_some_and(|t| t.is_punct('('));
    let after_dot = index >= 1 && tokens[index - 1].is_punct('.');
    let after_path =
        index >= 2 && tokens[index - 1].is_punct(':') && tokens[index - 2].is_punct(':');
    opens && (after_dot || after_path)
}
