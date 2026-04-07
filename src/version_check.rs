use crate::docker::CommandRunner;
use crate::paths::JackinPaths;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How long we trust the cached npm lookup before re-checking.
const NPM_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

/// File that caches the latest published Claude Code version from npm.
fn npm_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.cache_dir.join("claude-latest-version")
}

/// File that records the Claude Code version baked into a given Docker image.
fn image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-claude-version/{image}"))
}

/// Query npm for the latest published `@anthropic-ai/claude-code` version,
/// returning a cached value when the cache is still fresh.
pub fn latest_claude_version(
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
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Persist the Claude Code version that was just installed into an image.
pub fn store_image_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Returns `true` when the image contains an older Claude Code version than
/// the latest published release, meaning the image should be rebuilt.
pub fn needs_claude_update(
    paths: &JackinPaths,
    image: &str,
    runner: &mut impl CommandRunner,
) -> bool {
    let installed = match stored_image_version(paths, image) {
        Some(v) => v,
        None => return false, // first build — let it proceed normally
    };
    let latest = match latest_claude_version(paths, runner) {
        Some(v) => v,
        None => return false, // npm unavailable — don't force a rebuild
    };
    installed != latest
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

        store_image_version(&paths, "jackin-agent-smith", "2.1.91");

        assert_eq!(
            stored_image_version(&paths, "jackin-agent-smith"),
            Some("2.1.91".to_string())
        );
    }

    #[test]
    fn needs_update_when_versions_differ() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jackin-agent-smith", "2.1.91");
        // Seed the npm cache with a newer version
        let cache = npm_cache_path(&paths);
        let _ = write_cached(&cache, "2.1.92");

        let mut runner = StubRunner("2.1.92".to_string());
        assert!(needs_claude_update(&paths, "jackin-agent-smith", &mut runner));
    }

    #[test]
    fn no_update_when_versions_match() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jackin-agent-smith", "2.1.92");
        let cache = npm_cache_path(&paths);
        let _ = write_cached(&cache, "2.1.92");

        let mut runner = StubRunner("2.1.92".to_string());
        assert!(!needs_claude_update(&paths, "jackin-agent-smith", &mut runner));
    }

    #[test]
    fn no_update_on_first_build() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let mut runner = StubRunner("2.1.92".to_string());
        // No stored version yet → should not force rebuild
        assert!(!needs_claude_update(&paths, "jackin-agent-smith", &mut runner));
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

    /// Minimal [`CommandRunner`] that returns a fixed string for any `capture`.
    struct StubRunner(String);

    impl CommandRunner for StubRunner {
        fn run(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn capture(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<String> {
            Ok(self.0.clone())
        }
    }
}
