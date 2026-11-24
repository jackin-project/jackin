// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const WIDE: u16 = 80;
pub const MINIMUM: u16 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Strong,
    Dim,
    Ok,
    Warn,
    Crit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line(pub Vec<Span>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub width: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Summary,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    Usage,
    Snapshot,
    Details,
}

impl Span {
    pub fn new(text: impl Into<String>, tone: Tone) -> Span {
        Span {
            text: text.into(),
            tone,
        }
    }

    pub fn plain(text: impl Into<String>) -> Span {
        Span::new(text, Tone::Plain)
    }

    pub fn dim(text: impl Into<String>) -> Span {
        Span::new(text, Tone::Dim)
    }

    pub fn strong(text: impl Into<String>) -> Span {
        Span::new(text, Tone::Strong)
    }

    pub fn width(&self) -> usize {
        self.text.chars().count()
    }
}

impl Line {
    pub fn blank() -> Line {
        Line(Vec::new())
    }

    pub fn of(spans: Vec<Span>) -> Line {
        Line(
            spans
                .into_iter()
                .filter(|span| !span.text.is_empty())
                .collect(),
        )
    }

    pub fn width(&self) -> usize {
        self.0.iter().map(Span::width).sum()
    }

    pub fn text(&self) -> String {
        self.0.iter().map(|span| span.text.as_str()).collect()
    }
}

impl Ctx {
    pub fn new(width: u16) -> Ctx {
        Ctx { width }
    }

    pub fn columns(&self) -> usize {
        usize::from(self.width.max(MINIMUM))
    }

    pub fn narrow(&self) -> bool {
        self.width < WIDE
    }
}

fn join(lines: &[Line], tag: impl Fn(&Span) -> String) -> String {
    let mut out = String::new();
    for line in lines {
        for span in &line.0 {
            out.push_str(&tag(span));
        }
        out.push('\n');
    }
    out
}

pub fn plain(lines: &[Line]) -> String {
    join(lines, |span| span.text.clone())
}

pub fn ansi(lines: &[Line]) -> String {
    join(lines, |span| match span.tone {
        Tone::Plain => span.text.clone(),
        Tone::Strong => format!("\u{1b}[1m{}\u{1b}[0m", span.text),
        Tone::Dim => format!("\u{1b}[2m{}\u{1b}[0m", span.text),
        Tone::Ok => format!("\u{1b}[32m{}\u{1b}[0m", span.text),
        Tone::Warn => format!("\u{1b}[33m{}\u{1b}[0m", span.text),
        Tone::Crit => format!("\u{1b}[31m{}\u{1b}[0m", span.text),
    })
}

pub fn markup(lines: &[Line]) -> String {
    join(lines, |span| match span.tone {
        Tone::Plain => span.text.clone(),
        Tone::Strong => format!("[bold]{}[/]", span.text),
        Tone::Dim => format!("[dim]{}[/]", span.text),
        Tone::Ok => format!("[green]{}[/]", span.text),
        Tone::Warn => format!("[yellow]{}[/]", span.text),
        Tone::Crit => format!("[red]{}[/]", span.text),
    })
}
