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

/// Version of the rule-pack evaluation engine in this build. A pack may declare
/// a `min_engine_version` higher than this to opt out of older engines that
/// would silently misread a newer gate grammar; such packs are skipped, not
/// mis-evaluated. Bump this when the engine gains a capability a pack can depend
/// on (e.g. a richer gate form).
///
/// - v1: flat matchers + `line_regex`/`forbids_regex`.
/// - v2: the recursive nested-gate grammar (`gate = { all/any/not/… }`). A pack
///   using `gate` must declare `min_engine_version = 2`, so a v1 engine — which
///   has no `gate` field and would silently drop it (serde ignores unknown keys),
///   evaluating only the flat matchers and over-matching — skips the pack instead.
pub const RULE_ENGINE_VERSION: u32 = 2;

fn default_min_engine_version() -> u32 {
    1
}

/// Maximum nesting depth for a rule's recursive `Gate`. Packs are operator
/// authored, so this is a fail-loudly-at-load guard against a malformed deep
/// nest, not a security boundary; far beyond any pattern a real rule needs.
const MAX_GATE_DEPTH: usize = 16;

#[derive(Debug, Clone, Deserialize)]
pub struct RulePack {
    pub schema_version: u32,
    pub agent: String,
    pub validated_versions: String,
    /// Minimum [`RULE_ENGINE_VERSION`] this pack needs. Defaults to 1 (every
    /// engine). A pack authored against a future engine sets this higher so
    /// older engines skip it instead of misreading unknown gate fields.
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
    // Matcher pattern vecs are private: they are set once by deserialize and the
    // regex ones are compiled in lock-step by `finalize`. Keeping them out of the
    // public API means no external code can mutate a pattern in place and silently
    // desync it from its compiled regex (which `regex_all` would then trust).
    #[serde(default)]
    requires_all: Vec<String>,
    #[serde(default)]
    requires_any: Vec<String>,
    #[serde(default)]
    forbids: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    /// Each pattern must match *some single line* of the region (existential,
    /// anchored per line). The Herdr-borrowed disambiguator for "a spinner glyph
    /// at the start of its own line", numbered choices, and count tokens — cases
    /// a whole-region regex (which anchors to the joined blob) cannot express.
    #[serde(default)]
    line_regex: Vec<String>,
    /// No pattern may match *any line* of the region (anchored negation). Lets a
    /// rule say "blocked unless a line is a bare prompt caret", or derive idle
    /// from an OSC title by subtraction (title present AND not the spinner title
    /// AND not an action-required title) — `forbids` is substring-only and
    /// cannot express an anchored pattern.
    #[serde(default)]
    forbids_regex: Vec<String>,
    /// Recursive boolean gate, combined by logical AND with the flat matchers
    /// above. Expresses nested `all`/`any`/`not` that the flat lists cannot — a
    /// shared positive prefix needing its own nested OR (e.g. a hypothetical
    /// Claude bash-permission rule: `contains "do you want to proceed?"` AND
    /// `any` of the bash markers AND `any` numbered choice), which flattened into
    /// one `requires_any` would leak its branches across each other and
    /// over-match. `None` (the common case) leaves a rule on the flat matchers
    /// alone. Gate-using packs must declare `min_engine_version = 2`.
    #[serde(default)]
    gate: Option<Gate>,
    // Regexes compiled once by `RulePack::finalize` at load (parallel to the
    // pattern vecs above). `matches` falls back to per-eval compilation if these
    // are absent, so a pack that skipped finalize still matches — just slower.
    #[serde(skip)]
    compiled_regex: Vec<regex::Regex>,
    #[serde(skip)]
    compiled_line_regex: Vec<regex::Regex>,
    #[serde(skip)]
    compiled_forbids_regex: Vec<regex::Regex>,
    // The gate's regex leaves, compiled once by `finalize` (parallel to the
    // vecs above). `matches` falls back to the raw `Gate` (per-eval compile)
    // when this is absent, so a pack that skipped finalize still matches — just slower.
    #[serde(skip)]
    compiled_gate: Option<CompiledGate>,
}

/// Recursive boolean gate for rules whose match logic needs nested
/// `all`/`any`/`not` beyond the flat matcher lists. Externally tagged: each node
/// is a one-key table — `{ all = [..] }`, `{ any = [..] }`, `{ not = {..} }`,
/// `{ contains = ".." }`, `{ regex = ".." }`, `{ line_regex = ".." }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Gate {
    /// Every sub-gate must match.
    All(Vec<Gate>),
    /// At least one sub-gate must match.
    Any(Vec<Gate>),
    /// The sub-gate must not match.
    Not(Box<Gate>),
    /// Case-insensitive substring present in the joined region text.
    Contains(String),
    /// Case-insensitive regex matches the joined region text.
    Regex(String),
    /// Case-insensitive regex matches some single line of the region.
    LineRegex(String),
}

impl Gate {
    /// Compile every regex leaf once into a [`CompiledGate`] — the gate analogue
    /// of the flat path's `compiled_regex`, built by `RulePack::finalize` so the
    /// hot evaluation never compiles a pattern twice.
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

    /// Per-evaluation fallback used only when a pack skipped `finalize` (mirrors
    /// `regex_all`'s per-call compilation fallback). `region` is the raw extracted
    /// lines; `text` is their lowercased join.
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

    /// Number of leaf matchers in the tree — counted toward the per-rule matcher
    /// cap so a deeply nested gate cannot evade the pathological-pack guard.
    fn leaf_count(&self) -> usize {
        match self {
            Self::All(gates) | Self::Any(gates) => gates.iter().map(Self::leaf_count).sum(),
            Self::Not(gate) => gate.leaf_count(),
            Self::Contains(_) | Self::Regex(_) | Self::LineRegex(_) => 1,
        }
    }

    /// Compile-check every regex leaf at load so a broken pattern fails loudly
    /// here instead of silently never matching at runtime; enforce the same
    /// matcher-length cap on leaf strings; reject vacuous `all`/`any` (`all = []`
    /// is a silent no-op, `any = []` silently makes the rule unmatchable); and
    /// bound nesting depth so an over-nested pack fails at load rather than
    /// recursing unbounded through `compile`/`eval`.
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

/// A [`Gate`] with its regex leaves compiled once (built by `RulePack::finalize`,
/// the gate parallel to the flat path's `compiled_regex`). `Contains` strings are
/// pre-lowercased so the hot path matches against the already-lowercased region
/// text with no per-call allocation.
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

const RUNTIME_PACK_DIR: &str = "/jackin/runtime/agent-status/packs";
#[cfg(test)]
const TRUSTED_PACK_BUNDLE_IDENTITY: &str = "jackin-project/agent-status-packs";
#[cfg(test)]
const LOCAL_SIGNED_BUNDLE_SIGNATURE_PREFIX: &str = "jackin-agent-status-pack-bundle:v1:";
#[cfg(test)]
const MAX_SIGNED_BUNDLE_BYTES: usize = 512 * 1024;
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

    #[cfg(test)]
    fn from_pack_dirs(
        runtime_pack_dir: Option<&Path>,
        override_pack_dir: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let mut sources = vec![PackSource::Embedded];
        if let Some(dir) = runtime_pack_dir {
            sources.push(PackSource::LocalDir(dir.to_path_buf()));
        }
        if let Some(dir) = override_pack_dir {
            sources.push(PackSource::LocalDir(dir.to_path_buf()));
        }
        Self::from_sources(sources)
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

impl SignedPackBundle {
    #[cfg(test)]
    pub fn local_test_signature_for(identity: &str) -> String {
        format!("{LOCAL_SIGNED_BUNDLE_SIGNATURE_PREFIX}{identity}")
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
    #[cfg(not(test))]
    {
        let _ = bundle;
        anyhow::bail!("remote pack bundles require a production signature verifier");
    }
    #[cfg(test)]
    {
        verify_local_test_signed_bundle(bundle)
    }
}

#[cfg(test)]
fn verify_local_test_signed_bundle(
    bundle: &SignedPackBundle,
) -> anyhow::Result<&[SignedPackEntry]> {
    anyhow::ensure!(
        bundle.signer_identity == TRUSTED_PACK_BUNDLE_IDENTITY,
        "remote pack bundle signer identity rejected"
    );
    anyhow::ensure!(
        bundle.signature == SignedPackBundle::local_test_signature_for(&bundle.signer_identity),
        "remote pack bundle signature rejected"
    );
    let bundle_bytes = bundle
        .packs
        .iter()
        .map(|entry| entry.label.len() + entry.content.len())
        .sum::<usize>();
    anyhow::ensure!(
        bundle_bytes <= MAX_SIGNED_BUNDLE_BYTES,
        "remote pack bundle exceeds size limit"
    );
    Ok(&bundle.packs)
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
        // Forward-compat guard: a pack built against a newer engine is skipped
        // (the load path logs it) rather than mis-evaluated by this build.
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
        // validated_versions must be a real, *bounded* semver range. Runtime
        // matching always keeps bundled packs live; the bounded range is pack
        // provenance for capture/update review, not a daemon-side dark switch.
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

    /// Validate, sort rules by descending priority, and compile every regex
    /// once. Run at load so evaluation does no per-tick sort or regex build.
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
        // `self.rule` is pre-sorted by descending priority in `finalize`.
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
        // `self.rule` is pre-sorted by descending priority in `finalize`.
        self.rule
            .iter()
            .map(|rule| rule.evaluation(screen_rows, virtuals))
            .collect()
    }
}

/// Build a case-insensitive regex. Used by `finalize` to compile each pattern
/// once at load, by `validate` to compile-check patterns (so a broken pattern
/// fails loudly at load, not silently at eval), and by `regex_all` as the
/// per-call fallback for a pack loaded without `finalize`.
fn build_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
}

/// Evaluate `pred` against every pattern's compiled regex. Uses the regexes
/// `finalize` compiled when present (the production path); falls back to
/// per-call compilation when they are absent (a pack that skipped finalize), so
/// the result is correct either way — never a silent non-match.
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
            // line_regex: each pattern must match SOME line of the region.
            && regex_all(&self.line_regex, &self.compiled_line_regex, |re| {
                region.iter().any(|line| re.is_match(line))
            })
            // forbids_regex: NO pattern may match ANY line of the region.
            && regex_all(&self.forbids_regex, &self.compiled_forbids_regex, |re| {
                !region.iter().any(|line| re.is_match(line))
            })
            // Recursive nested gate (combined by AND with the flat matchers);
            // absent on most rules. Prefer the finalize-compiled gate; fall back
            // to per-eval compilation only for a pack that skipped finalize.
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
        // Skip-and-log a bad pack rather than failing the whole registry: one
        // malformed operator override pack must not discard the validated
        // embedded packs and take screen detection dark for every agent.
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

#[cfg(test)]
mod tests;
