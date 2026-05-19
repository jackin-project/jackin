use crate::docker::CommandRunner;
use crate::paths::JackinPaths;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How long we trust the cached npm lookup before re-checking.
const NPM_CACHE_TTL: Duration = Duration::from_hours(1);

/// File that caches the latest published Claude Code version from npm.
fn npm_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.cache_dir.join("claude-latest-version")
}

/// File that caches the latest published `OpenCode` version from `npm`.
fn opencode_npm_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.cache_dir.join("opencode-latest-version")
}

/// File that records the Claude Code version baked into a given Docker image.
fn image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-claude-version/{image}"))
}

/// File that records the `OpenCode` version baked into a given `Docker` image.
fn opencode_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-opencode-version/{image}"))
}

/// File that records the Kimi version baked into a given Docker image.
fn kimi_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-kimi-version/{image}"))
}

/// Query npm for the latest published `@anthropic-ai/claude-code` version,
/// returning a cached value when the cache is still fresh.
pub async fn latest_claude_version(
    paths: &JackinPaths,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    let cache_file = npm_cache_path(paths);

    // Return cached value if fresh enough
    if let Some(cached) = read_if_fresh(&cache_file, NPM_CACHE_TTL) {
        return Some(cached);
    }

    // Query npm
    let version = runner
        .capture(
            "npm",
            &["view", "@anthropic-ai/claude-code", "version"],
            None,
        )
        .await
        .ok()?;
    let version = version.trim().to_string();
    if version.is_empty() {
        return None;
    }

    // Persist to cache (best-effort)
    let _ = write_cached(&cache_file, &version);
    Some(version)
}

/// Read the Claude Code version we stored for a previously-built image.
pub fn stored_image_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Claude Code version that was just installed into an image.
pub fn store_image_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Returns `true` when the image contains an older Claude Code version than
/// the latest published release, meaning the image should be rebuilt.
pub async fn needs_claude_update(
    paths: &JackinPaths,
    image: &str,
    runner: &mut impl CommandRunner,
) -> bool {
    let Some(installed) = stored_image_version(paths, image) else {
        return false; // first build — let it proceed normally
    };
    let Some(latest) = latest_claude_version(paths, runner).await else {
        return false; // npm unavailable — don't force a rebuild
    };
    installed != latest
}

/// Query npm for the latest published `opencode-ai` version,
/// returning a cached value when the cache is still fresh.
pub async fn latest_opencode_version(
    paths: &JackinPaths,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    let cache_file = opencode_npm_cache_path(paths);

    if let Some(cached) = read_if_fresh(&cache_file, NPM_CACHE_TTL) {
        return Some(cached);
    }

    let version = runner
        .capture("npm", &["view", "opencode-ai", "version"], None)
        .await
        .ok()?;
    let version = version.trim().to_string();
    if version.is_empty() {
        return None;
    }

    let _ = write_cached(&cache_file, &version);
    Some(version)
}

/// Read the `OpenCode` version we stored for a previously-built image.
pub fn stored_opencode_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = opencode_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `OpenCode` version that was just installed into an image.
pub fn store_opencode_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = opencode_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Returns `true` when the image contains an older `OpenCode` version than
/// the latest published release, meaning the image should be rebuilt.
pub async fn needs_opencode_update(
    paths: &JackinPaths,
    image: &str,
    runner: &mut impl CommandRunner,
) -> bool {
    let Some(installed) = stored_opencode_version(paths, image) else {
        return false;
    };
    let Some(latest) = latest_opencode_version(paths, runner).await else {
        return false;
    };
    installed != latest
}

/// Read the Kimi version we stored for a previously-built image.
pub fn stored_kimi_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = kimi_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Kimi version that was just installed into an image.
pub fn store_kimi_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = kimi_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// File that records the last `JACKIN_CACHE_BUST` value used to build an image.
fn cache_bust_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-cache-bust/{image}"))
}

/// Read the last `JACKIN_CACHE_BUST` value used for an image build.
pub fn stored_cache_bust(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = cache_bust_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `JACKIN_CACHE_BUST` value used for a build so that
/// subsequent non-rebuild launches replay the same value and hit the
/// same Docker cache layer.
pub fn store_cache_bust(paths: &JackinPaths, image: &str, value: &str) {
    let path = cache_bust_path(paths, image);
    if let Err(e) = write_cached(&path, value) {
        eprintln!(
            "warning: failed to persist JACKIN_CACHE_BUST for {image}: {e}; \
             subsequent non-rebuild launches may replay the wrong cache layer"
        );
    }
}

/// Extract a bare semver string from `claude --version` output.
///
/// The command returns e.g. `"2.1.96 (Claude Code)"` but we only need the
/// `"2.1.96"` portion to compare against the npm registry.  Returns `None`
/// when the output doesn't look like a version string.
pub fn parse_claude_version(raw: &str) -> Option<&str> {
    let token = raw.split_whitespace().next()?;
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

/// Extract a bare semver string from `kimi --version` output.
///
/// The command returns e.g. `"kimi 1.2.3"` but we only need the `"1.2.3"`
/// portion. Returns `None` when the output doesn't look like a version string.
pub fn parse_kimi_version(raw: &str) -> Option<&str> {
    let mut tokens = raw.split_whitespace();
    let first = tokens.next()?;
    if first.split('.').count() >= 2 && first.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(first);
    }
    let second = tokens.next()?;
    if second.split('.').count() >= 2 && second.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(second);
    }
    None
}

/// Extract a bare semver string from `opencode --version` output.
///
/// The command returns e.g. `"1.14.48"` or `"v1.14.48"`. Strip a leading `v`
/// if present, then validate it looks like a semver.
pub fn parse_opencode_version(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let token = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

// ── helpers ────────────────────────────────────────────────────────────

/// Read a cache file only if it was modified within `ttl`.
fn read_if_fresh(path: &PathBuf, ttl: Duration) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    if SystemTime::now().duration_since(modified).unwrap_or(ttl) >= ttl {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Write content to a cache file, creating parent directories as needed.
fn write_cached(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    #[test]
    fn stores_and_reads_image_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_agent-smith", "2.1.91");

        assert_eq!(
            stored_image_version(&paths, "jk_agent-smith"),
            Some("2.1.91".to_string())
        );
    }

    #[tokio::test]
    async fn needs_update_when_versions_differ() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_agent-smith", "2.1.91");
        // Seed the npm cache with a newer version
        let cache = npm_cache_path(&paths);
        let _ = write_cached(&cache, "2.1.92");

        let mut runner = StubRunner("2.1.92".to_string());
        assert!(needs_claude_update(&paths, "jk_agent-smith", &mut runner).await);
    }

    #[tokio::test]
    async fn no_update_when_versions_match() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_agent-smith", "2.1.92");
        let cache = npm_cache_path(&paths);
        let _ = write_cached(&cache, "2.1.92");

        let mut runner = StubRunner("2.1.92".to_string());
        assert!(!needs_claude_update(&paths, "jk_agent-smith", &mut runner).await);
    }

    #[tokio::test]
    async fn no_update_on_first_build() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let mut runner = StubRunner("2.1.92".to_string());
        // No stored version yet → should not force rebuild
        assert!(!needs_claude_update(&paths, "jk_agent-smith", &mut runner).await);
    }

    #[test]
    fn expired_cache_is_ignored() {
        let temp = tempdir().unwrap();
        let cache_file = temp.path().join("expired");
        std::fs::write(&cache_file, "2.1.91").unwrap();

        // A zero TTL means any file is always expired
        assert!(read_if_fresh(&cache_file, Duration::ZERO).is_none());
    }

    #[test]
    fn fresh_cache_is_returned() {
        let temp = tempdir().unwrap();
        let cache_file = temp.path().join("fresh");
        std::fs::write(&cache_file, "2.1.92").unwrap();

        assert_eq!(
            read_if_fresh(&cache_file, NPM_CACHE_TTL),
            Some("2.1.92".to_string())
        );
    }

    #[test]
    fn parse_claude_version_strips_suffix() {
        assert_eq!(parse_claude_version("2.1.96 (Claude Code)"), Some("2.1.96"));
    }

    #[test]
    fn parse_claude_version_bare_semver() {
        assert_eq!(parse_claude_version("2.1.96"), Some("2.1.96"));
    }

    #[test]
    fn parse_claude_version_two_part() {
        assert_eq!(parse_claude_version("1.0"), Some("1.0"));
    }

    #[test]
    fn parse_claude_version_rejects_garbage() {
        assert_eq!(parse_claude_version("not-a-version"), None);
    }

    #[test]
    fn parse_claude_version_rejects_empty() {
        assert_eq!(parse_claude_version(""), None);
    }

    #[test]
    fn stores_and_reads_opencode_image_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.48");

        assert_eq!(
            stored_opencode_version(&paths, "jk_the-architect"),
            Some("1.14.48".to_string())
        );
    }

    #[tokio::test]
    async fn opencode_needs_update_when_versions_differ() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.47");
        let cache = opencode_npm_cache_path(&paths);
        let _ = write_cached(&cache, "1.14.48");

        let mut runner = StubRunner("1.14.48".to_string());
        assert!(needs_opencode_update(&paths, "jk_the-architect", &mut runner).await);
    }

    #[tokio::test]
    async fn opencode_no_update_when_versions_match() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.48");
        let cache = opencode_npm_cache_path(&paths);
        let _ = write_cached(&cache, "1.14.48");

        let mut runner = StubRunner("1.14.48".to_string());
        assert!(!needs_opencode_update(&paths, "jk_the-architect", &mut runner).await);
    }

    #[test]
    fn stores_and_reads_kimi_image_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_kimi_version(&paths, "jk_the-architect", "1.2.3");

        assert_eq!(
            stored_kimi_version(&paths, "jk_the-architect"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn kimi_version_stored_separately_from_claude_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_test", "2.0.0");
        store_kimi_version(&paths, "jk_test", "1.0.0");

        assert_eq!(
            stored_image_version(&paths, "jk_test"),
            Some("2.0.0".to_string())
        );
        assert_eq!(
            stored_kimi_version(&paths, "jk_test"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn parse_kimi_version_prefixed_with_kimi() {
        assert_eq!(parse_kimi_version("kimi 1.2.3"), Some("1.2.3"));
    }

    #[test]
    fn parse_kimi_version_bare_semver() {
        assert_eq!(parse_kimi_version("1.2.3"), Some("1.2.3"));
    }

    #[test]
    fn parse_kimi_version_two_part() {
        assert_eq!(parse_kimi_version("kimi 1.0"), Some("1.0"));
    }

    #[test]
    fn parse_kimi_version_rejects_v_prefix() {
        assert_eq!(parse_kimi_version("kimi v1.2.3"), None);
    }

    #[test]
    fn parse_kimi_version_rejects_garbage() {
        assert_eq!(parse_kimi_version("not-a-version"), None);
    }

    #[test]
    fn parse_kimi_version_rejects_empty() {
        assert_eq!(parse_kimi_version(""), None);
    }

    #[test]
    fn parse_opencode_version_bare_semver() {
        assert_eq!(parse_opencode_version("1.14.48"), Some("1.14.48"));
    }

    #[test]
    fn parse_opencode_version_strips_v_prefix() {
        assert_eq!(parse_opencode_version("v1.14.48"), Some("1.14.48"));
    }

    #[test]
    fn parse_opencode_version_rejects_garbage() {
        assert_eq!(parse_opencode_version("not-a-version"), None);
    }

    #[test]
    fn parse_opencode_version_rejects_empty() {
        assert_eq!(parse_opencode_version(""), None);
    }

    /// Minimal [`CommandRunner`] that returns a fixed string for any `capture`.
    struct StubRunner(String);

    impl CommandRunner for StubRunner {
        async fn run(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
            _opts: &crate::docker::RunOptions,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn capture(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<String> {
            Ok(self.0.clone())
        }

        async fn capture_secret(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<String> {
            // StubRunner is used only by version_check tests which never
            // exercise code paths that call build_agent_image. Panic here
            // to surface unexpected calls immediately rather than silently
            // returning the fixed stub value for a different purpose.
            panic!(
                "StubRunner::capture_secret called unexpectedly; version_check tests do not exercise secret-capture paths"
            )
        }
    }
}
