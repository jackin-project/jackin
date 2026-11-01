// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::process::{Command, Stdio};

use jackin_core::{EnvFile, EnvFileError, EnvName};

type TestResult = Result<(), Box<dyn Error>>;

const CORPUS: [&str; 22] = [
    "",
    "plain",
    "it's",
    "'",
    "''",
    "a b",
    "$HOME",
    "`x`",
    "$(x)",
    "\\",
    "\\'",
    "\"",
    "\u{e9}\u{20ac} unicode",
    "tab\tin",
    "= sign",
    "trailing ",
    " leading",
    "x;y&z|w",
    "#not comment",
    "multi'quote'es",
    "cr\rin",
    "ends\r",
];

fn name(text: &str) -> Result<EnvName, Box<dyn Error>> {
    Ok(EnvName::parse(text)?)
}

fn file(pairs: &[(&str, &str)]) -> Result<EnvFile, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    for (key, value) in pairs {
        values.insert(name(key)?, (*value).to_owned());
    }
    Ok(EnvFile::new(values)?)
}

fn corpus_file() -> Result<EnvFile, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    for (index, value) in CORPUS.iter().enumerate() {
        values.insert(name(&format!("V{index}"))?, (*value).to_owned());
    }
    Ok(EnvFile::new(values)?)
}

#[test]
fn render_writes_sorted_quoted_export_lines_and_parse_reads_them_back() -> TestResult {
    let sample = file(&[("B", "it's"), ("A", "x y"), ("E", "")])?;
    assert_eq!(
        sample.render(),
        "export A='x y'\nexport B='it'\\''s'\nexport E=''\n"
    );
    assert_eq!(file(&[])?.render(), "");
    for value in CORPUS {
        let single = file(&[("A", value)])?;
        let rendered = single.render();
        let parsed = EnvFile::parse(&rendered)?;
        assert_eq!(parsed, single, "{value:?}");
        assert_eq!(parsed.render(), rendered);
    }
    let all = corpus_file()?;
    assert_eq!(EnvFile::parse(&all.render())?, all);
    Ok(())
}

#[test]
fn a_posix_shell_reads_back_every_value() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("env");
    std::fs::write(&path, corpus_file()?.render())?;
    let names: Vec<String> = (0..CORPUS.len()).map(|i| format!("\"$V{i}\"")).collect();
    let script = format!(". \"$1\"; printf '%s\\0' {}", names.join(" "));
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(&path)
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    assert!(output.status.success());
    let mut fields: Vec<&[u8]> = output.stdout.split(|b| *b == 0).collect();
    assert_eq!(fields.pop(), Some(&b""[..]));
    let mut read = Vec::new();
    for field in fields {
        read.push(String::from_utf8(field.to_vec())?);
    }
    assert_eq!(read, CORPUS);
    Ok(())
}

#[test]
fn invalid_values_and_malformed_files_are_rejected_with_name_or_line() -> TestResult {
    let both = BTreeMap::from([
        (name("A")?, "ok".to_owned()),
        (name("B")?, "x\ny".to_owned()),
    ]);
    match EnvFile::new(both) {
        Err(EnvFileError::InvalidValue { name: got }) => assert_eq!(got, name("B")?),
        other => panic!("{other:?}"),
    }
    let nul = BTreeMap::from([(name("A")?, "x\0y".to_owned())]);
    match EnvFile::new(nul) {
        Err(EnvFileError::InvalidValue { name: got }) => assert_eq!(got, name("A")?),
        other => panic!("{other:?}"),
    }
    let cases = [
        ("A='x'\n", 1),
        ("export A=x\n", 1),
        ("export A=\"x\"\n", 1),
        ("export 1A='x'\n", 1),
        ("export A='x' extra\n", 1),
        ("export A='x'\nexport B='unterminated", 2),
        ("export A='1'\nexport A='2'\n", 2),
        ("export A='x'\n\nexport B='y'\n", 2),
    ];
    for (text, line) in cases {
        match EnvFile::parse(text) {
            Err(EnvFileError::Syntax { line: got }) => assert_eq!(got, line, "{text:?}"),
            other => panic!("{text:?}: {other:?}"),
        }
    }
    Ok(())
}
