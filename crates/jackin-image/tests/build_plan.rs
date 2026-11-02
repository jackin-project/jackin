// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

use jackin_core::{
    Agent, AttemptId, BuilderName, CommitId, DockerLayer, HookKind, HostPath, ImageId, Platform,
    RepoPath, RoleName, SupervisorState, sandbox_layout,
};
use jackin_image::{
    BuildStage, ContextSource, FinalBuilds, HookFiles, ImagePlan, ImageRequest, MaterializedFile,
    PlannedBuild, Rebuild, RoleRevision, SourceFile,
};

type TestResult = Result<(), Box<dyn Error>>;

const CONSTRUCT_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/Dockerfile"
));
const DERIVED_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/derived/Dockerfile"
));
const JACKIN_START: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/jackin-start"));
const ARG_NAMES: [&str; 4] = [
    "CODEX_VERSION",
    "CODEX_BIN_DIR",
    "CODEX_SHA256_ARM64",
    "CODEX_SHA256_AMD64",
];
const FLAG_PREFIXES: [&str; 9] = [
    "--builder=",
    "--platform=",
    "--provenance=",
    "--sbom=",
    "--progress=",
    "--tag=",
    "--file=",
    "--build-context=",
    "--build-arg=",
];

fn construct_id() -> Result<ImageId, Box<dyn Error>> {
    Ok(ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?)
}

fn source(path: &str, bytes: &[u8]) -> Result<SourceFile, Box<dyn Error>> {
    Ok(SourceFile {
        path: RepoPath::parse(path)?,
        bytes: bytes.to_vec(),
    })
}

fn base() -> Result<ImageRequest, Box<dyn Error>> {
    Ok(ImageRequest {
        role: RoleName::parse("sentinel")?,
        context: HostPath::parse(Path::new("/work/role"))?,
        dockerfile: source("Dockerfile", b"FROM construct AS role\n")?,
        hooks: HookFiles {
            setup: Some(source("hooks/setup.sh", b"#!/bin/sh\necho setup\n")?),
            preflight: None,
        },
        revision: RoleRevision::Clean {
            commit: CommitId::parse(&"1".repeat(40))?,
        },
        agent: Agent::Codex,
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        rebuild: Rebuild::No,
    })
}

fn tokens(args: &[String]) -> Result<Vec<String>, Box<dyn Error>> {
    for arg in args {
        let known = arg == "--load"
            || arg == "--no-cache"
            || FLAG_PREFIXES.iter().any(|prefix| arg.starts_with(prefix));
        if !known {
            return Err(format!("unexpected argument {arg:?}").into());
        }
    }
    let mut sorted = args.to_vec();
    sorted.sort();
    Ok(sorted)
}

fn common(request: &ImageRequest, build: &PlannedBuild) -> Vec<String> {
    let mut expected = vec![
        format!("--builder={}", request.builder),
        format!("--platform={}", request.platform.docker_arg()),
        "--provenance=false".to_owned(),
        "--sbom=false".to_owned(),
        "--load".to_owned(),
        "--progress=plain".to_owned(),
        format!("--tag={}", build.tag),
    ];
    if request.rebuild == Rebuild::Yes {
        expected.push("--no-cache".to_owned());
    }
    expected
}

fn sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items
}

fn split_build_args(args: &[String]) -> (Vec<String>, Vec<String>) {
    args.iter()
        .cloned()
        .partition(|arg| !arg.starts_with("--build-arg="))
}

fn check_stages(request: &ImageRequest) -> TestResult {
    let label = format!(
        "{:?} {:?} {:?}",
        request.platform, request.rebuild, request.docker
    );
    let plan = ImagePlan::new(request)?;
    let construct = plan.construct_build();
    let built: FinalBuilds = plan.final_builds(&construct_id()?);
    assert_eq!(construct.stage, BuildStage::Construct, "{label}");
    assert_eq!(built.role.stage, BuildStage::Role, "{label}");
    assert_eq!(built.derived.stage, BuildStage::Derived, "{label}");
    assert_eq!(
        built.derived.tag,
        plan.final_tag(&construct_id()?),
        "{label}"
    );
    assert_eq!(
        tokens(&construct.args)?,
        sorted(common(request, construct)),
        "construct {label}"
    );
    let mut role = common(request, &built.role);
    role.push(format!(
        "--file={}/{}",
        request.context.as_path().display(),
        request.dockerfile.path.as_str()
    ));
    role.push(format!(
        "--build-context=construct=docker-image://{}",
        construct.tag
    ));
    assert_eq!(tokens(&built.role.args)?, sorted(role), "role {label}");
    let (derived_rest, build_args) = split_build_args(&built.derived.args);
    let mut derived = common(request, &built.derived);
    derived.push(format!(
        "--build-context=role=docker-image://{}",
        built.role.tag
    ));
    assert_eq!(tokens(&derived_rest)?, sorted(derived), "derived {label}");
    assert_eq!(build_args.len(), 4, "derived build args {label}");
    let all = [&construct.args, &built.role.args, &built.derived.args];
    let no_cache = all
        .iter()
        .filter(|args| args.contains(&"--no-cache".to_owned()))
        .count();
    assert_eq!(
        no_cache,
        if request.rebuild == Rebuild::Yes {
            3
        } else {
            0
        },
        "{label}"
    );
    Ok(())
}

#[test]
fn buildx_arguments_are_explicit_single_tokens_for_all_three_stages() -> TestResult {
    for platform in [Platform::LinuxArm64, Platform::LinuxAmd64] {
        for rebuild in [Rebuild::No, Rebuild::Yes] {
            for docker in [DockerLayer::Absent, DockerLayer::Client] {
                let mut request = base()?;
                request.platform = platform;
                request.rebuild = rebuild;
                request.docker = docker;
                check_stages(&request)?;
            }
        }
    }
    let mut nested = base()?;
    nested.dockerfile.path = RepoPath::parse("docker/Role.Dockerfile")?;
    let built = ImagePlan::new(&nested)?.final_builds(&construct_id()?);
    assert!(
        built
            .role
            .args
            .contains(&"--file=/work/role/docker/Role.Dockerfile".to_owned())
    );
    for builder in ["--help", "-evil"] {
        let mut request = base()?;
        request.builder = BuilderName::parse(builder)?;
        let plan = ImagePlan::new(&request)?;
        let built = plan.final_builds(&construct_id()?);
        let want = format!("--builder={builder}");
        for args in [
            &plan.construct_build().args,
            &built.role.args,
            &built.derived.args,
        ] {
            assert!(args.contains(&want), "{builder}");
            tokens(args)?;
        }
    }
    assert_eq!(Rebuild::from(true), Rebuild::Yes);
    assert_eq!(Rebuild::from(false), Rebuild::No);
    Ok(())
}

fn materialized(build: &PlannedBuild) -> Result<&[MaterializedFile], Box<dyn Error>> {
    match &build.context {
        ContextSource::Materialized(files) => Ok(files),
        ContextSource::RoleRepository(path) => Err(format!("repository context {path:?}").into()),
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn hook_name(kind: HookKind) -> String {
    format!(
        "hooks/{}",
        last_segment(sandbox_layout::hook_path(kind).as_str())
    )
}

#[test]
fn build_contexts_hold_exactly_the_documented_files() -> TestResult {
    let setup: &[u8] = b"#!/bin/sh\necho setup\n";
    let preflight: &[u8] = b"#!/bin/sh\necho pre\n";
    let combos = [
        (Some(setup), Some(preflight)),
        (Some(setup), None),
        (None, Some(preflight)),
        (None, None),
    ];
    for (declared_setup, declared_preflight) in combos {
        let mut request = base()?;
        request.hooks.setup = declared_setup
            .map(|b| source("hooks/s.sh", b))
            .transpose()?;
        request.hooks.preflight = declared_preflight
            .map(|b| source("hooks/p.sh", b))
            .transpose()?;
        let plan = ImagePlan::new(&request)?;
        let label = format!("{declared_setup:?} {declared_preflight:?}");

        let construct = materialized(plan.construct_build())?;
        assert_eq!(construct.len(), 1, "{label}");
        assert_eq!(construct[0].name, "Dockerfile", "{label}");
        assert_eq!(construct[0].bytes, CONSTRUCT_DOCKERFILE, "{label}");

        let built = plan.final_builds(&construct_id()?);
        assert_eq!(
            built.role.context,
            ContextSource::RoleRepository(request.context.clone())
        );

        let files = materialized(&built.derived)?;
        let names: BTreeSet<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names.len(), files.len(), "duplicate name {label}");
        let setup_name = hook_name(HookKind::Setup);
        let preflight_name = hook_name(HookKind::Preflight);
        let expected: BTreeSet<&str> =
            ["Dockerfile", "jackin-start", &setup_name, &preflight_name].into();
        assert_eq!(names, expected, "{label}");
        let by_name: BTreeMap<&str, &[u8]> = files
            .iter()
            .map(|f| (f.name.as_str(), f.bytes.as_slice()))
            .collect();
        assert_eq!(by_name["Dockerfile"], DERIVED_DOCKERFILE, "{label}");
        assert_eq!(by_name["jackin-start"], JACKIN_START, "{label}");
        assert_eq!(
            by_name[setup_name.as_str()],
            declared_setup.unwrap_or(b""),
            "{label}"
        );
        assert_eq!(
            by_name[preflight_name.as_str()],
            declared_preflight.unwrap_or(b""),
            "{label}"
        );
    }
    Ok(())
}

fn build_arg_map(args: &[String]) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut map = BTreeMap::new();
    for arg in args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--build-arg="))
    {
        let (name, value) = arg.split_once('=').ok_or("build arg without a value")?;
        assert!(
            map.insert(name.to_owned(), value.to_owned()).is_none(),
            "{name} repeats"
        );
    }
    Ok(map)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn derived_args(platform: Platform) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut request = base()?;
    request.platform = platform;
    let plan = ImagePlan::new(&request)?;
    let built = plan.final_builds(&construct_id()?);
    assert!(build_arg_map(&plan.construct_build().args)?.is_empty());
    assert!(build_arg_map(&built.role.args)?.is_empty());
    build_arg_map(&built.derived.args)
}

#[test]
fn derived_build_args_come_from_the_codex_row() -> TestResult {
    let spec = Agent::Codex.spec();
    let arm = derived_args(Platform::LinuxArm64)?;
    let amd = derived_args(Platform::LinuxAmd64)?;
    assert_eq!(arm, amd);
    let names: Vec<&str> = arm.keys().map(String::as_str).collect();
    let mut want = ARG_NAMES.to_vec();
    want.sort_unstable();
    assert_eq!(names, want);
    assert_eq!(arm["CODEX_VERSION"], spec.install.version);
    assert_eq!(arm["CODEX_BIN_DIR"], "/usr/local/bin");
    assert_eq!(arm["CODEX_BIN_DIR"], parent(spec.binary.as_str()));
    assert_eq!(arm["CODEX_BIN_DIR"], parent(spec.code_mode_host.as_str()));
    for target in spec.install.targets {
        let name = format!(
            "CODEX_SHA256_{}",
            target.platform.target_arch().to_ascii_uppercase()
        );
        assert_eq!(arm[&name], target.sha256, "{name}");
        assert!(is_sha256(&arm[&name]), "{name}");
    }
    assert_ne!(arm["CODEX_SHA256_ARM64"], arm["CODEX_SHA256_AMD64"]);
    let profile_line = format!(
        "- {}/{}",
        arm["CODEX_BIN_DIR"],
        last_segment(spec.binary.as_str())
    );
    assert!(
        spec.openshell
            .chatgpt_profile
            .yaml
            .lines()
            .any(|line| line.trim() == profile_line),
        "profile lacks {profile_line}"
    );
    Ok(())
}

fn instructions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (body, continues) = match trimmed.strip_suffix('\\') {
            Some(body) => (body.trim_end(), true),
            None => (trimmed, false),
        };
        let joined = match open.take() {
            Some(previous) => format!("{previous} {body}"),
            None => body.to_owned(),
        };
        if continues {
            open = Some(joined);
        } else {
            out.push(joined);
        }
    }
    out.extend(open);
    out
}

fn derived_text() -> Result<&'static str, Box<dyn Error>> {
    Ok(std::str::from_utf8(DERIVED_DOCKERFILE)?)
}

fn declared_args(instructions: &[String]) -> BTreeSet<String> {
    instructions
        .iter()
        .filter_map(|line| line.strip_prefix("ARG "))
        .map(|rest| rest.split('=').next().unwrap_or(rest).trim().to_owned())
        .collect()
}

fn archive_stage_adds(instructions: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut stage = String::new();
    let mut adds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in instructions {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let ["FROM", _, "AS", name] = words.as_slice() {
            stage = (*name).to_owned();
        } else if let Some(rest) = line.strip_prefix("ADD ") {
            adds.entry(stage.clone()).or_default().push(rest.to_owned());
        }
    }
    adds
}

#[test]
fn derived_dockerfile_installs_what_the_codex_row_describes() -> TestResult {
    let spec = Agent::Codex.spec();
    let text = derived_text()?;
    let lines = instructions(text);
    let wanted: BTreeSet<String> = ARG_NAMES.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(declared_args(&lines), wanted);
    let adds = archive_stage_adds(&lines);
    for target in spec.install.targets {
        let arch = target.platform.target_arch();
        let url = spec
            .install
            .url_template
            .replace("{version}", "${CODEX_VERSION}")
            .replace("{target}", target.triple);
        let want = format!(
            "--checksum=sha256:${{CODEX_SHA256_{}}} {url} /codex-package.tar.gz",
            arch.to_ascii_uppercase()
        );
        let stage = format!("codex-archive-{arch}");
        assert_eq!(adds.get(&stage), Some(&vec![want]), "{stage}");
    }
    assert!(
        lines.contains(&"FROM codex-archive-${TARGETARCH} AS codex-archive".to_owned()),
        "no selector stage"
    );
    let unquoted = text.replace('"', "");
    let words: BTreeSet<&str> = unquoted.split_whitespace().collect();
    for path in [&spec.binary, &spec.code_mode_host] {
        let want = format!("${{CODEX_BIN_DIR}}/{}", last_segment(path.as_str()));
        assert!(words.contains(want.as_str()), "{want}");
    }
    for member in spec.install.archive_members {
        assert!(text.contains(member), "{member}");
    }
    assert!(text.contains(spec.install.system_config.as_str()));
    let printf = format!(
        "printf '{}'",
        spec.install.system_config_content.replace('\n', "\\n")
    );
    assert!(text.contains(&printf), "{printf}");
    let version_output = spec
        .install
        .version_output
        .replace("{version}", "${CODEX_VERSION}");
    assert!(text.contains(&version_output), "{version_output}");
    assert!(!text.contains(spec.install.version), "literal version");
    for target in spec.install.targets {
        assert!(!text.contains(target.sha256), "literal checksum");
    }
    Ok(())
}

#[test]
fn derived_dockerfile_installs_jackin_start_and_hooks_at_the_layout_paths() -> TestResult {
    let lines = instructions(derived_text()?);
    let last_from = lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("FROM "))
        .ok_or("no FROM")?;
    assert_eq!(last_from.split_whitespace().next(), Some("role"));
    let last_user = lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("USER "))
        .ok_or("no USER")?;
    assert_eq!(last_user, "sandbox");
    let copies: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.strip_prefix("COPY "))
        .collect();
    let start = "--chown=root:root --chmod=0755 jackin-start /usr/local/bin/jackin-start";
    assert_eq!(
        copies.iter().filter(|c| **c == start).count(),
        1,
        "{copies:?}"
    );
    let hooks = format!(
        "--chown=root:root --chmod=0755 {} {} {}/",
        hook_name(HookKind::Setup),
        hook_name(HookKind::Preflight),
        sandbox_layout::HOOKS_DIR.as_str()
    );
    assert_eq!(
        copies.iter().filter(|c| **c == hooks).count(),
        1,
        "{copies:?}"
    );
    Ok(())
}

fn script() -> Result<&'static str, Box<dyn Error>> {
    Ok(std::str::from_utf8(JACKIN_START)?)
}

fn assignment<'a>(script: &'a str, name: &str) -> Result<&'a str, Box<dyn Error>> {
    let prefix = format!("{name}=");
    script
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .ok_or_else(|| format!("no {name}= line").into())
}

#[test]
fn embedded_jackin_start_publishes_what_the_supervisor_parser_accepts() -> TestResult {
    let script = script()?;
    let state = sandbox_layout::RUN_STATE.as_str();
    let start = sandbox_layout::RUN_START.as_str();
    let run = assignment(script, "run")?;
    assert_eq!(run, parent(state));
    assert_eq!(run, parent(start));
    assert_eq!(last_segment(state), "state");
    assert_eq!(last_segment(start), "start");
    assert!(script.contains("\"$run/state\""));
    assert!(script.contains("\"$run/start\""));
    let project = assignment(script, "project")?;
    assert_eq!(project, sandbox_layout::PROJECT.as_str());
    assert_eq!(project, Agent::Codex.spec().workdir.as_str());

    let published: Vec<String> = script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("publish "))
        .map(|args| {
            args.replace('"', "")
                .replace("$attempt", "3")
                .replace("$rc", "7")
        })
        .collect();
    assert_eq!(published, ["waiting", "running 3", "exited 3 7"]);
    for text in &published {
        let parsed = SupervisorState::parse(text)?;
        assert_eq!(&parsed.render(), text);
    }

    let patterns: Vec<&str> = script
        .split(|c: char| c.is_whitespace() || c == '|' || c == ')')
        .filter(|word| word.starts_with('?'))
        .collect();
    assert_eq!(patterns.len(), 1, "{patterns:?}");
    assert_eq!(patterns[0], format!("{}*", "?".repeat(19)));
    AttemptId::parse(&"9".repeat(18))?;
    AttemptId::parse(&"9".repeat(19))
        .err()
        .ok_or("19 digits were accepted")?;
    Ok(())
}
