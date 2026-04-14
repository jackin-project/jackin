#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn write_jq_stub(path: &Path) {
    write_executable(
        path,
        r#"#!/usr/bin/env python3
import json
import sys

args = sys.argv[1:]
while args and args[0] in {"-r", "-c"}:
    args = args[1:]
flt = args[0]
if len(args) > 1:
    with open(args[1], "r", encoding="utf-8") as fh:
        data = json.load(fh)
else:
    data = json.load(sys.stdin)

if flt == ".marketplaces[]?":
    for item in data.get("marketplaces", []):
        print(json.dumps(item))
elif flt == ".plugins[]?":
    for item in data.get("plugins", []):
        print(item)
elif flt == ".source":
    print(data["source"])
elif flt == ".sparse[]?":
    for item in data.get("sparse", []):
        print(item)
elif flt == ".[].id":
    if isinstance(data, list):
        for item in data:
            if "id" in item:
                print(item["id"])
else:
    raise SystemExit(f"unsupported filter: {flt}")
"#,
    );
}

#[test]
fn install_plugins_script_adds_marketplaces_before_installing_plugins() {
    let temp = tempdir().unwrap();
    let plugins_file = temp.path().join("plugins.json");
    fs::write(
        &plugins_file,
        r#"{
  "marketplaces": [
    {
      "source": "obra/superpowers-marketplace",
      "sparse": ["plugins", ".claude-plugin"]
    },
    {
      "source": "jackin-project/jackin-marketplace",
      "sparse": []
    }
  ],
  "plugins": [
    "superpowers@superpowers-marketplace",
    "jackin-dev@jackin-marketplace"
  ]
}"#,
    )
    .unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let log_file = temp.path().join("claude.log");

    let claude_path = bin_dir.join("claude");
    write_executable(
        &claude_path,
        &format!(
            r#"#!/bin/sh
# Return empty JSON array for "plugin list --json" (no plugins installed)
if [ "$1" = "plugin" ] && [ "$2" = "list" ] && [ "$3" = "--json" ]; then
    echo '[]'
    exit 0
fi
printf '%s\n' "$*" >> '{}'
"#,
            log_file.display()
        ),
    );

    let jq_path = bin_dir.join("jq");
    write_jq_stub(&jq_path);

    let status = Command::new("bash")
        .arg("docker/construct/install-plugins.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("JACKIN_PLUGINS_FILE", &plugins_file)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(log_file).unwrap(),
        "plugin marketplace add anthropics/claude-plugins-official\n\
plugin marketplace add obra/superpowers-marketplace --sparse plugins .claude-plugin\n\
plugin marketplace add jackin-project/jackin-marketplace\n\
plugin install superpowers@superpowers-marketplace\n\
plugin install jackin-dev@jackin-marketplace\n"
    );
}

#[test]
fn install_plugins_script_surfaces_custom_marketplace_failures() {
    let temp = tempdir().unwrap();
    let plugins_file = temp.path().join("plugins.json");
    fs::write(
        &plugins_file,
        r#"{
  "marketplaces": [
    {
      "source": "obra/superpowers-marketplace",
      "sparse": ["plugins", ".claude-plugin"]
    }
  ],
  "plugins": [
    "superpowers@superpowers-marketplace"
  ]
}"#,
    )
    .unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let log_file = temp.path().join("claude.log");

    let claude_path = bin_dir.join("claude");
    write_executable(
        &claude_path,
        &format!(
            r#"#!/bin/sh
# Return empty JSON array for "plugin list --json"
if [ "$1" = "plugin" ] && [ "$2" = "list" ] && [ "$3" = "--json" ]; then
    echo '[]'
    exit 0
fi
printf '%s\n' "$*" >> '{}'
if [ "$1" = "plugin" ] && [ "$2" = "marketplace" ] && [ "$3" = "add" ] && [ "$4" = "obra/superpowers-marketplace" ]; then
  echo 'failed to add marketplace' >&2
  exit 1
fi
"#,
            log_file.display()
        ),
    );

    let jq_path = bin_dir.join("jq");
    write_jq_stub(&jq_path);

    let output = Command::new("bash")
        .arg("docker/construct/install-plugins.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("JACKIN_PLUGINS_FILE", &plugins_file)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to add marketplace"));
    assert_eq!(
        fs::read_to_string(log_file).unwrap(),
        "plugin marketplace add anthropics/claude-plugins-official\n\
plugin marketplace add obra/superpowers-marketplace --sparse plugins .claude-plugin\n"
    );
}
