//! Construct base-image build and publish tasks.
//!
//! Config resolution (registry, tags, git SHA, pinned tool versions) and
//! orchestration of `docker buildx bake` / `imagetools`. The declarative build
//! graph lives in `docker-bake.hcl`; these functions resolve its inputs and
//! invoke it.

use std::fs;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Subcommand, ValueEnum};

const CANONICAL_REGISTRY_IMAGE: &str = "projectjackin/construct";
const VERSION_FILE: &str = "docker/construct/VERSION";
const VERSIONS_ENV_FILE: &str = "docker/construct/versions.env";
const BAKE_FILE: &str = "docker-bake.hcl";

#[derive(Subcommand)]
pub(crate) enum ConstructCommand {
    /// Create and bootstrap the named Buildx builder.
    InitBuildx,
    /// Inspect the configured Buildx builder and list available builders.
    DoctorBuildx,
    /// Recreate the configured Buildx builder from scratch.
    ResetBuildx,
    /// Build the construct image for the host platform and load it locally.
    BuildLocal,
    /// Build for a specific platform and load it locally.
    BuildPlatform { platform: Platform },
    /// Push a single-platform image by digest (CI-only for the canonical registry).
    PushPlatform { platform: Platform },
    /// Fail when `docker/construct/VERSION` already exists in the registry.
    AssertVersionUnpublished,
    /// Combine per-platform digest pushes into a multi-platform manifest.
    PublishManifest,
    /// Print the resolved Bake configuration (dry-run inspection).
    Inspect,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Platform {
    Amd64,
    Arm64,
}

impl Platform {
    fn name(self) -> &'static str {
        match self {
            Platform::Amd64 => "amd64",
            Platform::Arm64 => "arm64",
        }
    }

    fn docker(self) -> &'static str {
        match self {
            Platform::Amd64 => "linux/amd64",
            Platform::Arm64 => "linux/arm64",
        }
    }
}

pub(crate) fn run(command: ConstructCommand) -> Result<()> {
    let cfg = Config::resolve()?;
    match command {
        ConstructCommand::InitBuildx => init_buildx(&cfg),
        ConstructCommand::DoctorBuildx => doctor_buildx(&cfg),
        ConstructCommand::ResetBuildx => reset_buildx(&cfg),
        ConstructCommand::BuildLocal => build_local(&cfg),
        ConstructCommand::BuildPlatform { platform } => build_platform(&cfg, platform),
        ConstructCommand::PushPlatform { platform } => push_platform(&cfg, platform),
        ConstructCommand::AssertVersionUnpublished => assert_version_unpublished(&cfg),
        ConstructCommand::PublishManifest => publish_manifest(&cfg),
        ConstructCommand::Inspect => inspect(&cfg),
    }
}

/// Resolved build variables. Env-var overrides take priority over computed
/// defaults.
struct Config {
    registry_image: String,
    local_registry_image: String,
    stable_tag: String,
    git_sha: String,
    sha_tag: String,
    version_tag: String,
    local_platform: String,
    tirith_version: String,
    shellfirm_version: String,
    mise_version: String,
    buildx_builder: String,
    digest_dir: String,
}

impl Config {
    fn resolve() -> Result<Self> {
        let registry_image = env_or("REGISTRY_IMAGE", CANONICAL_REGISTRY_IMAGE);
        let local_registry_image = env_or("LOCAL_REGISTRY_IMAGE", "jackin-local/construct");
        let stable_tag = env_or("STABLE_TAG", "trixie");
        let git_sha =
            env_present("GIT_SHA").unwrap_or_else(|| git_sha().unwrap_or_else(|| "dev".to_owned()));
        let sha_tag = format!("{stable_tag}-{git_sha}");
        let version_tag = env_present("CONSTRUCT_VERSION_TAG")
            .unwrap_or_else(|| read_version_file().unwrap_or_else(|| "unknown".to_owned()));
        let local_platform = match env_present("LOCAL_PLATFORM") {
            Some(platform) => platform,
            None => host_platform()?,
        };
        let tirith_version = env_or("TIRITH_VERSION", versions_env_value("TIRITH_VERSION"));
        let shellfirm_version =
            env_or("SHELLFIRM_VERSION", versions_env_value("SHELLFIRM_VERSION"));
        let mise_version = env_or("MISE_VERSION", versions_env_value("MISE_VERSION"));
        let buildx_builder = env_or("BUILDX_BUILDER", "jackin-construct");
        let digest_dir = env_or("DIGEST_DIR", "/tmp/jackin-construct-digests");
        Ok(Self {
            registry_image,
            local_registry_image,
            stable_tag,
            git_sha,
            sha_tag,
            version_tag,
            local_platform,
            tirith_version,
            shellfirm_version,
            mise_version,
            buildx_builder,
            digest_dir,
        })
    }

    /// Export exactly the variables `docker-bake.hcl` declares into the bake
    /// child process.
    fn apply_bake_env(&self, cmd: &mut Command) {
        cmd.env("REGISTRY_IMAGE", &self.registry_image)
            .env("LOCAL_REGISTRY_IMAGE", &self.local_registry_image)
            .env("STABLE_TAG", &self.stable_tag)
            .env("GIT_SHA", &self.git_sha)
            .env("SHA_TAG", &self.sha_tag)
            .env("LOCAL_PLATFORM", &self.local_platform)
            .env("TIRITH_VERSION", &self.tirith_version)
            .env("SHELLFIRM_VERSION", &self.shellfirm_version)
            .env("MISE_VERSION", &self.mise_version);
    }

    fn ref_for(&self, tag: &str) -> String {
        format!("{}:{}", self.registry_image, tag)
    }

    /// Refuse to publish to the canonical registry from a local (non-CI) shell.
    fn guard_local_publish(&self) -> Result<()> {
        let no_ci = std::env::var("CI").map_or(true, |v| v.is_empty());
        if no_ci && self.registry_image == CANONICAL_REGISTRY_IMAGE {
            bail!(
                "Set REGISTRY_IMAGE to your own namespace before publishing to {CANONICAL_REGISTRY_IMAGE} locally."
            );
        }
        Ok(())
    }
}

fn init_buildx(cfg: &Config) -> Result<()> {
    if builder_exists(&cfg.buildx_builder) {
        return run_checked(docker([
            "buildx",
            "inspect",
            &cfg.buildx_builder,
            "--bootstrap",
        ]));
    }
    run_checked(docker([
        "buildx",
        "create",
        "--name",
        &cfg.buildx_builder,
        "--driver",
        "docker-container",
        "--use",
    ]))?;
    run_checked(docker([
        "buildx",
        "inspect",
        &cfg.buildx_builder,
        "--bootstrap",
    ]))
}

fn doctor_buildx(cfg: &Config) -> Result<()> {
    run_checked(docker(["buildx", "ls"]))?;
    run_checked(docker([
        "buildx",
        "inspect",
        &cfg.buildx_builder,
        "--bootstrap",
    ]))
}

fn reset_buildx(cfg: &Config) -> Result<()> {
    // A missing builder is fine to "remove"; ignore that failure only.
    let mut remove = docker(["buildx", "rm", "--force", &cfg.buildx_builder]);
    remove.stdout(Stdio::null()).stderr(Stdio::null());
    drop(crate::cmd::run(&mut remove));
    run_checked(docker([
        "buildx",
        "create",
        "--name",
        &cfg.buildx_builder,
        "--driver",
        "docker-container",
        "--use",
    ]))?;
    run_checked(docker([
        "buildx",
        "inspect",
        &cfg.buildx_builder,
        "--bootstrap",
    ]))
}

fn build_local(cfg: &Config) -> Result<()> {
    init_buildx(cfg)?;
    let mut cmd = docker([
        "buildx",
        "bake",
        "--builder",
        &cfg.buildx_builder,
        "--file",
        BAKE_FILE,
        "--load",
        "construct-local",
    ]);
    cfg.apply_bake_env(&mut cmd);
    run_buildkit(cmd)
}

fn build_platform(cfg: &Config, platform: Platform) -> Result<()> {
    init_buildx(cfg)?;
    let mut cmd = docker([
        "buildx",
        "bake",
        "--builder",
        &cfg.buildx_builder,
        "--file",
        BAKE_FILE,
        "--load",
    ]);
    cfg.apply_bake_env(&mut cmd);
    cmd.env("LOCAL_PLATFORM", platform.docker());
    apply_cache_args(&mut cmd, "construct-local");
    cmd.arg("construct-local");
    run_buildkit(cmd)
}

fn push_platform(cfg: &Config, platform: Platform) -> Result<()> {
    cfg.guard_local_publish()?;
    init_buildx(cfg)?;
    fs::create_dir_all(&cfg.digest_dir)
        .with_context(|| format!("creating digest dir {}", cfg.digest_dir))?;
    let metadata_file = format!("{}/metadata-{}.json", cfg.digest_dir, platform.name());
    let digest_file = env_present("DIGEST_FILE")
        .unwrap_or_else(|| format!("{}/{}.digest", cfg.digest_dir, platform.name()));

    let mut cmd = docker([
        "buildx",
        "bake",
        "--builder",
        &cfg.buildx_builder,
        "--file",
        BAKE_FILE,
        "--metadata-file",
        &metadata_file,
    ]);
    cfg.apply_bake_env(&mut cmd);
    cmd.env("PLATFORMS", platform.docker());
    cmd.arg("--set").arg(format!(
        "construct-publish.output=type=image,name={},push-by-digest=true,name-canonical=true,push=true",
        cfg.registry_image
    ));
    apply_cache_args(&mut cmd, "construct-publish");
    cmd.arg("construct-publish");
    run_buildkit(cmd)?;

    let digest = read_digest_from_metadata(&metadata_file)?;
    drop(fs::remove_file(&metadata_file));
    fs::write(&digest_file, format!("{digest}\n"))
        .with_context(|| format!("writing digest to {digest_file}"))?;
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the digest path is its progress output"
    )]
    {
        println!("Wrote {} digest to {digest_file}", platform.name());
    }
    Ok(())
}

/// Apply `CACHE_FROM` and `CACHE_TO` env vars as `--set` overrides on a `docker buildx bake`
/// command. `CACHE_FROM` may contain multiple newline-separated sources; each becomes its own
/// `--set target.cache-from=<source>` flag so `docker buildx bake` appends them to the
/// `cache-from` list rather than replacing it.
fn apply_cache_args(cmd: &mut Command, target: &str) {
    if let Some(cache_from) = env_present("CACHE_FROM") {
        for source in cache_from.lines().map(str::trim).filter(|s| !s.is_empty()) {
            cmd.arg("--set")
                .arg(format!("{target}.cache-from={source}"));
        }
    }
    if let Some(cache_to) = env_present("CACHE_TO") {
        cmd.arg("--set")
            .arg(format!("{target}.cache-to={cache_to}"));
    }
}

fn assert_version_unpublished(cfg: &Config) -> Result<()> {
    if version_published(cfg)? {
        bail!(
            "{} already exists in the registry.\nBump {VERSION_FILE} before publishing a new construct version.",
            cfg.ref_for(&cfg.version_tag)
        );
    }
    Ok(())
}

/// Probe the registry for the immutable per-version tag. An indeterminate
/// probe (auth/network failure) is an error so an outage is never mistaken
/// for a published or unpublished version.
fn version_published(cfg: &Config) -> Result<bool> {
    if cfg.version_tag.is_empty() || cfg.version_tag == "unknown" {
        bail!(
            "VERSION_TAG is '{}' — {VERSION_FILE} is missing or empty.",
            cfg.version_tag
        );
    }
    let reference = cfg.ref_for(&cfg.version_tag);
    let mut inspect = docker(["buildx", "imagetools", "inspect", &reference]);
    let output = crate::cmd::output_raw(&mut inspect)
        .with_context(|| format!("running docker buildx imagetools inspect {reference}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    match classify_inspect(output.success, &stderr) {
        VersionStatus::AlreadyPublished => Ok(true),
        VersionStatus::Unpublished => Ok(false),
        VersionStatus::UnknownError => Err(anyhow!(
            "registry check for {reference} failed unexpectedly:\n{}",
            stderr.trim()
        )),
    }
}

fn publish_manifest(cfg: &Config) -> Result<()> {
    cfg.guard_local_publish()?;

    // Same version-bump invariant the PR rehearsal enforces, with one
    // difference: at publish time an existing tag is a successful no-op, not
    // an error. The per-version tag is immutable, so re-running the publish
    // workflow against an unchanged docker/construct/VERSION (e.g. a manual
    // dispatch from main) has nothing left to publish. Image changes that
    // forget the VERSION bump still fail before merge, because the PR-time
    // rehearsal runs assert-version-unpublished, which keeps treating the
    // collision as an error.
    match manifest_action(version_published(cfg)?) {
        ManifestAction::SkipAlreadyPublished => {
            #[expect(
                clippy::print_stdout,
                reason = "xtask is a CLI; the skip notice is its progress output"
            )]
            {
                println!(
                    "{} already published; skipping manifest publish (bump {VERSION_FILE} to publish a new one)",
                    cfg.ref_for(&cfg.version_tag)
                );
            }
            return Ok(());
        }
        ManifestAction::Publish => {}
    }

    let mut refs = Vec::new();
    for platform in [Platform::Amd64, Platform::Arm64] {
        let digest_file = format!("{}/{}.digest", cfg.digest_dir, platform.name());
        let raw = fs::read_to_string(&digest_file).with_context(|| {
            format!(
                "missing digest file for {} at {digest_file} — run push-platform first or point DIGEST_DIR at the downloaded digests",
                platform.name()
            )
        })?;
        let digest: String = raw.split_whitespace().collect();
        if digest.is_empty() {
            bail!("digest file {digest_file} is empty");
        }
        refs.push(format!("{}@{}", cfg.registry_image, digest));
    }

    let mut create = docker(["buildx", "imagetools", "create"]);
    for tag in [&cfg.stable_tag, &cfg.sha_tag, &cfg.version_tag] {
        create.arg("--tag").arg(cfg.ref_for(tag));
    }
    for image_ref in &refs {
        create.arg(image_ref);
    }
    run_checked(create)?;
    run_checked(docker([
        "buildx",
        "imagetools",
        "inspect",
        &cfg.ref_for(&cfg.version_tag),
    ]))
}

fn inspect(cfg: &Config) -> Result<()> {
    init_buildx(cfg)?;
    let mut cmd = docker([
        "buildx",
        "bake",
        "--builder",
        &cfg.buildx_builder,
        "--file",
        BAKE_FILE,
        "--print",
        "construct-local",
        "construct-publish",
    ]);
    cfg.apply_bake_env(&mut cmd);
    cmd.env("PLATFORMS", "linux/amd64,linux/arm64");
    run_checked(cmd)
}

#[derive(Debug, PartialEq, Eq)]
enum VersionStatus {
    Unpublished,
    AlreadyPublished,
    UnknownError,
}

/// What `publish-manifest` does after probing the registry for the immutable
/// version tag.
#[derive(Debug, PartialEq, Eq)]
enum ManifestAction {
    /// Tag absent: assemble and push the multi-platform manifest.
    Publish,
    /// Tag present: succeed without publishing. The tag is immutable and this
    /// run could only recreate it, so the re-run is an idempotent no-op rather
    /// than a failure. Forgotten VERSION bumps still fail at PR time via
    /// `assert-version-unpublished`, which errors on the same condition.
    SkipAlreadyPublished,
}

fn manifest_action(version_published: bool) -> ManifestAction {
    if version_published {
        ManifestAction::SkipAlreadyPublished
    } else {
        ManifestAction::Publish
    }
}

/// Classify a `docker buildx imagetools inspect` result. A success means the
/// tag exists; a failure is the tag being absent only when stderr names a
/// known "not found" shape — any other failure is an auth/network error that
/// must not be mistaken for an unpublished version.
fn classify_inspect(success: bool, stderr: &str) -> VersionStatus {
    if success {
        return VersionStatus::AlreadyPublished;
    }
    const ABSENT_MARKERS: [&str; 4] = [
        "not found",
        "MANIFEST_UNKNOWN",
        "does not exist",
        "NAME_UNKNOWN",
    ];
    if ABSENT_MARKERS.iter().any(|marker| stderr.contains(marker)) {
        VersionStatus::Unpublished
    } else {
        VersionStatus::UnknownError
    }
}

fn read_digest_from_metadata(path: &str) -> Result<String> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading bake metadata {path}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).with_context(|| format!("parsing bake metadata {path}"))?;
    find_digest(&value).with_context(|| format!("no containerimage.digest in {path}"))
}

fn find_digest(value: &serde_json::Value) -> Option<String> {
    let serde_json::Value::Object(map) = value else {
        return None;
    };
    if let Some(serde_json::Value::String(digest)) = map.get("containerimage.digest")
        && !digest.is_empty()
    {
        return Some(digest.clone());
    }
    map.values().find_map(find_digest)
}

fn docker<const N: usize>(args: [&str; N]) -> Command {
    let mut cmd = crate::cmd::command("docker");
    cmd.args(args);
    cmd
}

fn builder_exists(builder: &str) -> bool {
    let mut cmd = docker(["buildx", "inspect", builder]);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    crate::cmd::run(&mut cmd).is_ok()
}

fn run_checked(mut cmd: Command) -> Result<()> {
    crate::cmd::run(&mut cmd)
}

fn run_buildkit(mut cmd: Command) -> Result<()> {
    cmd.arg("--progress").arg("plain");
    crate::cmd::run_streaming(&mut cmd)
}

/// Env value if the variable is set, else the computed default. A set-but-empty
/// variable returns empty so explicit overrides are honored verbatim.
fn env_or(key: &str, default: impl Into<String>) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

/// Env value only when set and non-empty — for variables whose empty value
/// should fall through to a computed default (or be treated as absent) rather
/// than be honored.
fn env_present(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn git_sha() -> Option<String> {
    let mut cmd = crate::cmd::command("git");
    cmd.args(["rev-parse", "--short=12", "HEAD"]);
    let stdout = crate::cmd::output_string(&mut cmd).ok()?;
    let sha = stdout.trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

fn read_version_file() -> Option<String> {
    let raw = fs::read_to_string(VERSION_FILE).ok()?;
    let trimmed: String = raw.split_whitespace().collect();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn versions_env_value(key: &str) -> String {
    parse_versions_env(key).unwrap_or_default()
}

fn parse_versions_env(key: &str) -> Option<String> {
    let content = fs::read_to_string(VERSIONS_ENV_FILE).ok()?;
    content.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().to_owned())
    })
}

fn host_platform() -> Result<String> {
    let platform = match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" | "arm64" => "linux/arm64",
        other => bail!("unsupported host architecture: {other}"),
    };
    Ok(platform.to_owned())
}

#[cfg(test)]
mod tests;
