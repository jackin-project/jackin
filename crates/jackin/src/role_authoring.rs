//! `jackin role` subcommand handlers: init, validate, migrate, and pack role repos.
//!
//! Not responsible for: manifest schema definitions or migration registry —
//! those live in `src/manifest/`.

use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::cli::role::{RoleCommand, RoleCreateArgs, RolePublishLabelsArgs, RoleRepoPathArgs};
use jackin_core::RoleSelector;
use jackin_manifest::migrations::CURRENT_MANIFEST_VERSION;
use jackin_manifest::repo::validate_role_repo;
use jackin_manifest::repo_contract::{DOCKERFILE_NAME, MANIFEST_FILENAME};

pub fn run(command: RoleCommand) -> anyhow::Result<()> {
    match command {
        RoleCommand::Validate(args) => validate(args),
        RoleCommand::Migrate(args) => migrate(args),
        RoleCommand::Create(args) => create(&args),
        RoleCommand::ConstructVersion(args) => construct_version(args),
        RoleCommand::PublishedImage(args) => published_image(args),
        RoleCommand::PublishedImageRepository(args) => published_image_repository(args),
        RoleCommand::PublishLabels(args) => publish_labels(args),
    }
}

fn validate(args: RoleRepoPathArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    validate_role_repo(&repo_dir)?;
    println!("Role repository is valid: {}", repo_dir.display());
    Ok(())
}

fn construct_version(args: RoleRepoPathArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    let validated = validate_role_repo(&repo_dir)?;
    println!("{}", validated.dockerfile.construct_version);
    Ok(())
}

fn published_image(args: RoleRepoPathArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    let validated = validate_role_repo(&repo_dir)?;
    let image = validated
        .manifest
        .published_image
        .ok_or_else(|| anyhow::anyhow!("no published_image declared in jackin.role.toml"))?;
    println!("{image}");
    Ok(())
}

fn published_image_repository(args: RoleRepoPathArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    let validated = validate_role_repo(&repo_dir)?;
    let image = validated
        .manifest
        .published_image
        .ok_or_else(|| anyhow::anyhow!("no published_image declared in jackin.role.toml"))?;
    println!(
        "{}",
        jackin_manifest::repo_contract::published_image_repository(&image)
    );
    Ok(())
}

fn publish_labels(args: RolePublishLabelsArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    let validated = validate_role_repo(&repo_dir)?;
    for label in jackin_manifest::repo_contract::published_image_labels(
        &validated.dockerfile.construct_version,
        &args.role_git_sha,
    ) {
        println!("{label}");
    }
    Ok(())
}

fn migrate(args: RoleRepoPathArgs) -> anyhow::Result<()> {
    let repo_dir = resolve_repo_path(args.path)?;
    let manifest_path = repo_dir.join(MANIFEST_FILENAME);
    match jackin_manifest::migrations::migrate_manifest_file(&manifest_path)? {
        Some((old, new)) => println!("Migrated manifest {old} -> {new}"),
        None => println!("Manifest already at current version"),
    }
    validate_role_repo(&repo_dir)?;
    println!("Role repository is valid: {}", repo_dir.display());
    Ok(())
}

fn create(args: &RoleCreateArgs) -> anyhow::Result<()> {
    let selector = RoleSelector::parse(&args.role)
        .map_err(|err| anyhow::anyhow!("invalid role name {:?}: {err}", args.role))?;
    let projects_dir = resolve_projects_dir(args.projects_dir.as_deref())?;
    let repo_dir = scaffold_path(&projects_dir, &selector);

    if let Some(parent) = repo_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::create_dir(&repo_dir).with_context(|| format!("creating {}", repo_dir.display()))?;
    write_scaffold(&repo_dir, &selector)?;
    validate_role_repo(&repo_dir)?;

    println!("Created role repository: {}", repo_dir.display());
    println!(
        "Validate it with: jackin role validate {}",
        repo_dir.display()
    );
    Ok(())
}

fn resolve_repo_path(path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let path = match path {
        Some(path) => path,
        None => std::env::current_dir().context("resolving current directory")?,
    };
    let repo_dir = resolve_path(&path);
    match std::fs::metadata(&repo_dir) {
        Ok(metadata) if metadata.is_dir() => Ok(repo_dir),
        Ok(_) => anyhow::bail!("{} is not a directory", repo_dir.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("{} does not exist", repo_dir.display())
        }
        Err(err) => Err(err).with_context(|| format!("inspecting {}", repo_dir.display())),
    }
}

fn resolve_projects_dir(projects_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(path) = projects_dir {
        return Ok(resolve_path(path));
    }
    if let Some(path) = std::env::var_os("JACKIN_PROJECTS_DIR") {
        return Ok(resolve_path(Path::new(&path)));
    }
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?;
    Ok(base.home_dir().join("Projects"))
}

fn resolve_path(path: &Path) -> PathBuf {
    PathBuf::from(crate::workspace::resolve_path(&path.to_string_lossy()))
}

fn scaffold_path(projects_dir: &Path, selector: &RoleSelector) -> PathBuf {
    let repo_name = format!("jackin-{}", selector.name);
    selector.namespace.as_ref().map_or_else(
        || projects_dir.join(&repo_name),
        |ns| projects_dir.join(ns).join(&repo_name),
    )
}

fn write_scaffold(repo_dir: &Path, selector: &RoleSelector) -> anyhow::Result<()> {
    write_new_file(
        &repo_dir.join(MANIFEST_FILENAME),
        &manifest_contents(&selector.name),
    )?;
    write_new_file(&repo_dir.join(DOCKERFILE_NAME), dockerfile_contents())?;
    write_new_file(&repo_dir.join("README.md"), &readme_contents(selector))?;
    write_new_file(&repo_dir.join(".gitignore"), gitignore_contents())?;
    write_new_file(&repo_dir.join(".dockerignore"), dockerignore_contents())?;
    write_new_file(&repo_dir.join("renovate.json"), renovate_contents())?;
    let workflow_dir = repo_dir.join(".github/workflows");
    std::fs::create_dir_all(&workflow_dir)
        .with_context(|| format!("creating {}", workflow_dir.display()))?;
    write_new_file(&workflow_dir.join("validate.yml"), workflow_contents())?;
    Ok(())
}

fn write_new_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    use std::io::Write;

    #[expect(
        clippy::disallowed_methods,
        reason = "role-authoring CLI file creation does not run on render/runtime threads"
    )]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn manifest_contents(role_name: &str) -> String {
    format!(
        r#"version = "{CURRENT_MANIFEST_VERSION}"
dockerfile = "Dockerfile"

[claude]
plugins = []

[identity]
name = "{}"
"#,
        display_name(role_name)
    )
}

const fn dockerfile_contents() -> &'static str {
    jackin_manifest::repo_contract::BASE_DOCKERFILE_FROM
}

fn readme_contents(selector: &RoleSelector) -> String {
    let role = selector.to_string();
    format!(
        r"# {}

jackin❯ role repository for `{role}`.

## Validate

```sh
jackin role validate .
```

## Try it locally

```sh
jackin load {role} --rebuild --debug
```
",
        display_name(&selector.name)
    )
}

const fn gitignore_contents() -> &'static str {
    ".DS_Store\n.env\n"
}

const fn dockerignore_contents() -> &'static str {
    ".git\n.github\nREADME.md\n"
}

const fn renovate_contents() -> &'static str {
    r#"{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": ["config:best-practices"],
  "packageRules": [
    {
      "matchDatasources": ["docker"],
      "matchPackageNames": ["projectjackin/construct"],
      "versioning": "regex:^(?<major>\\d+)\\.(?<minor>\\d+)-trixie$"
    }
  ]
}
"#
}

const fn workflow_contents() -> &'static str {
    r"name: Validate role

on:
  pull_request:
  push:
    branches: [main]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: jackin-project/jackin-role-action@main
"
}

fn display_name(role_name: &str) -> String {
    role_name
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
