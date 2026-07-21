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
    let mise_config = command
        .get_envs()
        .find_map(|(name, value)| (name == "MISE_CONFIG_FILE").then_some(value))
        .flatten();

    assert_eq!(mise_config, Some(OsStr::new("/workspace/docs/mise.toml")));
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
