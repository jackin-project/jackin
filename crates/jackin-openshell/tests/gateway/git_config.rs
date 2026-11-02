// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{ProcessStatus, Program};
use jackin_openshell::{
    GatewayError, GitConfigEntry, TransferOp, sanitize_entries, sanitized_git_config,
};
use tempfile::TempDir;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::fakes::{ScriptedRunner, exited};
use super::harness::host;

type Listing = Vec<(String, Option<String>)>;

async fn listing(path: &Path) -> Result<Listing, String> {
    let output = Command::new("git")
        .args(["config", "-f"])
        .arg(path)
        .args(["--list", "-z"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .await
        .unwrap();
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let text = String::from_utf8(output.stdout).unwrap();
    Ok(text
        .split_terminator('\0')
        .map(|item| match item.split_once('\n') {
            Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
            None => (item.to_owned(), None),
        })
        .collect())
}

fn entries(items: &[(&str, Option<&str>)]) -> Listing {
    items
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.map(str::to_owned)))
        .collect()
}

fn userinfo_in_authority(value: &str) -> bool {
    value.match_indices("://").any(|(index, _)| {
        let rest = &value[index + 3..];
        let end = rest
            .find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'') || c.is_whitespace())
            .unwrap_or(rest.len());
        rest[..end].contains('@')
    })
}

fn forbidden_key(key: &str) -> bool {
    let (section, rest) = key.split_once('.').unwrap_or((key, ""));
    let name = rest.rsplit_once('.').map_or(rest, |(_, name)| name);
    matches!(section, "credential" | "include" | "includeif")
        || (section == "http" && name == "extraheader")
}

struct Case {
    name: &'static str,
    input: &'static str,
    kept: Vec<(&'static str, Option<&'static str>)>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "credential sections in upper and mixed case",
            input: "[CREDENTIAL \"https://x\"]\n\thelper = store --file=SECRET-1\n[Credential]\n\tusername = SECRET-2\n[core]\n\tbare = false\n",
            kept: vec![("core.bare", Some("false"))],
        },
        Case {
            name: "old dotted headers",
            input: "[credential.example]\n\thelper = SECRET-3\n[http.example.com]\n\textraheader = Authorization: SECRET-4\n[core]\n\tfilemode = true\n",
            kept: vec![("core.filemode", Some("true"))],
        },
        Case {
            name: "a key on the header line",
            input: "[remote \"origin\"] url = https://SECRET-5:x@h.example/o/r.git\n",
            kept: vec![("remote.origin.url", Some("https://h.example/o/r.git"))],
        },
        Case {
            name: "a continuation line",
            input: "[http]\n\textraHeader = Authorization: \\\n\t  Bearer SECRET-6\n[core]\n\tbare = false\n",
            kept: vec![("core.bare", Some("false"))],
        },
        Case {
            name: "user information in a subsection name",
            input: "[url \"https://SECRET-7@github.com/\"]\n\tinsteadOf = gh:\n[core]\n\tbare = false\n",
            kept: vec![("core.bare", Some("false"))],
        },
        Case {
            name: "proxy and push URLs",
            input: "[http]\n\tproxy = http://u:SECRET-8@proxy.example:3128\n[remote \"o\"]\n\tpushurl = ssh://git:SECRET-9@h.example/r.git\n",
            kept: vec![
                ("http.proxy", Some("http://proxy.example:3128")),
                ("remote.o.pushurl", Some("ssh://h.example/r.git")),
            ],
        },
        Case {
            name: "include sections",
            input: "[include]\n\tpath = ../SECRET-10.cfg\n[includeIf \"gitdir:~/work/\"]\n\tpath = SECRET-11.cfg\n[core]\n\tbare = false\n",
            kept: vec![("core.bare", Some("false"))],
        },
        Case {
            name: "quoted values with comment characters",
            input: "[alias]\n\tx = \"log # not a comment ; either\"\n[user]\n\tname = \"a;b#c\"\n",
            kept: vec![
                ("alias.x", Some("log # not a comment ; either")),
                ("user.name", Some("a;b#c")),
            ],
        },
        Case {
            name: "CRLF line ends",
            input: "[core]\r\n\tbare = false\r\n[credential]\r\n\thelper = SECRET-12\r\n",
            kept: vec![("core.bare", Some("false"))],
        },
        Case {
            name: "names that only look like removed sections",
            input: "[credentials]\n\thelper = x\n[includes]\n\tpath = y\n[user]\n\temail = me@example.com\n[remote \"origin\"]\n\turl = git@github.com:o/r.git\n[remote \"web\"]\n\turl = https://example.com/path@v1\n",
            kept: vec![
                ("credentials.helper", Some("x")),
                ("includes.path", Some("y")),
                ("user.email", Some("me@example.com")),
                ("remote.origin.url", Some("git@github.com:o/r.git")),
                ("remote.web.url", Some("https://example.com/path@v1")),
            ],
        },
        Case {
            name: "a clean config",
            input: "[core]\n\trepositoryformatversion = 0\n\tbare = false\n[remote \"origin\"]\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\tfetch = +refs/tags/*:refs/tags/*\n[branch \"main\"]\n\tmerge = refs/heads/main\n[url \"https://github.com/\"]\n\tinsteadOf = gh:\n",
            kept: vec![
                ("core.repositoryformatversion", Some("0")),
                ("core.bare", Some("false")),
                (
                    "remote.origin.fetch",
                    Some("+refs/heads/*:refs/remotes/origin/*"),
                ),
                ("remote.origin.fetch", Some("+refs/tags/*:refs/tags/*")),
                ("branch.main.merge", Some("refs/heads/main")),
                ("url.https://github.com/.insteadof", Some("gh:")),
            ],
        },
        Case {
            name: "escapes, a bare key and an empty value",
            input: "[alias]\n\ty = \"a\\\\b \\\"q\\\" \\t end\"\n[core]\n\tbare\n\teditor =\n",
            kept: vec![
                ("alias.y", Some("a\\b \"q\" \t end")),
                ("core.bare", None),
                ("core.editor", Some("")),
            ],
        },
        Case {
            name: "an at sign inside the password",
            input: "[remote \"o\"]\n\turl = https://SECRET-15:p@ss@h.example/x\n",
            kept: vec![("remote.o.url", Some("https://h.example/x"))],
        },
        Case {
            name: "a URL inside a longer value",
            input: "[alias]\n\tfetchit = !curl -s https://SECRET-13:pw@h.example/x -o out\n",
            kept: vec![("alias.fetchit", Some("!curl -s https://h.example/x -o out"))],
        },
    ]
}

#[tokio::test]
async fn the_sanitized_config_follows_git_parsing_and_keeps_everything_else() {
    let cancel = CancellationToken::new();
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    for case in cases() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("in.cfg");
        std::fs::write(&source, case.input).unwrap();
        listing(&source)
            .await
            .unwrap_or_else(|error| panic!("{}: the input is not valid: {error}", case.name));

        let text = sanitized_git_config(&runner, &host(source.to_str().unwrap()), &cancel)
            .await
            .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        let out = dir.path().join("out.cfg");
        std::fs::write(&out, &text).unwrap();

        let result = listing(&out)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert!(!text.contains("SECRET"), "{}: {text}", case.name);
        for (key, value) in &result {
            assert!(!forbidden_key(key), "{}: {key}", case.name);
            assert!(
                !value.as_deref().is_some_and(userinfo_in_authority),
                "{}: {key}",
                case.name
            );
        }
        assert_eq!(result, entries(&case.kept), "{}", case.name);
    }
}

#[tokio::test]
async fn a_config_git_cannot_parse_yields_an_error_and_no_text() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("in.cfg");
    std::fs::write(&source, "[remote \"o]\n\turl = SECRET-14\n").unwrap();
    let runner = ScriptedRunner::always(exited(0, b"", b""));
    let error = sanitized_git_config(
        &runner,
        &host(source.to_str().unwrap()),
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    match error {
        GatewayError::Transfer { op, status, tail } => {
            assert_eq!(
                (op, status),
                (TransferOp::GitConfig, ProcessStatus::Exited(128))
            );
            assert!(!tail.as_str().contains("SECRET"));
        }
        other => panic!("{other:?}"),
    }
    let missing = sanitized_git_config(
        &runner,
        &host(dir.path().join("absent.cfg").to_str().unwrap()),
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            missing,
            GatewayError::Transfer {
                op: TransferOp::GitConfig,
                ..
            }
        ),
        "{missing:?}"
    );
    let calls = runner.git_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].program, Program::Git);
}

#[test]
fn the_pure_filter_drops_secret_entries_and_rewrites_nothing_else() {
    let entry = |key: &str, value: Option<&str>| GitConfigEntry {
        key: key.to_owned(),
        value: value.map(str::to_owned),
    };
    let kept = sanitize_entries(vec![
        entry("credential.helper", Some("store")),
        entry("http.https://x/.extraheader", Some("A: b")),
        entry("include.path", Some("p")),
        entry("includeif.gitdir:/a/.path", Some("p")),
        entry("url.https://SECRET@h/.insteadof", Some("gh:")),
        entry("remote.origin.url", Some("https://SECRET@h/o/r.git")),
        entry("core.bare", None),
        entry("user.email", Some("me@example.com")),
    ]);
    assert_eq!(
        kept,
        vec![
            entry("remote.origin.url", Some("https://h/o/r.git")),
            entry("core.bare", None),
            entry("user.email", Some("me@example.com")),
        ]
    );
}
