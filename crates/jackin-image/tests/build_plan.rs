// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
const SCAN: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/derived/scan"));
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
        context_files: vec![RepoPath::parse("Dockerfile")?],
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
    role.push("--file=-".to_owned());
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
    derived.push(format!(
        "--build-context=construct=docker-image://{}",
        construct.tag
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
    let plan = ImagePlan::new(&nested)?;
    let file_args = |args: &[String]| args.iter().filter(|a| a.starts_with("--file=")).count();
    assert_eq!(
        built
            .role
            .args
            .iter()
            .filter(|arg| arg.as_str() == "--file=-")
            .count(),
        1,
        "role args carry one --file=-"
    );
    assert_eq!(file_args(&built.role.args), 1, "role --file= count");
    assert_eq!(
        file_args(&plan.construct_build().args),
        0,
        "construct --file="
    );
    assert_eq!(file_args(&built.derived.args), 0, "derived --file=");
    for args in [
        &plan.construct_build().args,
        &built.role.args,
        &built.derived.args,
    ] {
        assert!(
            args.iter().all(|arg| !arg.contains("Role.Dockerfile")),
            "{args:?}"
        );
    }
    for builder in ["help", "evil"] {
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
        ContextSource::Listed { root, .. } => Err(format!("listed context {root:?}").into()),
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
        request.context_files = ["src/a.rs", "Dockerfile", "src/a.rs", "src/b.rs"]
            .into_iter()
            .map(RepoPath::parse)
            .collect::<Result<_, _>>()?;
        let plan = ImagePlan::new(&request)?;
        let label = format!("{declared_setup:?} {declared_preflight:?}");

        let construct = materialized(plan.construct_build())?;
        assert_eq!(construct.len(), 1, "{label}");
        assert_eq!(construct[0].name, "Dockerfile", "{label}");
        assert_eq!(construct[0].bytes, CONSTRUCT_DOCKERFILE, "{label}");

        let built = plan.final_builds(&construct_id()?);
        let ContextSource::Listed {
            root,
            files: listed,
        } = &built.role.context
        else {
            return Err(format!("role context {:?}", built.role.context).into());
        };
        assert_eq!(root, &request.context, "role context root {label}");
        let listed_set: BTreeSet<&RepoPath> = listed.iter().collect();
        assert_eq!(listed_set.len(), listed.len(), "duplicate file {label}");
        let wanted_set: BTreeSet<&RepoPath> = request.context_files.iter().collect();
        assert_eq!(listed_set, wanted_set, "role context files {label}");

        let files = materialized(&built.derived)?;
        let names: BTreeSet<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names.len(), files.len(), "duplicate name {label}");
        let setup_name = hook_name(HookKind::Setup);
        let preflight_name = hook_name(HookKind::Preflight);
        let expected: BTreeSet<&str> = [
            "Dockerfile",
            "jackin-start",
            "scan",
            &setup_name,
            &preflight_name,
        ]
        .into();
        assert_eq!(names, expected, "{label}");
        let by_name: BTreeMap<&str, &[u8]> = files
            .iter()
            .map(|f| (f.name.as_str(), f.bytes.as_slice()))
            .collect();
        assert_eq!(by_name["Dockerfile"], DERIVED_DOCKERFILE, "{label}");
        assert_eq!(by_name["jackin-start"], JACKIN_START, "{label}");
        assert_eq!(by_name["scan"], SCAN, "{label}");
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
    assert_eq!(declared_args(&lines), wanted, "declared ARG names");
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
        assert!(words.contains(want.as_str()), "install path {want}");
    }
    let extraction = lines
        .iter()
        .flat_map(|line| line.split(" && "))
        .map(str::trim)
        .find(|command| command.split_whitespace().next() == Some("tar"))
        .ok_or("no tar command")?;
    let mut extracted: Vec<&str> = extraction
        .split_whitespace()
        .filter(|word| word.starts_with("bin/"))
        .collect();
    extracted.sort_unstable();
    let mut members = spec.install.archive_members.to_vec();
    members.sort_unstable();
    assert_eq!(extracted, members, "tar members {extraction}");
    assert!(
        text.contains(spec.install.system_config.as_str()),
        "system config path"
    );
    let printf = format!(
        "printf '{}'",
        spec.install.system_config_content.replace('\n', "\\n")
    );
    assert!(text.contains(&printf), "system config content {printf}");
    let version_output = spec
        .install
        .version_output
        .replace("{version}", "${CODEX_VERSION}");
    assert!(
        text.contains(&version_output),
        "version output {version_output}"
    );
    assert!(!text.contains(spec.install.version), "literal version");
    for target in spec.install.targets {
        assert!(!text.contains(target.sha256), "literal checksum");
    }
    Ok(())
}

#[test]
fn derived_dockerfile_installs_jackin_start_and_hooks_at_the_layout_paths() -> TestResult {
    let all = stages(&instructions(derived_text()?));
    let assembled = stage_named(&all, "assembled")?;
    let last = all.last().ok_or("no stage")?;
    let last_user = last
        .lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("USER "))
        .ok_or("no USER")?;
    assert_eq!(last_user, "sandbox", "last_user");
    let copies: Vec<&str> = assembled
        .lines
        .iter()
        .filter_map(|l| l.strip_prefix("COPY "))
        .collect();
    let start = "--from=trusted --chown=root:root --chmod=0755 /usr/local/bin/jackin-start /usr/local/bin/jackin-start";
    assert_eq!(
        copies.iter().filter(|c| **c == start).count(),
        1,
        "jackin-start copy in {copies:?}"
    );
    let hooks_dir = sandbox_layout::HOOKS_DIR.as_str();
    let hooks = format!(
        "--from=trusted --chown=root:root --chmod=0755 {} {} {hooks_dir}/",
        sandbox_layout::hook_path(HookKind::Setup).as_str(),
        sandbox_layout::hook_path(HookKind::Preflight).as_str(),
    );
    assert_eq!(
        copies.iter().filter(|c| **c == hooks).count(),
        1,
        "hooks copy in {copies:?}"
    );
    let trusted = stage_named(&all, "trusted")?;
    let trusted_copies: Vec<&str> = trusted
        .lines
        .iter()
        .filter_map(|l| l.strip_prefix("COPY "))
        .collect();
    let start_source = "--chown=root:root --chmod=0755 jackin-start /usr/local/bin/jackin-start";
    assert!(
        trusted_copies.contains(&start_source),
        "trusted jackin-start copy in {trusted_copies:?}"
    );
    let hook_sources = format!(
        "--chown=root:root --chmod=0755 {} {} {hooks_dir}/",
        hook_name(HookKind::Setup),
        hook_name(HookKind::Preflight),
    );
    assert!(
        trusted_copies.contains(&hook_sources.as_str()),
        "trusted hooks copy in {trusted_copies:?}"
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
fn role_build_is_planned_with_the_dockerfile_on_stdin_and_the_listed_context() -> TestResult {
    let dockerfile = b"FROM construct AS role\n# marker 9f3a\n";
    let mut request = base()?;
    request.dockerfile = source("docker/Role.Dockerfile", dockerfile)?;
    request.context_files = ["src/a.rs", "docker/Role.Dockerfile", "src/a.rs", "src/b.rs"]
        .into_iter()
        .map(RepoPath::parse)
        .collect::<Result<_, _>>()?;
    let plan = ImagePlan::new(&request)?;
    let built = plan.final_builds(&construct_id()?);
    assert_eq!(
        built.role.stdin.as_deref(),
        Some(dockerfile.as_slice()),
        "role stdin"
    );
    assert_eq!(plan.construct_build().stdin, None, "construct stdin");
    assert_eq!(built.derived.stdin, None, "derived stdin");
    let ContextSource::Listed { root, files } = &built.role.context else {
        return Err(format!("role context {:?}", built.role.context).into());
    };
    assert_eq!(root, &request.context, "role context root");
    let listed: BTreeSet<&RepoPath> = files.iter().collect();
    let wanted: BTreeSet<&RepoPath> = request.context_files.iter().collect();
    assert_eq!(listed.len(), files.len(), "duplicate file in {files:?}");
    assert_eq!(listed, wanted, "role context files");
    assert_eq!(
        built
            .role
            .args
            .iter()
            .filter(|arg| *arg == "--file=-")
            .count(),
        1,
        "--file=- count"
    );
    for args in [
        &plan.construct_build().args,
        &built.role.args,
        &built.derived.args,
    ] {
        assert!(
            args.iter()
                .all(|arg| !arg.contains("/work/role") && !arg.contains("Role.Dockerfile")),
            "{args:?}"
        );
    }
    Ok(())
}

fn text_of(bytes: &'static [u8]) -> Result<&'static str, Box<dyn Error>> {
    Ok(std::str::from_utf8(bytes)?)
}

struct Stage {
    name: String,
    from: String,
    lines: Vec<String>,
}

fn stages(lines: &[String]) -> Vec<Stage> {
    let mut out: Vec<Stage> = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("FROM ") {
            let words: Vec<&str> = rest.split_whitespace().collect();
            let name = match words.as_slice() {
                [_, "AS", name] => (*name).to_owned(),
                _ => String::new(),
            };
            out.push(Stage {
                name,
                from: words.first().map_or_else(String::new, |w| (*w).to_owned()),
                lines: Vec::new(),
            });
        } else if let Some(stage) = out.last_mut() {
            stage.lines.push(line.clone());
        }
    }
    out
}

fn stage_named<'a>(all: &'a [Stage], name: &str) -> Result<&'a Stage, Box<dyn Error>> {
    all.iter()
        .find(|stage| stage.name == name)
        .ok_or_else(|| format!("no stage {name}").into())
}

fn words_after<'a>(words: &[&'a str], flag: &str) -> Vec<&'a str> {
    words
        .windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1])
        .collect()
}

#[test]
fn derived_dockerfile_runs_the_scan_in_a_construct_stage_over_the_assembled_stage() -> TestResult {
    let spec = Agent::Codex.spec();
    let bin_dir = parent(spec.binary.as_str());
    let all = stages(&instructions(derived_text()?));
    let scan = stage_named(&all, "scan")?;
    assert_eq!(scan.from, "construct", "scan stage base");
    let user = scan.lines.iter().position(|l| l == "USER root");
    let run = scan.lines.iter().position(|l| l.starts_with("RUN "));
    let (Some(user), Some(run)) = (user, run) else {
        return Err(format!("scan stage lines {:?}", scan.lines).into());
    };
    assert!(user < run, "USER root before the scan RUN");
    let body = scan.lines[run].replace("${CODEX_BIN_DIR}", bin_dir);
    let words: Vec<&str> = body.split_whitespace().collect();
    assert!(words.contains(&"--mount=type=bind,from=assembled,source=/,target=/image,ro"));
    assert!(words.contains(&"--mount=type=bind,from=trusted,source=/,target=/trusted,ro"));
    assert_eq!(words_after(&words, "--owner"), ["0:0"], "scan owner");
    let command = words
        .iter()
        .position(|word| *word == "/bin/sh")
        .ok_or("the scan RUN does not start /bin/sh")?;
    assert_eq!(
        words.get(command..command + 2),
        Some(&["/bin/sh", "/jackin-scan"][..]),
        "the scan RUN must run /bin/sh /jackin-scan: {body}"
    );
    assert!(
        words[..command]
            .iter()
            .all(|word| word.starts_with("--mount=") || *word == "RUN"),
        "only mounts may precede the scan command: {body}"
    );
    assert_eq!(
        words.get(words.len() - 2..),
        Some(&[">", "/verdict"][..]),
        "the scan RUN must end with > /verdict, so its exit status gates the stage: {body}"
    );
    assert!(
        scan.lines.contains(&"COPY scan /jackin-scan".to_owned()),
        "the scan script must be copied into the scan stage: {:?}",
        scan.lines
    );
    let tail: Vec<&str> = words
        .iter()
        .skip_while(|word| **word != "/image")
        .skip(1)
        .take_while(|word| **word != ">")
        .copied()
        .collect();
    let mut protected = vec![
        "/usr/local/bin/jackin-start".to_owned(),
        sandbox_layout::hook_path(HookKind::Setup)
            .as_str()
            .to_owned(),
        sandbox_layout::hook_path(HookKind::Preflight)
            .as_str()
            .to_owned(),
        spec.binary.as_str().to_owned(),
        spec.code_mode_host.as_str().to_owned(),
    ];
    protected.sort();
    let mut passed: Vec<String> = tail.iter().map(|word| (*word).to_owned()).collect();
    passed.sort();
    assert_eq!(passed, protected, "protected paths passed to the scan");
    let mut same_as = words_after(&words, "--same-as");
    same_as.sort_unstable();
    let mut wanted: Vec<String> = protected
        .iter()
        .map(|path| format!("{path}=/trusted{path}"))
        .collect();
    wanted.sort();
    assert_eq!(
        same_as, wanted,
        "protected files compared with the trusted copy"
    );

    let assembled = stage_named(&all, "assembled")?;
    assert_eq!(assembled.from, "role", "assembled stage base");
    assert_eq!(all.iter().filter(|stage| stage.from == "role").count(), 1);
    for line in &assembled.lines {
        for tool in ["find", "getcap", "command -v"] {
            assert!(
                !line.contains(tool),
                "{tool} runs in the role stage: {line}"
            );
        }
    }
    assert_eq!(stage_named(&all, "codex")?.from, "construct", "codex stage");
    let config = spec.install.system_config.as_str();
    assert!(
        assembled.lines.contains(&format!(
            "COPY --from=codex --chown=root:root --chmod=u=rwX,go=rX {config} {config}"
        )),
        "the config copy must leave the created directory searchable: {:?}",
        assembled.lines
    );
    let last = all.last().ok_or("no stage")?;
    assert_eq!(last.from, "assembled", "final stage base");
    assert!(
        last.lines.contains(
            &"COPY --from=scan --chown=root:root --chmod=0644 /verdict /usr/local/lib/jackin/image-scan"
                .to_owned()
        ),
        "the final stage must COPY --from=scan the verdict, or BuildKit never builds the scan stage; final stage lines {:?}",
        last.lines
    );
    assert_eq!(
        last.lines.last().map(String::as_str),
        Some("WORKDIR /sandbox")
    );
    assert!(
        last.lines.contains(&"USER sandbox".to_owned()),
        "final USER"
    );
    Ok(())
}

#[test]
fn construct_dockerfile_removes_setuid_and_rejects_file_capabilities() -> TestResult {
    let lines = instructions(text_of(CONSTRUCT_DOCKERFILE)?);
    let position = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .ok_or_else(|| format!("no instruction contains {needle:?}"))
    };
    let install = &lines[position("apt-get install")?];
    let packages: BTreeSet<&str> = install.split_whitespace().collect();
    assert!(packages.contains("libcap2-bin"), "libcap2-bin in {install}");
    let users = position("useradd")?;
    let strip = &lines[position("-exec chmod a-s")?];
    assert!(
        strip.contains("find / -xdev -type f -perm /6000 -print -exec chmod a-s {} +"),
        "setuid strip in {strip}"
    );
    let capability = position("getcap -r /")?;
    let user = lines
        .iter()
        .rposition(|line| line == "USER sandbox")
        .ok_or("no USER sandbox")?;
    assert!(users < capability, "getcap after the user creation");
    assert!(capability < user, "getcap before USER sandbox");
    let check = lines[capability]
        .split(" && ")
        .map(str::trim)
        .find(|command| command.contains("getcap -r /"))
        .ok_or("no getcap command")?;
    assert!(check.starts_with("test -z "), "getcap check {check}");
    let scan = lines[capability]
        .strip_prefix("RUN for tool in ")
        .and_then(|rest| rest.split_once(';'))
        .map(|(tools, _)| tools.split_whitespace().collect::<BTreeSet<_>>())
        .ok_or("the getcap check is not in the forbidden-tool RUN")?;
    let wanted: BTreeSet<&str> = [
        "sudo",
        "su-exec",
        "gosu",
        "doas",
        "iptables",
        "ip6tables",
        "nft",
        "ufw",
        "docker",
        "podman",
        "nerdctl",
        "containerd",
        "runc",
    ]
    .into();
    assert_eq!(scan, wanted, "forbidden tools");
    Ok(())
}

#[test]
fn both_dockerfiles_pin_the_buildkit_frontend_by_the_same_digest() -> TestResult {
    let pattern =
        regex::Regex::new(r"^# syntax=docker/dockerfile:1(\.\d+){1,2}@sha256:[0-9a-f]{64}$")?;
    let first_lines: Vec<&str> = [CONSTRUCT_DOCKERFILE, DERIVED_DOCKERFILE]
        .into_iter()
        .map(|bytes| Ok(text_of(bytes)?.lines().next().unwrap_or("")))
        .collect::<Result<_, Box<dyn Error>>>()?;
    for (name, line) in ["construct", "derived"].iter().zip(&first_lines) {
        assert!(pattern.is_match(line), "{name} syntax line {line:?}");
    }
    assert_eq!(first_lines[0], first_lines[1], "the two syntax lines");
    Ok(())
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Result<Scratch, Box<dyn Error>> {
        let path =
            std::env::temp_dir().join(format!("jackin-image-{label}-{}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Scratch(path))
    }

    fn fresh(&self, name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.0.join(name);
        fs::create_dir(&path)?;
        Ok(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        match fs::remove_dir_all(&self.0) {
            Ok(()) | Err(_) => {}
        }
    }
}

fn run_sh(dir: &Path, body: &str, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
    let child = Command::new("sh")
        .arg("-c")
        .arg(body)
        .arg("sh")
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(child.wait_with_output()?)
}

fn segment<'a>(script: &'a str, first: &str, last: &str) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let lines: Vec<&str> = script.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.trim() == first)
        .ok_or_else(|| format!("no {first:?} line"))?;
    let end = lines[begin..]
        .iter()
        .position(|line| line.trim() == last)
        .ok_or_else(|| format!("no {last:?} line after {first:?}"))?;
    Ok(lines[begin..=begin + end].to_vec())
}

fn publish_all(scratch: &Scratch, name: &str, states: &[&str]) -> Result<PathBuf, Box<dyn Error>> {
    let function = segment(script()?, "publish() {", "}")?.join("\n");
    let calls: Vec<String> = (0..states.len())
        .map(|index| format!("publish \"${}\"", index + 2))
        .collect();
    let body = format!("set -u; run=$1; {function}\n{}", calls.join("; "));
    let dir = scratch.fresh(name)?;
    let dir_text = dir.to_str().ok_or("temp dir is not UTF-8")?;
    let mut args = vec![dir_text];
    args.extend_from_slice(states);
    let output = run_sh(&dir, &body, &args)?;
    assert!(
        output.status.success(),
        "publish {states:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(dir)
}

#[test]
fn embedded_jackin_start_publishes_what_the_supervisor_parser_accepts() -> TestResult {
    let script = script()?;
    let state = sandbox_layout::RUN_STATE.as_str();
    let start = sandbox_layout::RUN_START.as_str();
    let run = assignment(script, "run")?;
    assert_eq!(run, parent(state), "run directory against RUN_STATE");
    assert_eq!(run, parent(start), "run directory against RUN_START");
    assert_eq!(last_segment(state), "state", "state file name");
    assert_eq!(last_segment(start), "start", "start file name");
    assert!(
        script.contains("\"$run/state\""),
        "state path in the script"
    );
    assert!(
        script.contains("\"$run/start\""),
        "start path in the script"
    );
    let project = assignment(script, "project")?;
    assert_eq!(project, sandbox_layout::PROJECT.as_str(), "project path");
    assert_eq!(
        project,
        Agent::Codex.spec().workdir.as_str(),
        "project against the agent workdir"
    );

    let scratch = Scratch::new("publish")?;
    for (index, text) in ["waiting", "running 3", "exited 3 7"]
        .into_iter()
        .enumerate()
    {
        let dir = publish_all(&scratch, &format!("single-{index}"), &[text])?;
        let contents = fs::read_to_string(dir.join("state"))?;
        assert_eq!(contents, format!("{text}\n"), "state file after {text:?}");
        assert!(
            !dir.join("state.tmp").exists(),
            "temp file left after {text:?}"
        );
        let parsed = SupervisorState::parse(&contents)?;
        assert_eq!(parsed.render(), text, "render after {text:?}");
    }
    let dir = publish_all(&scratch, "twice", &["waiting", "running 3"])?;
    assert_eq!(
        fs::read_to_string(dir.join("state"))?,
        "running 3\n",
        "the second publish wins"
    );
    assert!(
        !dir.join("state.tmp").exists(),
        "temp file after two publishes"
    );
    Ok(())
}

fn read_attempt(
    scratch: &Scratch,
    name: &str,
    content: Option<&[u8]>,
) -> Result<(String, bool), Box<dyn Error>> {
    let segment = segment(script()?, "attempt=", "esac")?.join("\n");
    let body = format!("set -u; run=$1\n{segment}\nprintf %s \"$attempt\"");
    let dir = scratch.fresh(name)?;
    if let Some(bytes) = content {
        fs::write(dir.join("start"), bytes)?;
    }
    let dir_text = dir.to_str().ok_or("temp dir is not UTF-8")?;
    let output = run_sh(&dir, &body, &[dir_text])?;
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created: Vec<String> = fs::read_dir(&dir)?
        .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    let injected = created.iter().any(|file| file == "pwned");
    Ok((String::from_utf8(output.stdout)?, injected))
}

#[test]
fn embedded_jackin_start_reads_start_requests_exactly_as_attempt_id_parses() -> TestResult {
    let nines18 = "9".repeat(18);
    let nines19 = "9".repeat(19);
    let corpus: Vec<(String, Option<Vec<u8>>, bool)> = [
        ("1\n", true),
        ("9\n", true),
        ("10\n", true),
        (&format!("{nines18}\n"), true),
        (&nines18, true),
        (&format!("{nines19}\n"), false),
        (&nines19, false),
        ("0", false),
        ("00", false),
        ("01", false),
        ("-1", false),
        ("+1", false),
        ("1 ", false),
        (" 1", false),
        ("a1", false),
        ("1;touch pwned", false),
        ("$(touch pwned)", false),
        ("", false),
        ("\n", false),
        ("12\n34\n", true),
        ("\u{661}\u{662}\u{663}", false),
        ("1\r\n", false),
        ("1\\\n2", false),
    ]
    .into_iter()
    .map(|(content, accepted)| {
        (
            content.to_owned(),
            Some(content.as_bytes().to_vec()),
            accepted,
        )
    })
    .chain([("<no start file>".to_owned(), None, false)])
    .collect();
    let scratch = Scratch::new("start")?;
    for (index, (label, content, accepted)) in corpus.iter().enumerate() {
        let (attempt, injected) =
            read_attempt(&scratch, &format!("row-{index}"), content.as_deref())?;
        assert!(!injected, "{label:?} ran a command");
        let first_line = content
            .as_deref()
            .map(String::from_utf8_lossy)
            .map_or_else(String::new, |text| {
                text.split('\n').next().unwrap_or("").to_owned()
            });
        let expected = if *accepted { first_line.as_str() } else { "" };
        assert_eq!(attempt, expected, "shell result for {label:?}");
        if content.is_some() {
            assert_eq!(
                *accepted,
                AttemptId::parse(&first_line).is_ok(),
                "AttemptId verdict for {label:?}"
            );
        }
    }
    Ok(())
}

fn planned_dockerfile() -> Result<String, Box<dyn Error>> {
    let built = ImagePlan::new(&base()?)?.final_builds(&construct_id()?);
    let file = materialized(&built.derived)?
        .iter()
        .find(|file| file.name == "Dockerfile")
        .ok_or("the derived context holds no Dockerfile")?;
    Ok(String::from_utf8(file.bytes.clone())?)
}

#[test]
fn no_role_program_runs_after_the_trusted_files_land() -> TestResult {
    let all = stages(&instructions(&planned_dockerfile()?));
    let mut derived: BTreeSet<String> = BTreeSet::from(["role".to_owned()]);
    let mut checked = Vec::new();
    for stage in &all {
        if !derived.contains(&stage.from) {
            continue;
        }
        derived.insert(stage.name.clone());
        checked.push(stage.name.as_str());
        for line in &stage.lines {
            let kind = line.split_whitespace().next().unwrap_or("");
            assert!(
                ["ARG", "USER", "WORKDIR", "COPY"].contains(&kind),
                "stage {:?} derived from the role image runs {line:?}",
                stage.name
            );
        }
    }
    assert_eq!(
        checked,
        ["assembled", ""],
        "stages derived from the role image"
    );
    Ok(())
}

fn copy_destinations(stage: &Stage, bin_dir: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut out = Vec::new();
    for line in &stage.lines {
        let Some(rest) = line.strip_prefix("COPY ") else {
            continue;
        };
        let rest = rest.replace("${CODEX_BIN_DIR}", bin_dir);
        let words: Vec<&str> = rest.split_whitespace().collect();
        let (flags, paths): (Vec<&str>, Vec<&str>) = words
            .iter()
            .copied()
            .partition(|word| word.starts_with("--"));
        assert!(
            flags
                .iter()
                .all(|flag| !flag.starts_with("--from=") || *flag == "--from=codex"),
            "the trusted stage copies from a stage other than codex: {line}"
        );
        let (dest, sources) = paths.split_last().ok_or("a COPY without paths")?;
        for source in sources {
            if dest.ends_with('/') {
                out.push(format!("{dest}{}", last_segment(source)));
            } else {
                out.push((*dest).to_owned());
            }
        }
    }
    Ok(out)
}

#[test]
fn the_scan_compares_the_five_protected_files_with_copies_the_role_cannot_write() -> TestResult {
    let spec = Agent::Codex.spec();
    let bin_dir = parent(spec.binary.as_str());
    let all = stages(&instructions(&planned_dockerfile()?));
    let run = stage_named(&all, "scan")?
        .lines
        .iter()
        .find(|line| line.starts_with("RUN "))
        .ok_or("no scan RUN")?
        .replace("${CODEX_BIN_DIR}", bin_dir);
    let words: Vec<&str> = run.split_whitespace().collect();
    let trusted = stage_named(&all, "trusted")?;
    assert_eq!(trusted.from, "construct", "trusted stage base");
    assert!(
        words.contains(&"--mount=type=bind,from=trusted,source=/,target=/trusted,ro"),
        "the trusted mount: {run}"
    );
    let destinations = copy_destinations(trusted, bin_dir)?;
    let mut protected = vec![
        "/usr/local/bin/jackin-start".to_owned(),
        sandbox_layout::hook_path(HookKind::Setup)
            .as_str()
            .to_owned(),
        sandbox_layout::hook_path(HookKind::Preflight)
            .as_str()
            .to_owned(),
        spec.binary.as_str().to_owned(),
        spec.code_mode_host.as_str().to_owned(),
    ];
    protected.sort();
    let mut compared = Vec::new();
    for pair in words_after(&words, "--same-as") {
        let (path, expected) = pair.split_once('=').ok_or("a pair without =")?;
        let inside = expected
            .strip_prefix("/trusted")
            .ok_or_else(|| format!("{pair}: the expected side is not under the trusted mount"))?;
        assert_eq!(
            inside, path,
            "{pair}: the trusted copy sits at another path"
        );
        assert!(
            destinations.iter().any(|dest| dest == inside),
            "{pair}: no COPY in the trusted stage writes {inside}: {destinations:?}"
        );
        compared.push(path.to_owned());
    }
    compared.sort();
    assert_eq!(compared, protected, "files compared with a trusted copy");
    let layout = stage_named(&all, "layout")?;
    assert_eq!(layout.from, "construct", "layout stage base");
    let install = layout
        .lines
        .iter()
        .find_map(|line| line.strip_prefix("RUN install -d -m 0755 -o root -g root "))
        .ok_or("the layout stage does not create its directories 0755 root")?;
    let made: Vec<&str> = install.split_whitespace().collect();
    for dir in [
        "usr",
        "usr/local",
        "usr/local/bin",
        "usr/local/lib",
        "usr/local/lib/jackin",
        sandbox_layout::HOOKS_DIR.as_str().trim_start_matches('/'),
    ] {
        assert!(
            made.contains(&format!("/layout/{dir}").as_str()),
            "{dir}: {made:?}"
        );
    }
    let assembled = stage_named(&all, "assembled")?;
    assert!(
        assembled
            .lines
            .contains(&"COPY --from=layout --chown=root:root --chmod=0755 /layout/ /".to_owned()),
        "the layout copy: {:?}",
        assembled.lines
    );
    Ok(())
}
