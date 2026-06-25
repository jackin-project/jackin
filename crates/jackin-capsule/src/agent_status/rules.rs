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
    /// Each pattern must match *some single line* of the region (existential,
    /// anchored per line). The Herdr-borrowed disambiguator for "a spinner glyph
    /// at the start of its own line", numbered choices, and count tokens — cases
    /// a whole-region regex (which anchors to the joined blob) cannot express.
    #[serde(default)]
    pub line_regex: Vec<String>,
    /// No pattern may match *any line* of the region (anchored negation). Lets a
    /// rule say "blocked unless a line is a bare prompt caret", or derive idle
    /// from an OSC title by subtraction (title present AND not the spinner title
    /// AND not an action-required title) — `forbids` is substring-only and
    /// cannot express an anchored pattern.
    #[serde(default)]
    pub forbids_regex: Vec<String>,
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
    /// Lines after the last prompt-caret line (Codex `›`). Isolates text the
    /// agent emitted since the live prompt, so a question echoed in scrollback
    /// above the caret cannot match.
    AfterLastPromptMarker,
    /// Lines before the last prompt-caret line — prior conversation without the
    /// live input line polluting matches.
    BeforeCurrentPromptMarker,
    /// The whole recent screen, but empty when a live prompt caret is present.
    /// A rule keyed here fires only when the agent is NOT sitting at a fresh
    /// prompt (a self-disabling region).
    WholeRecentWithoutCurrentPromptMarker,
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

    /// Whether this pack's `validated_versions` range accepts `cli_version`.
    /// The image-build co-versioning check: a derived image must fail to build
    /// if a bundled pack does not cover the agent CLI version pinned in that
    /// image, so a pack and the TUI it targets can never silently drift apart.
    pub fn accepts_cli_version(&self, cli_version: &str) -> anyhow::Result<bool> {
        let req = semver::VersionReq::parse(self.validated_versions.trim())?;
        let version = semver::Version::parse(cli_version.trim())?;
        Ok(req.matches(&version))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema_version == 1, "unsupported rule schema");
        anyhow::ensure!(!self.agent.trim().is_empty(), "agent is required");
        anyhow::ensure!(
            !self.validated_versions.trim().is_empty(),
            "validated_versions is required"
        );
        // validated_versions must be a real, *bounded* semver range so a pack
        // is pinned to a known CLI window — `*` or a lower-only range (`>=x`)
        // would silently match any future CLI the agent's TUI may have changed
        // under. (Image-build enforcement compares this range against the
        // pinned CLI version; here we reject ranges that could never gate.)
        let req = semver::VersionReq::parse(self.validated_versions.trim())
            .with_context(|| format!("invalid validated_versions in pack {}", self.agent))?;
        anyhow::ensure!(
            !req.comparators.is_empty(),
            "validated_versions must be a bounded range, not a wildcard, in pack {}",
            self.agent
        );
        anyhow::ensure!(
            req.comparators.iter().any(|c| matches!(
                c.op,
                semver::Op::Exact
                    | semver::Op::Less
                    | semver::Op::LessEq
                    | semver::Op::Tilde
                    | semver::Op::Caret
            )),
            "validated_versions must have an upper bound (no lower-only drift) in pack {}",
            self.agent
        );
        anyhow::ensure!(self.rule.len() <= 128, "too many rules");
        for rule in &self.rule {
            anyhow::ensure!(!rule.id.trim().is_empty(), "rule id is required");
            let matcher_count = rule.requires_all.len()
                + rule.requires_any.len()
                + rule.forbids.len()
                + rule.regex.len()
                + rule.line_regex.len()
                + rule.forbids_regex.len();
            anyhow::ensure!(matcher_count <= 32, "too many matchers in {}", rule.id);
            for matcher in rule
                .requires_all
                .iter()
                .chain(rule.requires_any.iter())
                .chain(rule.forbids.iter())
                .chain(rule.regex.iter())
                .chain(rule.line_regex.iter())
                .chain(rule.forbids_regex.iter())
            {
                anyhow::ensure!(matcher.len() <= 512, "matcher too long in rule {}", rule.id);
            }
            // Compile-check every regex pattern at load so a broken pattern
            // fails loudly here instead of silently never matching at runtime.
            for matcher in rule
                .regex
                .iter()
                .chain(rule.line_regex.iter())
                .chain(rule.forbids_regex.iter())
            {
                build_regex(matcher)
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

/// Build a case-insensitive regex. Rebuilt per evaluation today; `validate`
/// compile-checks every pattern at pack load so a broken pattern fails loudly
/// there rather than silently never matching. (Caching compiled regexes is a
/// perf follow-up — see the roadmap; correctness does not depend on it.)
fn build_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
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
            && self
                .regex
                .iter()
                .all(|matcher| build_regex(matcher).is_ok_and(|re| re.is_match(&text)))
            // line_regex: each pattern must match SOME line of the region.
            && self.line_regex.iter().all(|matcher| {
                build_regex(matcher)
                    .is_ok_and(|re| region.iter().any(|line| re.is_match(line)))
            })
            // forbids_regex: NO pattern may match ANY line of the region.
            && self.forbids_regex.iter().all(|matcher| {
                build_regex(matcher)
                    .is_ok_and(|re| !region.iter().any(|line| re.is_match(line)))
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
            Self::BottomNonEmpty(n) => {
                // Scan a wider window so up to `n` non-empty lines survive even
                // when the bottom of the screen is blank, then keep the last `n`
                // in original top-to-bottom order.
                let non_empty: Vec<String> = bottom(screen_rows, n.saturating_mul(2))
                    .into_iter()
                    .filter(|line| !line.trim().is_empty())
                    .collect();
                let start = non_empty.len().saturating_sub(n);
                non_empty[start..].to_vec()
            }
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
            Self::AfterLastPromptMarker => match last_prompt_marker(screen_rows) {
                Some(index) => screen_rows.get(index + 1..).unwrap_or_default().to_vec(),
                None => Vec::new(),
            },
            Self::BeforeCurrentPromptMarker => match last_prompt_marker(screen_rows) {
                Some(index) => screen_rows.get(..index).unwrap_or_default().to_vec(),
                None => screen_rows.to_vec(),
            },
            Self::WholeRecentWithoutCurrentPromptMarker => {
                if last_prompt_marker(screen_rows).is_some() {
                    Vec::new()
                } else {
                    screen_rows.to_vec()
                }
            }
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
            Self::AfterLastPromptMarker => "after_last_prompt_marker".to_owned(),
            Self::BeforeCurrentPromptMarker => "before_current_prompt_marker".to_owned(),
            Self::WholeRecentWithoutCurrentPromptMarker => {
                "whole_recent_without_current_prompt_marker".to_owned()
            }
        }
    }
}

/// A Codex prompt-caret line: exactly `›` or starting `› ` (after trim).
fn is_prompt_marker_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed == "›" || trimmed.starts_with("› ")
}

/// Index of the last prompt-caret line, if any.
fn last_prompt_marker(screen_rows: &[String]) -> Option<usize> {
    screen_rows
        .iter()
        .rposition(|line| is_prompt_marker_line(line))
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
        "after_last_prompt_marker" => Ok(Region::AfterLastPromptMarker),
        "before_current_prompt_marker" => Ok(Region::BeforeCurrentPromptMarker),
        "whole_recent_without_current_prompt_marker" => {
            Ok(Region::WholeRecentWithoutCurrentPromptMarker)
        }
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
mod tests;
