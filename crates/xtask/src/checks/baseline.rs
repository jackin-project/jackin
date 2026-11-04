// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use toml::{Table, Value};

use super::Check;
use crate::ci::CiStep;
use crate::repo::{Dependency, DependencyKind, DependencySource, GitReference, Member, Repo};
use crate::violation::{Violation, sorted};

pub const BASELINE_URL: &str = "https://github.com/tailrocks/terminal-components-baseline";

pub const BASELINE_BRANCH: &str = "feature/openshell-native";

pub const ALLOWED_GIT: [&str; 3] = [
    "https://github.com/NVIDIA/OpenShell",
    BASELINE_URL,
    "https://github.com/tailrocks/termpane",
];

const JUNIE_TUI: &str = "junie-tui";
const JACKIN_CONSOLE: &str = "jackin-console";
const DEMO_EXEMPT_MEMBER: &str = "jackin-testkit";
const ROOT_MANIFEST: &str = "Cargo.toml";
const CARGO_CONFIG: &str = ".cargo/config.toml";
const LOCK: &str = "Cargo.lock";
const DENY: &str = "deny.toml";

#[derive(Clone, Copy, Debug, Default)]
pub struct BaselineCheck;

impl Check for BaselineCheck {
    fn step(&self) -> CiStep {
        CiStep::Baseline
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let revs = pinned_revs(repo);
        let mut violations = source_violations(repo);
        violations.extend(demo_violations(repo, self.step()));
        violations.extend(rev_disagreement(&revs));
        violations.extend(override_violations(repo));
        violations.extend(lock_violations(repo, &revs));
        violations.extend(deny_violations(repo));
        sorted(violations)
    }
}

fn is_baseline_crate(name: &str) -> bool {
    name == JUNIE_TUI || name == JACKIN_CONSOLE
}

fn edges(repo: &Repo) -> impl Iterator<Item = (&Member, &Dependency)> {
    repo.members().iter().flat_map(|member| {
        member
            .dependencies()
            .iter()
            .filter(|dependency| is_baseline_crate(dependency.name()))
            .map(move |dependency| (member, dependency))
    })
}

fn is_full_rev(rev: &str) -> bool {
    rev.len() == 40
        && rev
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn rule(step: CiStep, path: &Path, rule: &str, message: impl AsRef<str>) -> Violation {
    Violation::at_file(step, path, format!("{rule}: {}", message.as_ref()))
}

fn rev_problem(rev: &str) -> Option<String> {
    if !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!(
            "is pinned to {rev}, which is not hexadecimal; pin a commit, not a name"
        ))
    } else if rev.chars().any(|c| c.is_ascii_uppercase()) {
        Some(format!("has the uppercase rev {rev}; use lowercase"))
    } else if rev.len() != 40 {
        Some(format!(
            "has the short rev {rev}; pin the full 40-character commit"
        ))
    } else {
        None
    }
}

fn source_problem(dependency: &Dependency) -> Option<String> {
    match dependency.source() {
        DependencySource::Path => {
            Some("is a path dependency; it must be a git dependency pinned by rev".to_owned())
        }
        DependencySource::Registry => Some(format!(
            "comes from a registry; it must be a git dependency on {BASELINE_URL}"
        )),
        DependencySource::Git { url, reference } => {
            if url.strip_suffix(".git") == Some(BASELINE_URL) {
                return Some(format!(
                    "uses the URL {url} with a .git suffix; the URL is exactly {BASELINE_URL}"
                ));
            }
            if url != BASELINE_URL {
                return Some(format!(
                    "uses another URL {url}; the URL is exactly {BASELINE_URL}"
                ));
            }
            match reference {
                GitReference::Branch(name) => {
                    Some(format!("is pinned to the branch {name}; pin a full rev"))
                }
                GitReference::Tag(name) => {
                    Some(format!("is pinned to the tag {name}; pin a full rev"))
                }
                GitReference::Default => Some("has no reference; pin a full rev".to_owned()),
                GitReference::Rev(rev) => rev_problem(rev),
            }
        }
    }
}

fn source_violations(repo: &Repo) -> Vec<Violation> {
    edges(repo)
        .filter_map(|(member, dependency)| {
            source_problem(dependency).map(|problem| {
                rule(
                    CiStep::Baseline,
                    member.manifest(),
                    "R1",
                    format!("{} {problem}", dependency.name()),
                )
            })
        })
        .collect()
}

fn is_product_edge(dependency: &Dependency) -> bool {
    matches!(
        dependency.kind(),
        DependencyKind::Normal | DependencyKind::Build
    )
}

fn edge_problems(member: &Member, dependency: &Dependency) -> Vec<String> {
    let name = dependency.name();
    let mut problems = Vec::new();
    if dependency.uses_default_features() {
        problems.push(format!(
            "{name} uses its default features; set default-features = false"
        ));
    }
    let exempt = member.name() == DEMO_EXEMPT_MEMBER;
    for feature in dependency.features() {
        let allowed = match name {
            JACKIN_CONSOLE => feature != "demo" || exempt,
            _ => feature == "terminal",
        };
        if !allowed {
            problems.push(format!(
                "{name} enables the feature {feature}, which the product build must not contain"
            ));
        }
    }
    problems
}

fn forwarding_problems(member: &Member, console_aliases: &[&str]) -> Vec<String> {
    if member.name() == DEMO_EXEMPT_MEMBER {
        return Vec::new();
    }
    let mut problems = Vec::new();
    for (feature, targets) in member.features() {
        for target in targets {
            let forwards_demo = console_aliases.iter().any(|alias| {
                target == &format!("{alias}/demo") || target == &format!("{alias}?/demo")
            });
            if forwards_demo {
                problems.push(format!(
                    "the feature {feature} forwards {target}, which enables the demo feature of {JACKIN_CONSOLE}"
                ));
            }
        }
    }
    problems
}

pub(crate) fn demo_violations(repo: &Repo, step: CiStep) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (member, dependency) in edges(repo).filter(|(_, dependency)| is_product_edge(dependency)) {
        for problem in edge_problems(member, dependency) {
            violations.push(rule(step, member.manifest(), "R3", problem));
        }
    }
    for member in repo.members() {
        let aliases: Vec<&str> = member
            .dependencies()
            .iter()
            .filter(|dependency| dependency.name() == JACKIN_CONSOLE)
            .map(Dependency::alias)
            .collect();
        for problem in forwarding_problems(member, &aliases) {
            violations.push(rule(step, member.manifest(), "R3", problem));
        }
    }
    violations
}

fn parse_table(repo: &Repo, path: &str) -> Option<Table> {
    let file = repo
        .files()
        .iter()
        .find(|file| file.path() == Path::new(path))?;
    toml::from_str(file.text()).ok()
}

fn table_pin(entry: &Value) -> Option<&str> {
    entry.as_table()?.get("rev")?.as_str()
}

fn names_baseline_crate(key: &str, entry: &Value) -> bool {
    is_baseline_crate(key)
        || entry
            .as_table()
            .and_then(|table| table.get("package"))
            .and_then(Value::as_str)
            .is_some_and(is_baseline_crate)
}

pub fn pinned_revs(repo: &Repo) -> Vec<String> {
    let mut revs: BTreeSet<String> = edges(repo)
        .filter_map(|(_, dependency)| match dependency.source() {
            DependencySource::Git {
                reference: GitReference::Rev(rev),
                ..
            } => Some(rev.clone()),
            _ => None,
        })
        .collect();
    let workspace_table = parse_table(repo, ROOT_MANIFEST).and_then(|root| {
        root.get("workspace")?
            .get("dependencies")?
            .as_table()
            .cloned()
    });
    for (key, entry) in workspace_table.iter().flatten() {
        if names_baseline_crate(key, entry)
            && let Some(rev) = table_pin(entry)
        {
            revs.insert(rev.to_owned());
        }
    }
    revs.into_iter().filter(|rev| is_full_rev(rev)).collect()
}

fn rev_disagreement(revs: &[String]) -> Option<Violation> {
    (revs.len() > 1).then(|| {
        rule(
            CiStep::Baseline,
            Path::new(ROOT_MANIFEST),
            "R2",
            format!(
                "{JUNIE_TUI} and {JACKIN_CONSOLE} are pinned to different revs: {}",
                revs.join(", ")
            ),
        )
    })
}

fn normalized_url(url: &str) -> &str {
    let trimmed = url.trim_end_matches('/');
    trimmed.strip_suffix(".git").unwrap_or(trimmed)
}

fn replace_names_baseline(key: &str) -> bool {
    let (url, package) = key
        .rsplit_once('#')
        .map_or((None, key), |(url, rest)| (Some(url), rest));
    let name = package.split([':', '@']).next().unwrap_or(package);
    is_baseline_crate(name) || url.is_some_and(|url| normalized_url(url) == BASELINE_URL)
}

fn override_violations(repo: &Repo) -> Vec<Violation> {
    let mut violations = Vec::new();
    for path in [ROOT_MANIFEST, CARGO_CONFIG] {
        let Some(table) = parse_table(repo, path) else {
            continue;
        };
        let at = Path::new(path);
        for (registry, patched) in table
            .get("patch")
            .and_then(Value::as_table)
            .into_iter()
            .flatten()
        {
            let Some(patched) = patched.as_table() else {
                continue;
            };
            if normalized_url(registry) == BASELINE_URL {
                violations.push(rule(
                    CiStep::Baseline,
                    at,
                    "R4",
                    format!("{path} has a [patch] table for {registry}, the baseline URL"),
                ));
                continue;
            }
            for (name, entry) in patched
                .iter()
                .filter(|(name, entry)| names_baseline_crate(name, entry))
            {
                let package = entry
                    .as_table()
                    .and_then(|table| table.get("package"))
                    .and_then(Value::as_str)
                    .unwrap_or(name);
                violations.push(rule(
                    CiStep::Baseline,
                    at,
                    "R4",
                    format!("{path} has a [patch] entry {name} for {package} under {registry}"),
                ));
            }
        }
        for key in table
            .get("replace")
            .and_then(Value::as_table)
            .into_iter()
            .flat_map(Table::keys)
            .filter(|key| replace_names_baseline(key))
        {
            violations.push(rule(
                CiStep::Baseline,
                at,
                "R4",
                format!("{path} has a [replace] entry for {key}"),
            ));
        }
        if table
            .get("paths")
            .and_then(Value::as_array)
            .is_some_and(|paths| !paths.is_empty())
        {
            violations.push(rule(
                CiStep::Baseline,
                at,
                "R4",
                format!("{path} sets paths, which override dependencies by directory"),
            ));
        }
    }
    violations
}

fn lock_source_matches(source: &str, revs: &[String]) -> bool {
    let Some(rest) = source.strip_prefix(&format!("git+{BASELINE_URL}?rev=")) else {
        return false;
    };
    let Some((requested, resolved)) = rest.split_once('#') else {
        return false;
    };
    requested == resolved
        && is_full_rev(requested)
        && (revs.is_empty() || revs.iter().any(|rev| rev == requested))
}

fn lock_violations(repo: &Repo, revs: &[String]) -> Vec<Violation> {
    let lock = Path::new(LOCK);
    let text = fs::read_to_string(repo.root().join(LOCK)).unwrap_or_default();
    let table: Table = match toml::from_str(&text) {
        Ok(table) => table,
        Err(error) => {
            return vec![rule(
                CiStep::Baseline,
                lock,
                "R5",
                format!("{LOCK} does not parse: {error}"),
            )];
        }
    };
    let packages: Vec<&Table> = table
        .get("package")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_table)
        .collect();
    let mut violations = Vec::new();
    for name in [JUNIE_TUI, JACKIN_CONSOLE] {
        let entries: Vec<&&Table> = packages
            .iter()
            .filter(|package| package.get("name").and_then(Value::as_str) == Some(name))
            .collect();
        if entries.is_empty() && edges(repo).any(|(_, dependency)| dependency.name() == name) {
            violations.push(rule(
                CiStep::Baseline,
                lock,
                "R5",
                format!("{LOCK} has no entry for {name}; run cargo update -p {name}"),
            ));
        }
        for entry in entries {
            let source = entry.get("source").and_then(Value::as_str);
            if !source.is_some_and(|source| lock_source_matches(source, revs)) {
                violations.push(rule(
                    CiStep::Baseline,
                    lock,
                    "R5",
                    format!(
                        "{LOCK} locks {name} from {}, not from git+{BASELINE_URL}?rev=<rev>#<rev> with the manifest rev",
                        source.unwrap_or("a path")
                    ),
                ));
            }
        }
    }
    violations
}

fn deny_violations(repo: &Repo) -> Vec<Violation> {
    let path = Path::new(DENY);
    let Some(file) = repo.files().iter().find(|file| file.path() == path) else {
        return vec![rule(
            CiStep::Baseline,
            path,
            "R6",
            format!("{DENY} is missing; it must allow exactly the git sources of the build"),
        )];
    };
    let table: Table = match toml::from_str(file.text()) {
        Ok(table) => table,
        Err(error) => {
            return vec![rule(
                CiStep::Baseline,
                path,
                "R6",
                format!("{DENY} does not parse: {error}"),
            )];
        }
    };
    let sources = table.get("sources").and_then(Value::as_table);
    let mut found: Vec<&str> = sources
        .and_then(|sources| sources.get("allow-git"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    found.sort_unstable();
    let mut expected = ALLOWED_GIT.to_vec();
    expected.sort_unstable();
    let mut violations = Vec::new();
    if found != expected {
        violations.push(rule(
            CiStep::Baseline,
            path,
            "R6",
            format!(
                "{DENY} [sources] allow-git is [{}], not exactly [{}]",
                found.join(", "),
                ALLOWED_GIT.join(", ")
            ),
        ));
    }
    if sources
        .and_then(|sources| sources.get("unknown-git"))
        .and_then(Value::as_str)
        != Some("deny")
    {
        violations.push(rule(
            CiStep::Baseline,
            path,
            "R6",
            format!("{DENY} [sources] unknown-git must be \"deny\""),
        ));
    }
    violations
}
