use std::ffi::OsStr;
use std::path::PathBuf;

use super::command;

#[test]
fn command_preserves_all_link_remaps() {
    let command = command(
        "https://docs.example",
        &PathBuf::from("/workspace"),
        "https://github.example/blob/main",
        "https://github.example/edit/main",
    );
    let args = command
        .get_args()
        .map(OsStr::to_string_lossy)
        .collect::<Vec<_>>();

    assert!(
        args.contains(&"https://docs.example/(.*) file:///workspace/docs/.output/public/$1".into())
    );
    assert!(args.contains(&"https://github.example/blob/main/(.*) file:///workspace/$1".into()));
    assert!(args.contains(&"https://github.example/edit/main/(.*) file:///workspace/$1".into()));
    assert_eq!(
        args.last().map(AsRef::as_ref),
        Some("docs/.output/public/**/*.html")
    );
}
