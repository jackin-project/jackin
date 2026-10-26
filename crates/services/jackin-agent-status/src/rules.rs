use jackin_core::container_paths;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::evidence::RawAgentState;

#[derive(Debug, Clone, Copy, Default)]
pub struct VirtualRegions<'a> {
    pub osc_title: Option<&'a str>,
    pub osc_progress: Option<&'a str>,
}

pub const RULE_ENGINE_VERSION: u32 = 2;

fn default_min_engine_version() -> u32 {
    1
}

const MAX_GATE_DEPTH: usize = 16;

#[derive(Debug, Clone, Deserialize)]
pub struct RulePack {
    pub schema_version: u32,
    pub agent: String,
    pub validated_versions: String,
    #[serde(default = "default_min_engine_version")]
    pub min_engine_version: u32,
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
    requires_all: Vec<String>,
    #[serde(default)]
    requires_any: Vec<String>,
    #[serde(default)]
    forbids: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
    #[serde(default)]
    forbids_regex: Vec<String>,
    #[serde(default)]
    gate: Option<Gate>,
    #[serde(skip)]
    compiled_regex: Vec<regex::Regex>,
    #[serde(skip)]
    compiled_line_regex: Vec<regex::Regex>,
    #[serde(skip)]
    compiled_forbids_regex: Vec<regex::Regex>,
    #[serde(skip)]
    compiled_gate: Option<CompiledGate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Gate {
    All(Vec<Gate>),
    Any(Vec<Gate>),
    Not(Box<Gate>),
    Contains(String),
    Regex(String),
    LineRegex(String),
}

impl Gate {
    fn compile(&self) -> anyhow::Result<CompiledGate> {
        Ok(match self {
            Self::All(gates) => CompiledGate::All(
                gates
                    .iter()
                    .map(Self::compile)
                    .collect::<anyhow::Result<_>>()?,
            ),
            Self::Any(gates) => CompiledGate::Any(
                gates
                    .iter()
                    .map(Self::compile)
                    .collect::<anyhow::Result<_>>()?,
            ),
            Self::Not(gate) => CompiledGate::Not(Box::new(gate.compile()?)),
            Self::Contains(s) => CompiledGate::Contains(s.to_ascii_lowercase()),
            Self::Regex(p) => CompiledGate::Regex(build_regex(p)?),
            Self::LineRegex(p) => CompiledGate::LineRegex(build_regex(p)?),
        })
    }

    fn eval(&self, region: &[String], text: &str) -> bool {
        match self {
            Self::All(gates) => gates.iter().all(|g| g.eval(region, text)),
            Self::Any(gates) => gates.iter().any(|g| g.eval(region, text)),
            Self::Not(gate) => !gate.eval(region, text),
            Self::Contains(s) => text.contains(&s.to_ascii_lowercase()),
            Self::Regex(p) => build_regex(p).is_ok_and(|re| re.is_match(text)),
            Self::LineRegex(p) => {
                build_regex(p).is_ok_and(|re| region.iter().any(|line| re.is_match(line)))
            }
        }
    }

    fn leaf_count(&self) -> usize {
        match self {
            Self::All(gates) | Self::Any(gates) => gates.iter().map(Self::leaf_count).sum(),
            Self::Not(gate) => gate.leaf_count(),
            Self::Contains(_) | Self::Regex(_) | Self::LineRegex(_) => 1,
        }
    }

    fn validate(&self, rule_id: &str, depth: usize) -> anyhow::Result<()> {
        anyhow::ensure!(
            depth <= MAX_GATE_DEPTH,
            "gate nested deeper than {MAX_GATE_DEPTH} in rule {rule_id}"
        );
        match self {
            Self::All(gates) | Self::Any(gates) => {
                anyhow::ensure!(!gates.is_empty(), "empty all/any gate in rule {rule_id}");
                gates
                    .iter()
                    .try_for_each(|g| g.validate(rule_id, depth + 1))
            }
            Self::Not(gate) => gate.validate(rule_id, depth + 1),
            Self::Contains(s) => {
                anyhow::ensure!(s.len() <= 512, "matcher too long in rule {rule_id} gate");
                Ok(())
            }
            Self::Regex(p) | Self::LineRegex(p) => {
                anyhow::ensure!(p.len() <= 512, "matcher too long in rule {rule_id} gate");
                build_regex(p).with_context(|| format!("invalid regex in rule {rule_id} gate"))?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone)]
enum CompiledGate {
    All(Vec<CompiledGate>),
    Any(Vec<CompiledGate>),
    Not(Box<CompiledGate>),
    Contains(String),
    Regex(regex::Regex),
    LineRegex(regex::Regex),
}

impl CompiledGate {
    fn eval(&self, region: &[String], text: &str) -> bool {
        match self {
            Self::All(gates) => gates.iter().all(|g| g.eval(region, text)),
            Self::Any(gates) => gates.iter().any(|g| g.eval(region, text)),
            Self::Not(gate) => !gate.eval(region, text),
            Self::Contains(s) => text.contains(s.as_str()),
            Self::Regex(re) => re.is_match(text),
            Self::LineRegex(re) => region.iter().any(|line| re.is_match(line)),
        }
    }
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
    LastPromptMarker,
    OscTitle,
    OscProgress,
    AfterLastPromptMarker,
    BeforeCurrentPromptMarker,
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
    notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PackSource {
    Embedded,
    LocalDir(PathBuf),
    SignedRemoteBundle(SignedPackBundle),
}

#[derive(Debug, Clone)]
pub struct SignedPackBundle {
    pub signer_identity: String,
    pub signature: String,
    pub packs: Vec<SignedPackEntry>,
}

#[derive(Debug, Clone)]
pub struct SignedPackEntry {
    pub label: String,
    pub content: String,
}

const RUNTIME_PACK_DIR: &str = container_paths::AGENT_STATUS_PACKS_DIR;
const MAX_SIGNED_PACK_BYTES: usize = 64 * 1024;

impl RulePackRegistry {
    pub fn bundled() -> anyhow::Result<Self> {
        let override_dir = override_pack_dir();
        let mut sources = vec![
            PackSource::Embedded,
            PackSource::LocalDir(PathBuf::from(RUNTIME_PACK_DIR)),
        ];
        if let Some(dir) = override_dir.as_deref() {
            sources.push(PackSource::LocalDir(dir.to_path_buf()));
        }
        Self::from_sources(sources)
    }

    pub fn from_packs(packs: impl IntoIterator<Item = RulePack>) -> Self {
        Self {
            packs: packs
                .into_iter()
                .map(|pack| (pack.agent.clone(), pack))
                .collect(),
            notes: Vec::new(),
        }
    }

    pub fn from_sources(sources: impl IntoIterator<Item = PackSource>) -> anyhow::Result<Self> {
        let mut packs = HashMap::new();
        let mut notes = Vec::new();
        for source in sources {
            match source {
                PackSource::Embedded => load_embedded_packs(&mut packs)?,
                PackSource::LocalDir(dir) => {
                    if !dir.is_dir() {
                        continue;
                    }
                    load_packs_from_dir(&mut packs, &dir).with_context(|| {
                        format!("load agent-status packs from {}", dir.display())
                    })?;
                }
                PackSource::SignedRemoteBundle(bundle) => {
                    notes.extend(load_signed_bundle(&mut packs, &bundle));
                }
            }
        }
        anyhow::ensure!(!packs.is_empty(), "no agent-status rule packs loaded");
        Ok(Self { packs, notes })
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

    pub fn notes(&self) -> &[String] {
        &self.notes
    }
}

fn load_embedded_packs(packs: &mut HashMap<String, RulePack>) -> anyhow::Result<()> {
    let failures = load_pack_sources(
        packs,
        [
            ("claude", include_str!("../packs/claude.toml")),
            ("codex", include_str!("../packs/codex.toml")),
            ("amp", include_str!("../packs/amp.toml")),
            ("kimi", include_str!("../packs/kimi.toml")),
            ("opencode", include_str!("../packs/opencode.toml")),
            ("grok", include_str!("../packs/grok.toml")),
        ],
    );
    anyhow::ensure!(
        !packs.is_empty(),
        "all embedded agent-status packs failed to load: {}",
        failures.join("; ")
    );
    Ok(())
}

fn load_signed_bundle(
    packs: &mut HashMap<String, RulePack>,
    bundle: &SignedPackBundle,
) -> Vec<String> {
    let mut notes = Vec::new();
    let entries = match verify_signed_bundle(bundle) {
        Ok(entries) => entries,
        Err(error) => {
            notes.push(format!(
                "remote pack bundle failed verification - using baked packs: {error:#}"
            ));
            return notes;
        }
    };
    load_bundle_entries(packs, entries)
}

fn load_bundle_entries(
    packs: &mut HashMap<String, RulePack>,
    entries: &[SignedPackEntry],
) -> Vec<String> {
    let mut notes = Vec::new();
    if entries.is_empty() {
        notes.push("remote pack bundle was empty - using baked packs".to_owned());
        return notes;
    }
    for entry in entries {
        if entry.content.len() > MAX_SIGNED_PACK_BYTES {
            notes.push(format!(
                "remote pack {} skipped: pack exceeds size limit",
                entry.label
            ));
            continue;
        }
        let failures = load_pack_sources(packs, [(entry.label.as_str(), entry.content.as_str())]);
        if failures.is_empty() {
            notes.push(format!("remote pack {} applied", entry.label));
        } else {
            notes.extend(
                failures
                    .into_iter()
                    .map(|failure| format!("remote pack {failure} skipped")),
            );
        }
    }
    notes
}

fn verify_signed_bundle(bundle: &SignedPackBundle) -> anyhow::Result<&[SignedPackEntry]> {
    let _ = bundle;
    anyhow::bail!("remote pack bundles require a production signature verifier");
}

fn load_pack_sources<'a>(
    packs: &mut HashMap<String, RulePack>,
    sources: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (label, content) in sources {
        match toml::from_str::<RulePack>(content) {
            Ok(pack) => match pack.finalize() {
                Ok(pack) => {
                    packs.insert(pack.agent.clone(), pack);
                }
                Err(error) => {
                    failures.push(format!("{label}: {error:#}"));
                }
            },
            Err(error) => {
                failures.push(format!("{label}: {error}"));
            }
        }
    }
    failures
}

impl RulePack {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        toml::from_str::<Self>(&content)?.finalize()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema_version == 1, "unsupported rule schema");
        anyhow::ensure!(!self.agent.trim().is_empty(), "agent is required");
        anyhow::ensure!(
            self.min_engine_version >= 1,
            "min_engine_version must be >= 1 in pack {}",
            self.agent
        );
        anyhow::ensure!(
            self.min_engine_version <= RULE_ENGINE_VERSION,
            "pack {} needs rule engine v{} but this build is v{RULE_ENGINE_VERSION}",
            self.agent,
            self.min_engine_version
        );
        anyhow::ensure!(
            !self.validated_versions.trim().is_empty(),
            "validated_versions is required"
        );
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
                + rule.forbids_regex.len()
                + rule.gate.as_ref().map_or(0, Gate::leaf_count);
            anyhow::ensure!(matcher_count <= 32, "too many matchers in {}", rule.id);
            if let Some(gate) = &rule.gate {
                gate.validate(&rule.id, 0)?;
            }
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

    pub fn finalize(mut self) -> anyhow::Result<Self> {
        self.validate()?;
        self.rule
            .sort_by_key(|rule| std::cmp::Reverse(rule.priority));
        for rule in &mut self.rule {
            rule.compiled_regex = compile_all(&rule.regex, &rule.id)?;
            rule.compiled_line_regex = compile_all(&rule.line_regex, &rule.id)?;
            rule.compiled_forbids_regex = compile_all(&rule.forbids_regex, &rule.id)?;
            rule.compiled_gate = rule.gate.as_ref().map(Gate::compile).transpose()?;
        }
        Ok(self)
    }

    pub fn evaluate(&self, screen_rows: &[String]) -> Option<RuleMatch> {
        self.evaluate_with_virtuals(screen_rows, VirtualRegions::default())
    }

    pub fn evaluate_with_virtuals(
        &self,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Option<RuleMatch> {
        self.rule
            .iter()
            .find(|rule| rule.matches(screen_rows, virtuals))
            .map(Rule::to_match)
    }

    pub fn explain_with_virtuals(
        &self,
        screen_rows: &[String],
        virtuals: VirtualRegions<'_>,
    ) -> Vec<RuleEvaluation> {
        self.rule
            .iter()
            .map(|rule| rule.evaluation(screen_rows, virtuals))
            .collect()
    }
}

fn build_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
}

fn regex_all(
    patterns: &[String],
    compiled: &[regex::Regex],
    pred: impl Fn(&regex::Regex) -> bool,
) -> bool {
    if compiled.len() == patterns.len() {
        compiled.iter().all(pred)
    } else {
        patterns
            .iter()
            .all(|pattern| build_regex(pattern).is_ok_and(|re| pred(&re)))
    }
}

fn compile_all(patterns: &[String], rule_id: &str) -> anyhow::Result<Vec<regex::Regex>> {
    patterns
        .iter()
        .map(|pattern| {
            build_regex(pattern).with_context(|| format!("invalid regex in rule {rule_id}"))
        })
        .collect()
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
            && regex_all(&self.regex, &self.compiled_regex, |re| re.is_match(&text))
            && regex_all(&self.line_regex, &self.compiled_line_regex, |re| {
                region.iter().any(|line| re.is_match(line))
            })
            && regex_all(&self.forbids_regex, &self.compiled_forbids_regex, |re| {
                !region.iter().any(|line| re.is_match(line))
            })
            && match (self.compiled_gate.as_ref(), self.gate.as_ref()) {
                (Some(compiled), _) => compiled.eval(&region, &text),
                (None, Some(gate)) => gate.eval(&region, &text),
                (None, None) => true,
            }
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
            Self::LastPromptMarker => match last_prompt_marker(screen_rows) {
                Some(index) => screen_rows.get(index).cloned().into_iter().collect(),
                None => Vec::new(),
            },
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
            Self::LastPromptMarker => "last_prompt_marker".to_owned(),
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

fn is_prompt_marker_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed == "›" || trimmed.starts_with("› ")
}

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
        "last_prompt_marker" => Ok(Region::LastPromptMarker),
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
        match RulePack::load(&path) {
            Ok(pack) => {
                packs.insert(pack.agent.clone(), pack);
            }
            Err(_e) => {}
        }
    }
    Ok(())
}

fn override_pack_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("JACKIN_STATUS_PACK_DIR") {
        return Some(path.into());
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".jackin/agent-status/packs"))
}
