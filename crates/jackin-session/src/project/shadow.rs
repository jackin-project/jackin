// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{FileMode, FileSystem, HostPath, RepoPath, UNBOUNDED_READ};

use crate::launch::LaunchError;
use crate::project::ProjectStep;
use crate::project::facts::{Facts, ObjectFormat};
use crate::project::git::{HostGit, Pace, words};

const DATA_KEYS: [&str; 9] = [
    "core.autocrlf",
    "core.eol",
    "core.safecrlf",
    "core.symlinks",
    "core.filemode",
    "core.ignorecase",
    "core.precomposeunicode",
    "core.attributesfile",
    "core.checkroundtripencoding",
];

struct Entry {
    key: String,
    value: Option<String>,
}

fn is_include(key: &str) -> bool {
    key == "include.path"
        || key.strip_prefix("includeif.").is_some_and(|rest| {
            rest.rsplit_once('.')
                .is_some_and(|(_, variable)| variable == "path")
        })
}

fn entries(config: &[u8]) -> Vec<Entry> {
    let fields: Vec<&[u8]> = config.split(|byte| *byte == 0).collect();
    let mut kept = Vec::new();
    for pair in fields.as_chunks::<2>().0 {
        let scope = pair[0];
        let record = String::from_utf8_lossy(pair[1]).into_owned();
        let (key, value) = match record.split_once('\n') {
            Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
            None => (record, None),
        };
        let wanted = match scope {
            b"system" | b"global" => !is_include(&key),
            b"local" | b"worktree" => DATA_KEYS.contains(&key.as_str()),
            _ => false,
        };
        if wanted {
            kept.push(Entry { key, value });
        }
    }
    kept
}

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn render(entries: &[Entry], format: ObjectFormat) -> String {
    let mut text = String::from("[core]\n\tbare = false\n");
    match format {
        ObjectFormat::Sha1 => text.push_str("\trepositoryformatversion = 0\n"),
        ObjectFormat::Sha256 => {
            text.push_str("\trepositoryformatversion = 1\n[extensions]\n\tobjectformat = sha256\n");
        }
    }
    for entry in entries {
        let (section, rest) = entry.key.split_once('.').unwrap_or((&entry.key, ""));
        let (subsection, variable) = match rest.rsplit_once('.') {
            Some((subsection, variable)) => (Some(subsection), variable),
            None => (None, rest),
        };
        match subsection {
            Some(name) => text.push_str(&format!("[{section} {}]\n", quoted(name))),
            None => text.push_str(&format!("[{section}]\n")),
        }
        match &entry.value {
            Some(value) => text.push_str(&format!("\t{variable} = {}\n", quoted(value))),
            None => text.push_str(&format!("\t{variable}\n")),
        }
    }
    text
}

pub(super) async fn create(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    parent: &HostPath,
    facts: &Facts,
) -> Result<HostPath, LaunchError> {
    let config = git
        .require(
            ProjectStep::HostTree,
            Pace::Quick,
            words(&["config", "--show-scope", "-z", "--list"]),
            Vec::new(),
        )
        .await?;
    let dir = HostPath::resolve(Path::new("shadow"), parent)?;
    let at = |name: &str| HostPath::resolve(Path::new(name), &dir);
    for sub in ["objects", "refs", "info"] {
        fs.create_private_dir_all(&at(sub)?).await?;
    }
    let text = render(&entries(&config), facts.format);
    fs.write_new(&at("config")?, text.as_bytes(), FileMode(0o600))
        .await?;
    fs.write_new(&at("HEAD")?, b"ref: refs/heads/main\n", FileMode(0o600))
        .await?;
    let attributes =
        RepoPath::parse("info/attributes").map_err(|error| LaunchError::ProjectOutput {
            step: ProjectStep::HostTree,
            message: error.to_string(),
        })?;
    if let Ok(file) = fs
        .read_regular(&facts.git_dir, &attributes, UNBOUNDED_READ)
        .await?
    {
        fs.write_new(&at("info/attributes")?, &file.bytes, FileMode(0o600))
            .await?;
    }
    Ok(dir)
}
