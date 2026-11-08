// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

use jackin_core::{ProcessError, ProcessStdin, Program, SecretBytes, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DockerfileFacts {
    pub(super) stages: Vec<DockerfileStage>,
    pub(super) env: Vec<String>,
    pub(super) on_build: Vec<String>,
    args: BTreeMap<String, String>,
}

impl Debug for DockerfileFacts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockerfileFacts")
            .field("stage_count", &self.stages.len())
            .field("env_count", &self.env.len())
            .field("on_build_count", &self.on_build.len())
            .field("arg_count", &self.args.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DockerfileStage {
    pub(super) base: String,
    pub(super) name: Option<String>,
}

impl Debug for DockerfileStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockerfileStage")
            .field("has_name", &self.name.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) struct StdinCapture {
    pub(super) digest: Sha256,
    pub(super) dockerfile: DockerfileFacts,
}

pub(super) async fn capture(
    stdin: &ProcessStdin,
    args: &[String],
    program: Program,
) -> Result<Option<StdinCapture>, ProcessError> {
    match stdin {
        ProcessStdin::Null => Ok(None),
        ProcessStdin::Bytes(bytes) => summarize(bytes, args, program).map(Some),
        ProcessStdin::File { path, offset } => {
            let mut file = tokio::fs::File::open(path.as_path())
                .await
                .map_err(|_| unsupported(program))?;
            file.seek(SeekFrom::Start(*offset))
                .await
                .map_err(|_| unsupported(program))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .await
                .map_err(|_| unsupported(program))?;
            summarize(&SecretBytes::new(bytes), args, program).map(Some)
        }
    }
}

pub(super) fn from_context(
    bytes: &[u8],
    args: &[String],
    program: Program,
) -> Result<DockerfileFacts, ProcessError> {
    parse(bytes, args, program)
}

impl DockerfileFacts {
    pub(super) fn expanded_base(&self, base: &str) -> Option<String> {
        let mut expanded = String::new();
        let mut remaining = base;
        while let Some(start) = remaining.find('$') {
            expanded.push_str(&remaining[..start]);
            remaining = &remaining[start + 1..];
            let (name, rest) = if let Some(braced) = remaining.strip_prefix('{') {
                let end = braced.find('}')?;
                (&braced[..end], &braced[end + 1..])
            } else {
                let end = remaining
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(remaining.len());
                if end == 0 {
                    return None;
                }
                (&remaining[..end], &remaining[end..])
            };
            expanded.push_str(self.args.get(name)?);
            remaining = rest;
        }
        expanded.push_str(remaining);
        Some(expanded)
    }
}

fn summarize(
    bytes: &SecretBytes,
    args: &[String],
    program: Program,
) -> Result<StdinCapture, ProcessError> {
    let content = bytes.expose();
    Ok(StdinCapture {
        digest: Sha256::of(content),
        dockerfile: parse(content, args, program)?,
    })
}

fn parse(bytes: &[u8], args: &[String], program: Program) -> Result<DockerfileFacts, ProcessError> {
    let text = std::str::from_utf8(bytes).map_err(|_| unsupported(program))?;
    let mut build_args = BTreeMap::new();
    for argument in args {
        if let Some(value) = argument.strip_prefix("--build-arg=")
            && let Some((name, value)) = value.split_once('=')
        {
            build_args.insert(name.to_owned(), value.to_owned());
        }
    }
    let mut defaults = BTreeMap::new();
    let mut stages = Vec::new();
    let mut env = Vec::new();
    let mut on_build = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_ascii_whitespace();
        let Some(instruction) = words.next() else {
            continue;
        };
        if instruction.eq_ignore_ascii_case("FROM") {
            let words: Vec<&str> = words.collect();
            let flags = words
                .iter()
                .take_while(|word| word.starts_with("--"))
                .count();
            let stage = match &words[flags..] {
                [base] => DockerfileStage {
                    base: (*base).to_owned(),
                    name: None,
                },
                [base, as_keyword, name] if as_keyword.eq_ignore_ascii_case("AS") => {
                    DockerfileStage {
                        base: (*base).to_owned(),
                        name: Some((*name).to_owned()),
                    }
                }
                _ => return Err(unsupported(program)),
            };
            stages.push(stage);
        } else if instruction.eq_ignore_ascii_case("ARG") && stages.is_empty() {
            if let Some((name, value)) = words.next().and_then(|word| word.split_once('=')) {
                defaults.insert(name.to_owned(), value.to_owned());
            }
        } else if instruction.eq_ignore_ascii_case("ENV") {
            if let Some(value) = line
                .strip_prefix("ENV ")
                .or_else(|| line.strip_prefix("env "))
            {
                env.push(value.to_owned());
            }
        } else if instruction.eq_ignore_ascii_case("ONBUILD")
            && let Some(value) = line
                .strip_prefix("ONBUILD ")
                .or_else(|| line.strip_prefix("onbuild "))
        {
            on_build.push(value.to_owned());
        }
    }
    for (name, value) in defaults {
        build_args.entry(name).or_insert(value);
    }
    if stages.is_empty() {
        return Err(unsupported(program));
    }
    Ok(DockerfileFacts {
        stages,
        env,
        on_build,
        args: build_args,
    })
}

fn unsupported(program: Program) -> ProcessError {
    ProcessError::Spawn {
        program,
        message: "unsupported Dockerfile input".to_owned(),
    }
}
