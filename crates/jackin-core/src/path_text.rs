//! Path text helpers shared by non-TUI and TUI crates.

/// Shorten an absolute path by replacing the operator's `$HOME` prefix with `~`.
#[must_use]
pub fn shorten_home(path: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_owned();
    };
    let home = home.to_string_lossy().into_owned();
    if home.is_empty() || !path.starts_with(&home) {
        return path.to_owned();
    }
    let rest = &path[home.len()..];
    // Only collapse when the next character after `$HOME` is a path separator
    // (or end of string). Otherwise `/Users/alice.notmine` would incorrectly
    // compact to `~.notmine`.
    if rest.is_empty() || rest.starts_with('/') {
        format!("~{rest}")
    } else {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::shorten_home;

    #[test]
    fn shorten_home_returns_path_when_no_match() {
        let home = std::env::var("HOME").unwrap_or_default();
        let alien = if home == "/" {
            "etc/hosts".to_owned()
        } else {
            format!("{home}.notmine")
        };
        assert_eq!(shorten_home(&alien), alien);
    }
}
