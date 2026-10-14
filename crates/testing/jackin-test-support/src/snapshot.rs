#[cfg(test)]
mod tests;

#[must_use]
pub fn redact_digit_runs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut run = String::new();
            run.push(c);
            while chars.peek().is_some_and(char::is_ascii_digit) {
                if let Some(d) = chars.next() {
                    run.push(d);
                }
            }
            if run.len() >= 4 {
                out.push_str("<digits>");
            } else {
                out.push_str(&run);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[must_use]
pub fn normalize_snapshot_text(input: &str) -> String {
    let lines: Vec<String> = input
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].is_empty() {
        end -= 1;
    }
    let mut out = lines[..end].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}
