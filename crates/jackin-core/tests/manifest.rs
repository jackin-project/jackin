// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jackin_core::{
    Agent, DirEntry, EnvFile, EnvName, EnvSpec, FileIdentity, FileKind, FileLock, FileMode,
    FileSystem, FsError, HookKind, HostPath, LinkProblem, ManifestError, RegularFile,
    RegularFileProblem, RepoPath, RoleManifest, StdFileSystem,
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;
type TestResult = Fallible<()>;

const FULL: &str = r#"schema = 1

[role]
name = "sentinel"
dockerfile = "Dockerfile"
policy = "policy.yaml"

[agents.codex]
model = "gpt-6.1-sol"
instructions = "instructions/codex.md"

[docker]
enabled = false

[hooks]
setup = "hooks/setup.sh"
preflight = "hooks/preflight.sh"

[env.EXAMPLE_NAME]
default = "value"
prompt = "Text for the operator"
"#;

const MIN: &str = "schema = 1\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n";

fn label(error: &ManifestError) -> String {
    match error {
        ManifestError::Syntax { line, .. } => format!("syntax@{line}"),
        ManifestError::UnknownField { key } => format!("unknown:{key}"),
        ManifestError::MissingField { key } => format!("missing:{key}"),
        ManifestError::InvalidValue { key, .. } => format!("invalid:{key}"),
        ManifestError::UnsupportedSchema { found } => format!("schema:{found}"),
        ManifestError::ReservedEnvName(name) => format!("reserved:{name}"),
        ManifestError::NoInstructionMapping { role, agent } => {
            format!("no-instructions:{role}:{}", agent.as_str())
        }
        ManifestError::UnsupportedAgentSection { agent } => {
            format!("unsupported-agent:{}", agent.as_str())
        }
        ManifestError::Fs(_) => "fs".to_owned(),
    }
}

fn outcome(text: &str) -> String {
    match RoleManifest::parse(text) {
        Ok(_) => "ok".to_owned(),
        Err(error) => label(&error),
    }
}

fn model(text: &str) -> Fallible<toml::Table> {
    Ok(text.parse::<toml::Table>()?)
}

fn env_name(name: &str) -> Fallible<EnvName> {
    Ok(EnvName::parse(name)?)
}

#[test]
fn manifest_parses_the_d11_example() -> TestResult {
    let manifest = RoleManifest::parse(FULL)?;
    assert_eq!(manifest.role.name.as_str(), "sentinel");
    assert_eq!(manifest.role.dockerfile.as_str(), "Dockerfile");
    assert_eq!(manifest.role.policy.as_str(), "policy.yaml");
    assert_eq!(
        manifest
            .agents
            .get(&Agent::Codex)
            .and_then(|s| s.model.as_ref())
            .map(ToString::to_string),
        Some("gpt-6.1-sol".to_owned())
    );
    assert_eq!(
        manifest
            .agents
            .get(&Agent::Codex)
            .map(|s| s.instructions.as_str()),
        Some("instructions/codex.md")
    );
    assert!(!manifest.docker.enabled);
    assert_eq!(
        manifest.hook(HookKind::Setup).map(RepoPath::as_str),
        Some("hooks/setup.sh")
    );
    assert_eq!(
        manifest.hook(HookKind::Preflight).map(RepoPath::as_str),
        Some("hooks/preflight.sh")
    );
    assert_eq!(manifest.env.len(), 1);
    let spec = manifest.env.get(&env_name("EXAMPLE_NAME")?).ok_or("env")?;
    assert_eq!(spec.default.as_deref(), Some("value"));
    assert_eq!(spec.prompt.as_deref(), Some("Text for the operator"));
    Ok(())
}

#[test]
fn manifest_render_round_trips_and_writes_the_documented_keys() -> TestResult {
    let manifest = RoleManifest::parse(FULL)?;
    let rendered = manifest.render();
    assert_eq!(RoleManifest::parse(&rendered)?, manifest);
    assert_eq!(RoleManifest::parse(&rendered)?.render(), rendered);
    assert_eq!(model(&rendered)?, model(FULL)?);
    let min = RoleManifest::parse(MIN)?;
    assert_eq!(RoleManifest::parse(&min.render())?, min);
    Ok(())
}

#[test]
fn manifest_render_escapes_arbitrary_env_text() -> TestResult {
    let samples = [
        "",
        "plain",
        "a\"b",
        "a\\b",
        "cr\rin",
        "tab\there",
        "\u{1}",
        "é€😀",
        "'single'",
        "'''",
        "\"\"\"",
        "# not a comment",
        "[table]",
        "key = \"v\"",
        " padded ",
    ];
    for sample in samples {
        let mut manifest = RoleManifest::parse(MIN)?;
        manifest.env.insert(
            env_name("X")?,
            EnvSpec {
                default: Some(sample.to_owned()),
                prompt: Some(sample.to_owned()),
            },
        );
        let rendered = manifest.render();
        assert_eq!(RoleManifest::parse(&rendered)?, manifest, "{sample:?}");
        let table = model(&rendered)?;
        let entry = table
            .get("env")
            .and_then(|env| env.get("X"))
            .ok_or("env.X")?;
        assert_eq!(entry.get("default").and_then(|v| v.as_str()), Some(sample));
        assert_eq!(entry.get("prompt").and_then(|v| v.as_str()), Some(sample));
    }
    Ok(())
}

fn toml_basic(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[test]
fn manifest_env_default_and_prompt_reject_newline_and_nul_exactly_when_the_env_file_does()
-> TestResult {
    let samples = [
        "",
        "plain",
        "a b",
        "a\"b",
        "a\\b",
        "tab\t",
        "cr\r",
        "\u{1}",
        "é€😀",
        "'quote'",
        "a\nb",
        "a\r\nb",
        "\n",
        "a\0b",
        "\0",
        "x\n",
    ];
    for sample in samples {
        let bad = sample.contains(['\n', '\0']);
        let accepted_by_env_file =
            EnvFile::new(BTreeMap::from([(env_name("A")?, sample.to_owned())])).is_ok();
        assert_eq!(accepted_by_env_file, !bad, "{sample:?}");
        for key in ["default", "prompt"] {
            let text = format!("{MIN}[env.A]\n{key} = {}\n", toml_basic(sample));
            let expected = if bad {
                format!("invalid:env.A.{key}")
            } else {
                "ok".to_owned()
            };
            assert_eq!(outcome(&text), expected, "{key} {sample:?}");
        }
    }
    for key in ["default", "prompt"] {
        let text = format!("{MIN}[env.A]\n{key} = \"\"\"\nline1\nline2\"\"\"\n");
        assert_eq!(outcome(&text), format!("invalid:env.A.{key}"), "{key}");
    }
    Ok(())
}

#[test]
fn manifest_rejects_an_unknown_field_at_every_level_naming_its_path() -> TestResult {
    let cases = [
        ("root", "", "extra = 1", "extra", true),
        ("role", "", "nmae = \"x\"", "role.nmae", false),
        (
            "agents.codex",
            "[agents.codex]\ninstructions = \"i.md\"\n",
            "modle = \"x\"",
            "agents.codex.modle",
            false,
        ),
        ("legacy codex", "", "[codex]\nmodel = \"x\"", "codex", false),
        (
            "docker",
            "[docker]\n",
            "enable = true",
            "docker.enable",
            false,
        ),
        (
            "hooks",
            "[hooks]\n",
            "setpu = \"a.sh\"",
            "hooks.setpu",
            false,
        ),
        (
            "env",
            "[env.FOO]\n",
            "defualt = \"x\"",
            "env.FOO.defualt",
            false,
        ),
        (
            "env description",
            "[env.FOO]\ndefault = \"d\"\nprompt = \"p\"\n",
            "description = \"c\"",
            "env.FOO.description",
            false,
        ),
        ("claude", "", "[claude]\nmodel = \"x\"", "claude", false),
    ];
    for (name, context, injected, key, first) in cases {
        let (control, text) = if first {
            (MIN.to_owned(), format!("{injected}\n{MIN}"))
        } else {
            let base = format!("{MIN}{context}");
            (base.clone(), format!("{base}{injected}\n"))
        };
        assert_eq!(outcome(&control), "ok", "control {name}");
        assert_eq!(outcome(&text), format!("unknown:{key}"), "{name}");
        let shown = RoleManifest::parse(&text).err().ok_or(name)?.to_string();
        assert!(shown.contains(key), "{name}: {shown}");
    }
    Ok(())
}

fn hook_cases() -> Vec<(String, String)> {
    let mut cases = Vec::new();
    for bad in ["/etc/x.sh", "../x.sh", "hooks/../x.sh", "hooks//x.sh", ""] {
        cases.push((
            format!("[hooks]\nsetup = \"{bad}\"\n"),
            "invalid:hooks.setup".to_owned(),
        ));
    }
    cases.push((
        "[hooks]\npreflight = \"/abs\"\n".to_owned(),
        "invalid:hooks.preflight".to_owned(),
    ));
    for good in ["hooks/setup.sh", "setup.sh"] {
        cases.push((format!("[hooks]\nsetup = \"{good}\"\n"), "ok".to_owned()));
    }
    cases
}

#[test]
fn manifest_rejects_invalid_values_naming_the_key() -> TestResult {
    let role = |name: &str, dockerfile: &str, policy: &str| {
        format!(
            "schema = 1\n[role]\nname = \"{name}\"\ndockerfile = \"{dockerfile}\"\npolicy = \"{policy}\"\n"
        )
    };
    let mut cases: Vec<(String, String)> = vec![
        (
            "schema = 2\n".to_owned() + &MIN["schema = 1\n".len()..],
            "schema:2".to_owned(),
        ),
        (
            "schema = 0\n".to_owned() + &MIN["schema = 1\n".len()..],
            "schema:0".to_owned(),
        ),
        (
            MIN["schema = 1\n".len()..].to_owned(),
            "missing:schema".to_owned(),
        ),
        (
            "schema = \"1\"\n".to_owned() + &MIN["schema = 1\n".len()..],
            "invalid:schema".to_owned(),
        ),
        (
            "schema = 1\n[role]\nname = \"s\"\ndockerfile = \"Dockerfile\"\n".to_owned(),
            "missing:role.policy".to_owned(),
        ),
        (
            role("bad name", "Dockerfile", "p.yaml"),
            "invalid:role.name".to_owned(),
        ),
        (
            role("s", "/Dockerfile", "p.yaml"),
            "invalid:role.dockerfile".to_owned(),
        ),
        (
            role("s", "Dockerfile", "../p.yaml"),
            "invalid:role.policy".to_owned(),
        ),
        (
            format!(
                "{MIN}[agents.codex]\nmodel = \"<optional model name>\"\ninstructions = \"i.md\"\n"
            ),
            "invalid:agents.codex.model".to_owned(),
        ),
        (
            format!(
                "{MIN}[agents.codex]\nmodel = \"gpt\\\"\\nmodel_provider=\\\"x\"\ninstructions = \"i.md\"\n"
            ),
            "invalid:agents.codex.model".to_owned(),
        ),
        (
            format!("{MIN}[docker]\nenabled = \"yes\"\n"),
            "invalid:docker.enabled".to_owned(),
        ),
        (format!("{MIN}[env.\"1A\"]\n"), "invalid:env.1A".to_owned()),
        (
            format!("{MIN}[env.FOO]\ndefault = 1\n"),
            "invalid:env.FOO.default".to_owned(),
        ),
        (
            format!("{MIN}[env.FOO]\nprompt = true\n"),
            "invalid:env.FOO.prompt".to_owned(),
        ),
        ("schema =".to_owned(), "syntax@1".to_owned()),
    ];
    for name in [
        "JACKIN_X",
        "DOCKER_HOST",
        "CODEX_HOME",
        "CODEX_AUTH_ACCESS_TOKEN",
        "PATH",
        "OPENAI_API_KEY",
    ] {
        cases.push((format!("{MIN}[env.{name}]\n"), format!("reserved:{name}")));
    }
    for name in ["FOO", "MY_DOCKER_HOST", "PATHS", "MY_PATH"] {
        cases.push((format!("{MIN}[env.{name}]\n"), "ok".to_owned()));
    }
    for (hook, expected) in hook_cases() {
        cases.push((format!("{MIN}{hook}"), expected));
    }
    for (text, expected) in cases {
        assert_eq!(outcome(&text), expected, "{text}");
    }
    Ok(())
}

#[test]
fn manifest_agents_sections_validate_and_render() -> TestResult {
    let text = format!(
        "{MIN}[agents.omp]\ninstructions = \"instructions/omp.md\"\n[agents.codex]\nmodel = \"gpt-6.1-sol\"\ninstructions = \"instructions/codex.md\"\n"
    );
    let manifest = RoleManifest::parse(&text)?;
    let rendered = manifest.render();
    let expected_agents = "[agents.codex]\nmodel = \"gpt-6.1-sol\"\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n";
    assert!(rendered.contains(expected_agents), "{rendered}");
    assert_eq!(RoleManifest::parse(&rendered)?, manifest);
    assert_eq!(
        outcome(&format!("{MIN}[agents.nope]\ninstructions = \"i.md\"\n")),
        "invalid:agents.nope"
    );
    assert_eq!(
        outcome(&format!("{MIN}[agents.grok]\ninstructions = \"i.md\"\n")),
        "unsupported-agent:grok"
    );
    let grok = RoleManifest::parse(&format!("{MIN}[agents.grok]\ninstructions = \"i.md\"\n"))
        .err()
        .ok_or("grok")?;
    assert!(matches!(
        grok,
        ManifestError::UnsupportedAgentSection {
            agent: Agent::GrokBuild
        }
    ));
    RoleManifest::parse(&format!(
        "{MIN}[agents.omp]\nmodel = \"glm-5.3\"\ninstructions = \"i.md\"\n"
    ))?;
    RoleManifest::parse(&format!(
        "{MIN}[agents.codex]\nmodel = \"glm-5.3\"\ninstructions = \"i.md\"\n"
    ))?;
    assert_eq!(
        outcome(&format!(
            "{MIN}[agents.codex]\nmodel = \"kimi-for-coding\"\ninstructions = \"i.md\"\n"
        )),
        "invalid:agents.codex.model"
    );
    assert_eq!(
        outcome(&format!("{MIN}[codex]\nmodel = \"gpt-6.1-sol\"\n")),
        "unknown:codex"
    );
    assert_eq!(outcome(MIN), "ok");
    assert_eq!(
        outcome(&format!("{MIN}[agents.codex]\nmodel = \"gpt-6.1-sol\"\n")),
        "missing:agents.codex.instructions"
    );
    assert_eq!(
        outcome(&format!(
            "{MIN}[agents.codex]\ninstructions = \"../x.md\"\n"
        )),
        "invalid:agents.codex.instructions"
    );
    assert_eq!(
        outcome(&format!(
            "{MIN}[agents.codex]\ninstructions = \"i.md\"\nextra = 1\n"
        )),
        "unknown:agents.codex.extra"
    );
    Ok(())
}

#[test]
fn manifest_rejects_the_removed_t01_model_with_a_typed_invalid_value() -> TestResult {
    let error = RoleManifest::parse(&format!(
        "{MIN}[agents.codex]\nmodel = \"gpt-5.1-codex\"\ninstructions = \"instructions/codex.md\"\n"
    ))
    .err()
    .ok_or("expected invalid model")?;
    assert!(
        matches!(error, ManifestError::InvalidValue { ref key, .. } if key == "agents.codex.model"),
        "{error}"
    );
    Ok(())
}

#[test]
fn manifest_syntax_errors_never_echo_a_pasted_secret() -> TestResult {
    let sentinel = "Zq7#Xv9!Lm2@Hk4wP";
    let chars: Vec<char> = sentinel.chars().collect();
    let error = RoleManifest::parse(&format!("schema = {sentinel}"))
        .err()
        .ok_or("syntax")?;
    assert!(matches!(error, ManifestError::Syntax { .. }));
    for shown in [error.to_string(), format!("{error:?}")] {
        for window in chars.windows(3) {
            let window: String = window.iter().collect();
            assert!(!shown.contains(&window), "{window} in {shown}");
        }
    }
    Ok(())
}

#[test]
fn manifest_optional_sections_default_and_env_entries_keep_default_and_prompt_distinct()
-> TestResult {
    let min = RoleManifest::parse(MIN)?;
    assert!(min.agents.is_empty());
    assert!(!min.docker.enabled);
    assert!(min.hook(HookKind::Setup).is_none() && min.hook(HookKind::Preflight).is_none());
    assert!(min.env.is_empty());
    assert!(
        RoleManifest::parse(&format!("{MIN}[docker]\nenabled = true\n"))?
            .docker
            .enabled
    );
    let text = format!(
        "{MIN}[env.A]\ndefault = \"d\"\n[env.B]\nprompt = \"p\"\n[env.C]\ndefault = \"d\"\nprompt = \"p\"\n[env.D]\n[env.E]\ndefault = \"\"\n"
    );
    let manifest = RoleManifest::parse(&text)?;
    let entry = |name: &str| -> Fallible<(Option<String>, Option<String>)> {
        let spec = manifest.env.get(&env_name(name)?).ok_or(name.to_owned())?;
        Ok((spec.default.clone(), spec.prompt.clone()))
    };
    assert_eq!(entry("A")?, (Some("d".to_owned()), None));
    assert_eq!(entry("B")?, (None, Some("p".to_owned())));
    assert_eq!(entry("C")?, (Some("d".to_owned()), Some("p".to_owned())));
    assert_eq!(entry("D")?, (None, None));
    assert_eq!(entry("E")?, (Some(String::new()), None));
    assert_eq!(RoleManifest::parse(&manifest.render())?, manifest);
    Ok(())
}

#[test]
fn manifest_findings_report_every_problem_in_one_pass() -> TestResult {
    let text = "schema = 1\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"/abs/policy.yaml\"\n[agents.codex]\nmodel = \"bad model\"\ninstructions = \"i.md\"\n[docker]\nenabled = \"yes\"\n[hooks]\nsetpu = \"a.sh\"\nprefight = \"b.sh\"\n[env.JACKIN_X]\n";
    let mut found: Vec<String> = RoleManifest::findings(text).iter().map(label).collect();
    found.sort();
    assert_eq!(
        found,
        [
            "invalid:agents.codex.model",
            "invalid:docker.enabled",
            "invalid:role.policy",
            "reserved:JACKIN_X",
            "unknown:hooks.prefight",
            "unknown:hooks.setpu",
        ]
    );
    assert_eq!(
        label(&RoleManifest::parse(text).err().ok_or("parse")?),
        "invalid:role.policy"
    );
    assert!(RoleManifest::findings(FULL).is_empty());
    let syntax = RoleManifest::findings("schema =");
    assert_eq!(syntax.len(), 1);
    assert!(matches!(syntax[0], ManifestError::Syntax { .. }));
    Ok(())
}

#[derive(Debug)]
struct CrashFs {
    inner: StdFileSystem,
    fired: AtomicBool,
}

impl CrashFs {
    fn gate(&self, path: &HostPath) -> Result<(), FsError> {
        if self.fired.load(Ordering::SeqCst) {
            return Err(FsError::Io {
                path: path.clone(),
                kind: ErrorKind::StorageFull,
                message: "injected".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystem for CrashFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(p).await
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(p).await
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(p).await
    }
    async fn canonicalize(&self, p: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn identity(&self, p: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
    }
    async fn create_private_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(d).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.gate(p)?;
        self.inner.remove_file(p).await
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(root, file).await
    }
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(path, target).await
    }
    async fn read_regular(
        &self,
        r: &HostPath,
        f: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(r, f, max_bytes).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(p).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, &b[..b.len() / 2], m).await?;
        self.fired.store(true, Ordering::SeqCst);
        self.gate(p)
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(d).await
    }
}

#[tokio::test]
async fn manifest_write_replaces_the_file_atomically() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = HostPath::parse(&dir.path().join("jackin.role.toml"))?;
    let fs = StdFileSystem::default();
    let long = RoleManifest::parse(FULL)?;
    long.write(&fs, &path).await?;
    assert_eq!(std::fs::read(path.as_path())?, long.render().as_bytes());
    assert_eq!(
        RoleManifest::parse(&std::fs::read_to_string(path.as_path())?)?,
        long
    );
    let short = RoleManifest::parse(MIN)?;
    short.write(&fs, &path).await?;
    assert_eq!(std::fs::read(path.as_path())?, short.render().as_bytes());
    let before = std::fs::read(path.as_path())?;
    let crashing = CrashFs {
        inner: StdFileSystem::default(),
        fired: AtomicBool::new(false),
    };
    let error = long.write(&crashing, &path).await.err().ok_or("crash")?;
    assert!(matches!(
        error,
        ManifestError::Fs(FsError::Io {
            kind: ErrorKind::StorageFull,
            ..
        })
    ));
    assert_eq!(std::fs::read(path.as_path())?, before);
    assert!(Path::new(dir.path()).exists());
    Ok(())
}
