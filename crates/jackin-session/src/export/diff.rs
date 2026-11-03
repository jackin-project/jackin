// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::TerminalText;

const DEV_NULL: &str = "/dev/null";
const MALFORMED_HUNK: &str = "git diff printed an unreadable hunk header";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed { from: TerminalText },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeLine {
    Context(TerminalText),
    Added(TerminalText),
    Removed(TerminalText),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeHunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<ChangeLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: TerminalText,
    pub status: ChangeStatus,
    pub hunks: Vec<ChangeHunk>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    pub files: Vec<ChangedFile>,
}

impl ChangedFile {
    pub fn additions(&self) -> usize {
        self.count(|line| matches!(line, ChangeLine::Added(_)))
    }

    pub fn deletions(&self) -> usize {
        self.count(|line| matches!(line, ChangeLine::Removed(_)))
    }

    fn count(&self, wanted: impl Fn(&ChangeLine) -> bool) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| wanted(line))
            .count()
    }
}

impl ChangeSet {
    pub fn additions(&self) -> usize {
        self.files.iter().map(ChangedFile::additions).sum()
    }

    pub fn deletions(&self) -> usize {
        self.files.iter().map(ChangedFile::deletions).sum()
    }
}

struct Draft {
    path: String,
    status: ChangeStatus,
    hunks: Vec<ChangeHunk>,
}

fn text(value: &str) -> TerminalText {
    TerminalText::sanitize(value.as_bytes())
}

fn unquote(name: &str) -> String {
    let Some(inner) = name
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return name.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(byte) = chars.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(digit @ b'0'..=b'7') => {
                let mut value = u32::from(digit - b'0');
                for _ in 0..2 {
                    if let Some(next @ b'0'..=b'7') = chars.peek().copied() {
                        value = value * 8 + u32::from(next - b'0');
                        chars.next();
                    }
                }
                bytes.push(u8::try_from(value & 0xff).unwrap_or(b'?'));
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn header_path(rest: &str) -> String {
    if let Some(index) = rest.find("\" \"") {
        return unquote(&rest[index + 2..])
            .trim_start_matches("b/")
            .to_owned();
    }
    let middle = rest.len().saturating_sub(5) / 2;
    let candidate = rest.get(2..2 + middle).unwrap_or(rest);
    unquote(candidate)
}

fn side(rest: &str, prefix: &str) -> Option<String> {
    let name = rest.split('\t').next().unwrap_or(rest);
    if name == DEV_NULL {
        return None;
    }
    Some(
        unquote(name)
            .strip_prefix(prefix)
            .unwrap_or(name)
            .to_owned(),
    )
}

fn range(spec: &str, sign: char) -> Option<usize> {
    let body = spec.strip_prefix(sign)?;
    body.split(',').next()?.parse().ok()
}

fn hunk_header(line: &str) -> Result<ChangeHunk, &'static str> {
    let body = line.strip_prefix("@@ ").ok_or(MALFORMED_HUNK)?;
    let mut parts = body.split(' ');
    let old_start = parts
        .next()
        .and_then(|part| range(part, '-'))
        .ok_or(MALFORMED_HUNK)?;
    let new_start = parts
        .next()
        .and_then(|part| range(part, '+'))
        .ok_or(MALFORMED_HUNK)?;
    Ok(ChangeHunk {
        old_start,
        new_start,
        lines: Vec::new(),
    })
}

pub(super) fn parse_patch(output: &[u8]) -> Result<Vec<ChangedFile>, &'static str> {
    let mut files: Vec<Draft> = Vec::new();
    let patch = String::from_utf8_lossy(output);
    for line in patch.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            files.push(Draft {
                path: header_path(rest),
                status: ChangeStatus::Modified,
                hunks: Vec::new(),
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if let Some(hunk) = file.hunks.last_mut() {
            match line.chars().next() {
                Some(' ') => hunk.lines.push(ChangeLine::Context(text(&line[1..]))),
                Some('+') => hunk.lines.push(ChangeLine::Added(text(&line[1..]))),
                Some('-') => hunk.lines.push(ChangeLine::Removed(text(&line[1..]))),
                Some('@') => file.hunks.push(hunk_header(line)?),
                _ => {}
            }
            continue;
        }
        if line.starts_with("@@ ") {
            file.hunks.push(hunk_header(line)?);
        } else if line.starts_with("new file mode") {
            file.status = ChangeStatus::Added;
        } else if line.starts_with("deleted file mode") {
            file.status = ChangeStatus::Deleted;
        } else if let Some(from) = line.strip_prefix("rename from ") {
            file.status = ChangeStatus::Renamed {
                from: text(&unquote(from)),
            };
        } else if let Some(to) = line.strip_prefix("rename to ") {
            file.path = unquote(to);
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            if let Some(path) = side(rest, "b/") {
                file.path = path;
            }
        } else if let Some(rest) = line.strip_prefix("--- ")
            && file.status == ChangeStatus::Deleted
            && let Some(path) = side(rest, "a/")
        {
            file.path = path;
        }
    }
    Ok(files
        .into_iter()
        .map(|draft| ChangedFile {
            path: text(&draft.path),
            status: draft.status,
            hunks: draft.hunks,
        })
        .collect())
}

pub(super) fn added_file(path: &str, content: Option<&[u8]>) -> ChangedFile {
    let lines: Option<Vec<ChangeLine>> = content
        .filter(|bytes| !bytes.contains(&0))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(|body| {
            body.strip_suffix('\n')
                .unwrap_or(body)
                .split('\n')
                .filter(|_| !body.is_empty())
                .map(|line| ChangeLine::Added(text(line)))
                .collect()
        });
    let hunks = match lines {
        Some(lines) if !lines.is_empty() => vec![ChangeHunk {
            old_start: 0,
            new_start: 1,
            lines,
        }],
        _ => Vec::new(),
    };
    ChangedFile {
        path: text(path),
        status: ChangeStatus::Added,
        hunks,
    }
}
