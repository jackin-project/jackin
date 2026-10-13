// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("image-id".into(), "jackin-image".into()),
            ("status-id".into(), "jackin-agent-status".into()),
            ("app-id".into(), "jackin".into()),
        ]),
        package_names: BTreeMap::from([
            ("image-id".into(), "jackin-image".into()),
            ("status-id".into(), "jackin-agent-status".into()),
            ("app-id".into(), "jackin".into()),
            ("arrayref 0.3.9".into(), "arrayref".into()),
        ]),
        roots: BTreeMap::from([
            (
                "image-id".into(),
                PathBuf::from("crates/adapters/jackin-image"),
            ),
            (
                "status-id".into(),
                PathBuf::from("crates/services/jackin-agent-status"),
            ),
            ("app-id".into(), PathBuf::from("crates/apps/jackin")),
        ]),
        dependents: BTreeMap::from([("image-id".into(), BTreeSet::from(["app-id".into()]))]),
        resolved_dependencies: BTreeMap::from([
            ("image-id".into(), BTreeSet::from(["arrayref 0.3.9".into()])),
            ("status-id".into(), BTreeSet::new()),
            ("app-id".into(), BTreeSet::from(["image-id".into()])),
        ]),
        resolved_features: BTreeMap::new(),
    }
}

pub(super) fn nested_fixtures() -> Vec<NestedPackage> {
    vec![
        NestedPackage {
            dir: PathBuf::from("crates/services/jackin-agent-status/fuzz"),
            name: "jackin-agent-status-fuzz".into(),
            member_dependencies: BTreeSet::from(["jackin-agent-status".into()]),
            depends_on_unknown: false,
            excluded: false,
        },
        NestedPackage {
            dir: PathBuf::from("vendor/arrayref"),
            name: "arrayref".into(),
            member_dependencies: BTreeSet::new(),
            depends_on_unknown: false,
            excluded: false,
        },
        NestedPackage {
            dir: PathBuf::from("crates/tools/jackin-lints"),
            name: "jackin-lints".into(),
            member_dependencies: BTreeSet::new(),
            depends_on_unknown: false,
            excluded: true,
        },
    ]
}

pub(super) fn inputs_fixture() -> InputOwners {
    InputOwners {
        members: BTreeMap::from([(
            PathBuf::from("docker/runtime/entrypoint.sh"),
            BTreeSet::from(["jackin-image".into()]),
        )]),
        nested: BTreeMap::new(),
        unknown: false,
        unknown_sources: Vec::new(),
    }
}

pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("workspace root above crates/tools/jackin-xtask")
        .to_path_buf()
}

pub(super) fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut stack = vec![root.join("crates"), root.join("vendor")];
    while let Some(dir) = stack.pop() {
        let entries = fs_util::read_dir_sorted(&dir).expect("read dir");
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}

pub(super) fn find_include_macros(source: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        if let Some(tail) = rest.strip_prefix("include_str!") {
            hits.push(source.len() - tail.len());
            index += "include_str!".len();
        } else if let Some(tail) = rest.strip_prefix("include_bytes!") {
            hits.push(source.len() - tail.len());
            index += "include_bytes!".len();
        } else if rest.starts_with("include!") && !rest.starts_with("include_") {
            index += "include!".len();
            hits.push(index);
        } else {
            index += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    hits
}

pub(super) fn skip_ascii_trivia(mut source: &str) -> &str {
    loop {
        let trimmed = source.trim_start_matches([' ', '\t', '\n', '\r']);
        if let Some(rest) = trimmed.strip_prefix("//") {
            match rest.find('\n') {
                Some(index) => source = &rest[index + 1..],
                None => return "",
            }
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            match rest.find("*/") {
                Some(index) => source = &rest[index + 2..],
                None => return "",
            }
        } else {
            return trimmed;
        }
    }
}
