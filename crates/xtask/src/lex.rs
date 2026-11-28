// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug)]
pub(crate) struct Cursor {
    chars: Vec<char>,
    pos: usize,
    line: u32,
}

impl Cursor {
    pub(crate) fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
            line: 1,
        }
    }

    pub(crate) fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    pub(crate) fn skip(&mut self, count: usize) {
        for _ in 0..count {
            self.bump();
        }
    }

    pub(crate) fn run_of(&self, expected: char) -> usize {
        let mut count = 0;
        while self.peek(count) == Some(expected) {
            count += 1;
        }
        count
    }

    pub(crate) fn line(&self) -> u32 {
        self.line
    }

    pub(crate) fn rest_of_line(&mut self) -> String {
        let start = self.pos;
        while self.peek(0).is_some_and(|c| c != '\n') {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Lexed {
    pub(crate) tokens: Vec<Token>,
    pub(crate) comments: Vec<Comment>,
}

#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) kind: Tok,
    pub(crate) line: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tok {
    Ident(String),
    Str(String),
    Number(String),
    Punct(char),
    Other,
}

#[derive(Debug)]
pub(crate) struct Comment {
    pub(crate) line: u32,
    pub(crate) text: String,
    pub(crate) tokens_before: usize,
}

impl Token {
    pub(crate) fn ident(&self) -> Option<&str> {
        match &self.kind {
            Tok::Ident(name) => Some(name),
            _ => None,
        }
    }

    pub(crate) fn number(&self) -> Option<&str> {
        match &self.kind {
            Tok::Number(digits) => Some(digits),
            _ => None,
        }
    }

    pub(crate) fn string(&self) -> Option<&str> {
        match &self.kind {
            Tok::Str(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn is_punct(&self, expected: char) -> bool {
        self.kind == Tok::Punct(expected)
    }

    pub(crate) fn is_ident(&self, expected: &str) -> bool {
        self.ident() == Some(expected)
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

pub(crate) fn lex_rust(text: &str) -> Lexed {
    let mut lexer = Lexer {
        cur: Cursor::new(text),
        out: Lexed::default(),
    };
    lexer.run();
    lexer.out
}

struct Lexer {
    cur: Cursor,
    out: Lexed,
}

impl Lexer {
    fn run(&mut self) {
        while let Some(c) = self.cur.peek(0) {
            let line = self.cur.line();
            match c {
                '/' if self.cur.peek(1) == Some('/') => self.line_comment(line),
                '/' if self.cur.peek(1) == Some('*') => self.block_comment(line),
                '"' => {
                    self.cur.bump();
                    let content = self.cooked_string();
                    self.push(Tok::Str(content), line);
                }
                '\'' => self.quote(line),
                c if is_ident_start(c) => self.word(line),
                c if c.is_ascii_digit() => {
                    let start = self.cur.pos;
                    while self.cur.peek(0).is_some_and(is_ident_continue) {
                        self.cur.bump();
                    }
                    let digits = self.cur.slice(start, self.cur.pos);
                    self.push(Tok::Number(digits), line);
                }
                c if c.is_whitespace() => {
                    self.cur.bump();
                }
                c => {
                    self.cur.bump();
                    self.push(Tok::Punct(c), line);
                }
            }
        }
    }

    fn push(&mut self, kind: Tok, line: u32) {
        self.out.tokens.push(Token { kind, line });
    }

    fn line_comment(&mut self, line: u32) {
        let text = self.cur.rest_of_line();
        self.out.comments.push(Comment {
            line,
            text,
            tokens_before: self.out.tokens.len(),
        });
    }

    fn block_comment(&mut self, line: u32) {
        let start = self.cur.pos;
        self.cur.skip(2);
        let mut depth = 1u32;
        while depth > 0 {
            match (self.cur.peek(0), self.cur.peek(1)) {
                (None, _) => break,
                (Some('/'), Some('*')) => {
                    depth += 1;
                    self.cur.skip(2);
                }
                (Some('*'), Some('/')) => {
                    depth -= 1;
                    self.cur.skip(2);
                }
                _ => {
                    self.cur.bump();
                }
            }
        }
        let text = self.cur.slice(start, self.cur.pos);
        self.out.comments.push(Comment {
            line,
            text,
            tokens_before: self.out.tokens.len(),
        });
    }

    fn cooked_string(&mut self) -> String {
        let start = self.cur.pos;
        loop {
            match self.cur.peek(0) {
                None => return self.cur.slice(start, self.cur.pos),
                Some('"') => {
                    let content = self.cur.slice(start, self.cur.pos);
                    self.cur.bump();
                    return content;
                }
                Some('\\') => self.cur.skip(2),
                Some(_) => {
                    self.cur.bump();
                }
            }
        }
    }

    fn raw_string(&mut self) -> Option<String> {
        let hashes = self.cur.run_of('#');
        if self.cur.peek(hashes) != Some('"') {
            return None;
        }
        self.cur.skip(hashes + 1);
        let start = self.cur.pos;
        while let Some(c) = self.cur.peek(0) {
            if c == '"' && (1..=hashes).all(|k| self.cur.peek(k) == Some('#')) {
                let content = self.cur.slice(start, self.cur.pos);
                self.cur.skip(hashes + 1);
                return Some(content);
            }
            self.cur.bump();
        }
        Some(self.cur.slice(start, self.cur.pos))
    }

    fn quote(&mut self, line: u32) {
        match (self.cur.peek(1), self.cur.peek(2)) {
            (Some('\\'), _) => {
                self.cur.skip(3);
                while let Some(c) = self.cur.bump() {
                    if c == '\'' {
                        break;
                    }
                }
            }
            (Some(c), Some('\'')) if c != '\'' => self.cur.skip(3),
            _ => {
                self.cur.bump();
                while self.cur.peek(0).is_some_and(is_ident_continue) {
                    self.cur.bump();
                }
            }
        }
        self.push(Tok::Other, line);
    }

    fn word(&mut self, line: u32) {
        let start = self.cur.pos;
        while self.cur.peek(0).is_some_and(is_ident_continue) {
            self.cur.bump();
        }
        let word = self.cur.slice(start, self.cur.pos);
        let next = self.cur.peek(0);
        match (word.as_str(), next) {
            ("b" | "c", Some('"')) => {
                self.cur.bump();
                let content = self.cooked_string();
                self.push(Tok::Str(content), line);
            }
            ("b", Some('\'')) => self.quote(line),
            ("r" | "br" | "cr", Some('"' | '#')) => {
                if let Some(content) = self.raw_string() {
                    self.push(Tok::Str(content), line);
                } else if word == "r" && self.cur.peek(1).is_some_and(is_ident_start) {
                    self.cur.bump();
                    let raw_start = self.cur.pos;
                    while self.cur.peek(0).is_some_and(is_ident_continue) {
                        self.cur.bump();
                    }
                    let name = self.cur.slice(raw_start, self.cur.pos);
                    self.push(Tok::Ident(name), line);
                } else {
                    self.push(Tok::Ident(word), line);
                }
            }
            _ => self.push(Tok::Ident(word), line),
        }
    }
}
