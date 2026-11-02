// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::plain;
use thiserror::Error;

const CONSTRUCT: &str = "construct";

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

struct Reader<'a> {
    lines: Vec<&'a str>,
    next: usize,
    escape: char,
}

pub fn validate_dockerfile(text: &str) -> Result<(), DockerfileError> {
    let mut reader = Reader::new(text);
    let mut stages = Vec::new();
    while let Some(instruction) = reader.read() {
        if let Some(stage) = parse_from(&instruction)? {
            stages.push(stage);
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
    Ok(())
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

fn directive(line: &str) -> Option<(&str, &str)> {
    let comment = trim_leading(line).strip_prefix('#')?;
    let (key, value) = trim_leading(comment).split_once('=')?;
    let key = key.trim_matches(is_blank);
    let value = value.trim_matches(is_blank);
    let valid_key = key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric());
    (valid_key && !value.is_empty()).then_some((key, value))
}

fn escape_character(lines: &[&str]) -> char {
    for line in lines {
        let Some((key, value)) = directive(line) else {
            break;
        };
        let known = ["syntax", "escape", "check"]
            .iter()
            .any(|name| key.eq_ignore_ascii_case(name));
        if !known {
            break;
        }
        if key.eq_ignore_ascii_case("escape") {
            return match value {
                "`" => '`',
                _ => '\\',
            };
        }
    }
    '\\'
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
            let Some(body) = physical
                .trim_end_matches(is_blank)
                .strip_suffix(self.escape)
            else {
                text.push_str(physical);
                break;
            };
            text.push_str(body);
            self.skip_noise();
        }
        self.skip_heredocs(&text);
        Some(Instruction { line, text })
    }

    fn skip_heredocs(&mut self, text: &str) {
        let takes_heredoc = words(text).next().is_some_and(|word| {
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
