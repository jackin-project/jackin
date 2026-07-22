//! Independent publication state for jackin❯ Desktop release assets.
//!
//! Prints `KEY=value` lines suitable for `GITHUB_OUTPUT` / shell capture.

use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::cmd;

const DEFAULT_REPO: &str = "jackin-project/jackin";
const DEFAULT_TAP: &str = "jackin-project/homebrew-tap";

#[derive(Args)]
pub(crate) struct ReleaseStateArgs {
    /// Release version without leading `v` (e.g. `0.6.0`).
    version: String,
    /// GitHub repo `owner/name` hosting the release.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    /// Homebrew tap repo `owner/name`.
    #[arg(long, default_value = DEFAULT_TAP)]
    tap: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors independent GITHUB_OUTPUT keys from the release state protocol"
)]
pub(crate) struct ReleaseState {
    pub release_exists: bool,
    pub app_file_assets_complete: bool,
    pub formula_complete: bool,
    pub cask_complete: bool,
    pub complete: bool,
    pub asset: String,
}

impl ReleaseState {
    pub(crate) fn render(&self) -> String {
        format!(
            "release_exists={}\napp_file_assets_complete={}\nformula_complete={}\ncask_complete={}\ncomplete={}\nasset={}\n",
            self.release_exists,
            self.app_file_assets_complete,
            self.formula_complete,
            self.cask_complete,
            self.complete,
            self.asset,
        )
    }
}

pub(crate) fn desktop_asset_name(version: &str) -> String {
    format!("jackin-desktop-{version}-aarch64-apple-darwin.zip")
}

/// Pure computation — used by live fetch and offline fixtures.
pub(crate) fn compute_state(
    version: &str,
    repo: &str,
    release_exists: bool,
    asset_names: &[String],
    formula_body: Option<&str>,
    cask_body: Option<&str>,
) -> ReleaseState {
    let asset = desktop_asset_name(version);
    let mut has_zip = false;
    let mut has_sha = false;
    let mut has_bundle = false;
    let mut has_sbom = false;
    for name in asset_names {
        if name == &asset {
            has_zip = true;
        } else if name == &format!("{asset}.sha256") {
            has_sha = true;
        } else if name == &format!("{asset}.bundle") {
            has_bundle = true;
        } else if name == &format!("{asset}.sbom.json") {
            has_sbom = true;
        }
    }
    let app_file_assets_complete =
        release_exists && has_zip && has_sha && has_bundle && has_sbom;

    let formula_complete = formula_body.is_some_and(|body| {
        extract_quoted_field(body, "version").as_deref() == Some(version)
    });

    let expected_url =
        format!("https://github.com/{repo}/releases/download/v{version}/{asset}");
    let cask_complete = cask_body.is_some_and(|body| {
        let cask_version = extract_quoted_field(body, "version");
        let cask_url = extract_quoted_field(body, "url");
        let cask_sha = extract_quoted_field(body, "sha256");
        cask_version.as_deref() == Some(version)
            && cask_url.as_deref() == Some(expected_url.as_str())
            && cask_sha.as_ref().is_some_and(|sha| sha.len() == 64)
    });

    let complete = release_exists && app_file_assets_complete && cask_complete;
    ReleaseState {
        release_exists,
        app_file_assets_complete,
        formula_complete,
        cask_complete,
        complete,
        asset,
    }
}

fn extract_quoted_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("{key} \"");
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&needle)
            && let Some(end) = rest.find('"')
        {
            return Some(rest[..end].to_owned());
        }
    }
    None
}

pub(crate) fn run(args: ReleaseStateArgs) -> Result<()> {
    if args.version.is_empty() {
        bail!("version is required");
    }
    let state = live_state(&args.version, &args.repo, &args.tap)?;
    // stdout is the capture surface for GITHUB_OUTPUT / tee.
    #[expect(
        clippy::print_stdout,
        reason = "release-state KEY=value lines are the command product"
    )]
    {
        print!("{}", state.render());
    }
    Ok(())
}

fn live_state(version: &str, repo: &str, tap: &str) -> Result<ReleaseState> {
    let tag = format!("v{version}");
    let release_exists = gh_release_exists(repo, &tag);
    let assets = if release_exists {
        gh_release_asset_names(repo, &tag).unwrap_or_default()
    } else {
        Vec::new()
    };
    let formula_url =
        format!("https://raw.githubusercontent.com/{tap}/main/Formula/jackin.rb");
    let cask_url =
        format!("https://raw.githubusercontent.com/{tap}/main/Casks/jackin-desktop.rb");
    let formula_body = curl_text(&formula_url).ok();
    let cask_body = curl_text(&cask_url).ok();
    Ok(compute_state(
        version,
        repo,
        release_exists,
        &assets,
        formula_body.as_deref(),
        cask_body.as_deref(),
    ))
}

fn gh_release_exists(repo: &str, tag: &str) -> bool {
    let mut gh = cmd::command("gh");
    gh.args(["release", "view", tag, "--repo", repo]);
    // Existence probe — failure means missing.
    cmd::run(&mut gh).is_ok()
}

fn gh_release_asset_names(repo: &str, tag: &str) -> Result<Vec<String>> {
    let mut gh = cmd::command("gh");
    gh.args([
        "release",
        "view",
        tag,
        "--repo",
        repo,
        "--json",
        "assets",
        "--jq",
        ".assets[].name",
    ]);
    let out = cmd::output_string(&mut gh).context("listing release assets")?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect())
}

fn curl_text(url: &str) -> Result<String> {
    let mut curl = Command::new("curl");
    curl.args(["-fsSL", "--max-time", "30", url]);
    cmd::output_string(&mut curl).with_context(|| format!("fetching {url}"))
}

#[cfg(test)]
mod pure_tests {
    use super::*;

    #[test]
    fn missing_release() {
        let state = compute_state("1.2.3", DEFAULT_REPO, false, &[], None, None);
        assert!(!state.release_exists);
        assert!(!state.app_file_assets_complete);
        assert!(!state.complete);
        assert_eq!(state.asset, desktop_asset_name("1.2.3"));
    }

    #[test]
    fn complete_release_formula_cask() {
        let version = "1.2.3";
        let asset = desktop_asset_name(version);
        let names = vec![
            asset.clone(),
            format!("{asset}.sha256"),
            format!("{asset}.bundle"),
            format!("{asset}.sbom.json"),
        ];
        let formula = r#"  version "1.2.3""#;
        let cask = format!(
            r#"  version "1.2.3"
  sha256 "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  url "https://github.com/{DEFAULT_REPO}/releases/download/v1.2.3/{asset}"
"#
        );
        let state = compute_state(
            version,
            DEFAULT_REPO,
            true,
            &names,
            Some(formula),
            Some(&cask),
        );
        assert!(state.release_exists);
        assert!(state.app_file_assets_complete);
        assert!(state.formula_complete);
        assert!(state.cask_complete);
        assert!(state.complete);
        // idempotent pure computation
        let again = compute_state(
            version,
            DEFAULT_REPO,
            true,
            &names,
            Some(formula),
            Some(&cask),
        );
        assert_eq!(state, again);
    }

    #[test]
    fn partial_assets_incomplete() {
        let names = vec![desktop_asset_name("9.9.9")];
        let state = compute_state("9.9.9", DEFAULT_REPO, true, &names, None, None);
        assert!(state.release_exists);
        assert!(!state.app_file_assets_complete);
        assert!(!state.complete);
    }

    #[test]
    fn render_key_value_lines() {
        let state = compute_state("0.1.0", DEFAULT_REPO, false, &[], None, None);
        let rendered = state.render();
        assert!(rendered.contains("release_exists=false\n"));
        assert!(rendered.contains("complete=false\n"));
        assert!(rendered.contains(&format!("asset={}\n", desktop_asset_name("0.1.0"))));
    }
}
