use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::agent_status::evidence::RawAgentState;

#[derive(Debug, Clone, Copy, Default)]
pub struct VirtualRegions<'a> {
    pub osc_title: Option<&'a str>,
    pub osc_progress: Option<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulePack {
    pub schema_version: u32,
    pub agent: String,
    pub validated_versions: String,
    #[serde(default)]
    pub rule: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub state: RuleState,
    pub priority: i32,
    pub region: Region,
    #[serde(default)]
    pub strength: RuleStrength,
    #[serde(default)]
    pub requires_all: Vec<String>,
    #[serde(default)]
    pub requires_any: Vec<String>,
    #[serde(default)]
    pub forbids: Vec<String>,
    #[serde(default)]
    pub regex: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleState {
    Working,
    Blocked,
    Idle,
    Freeze,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleStrength {
    #[default]
    Weak,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Bottom(usize),
    BottomNonEmpty(usize),
    PromptBoxBody,
    AbovePromptBox,
    AfterLastRule,
    LastNonEmptyLine,
    OscTitle,
    OscProgress,
}

impl<'de> Deserialize<'de> for Region {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        parse_region(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub state: Option<RawAgentState>,
    pub rule_id: String,
    pub strong: bool,
    pub freeze: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuleEvaluation {
    pub id: String,
    pub state: String,
    pub priority: i32,
    pub region: String,
    pub strength: String,
    pub matched: bool,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct RulePackRegistry {
    packs: HashMap<String, RulePack>,
}

const RUNTIME_PACK_DIR: &str = "/jackin/runtime/agent-status/packs";

impl RulePackRegistry {
    pub fn bundled() -> anyhow::Result<Self> {
        let override_dir = override_pack_dir();
        Self::from_pack_dirs(Some(Path::new(RUNTIME_PACK_DIR)), override_dir.as_deref())
    }

    fn from_pack_dirs(
        runtime_pack_dir: Option<&Path>,
        override_pack_dir: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let mut packs = HashMap::new();
        load_embedded_packs(&mut packs)?;
        if let Some(dir) = runtime_pack_dir
            && dir.is_dir()
        {
            load_packs_from_dir(&mut packs, dir).with_context(|| {
                format!("load runtime agent-status packs from {}", dir.display())
            })?;
        }
        if let Some(dir) = override_pack_dir
            && dir.is_dir()
        {
            load_packs_from_dir(&mut packs, dir).with_context(|| {
                format!("load agent-status override packs from {}", dir.display())
            })?;
        }
        Ok(Self { packs })
    }

    pub fn evaluate(&self, agent: Option<&str>, screen_rows: &[String]) -> Option<RuleMatch> {
        self.packs.get(agent?)?.evaluate(screen_rows)
    }

    pub fn evaluate_with_virtuals(
        &self,
        agent: Option<&str>,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Option<RuleMatch> {
        self.packs
            .get(agent?)?
            .evaluate_with_virtuals(screen_rows, virtuals)
    }

    pub fn explain(&self, agent: Option<&str>, screen_rows: &[String]) -> Vec<RuleEvaluation> {
        self.packs
            .get(agent.unwrap_or_default())
            .map_or_else(Vec::new, |pack| pack.explain(screen_rows))
    }

    pub fn explain_with_virtuals(
        &self,
        agent: Option<&str>,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Vec<RuleEvaluation> {
        self.packs
            .get(agent.unwrap_or_default())
            .map_or_else(Vec::new, |pack| {
                pack.explain_with_virtuals(screen_rows, virtuals)
            })
    }
}

fn load_embedded_packs(packs: &mut HashMap<String, RulePack>) -> anyhow::Result<()> {
    for content in [
        include_str!("../../../../docker/runtime/agent-status/packs/claude.toml"),
        include_str!("../../../../docker/runtime/agent-status/packs/codex.toml"),
        include_str!("../../../../docker/runtime/agent-status/packs/amp.toml"),
        include_str!("../../../../docker/runtime/agent-status/packs/kimi.toml"),
        include_str!("../../../../docker/runtime/agent-status/packs/opencode.toml"),
    ] {
        let pack: RulePack = toml::from_str(content)?;
        pack.validate()?;
        packs.insert(pack.agent.clone(), pack);
    }
    Ok(())
}

impl RulePack {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let pack: Self = toml::from_str(&content)?;
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema_version == 1, "unsupported rule schema");
        anyhow::ensure!(!self.agent.trim().is_empty(), "agent is required");
        anyhow::ensure!(
            !self.validated_versions.trim().is_empty(),
            "validated_versions is required"
        );
        anyhow::ensure!(self.rule.len() <= 128, "too many rules");
        for rule in &self.rule {
            anyhow::ensure!(!rule.id.trim().is_empty(), "rule id is required");
            let matcher_count = rule.requires_all.len()
                + rule.requires_any.len()
                + rule.forbids.len()
                + rule.regex.len();
            anyhow::ensure!(matcher_count <= 32, "too many matchers in {}", rule.id);
            for matcher in rule
                .requires_all
                .iter()
                .chain(rule.requires_any.iter())
                .chain(rule.forbids.iter())
                .chain(rule.regex.iter())
            {
                anyhow::ensure!(matcher.len() <= 512, "matcher too long in rule {}", rule.id);
            }
            for matcher in &rule.regex {
                regex::RegexBuilder::new(matcher)
                    .case_insensitive(true)
                    .build()
                    .with_context(|| format!("invalid regex in rule {}", rule.id))?;
            }
        }
        Ok(())
    }

    pub fn evaluate(&self, screen_rows: &[String]) -> Option<RuleMatch> {
        self.evaluate_with_virtuals(screen_rows, VirtualRegions::default())
    }

    pub fn evaluate_with_virtuals(
        &self,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Option<RuleMatch> {
        let mut rules: Vec<&Rule> = self.rule.iter().collect();
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.priority));
        rules
            .into_iter()
            .find(|rule| rule.matches(screen_rows, virtuals))
            .map(Rule::to_match)
    }

    pub fn explain(&self, screen_rows: &[String]) -> Vec<RuleEvaluation> {
        self.explain_with_virtuals(screen_rows, VirtualRegions::default())
    }

    pub fn explain_with_virtuals(
        &self,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Vec<RuleEvaluation> {
        let mut rules: Vec<&Rule> = self.rule.iter().collect();
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.priority));
        rules
            .into_iter()
            .map(|rule| rule.evaluation(screen_rows, virtuals))
            .collect()
    }
}

impl Rule {
    fn matches(&self, screen_rows: &[String], virtuals: VirtualRegions<'_>) -> bool {
        let region = self.region.extract(screen_rows, virtuals);
        let text = region.join("\n").to_ascii_lowercase();
        self.forbids
            .iter()
            .all(|matcher| !text.contains(&matcher.to_ascii_lowercase()))
            && self
                .requires_all
                .iter()
                .all(|matcher| text.contains(&matcher.to_ascii_lowercase()))
            && (self.requires_any.is_empty()
                || self
                    .requires_any
                    .iter()
                    .any(|matcher| text.contains(&matcher.to_ascii_lowercase())))
            && self.regex.iter().all(|matcher| {
                regex::RegexBuilder::new(matcher)
                    .case_insensitive(true)
                    .build()
                    .is_ok_and(|regex| regex.is_match(&text))
            })
    }

    fn to_match(&self) -> RuleMatch {
        RuleMatch {
            state: match self.state {
                RuleState::Working => Some(RawAgentState::Working),
                RuleState::Blocked => Some(RawAgentState::Blocked),
                RuleState::Idle => Some(RawAgentState::Idle),
                RuleState::Freeze => None,
            },
            rule_id: self.id.clone(),
            strong: self.strength == RuleStrength::Strong,
            freeze: self.state == RuleState::Freeze,
        }
    }

    fn evaluation(&self, screen_rows: &[String], virtuals: VirtualRegions<'_>) -> RuleEvaluation {
        let region = self.region.extract(screen_rows, virtuals);
        RuleEvaluation {
            id: self.id.clone(),
            state: self.state.label().to_owned(),
            priority: self.priority,
            region: self.region.label(),
            strength: self.strength.label().to_owned(),
            matched: self.matches(screen_rows, virtuals),
            preview: preview(&region.join("\n"), 240),
        }
    }
}

impl RuleState {
    const fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Idle => "idle",
            Self::Freeze => "freeze",
        }
    }
}

impl RuleStrength {
    const fn label(self) -> &'static str {
        match self {
            Self::Weak => "weak",
            Self::Strong => "strong",
        }
    }
}

impl Region {
    fn extract(self, screen_rows: &[String], virtuals: VirtualRegions<'_>) -> Vec<String> {
        match self {
            Self::Bottom(n) => bottom(screen_rows, n),
            Self::BottomNonEmpty(n) => bottom(screen_rows, n.saturating_mul(2).max(n))
                .into_iter()
                .filter(|line| !line.trim().is_empty())
                .rev()
                .take(n)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            Self::PromptBoxBody => prompt_box_body(screen_rows),
            Self::AbovePromptBox => above_prompt_box(screen_rows),
            Self::AfterLastRule => after_last_rule(screen_rows),
            Self::LastNonEmptyLine => screen_rows
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .into_iter()
                .collect(),
            Self::OscTitle => virtuals.osc_title.map(str::to_owned).into_iter().collect(),
            Self::OscProgress => virtuals
                .osc_progress
                .map(str::to_owned)
                .into_iter()
                .collect(),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Bottom(n) => format!("bottom:{n}"),
            Self::BottomNonEmpty(n) => format!("bottom_nonempty:{n}"),
            Self::PromptBoxBody => "prompt_box_body".to_owned(),
            Self::AbovePromptBox => "above_prompt_box".to_owned(),
            Self::AfterLastRule => "after_last_rule".to_owned(),
            Self::LastNonEmptyLine => "last_nonempty_line".to_owned(),
            Self::OscTitle => "osc_title".to_owned(),
            Self::OscProgress => "osc_progress".to_owned(),
        }
    }
}

fn preview(value: &str, max_chars: usize) -> String {
    let mut out: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn parse_region(raw: &str) -> anyhow::Result<Region> {
    if let Some(n) = raw.strip_prefix("bottom:") {
        return Ok(Region::Bottom(n.parse()?));
    }
    if let Some(n) = raw.strip_prefix("bottom_nonempty:") {
        return Ok(Region::BottomNonEmpty(n.parse()?));
    }
    match raw {
        "prompt_box_body" => Ok(Region::PromptBoxBody),
        "above_prompt_box" => Ok(Region::AbovePromptBox),
        "after_last_rule" => Ok(Region::AfterLastRule),
        "last_nonempty_line" => Ok(Region::LastNonEmptyLine),
        "osc_title" => Ok(Region::OscTitle),
        "osc_progress" => Ok(Region::OscProgress),
        _ => anyhow::bail!("unknown region {raw:?}"),
    }
}

fn bottom(screen_rows: &[String], n: usize) -> Vec<String> {
    let start = screen_rows.len().saturating_sub(n);
    screen_rows[start..].to_vec()
}

fn prompt_box_bounds(screen_rows: &[String]) -> Option<(usize, usize)> {
    let start = screen_rows.iter().rposition(|line| line.contains('╭'))?;
    let end = screen_rows
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, line)| line.contains('╰').then_some(index))
        .unwrap_or(start);
    Some((start, end))
}

fn prompt_box_body(screen_rows: &[String]) -> Vec<String> {
    let Some((start, end)) = prompt_box_bounds(screen_rows) else {
        return Vec::new();
    };
    screen_rows
        .get(start + 1..end)
        .unwrap_or_default()
        .iter()
        .map(|line| line.trim_matches(['│', ' ']).to_owned())
        .collect()
}

fn above_prompt_box(screen_rows: &[String]) -> Vec<String> {
    let Some((start, _)) = prompt_box_bounds(screen_rows) else {
        return screen_rows.to_vec();
    };
    screen_rows[..start].to_vec()
}

fn after_last_rule(screen_rows: &[String]) -> Vec<String> {
    let Some(rule_index) = screen_rows.iter().rposition(|line| {
        !line.contains('╭')
            && !line.contains('╰')
            && !line.contains('┌')
            && !line.contains('└')
            && !line.contains('╔')
            && !line.contains('╚')
            && line
                .chars()
                .filter(|ch| matches!(ch, '─' | '-' | '═' | '='))
                .count()
                >= 10
    }) else {
        return screen_rows.to_vec();
    };
    screen_rows
        .get(rule_index + 1..)
        .unwrap_or_default()
        .to_vec()
}

fn load_packs_from_dir(packs: &mut HashMap<String, RulePack>, dir: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let pack = RulePack::load(&path)
            .with_context(|| format!("load agent-status pack {}", path.display()))?;
        packs.insert(pack.agent.clone(), pack);
    }
    Ok(())
}

fn override_pack_dir() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("JACKIN_STATUS_PACK_DIR") {
        return Some(path.into());
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".jackin/agent-status/packs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> Vec<String> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn fixture_for_detection(path: &Path) -> (Option<RawAgentState>, Vec<String>) {
        let mut rows = fixture(path.to_str().unwrap());
        let forbidden = rows
            .first()
            .and_then(|line| line.trim().strip_prefix("# not:"))
            .map(str::trim)
            .map(|state| match state {
                "working" => RawAgentState::Working,
                "blocked" => RawAgentState::Blocked,
                "idle" => RawAgentState::Idle,
                other => panic!("unknown forbidden state {other:?} in {path:?}"),
            });
        if forbidden.is_some() {
            rows.remove(0);
        }
        (forbidden, rows)
    }

    fn write_test_pack(dir: &Path, agent: &str, id: &str, state: &str, needle: &str) {
        fs::write(
            dir.join(format!("{agent}.toml")),
            format!(
                r#"
schema_version = 1
agent = "{agent}"
validated_versions = "*"

[[rule]]
id = "{id}"
state = "{state}"
priority = 1
region = "bottom:12"
strength = "strong"
requires_all = ["{needle}"]
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn packs_load_and_match_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        for agent in ["claude", "codex", "amp", "kimi", "opencode"] {
            let pack = RulePack::load(
                &root
                    .join("docker/runtime/agent-status/packs")
                    .join(format!("{agent}.toml")),
            )
            .unwrap();
            let fixture_dir = root
                .join("crates/jackin-capsule/src/agent_status/screen/fixtures")
                .join(agent);
            for entry in fs::read_dir(fixture_dir).unwrap() {
                let path = entry.unwrap().path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let (forbidden, rows) = fixture_for_detection(&path);
                let matched = pack.evaluate(&rows).and_then(|matched| matched.state);
                if name.starts_with("working") {
                    assert_eq!(matched, Some(RawAgentState::Working), "{path:?}");
                } else if name.starts_with("blocked") {
                    assert_eq!(matched, Some(RawAgentState::Blocked), "{path:?}");
                } else if name.starts_with("idle") {
                    assert_eq!(matched, Some(RawAgentState::Idle), "{path:?}");
                } else if name.starts_with("false_positive") {
                    assert_ne!(
                        matched,
                        Some(forbidden.unwrap_or(RawAgentState::Working)),
                        "{path:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn regex_matchers_participate_in_rules() {
        let pack: RulePack = toml::from_str(
            r#"
schema_version = 1
agent = "test"
validated_versions = "*"

[[rule]]
id = "anchored-spinner"
state = "working"
priority = 1
region = "bottom:12"
strength = "strong"
regex = ["^\\* thinking"]
"#,
        )
        .unwrap();
        pack.validate().unwrap();
        let rows = vec!["* Thinking".to_owned()];
        assert_eq!(
            pack.evaluate(&rows).and_then(|matched| matched.state),
            Some(RawAgentState::Working)
        );
    }

    #[test]
    fn structural_regions_extract_prompt_and_rule_areas() {
        let rows = vec![
            "before".to_owned(),
            "────────────────────".to_owned(),
            "after rule".to_owned(),
            "╭────────────╮".to_owned(),
            "│ > hello    │".to_owned(),
            "╰────────────╯".to_owned(),
        ];

        assert_eq!(
            parse_region("prompt_box_body")
                .unwrap()
                .extract(&rows, VirtualRegions::default()),
            vec!["> hello".to_owned()]
        );
        assert_eq!(
            parse_region("above_prompt_box")
                .unwrap()
                .extract(&rows, VirtualRegions::default()),
            vec![
                "before".to_owned(),
                "────────────────────".to_owned(),
                "after rule".to_owned(),
            ]
        );
        assert_eq!(
            parse_region("after_last_rule")
                .unwrap()
                .extract(&rows, VirtualRegions::default()),
            vec![
                "after rule".to_owned(),
                "╭────────────╮".to_owned(),
                "│ > hello    │".to_owned(),
                "╰────────────╯".to_owned(),
            ]
        );
    }

    #[test]
    fn virtual_osc_regions_participate_in_matching_and_explain() {
        let pack: RulePack = toml::from_str(
            r#"
schema_version = 1
agent = "codex"
validated_versions = "*"

[[rule]]
id = "title-spinner"
state = "working"
priority = 10
region = "osc_title"
strength = "strong"
requires_all = ["codex", "working"]

[[rule]]
id = "progress-cleared"
state = "idle"
priority = 9
region = "osc_progress"
strength = "strong"
requires_all = ["cleared"]
"#,
        )
        .unwrap();
        pack.validate().unwrap();

        let title_virtuals = VirtualRegions {
            osc_title: Some("Codex - working"),
            osc_progress: Some("inactive"),
        };
        let matched = pack
            .evaluate_with_virtuals(&[], title_virtuals)
            .expect("title rule should match");
        assert_eq!(matched.rule_id, "title-spinner");
        assert_eq!(matched.state, Some(RawAgentState::Working));

        let progress_virtuals = VirtualRegions {
            osc_title: None,
            osc_progress: Some("cleared"),
        };
        let explain = pack.explain_with_virtuals(&[], progress_virtuals);
        assert!(explain.iter().any(|rule| {
            rule.id == "progress-cleared" && rule.matched && rule.preview == "cleared"
        }));
    }

    #[test]
    fn runtime_pack_directory_overrides_embedded_pack() {
        let runtime = tempfile::tempdir().unwrap();
        write_test_pack(
            runtime.path(),
            "claude",
            "runtime-pack",
            "idle",
            "runtime marker",
        );

        let registry = RulePackRegistry::from_pack_dirs(Some(runtime.path()), None).unwrap();

        let matched = registry
            .evaluate(Some("claude"), &["runtime marker".to_owned()])
            .unwrap();
        assert_eq!(matched.rule_id, "runtime-pack");
        assert_eq!(matched.state, Some(RawAgentState::Idle));
    }

    #[test]
    fn override_pack_directory_overrides_runtime_pack() {
        let runtime = tempfile::tempdir().unwrap();
        let override_dir = tempfile::tempdir().unwrap();
        write_test_pack(
            runtime.path(),
            "claude",
            "runtime-pack",
            "idle",
            "runtime marker",
        );
        write_test_pack(
            override_dir.path(),
            "claude",
            "override-pack",
            "blocked",
            "override marker",
        );

        let registry =
            RulePackRegistry::from_pack_dirs(Some(runtime.path()), Some(override_dir.path()))
                .unwrap();

        assert!(
            registry
                .evaluate(Some("claude"), &["runtime marker".to_owned()])
                .is_none(),
            "override pack should replace the runtime pack for the same agent"
        );
        let matched = registry
            .evaluate(Some("claude"), &["override marker".to_owned()])
            .unwrap();
        assert_eq!(matched.rule_id, "override-pack");
        assert_eq!(matched.state, Some(RawAgentState::Blocked));
    }

    #[test]
    fn loaded_pack_directory_replaces_existing_pack_for_same_agent() {
        let mut packs = HashMap::new();
        let bundled: RulePack = toml::from_str(
            r#"
schema_version = 1
agent = "test"
validated_versions = "*"

[[rule]]
id = "bundled"
state = "working"
priority = 1
region = "bottom:12"
strength = "strong"
requires_all = ["bundled"]
"#,
        )
        .unwrap();
        packs.insert(bundled.agent.clone(), bundled);

        let tmp = tempfile::tempdir().unwrap();
        write_test_pack(tmp.path(), "test", "override", "blocked", "override");

        load_packs_from_dir(&mut packs, tmp.path()).unwrap();

        let matched = packs
            .get("test")
            .unwrap()
            .evaluate(&["override".to_owned()])
            .unwrap();
        assert_eq!(matched.rule_id, "override");
        assert_eq!(matched.state, Some(RawAgentState::Blocked));
    }
}
