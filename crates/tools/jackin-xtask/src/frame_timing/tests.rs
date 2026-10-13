use super::absolute_from;
use std::path::Path;

#[test]
fn relative_artifact_paths_resolve_from_repo_root() {
    assert_eq!(
        absolute_from(Path::new("/repo"), Path::new("target/frame.json")),
        Path::new("/repo/target/frame.json")
    );
}
