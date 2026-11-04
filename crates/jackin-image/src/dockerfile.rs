// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{EnvName, plain};
use thiserror::Error;

const CONSTRUCT: &str = "construct";
const FRONTENDS: [&str; 2] = ["docker/dockerfile", "docker.io/docker/dockerfile"];
const DIRECTIVE_KEYS: [&str; 3] = ["syntax", "escape", "check"];
const DIRECTIVE_MARKERS: [&str; 2] = ["#", "//"];

#[derive(Debug, Error)]
pub enum DockerfileError {
    #[error("the Dockerfile has no stages")]
    NoStages,
    #[error("line {line}: the final stage must start FROM construct, found {}", plain(.found))]
    FinalStageNotConstruct { line: usize, found: String },
    #[error("line {line}: a stage must not be named construct")]
    ConstructStageDefined { line: usize },
    #[error("line {line}: cannot read the FROM instruction {}", plain(.text))]
    UnreadableFrom { line: usize, text: String },
    #[error("line {line}: ENV sets {}, which names the Docker engine client; jackin sets it from the Docker capability", plain(.name))]
    EngineClientEnv { line: usize, name: String },
    #[error("line {line}: the ENV name {} expands a variable, so it cannot be checked", plain(.name))]
    ExpandingEnvName { line: usize, name: String },
    #[error("line {line}: the syntax directive names the frontend {}, but a role Dockerfile may name only the docker/dockerfile frontend", plain(.frontend))]
    ForeignFrontend { line: usize, frontend: String },
    #[error("the Dockerfile is not valid UTF-8")]
    NotUtf8,
}

struct Stage {
    line: usize,
    image: String,
    name: Option<String>,
}

struct Instruction {
    line: usize,
    text: String,
}

struct Heredoc {
    word: String,
    chomp: bool,
}

struct Directive<'a> {
    line: usize,
    key: &'a str,
    value: &'a str,
}

struct Reader<'a> {
    lines: Vec<&'a str>,
    next: usize,
    escape: char,
}

pub fn validate_dockerfile(text: &str) -> Result<(), DockerfileError> {
    let mut reader = Reader::new(text);
    if let Some(problem) = foreign_frontend(&reader.lines) {
        return Err(problem);
    }
    let mut stages = Vec::new();
    let mut final_env = None;
    while let Some(instruction) = reader.read() {
        if let Some(stage) = parse_from(&instruction)? {
            stages.push(stage);
            final_env = None;
        } else if final_env.is_none() {
            final_env = env_problem(&instruction);
        }
    }
    let Some(last) = stages.last() else {
        return Err(DockerfileError::NoStages);
    };
    if let Some(stage) = stages.iter().find(|stage| {
        stage
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(CONSTRUCT))
    }) {
        return Err(DockerfileError::ConstructStageDefined { line: stage.line });
    }
    if last.image != CONSTRUCT {
        return Err(DockerfileError::FinalStageNotConstruct {
            line: last.line,
            found: last.image.clone(),
        });
    }
    final_env.map_or(Ok(()), Err)
}

fn keyword_rest<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let text = text.trim_start_matches(is_blank);
    let head = text.get(..keyword.len())?;
    let rest = &text[keyword.len()..];
    (head.eq_ignore_ascii_case(keyword) && (rest.is_empty() || rest.starts_with(is_blank)))
        .then_some(rest)
}

fn deferred(text: &str) -> &str {
    let mut text = text.trim_start_matches(is_blank);
    while let Some(rest) = keyword_rest(text, "onbuild") {
        text = rest.trim_start_matches(is_blank);
    }
    text
}

fn env_problem(instruction: &Instruction) -> Option<DockerfileError> {
    let rest = keyword_rest(deferred(&instruction.text), "env")?;
    let line = instruction.line;
    for key in env_keys(rest) {
        if key.contains('$') {
            return Some(DockerfileError::ExpandingEnvName { line, name: key });
        }
        let name = unquote(&key);
        if EnvName::is_engine_client(&name) {
            return Some(DockerfileError::EngineClientEnv { line, name });
        }
    }
    None
}

fn env_keys(arguments: &str) -> Vec<String> {
    let words = split_shell_words(arguments);
    let keyed = words.first().is_some_and(|word| word.contains('='));
    let keys = words.iter().map(|word| match word.split_once('=') {
        Some((key, _)) => key.to_owned(),
        None => word.clone(),
    });
    if keyed {
        keys.collect()
    } else {
        keys.take(1).collect()
    }
}

fn is_blank(c: char) -> bool {
    c.is_ascii_whitespace() || c == '\x0b'
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_blank).filter(|word| !word.is_empty())
}

fn parse_from(instruction: &Instruction) -> Result<Option<Stage>, DockerfileError> {
    let mut parts = words(&instruction.text);
    if !parts
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("from"))
    {
        return Ok(None);
    }
    let unreadable = || DockerfileError::UnreadableFrom {
        line: instruction.line,
        text: instruction.text.clone(),
    };
    let rest: Vec<&str> = parts.collect();
    let flags = rest
        .iter()
        .take_while(|word| word.starts_with("--"))
        .count();
    if !rest[..flags].iter().all(|flag| is_flag(flag)) {
        return Err(unreadable());
    }
    let (image, name) = match &rest[flags..] {
        [image] => (*image, None),
        [image, keyword, name] if keyword.eq_ignore_ascii_case("as") => (*image, Some(*name)),
        _ => return Err(unreadable()),
    };
    Ok(Some(Stage {
        line: instruction.line,
        image: image.to_owned(),
        name: name.map(str::to_owned),
    }))
}

fn is_flag(word: &str) -> bool {
    word.strip_prefix("--")
        .and_then(|flag| flag.split_once('='))
        .is_some_and(|(name, value)| !name.is_empty() && !value.is_empty())
}

fn trim_leading(line: &str) -> &str {
    line.trim_start_matches(char::is_whitespace)
}

fn directive<'a>(line: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let comment = trim_leading(line).strip_prefix(marker)?;
    let (key, value) = trim_leading(comment).split_once('=')?;
    let key = key.trim_matches(is_blank);
    let value = value.trim_matches(is_blank);
    let valid_key = key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric());
    (valid_key && !value.is_empty()).then_some((key, value))
}

fn directive_block<'a>(lines: &[&'a str], marker: &str, first: usize) -> Vec<Directive<'a>> {
    lines
        .iter()
        .enumerate()
        .skip(first)
        .map_while(|(index, line)| {
            let (key, value) = directive(line, marker)?;
            DIRECTIVE_KEYS
                .iter()
                .any(|name| key.eq_ignore_ascii_case(name))
                .then_some(Directive {
                    line: index + 1,
                    key,
                    value,
                })
        })
        .collect()
}

fn escape_character(lines: &[&str]) -> char {
    let escape = directive_block(lines, "#", 0)
        .into_iter()
        .find(|directive| directive.key.eq_ignore_ascii_case("escape"));
    match escape {
        Some(Directive { value: "`", .. }) => '`',
        _ => '\\',
    }
}

fn foreign_frontend(lines: &[&str]) -> Option<DockerfileError> {
    let first = usize::from(lines.first().is_some_and(|line| line.starts_with("#!")));
    DIRECTIVE_MARKERS
        .iter()
        .flat_map(|marker| directive_block(lines, marker, first))
        .filter(|directive| directive.key.eq_ignore_ascii_case("syntax"))
        .find_map(|directive| {
            let frontend = words(directive.value).next()?;
            let repository = frontend.split([':', '@']).next()?;
            (!FRONTENDS.contains(&repository)).then(|| DockerfileError::ForeignFrontend {
                line: directive.line,
                frontend: frontend.to_owned(),
            })
        })
}

fn skippable(line: &str) -> bool {
    let trimmed = trim_leading(line);
    trimmed.is_empty() || trimmed.starts_with('#')
}

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Reader<'a> {
        let mut lines: Vec<&str> = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();
        if let Some(first) = lines.first_mut() {
            *first = first.strip_prefix('\u{feff}').unwrap_or(first);
        }
        let escape = escape_character(&lines);
        Reader {
            lines,
            next: 0,
            escape,
        }
    }

    fn skip_noise(&mut self) {
        while self
            .lines
            .get(self.next)
            .is_some_and(|line| skippable(line))
        {
            self.next += 1;
        }
    }

    fn read(&mut self) -> Option<Instruction> {
        self.skip_noise();
        if self.next >= self.lines.len() {
            return None;
        }
        let line = self.next + 1;
        let mut text = String::new();
        let mut leading = true;
        while let Some(physical) = self.lines.get(self.next) {
            self.next += 1;
            let physical = if std::mem::take(&mut leading) {
                trim_leading(physical)
            } else {
                physical
            };
            let Some(body) = self.continued(physical) else {
                text.push_str(physical);
                break;
            };
            text.push_str(body);
            self.skip_noise();
        }
        self.skip_heredocs(&text);
        Some(Instruction { line, text })
    }

    fn continued<'l>(&self, physical: &'l str) -> Option<&'l str> {
        let body = physical
            .trim_end_matches([' ', '\t'])
            .strip_suffix(self.escape)?;
        body.chars()
            .next_back()
            .filter(|before| *before != self.escape)
            .map(|_| body)
    }

    fn skip_heredocs(&mut self, text: &str) {
        let takes_heredoc = words(deferred(text)).next().is_some_and(|word| {
            ["run", "copy", "add"]
                .iter()
                .any(|name| word.eq_ignore_ascii_case(name))
        });
        if !takes_heredoc {
            return;
        }
        for heredoc in heredocs(text) {
            while let Some(body) = self.lines.get(self.next) {
                self.next += 1;
                let candidate = if heredoc.chomp {
                    body.trim_start_matches('\t')
                } else {
                    body
                };
                if candidate == heredoc.word {
                    break;
                }
            }
        }
    }
}

fn split_shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(open) => {
                current.push(c);
                if c == open {
                    quote = None;
                } else if c == '\\' && open == '"' {
                    current.extend(chars.next());
                }
            }
            None if is_blank(c) => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                started = true;
                current.push(c);
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == '\\' {
                    current.extend(chars.next());
                }
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

fn unquote(word: &str) -> String {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {}
            '\\' => out.extend(chars.next()),
            other => out.push(other),
        }
    }
    out
}

fn heredocs(arguments: &str) -> Vec<Heredoc> {
    split_shell_words(arguments)
        .iter()
        .filter_map(|word| {
            let rest = word
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .strip_prefix("<<")?;
            let (chomp, name) = match rest.strip_prefix('-') {
                Some(name) => (true, name),
                None => (false, rest),
            };
            if name.is_empty() || name.contains('<') {
                return None;
            }
            Some(Heredoc {
                word: unquote(name),
                chomp,
            })
        })
        .collect()
}
