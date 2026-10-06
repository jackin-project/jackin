// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn write_tree(root: &std::path::Path, files: &[(&str, &[u8])]) {
    for (relative, bytes) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

pub(super) fn repo_text(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

pub(super) fn task_block<'a>(mise: &'a str, name: &str) -> &'a str {
    let marker = format!("[tasks.{name}]\n");
    let start = mise
        .find(&marker)
        .unwrap_or_else(|| panic!("mise.toml missing {marker}"));
    let rest = &mise[start + marker.len()..];
    let end = rest.find("\n[").map_or(rest.len(), |index| index + 1);
    &rest[..end]
}

pub(super) fn assert_subsequence(haystack: &str, needles: &[&str], label: &str) {
    let mut cursor = 0;
    for needle in needles {
        let found = haystack[cursor..]
            .find(needle)
            .unwrap_or_else(|| panic!("{label}: `{needle}` missing or out of order"));
        cursor += found + needle.len();
    }
}
