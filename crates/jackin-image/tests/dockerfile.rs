// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, HostPath, Platform, RepoPath, RoleName,
};
use jackin_image::{
    DockerfileError, HookFiles, ImageError, ImagePlan, ImageRequest, Rebuild, RoleRevision,
    SourceFile, validate_dockerfile,
};

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Accepted,
    NoStages,
    NotConstruct(usize, String),
    ConstructStage(usize),
    Unreadable(usize),
    NotUtf8,
    Other,
}

#[derive(Debug)]
struct Row {
    label: &'static str,
    text: String,
    want: Verdict,
}

fn row(label: &'static str, text: &str, want: Verdict) -> Row {
    Row {
        label,
        text: text.to_owned(),
        want,
    }
}

fn not_construct(line: usize, found: &str) -> Verdict {
    Verdict::NotConstruct(line, found.to_owned())
}

fn classify(error: &DockerfileError) -> Verdict {
    match error {
        DockerfileError::NoStages => Verdict::NoStages,
        DockerfileError::FinalStageNotConstruct { line, found } => {
            Verdict::NotConstruct(*line, found.clone())
        }
        DockerfileError::ConstructStageDefined { line } => Verdict::ConstructStage(*line),
        DockerfileError::UnreadableFrom { line, .. } => Verdict::Unreadable(*line),
        DockerfileError::NotUtf8 => Verdict::NotUtf8,
    }
}

fn verdict_of(result: Result<(), DockerfileError>) -> Verdict {
    result.err().map_or(Verdict::Accepted, |e| classify(&e))
}

fn plan_verdict(result: Result<ImagePlan, ImageError>) -> Verdict {
    match result {
        Ok(_) => Verdict::Accepted,
        Err(ImageError::Dockerfile(error)) => classify(&error),
        Err(
            ImageError::MissingInstallTarget { .. }
            | ImageError::Engine(_)
            | ImageError::NotDelivered { .. }
            | ImageError::SourceMoved { .. }
            | ImageError::LockTimeout { .. }
            | ImageError::Cancelled,
        ) => Verdict::Other,
    }
}

fn check(rows: &[Row]) {
    for row in rows {
        assert_eq!(
            verdict_of(validate_dockerfile(&row.text)),
            row.want,
            "{}: {:?}",
            row.label,
            row.text
        );
    }
}

fn accepted() -> Vec<Row> {
    let ok = |label, text| row(label, text, Verdict::Accepted);
    vec![
        ok("bare construct", "FROM construct\n"),
        ok("named construct", "FROM construct AS role\n"),
        ok("lowercase keywords", "from construct as role\n"),
        ok(
            "multi-stage",
            "FROM golang:1.23 AS build\nRUN go build ./...\nFROM construct AS role\nCOPY --from=build /out/app /usr/local/bin/app\n",
        ),
        ok(
            "mixed stages",
            "ARG BASE=ubuntu\nFROM --platform=$BUILDPLATFORM ${BASE} AS build\nFROM scratch AS assets\nFROM construct AS role\n",
        ),
        ok("leading arg", "ARG VERSION=1\nFROM construct\n"),
        ok(
            "construct as copy source",
            "FROM construct AS role\nCOPY --from=construct /etc/os-release /tmp/os\n",
        ),
        ok(
            "stage name with construct prefix",
            "FROM ubuntu AS constructor\nFROM construct\n",
        ),
    ]
}

fn wrong_final_stage() -> Vec<Row> {
    let digest = format!("FROM construct@sha256:{}\n", "0".repeat(64));
    let digest_token = format!("construct@sha256:{}", "0".repeat(64));
    let bad = |label, text, line, found| row(label, text, not_construct(line, found));
    vec![
        bad("plain base", "FROM ubuntu:24.04\n", 1, "ubuntu:24.04"),
        bad(
            "second stage is not construct",
            "FROM construct AS role\nFROM ubuntu AS final\n",
            2,
            "ubuntu",
        ),
        bad("digest pin", &digest, 1, &digest_token),
        bad("tag", "FROM construct:latest\n", 1, "construct:latest"),
        bad("upper case", "FROM Construct\n", 1, "Construct"),
        bad("scratch", "FROM scratch\n", 1, "scratch"),
        bad(
            "docker hub path",
            "FROM docker.io/library/construct\n",
            1,
            "docker.io/library/construct",
        ),
        bad(
            "variable",
            "ARG BASE=construct\nFROM ${BASE}\n",
            2,
            "${BASE}",
        ),
        bad(
            "stage chain",
            "FROM construct AS build\nFROM build AS role\n",
            2,
            "build",
        ),
    ]
}

fn stage_names_and_syntax() -> Vec<Row> {
    let defined = |label, text, line| row(label, text, Verdict::ConstructStage(line));
    let unreadable = |label, text, line| row(label, text, Verdict::Unreadable(line));
    let none = |label, text| row(label, text, Verdict::NoStages);
    vec![
        defined(
            "stage named construct",
            "FROM ubuntu AS construct\nFROM construct\n",
            1,
        ),
        defined(
            "stage named Construct",
            "FROM ubuntu AS Construct\nFROM construct\n",
            1,
        ),
        defined(
            "final stage named construct",
            "FROM construct AS construct\n",
            1,
        ),
        defined(
            "middle stage named CONSTRUCT",
            "FROM ubuntu AS build\nFROM ubuntu AS CONSTRUCT\nFROM construct AS role\n",
            2,
        ),
        defined(
            "lowercase keywords",
            "from alpine as construct\nfrom construct\n",
            1,
        ),
        unreadable("no image", "FROM\n", 1),
        unreadable("missing name", "FROM construct AS\n", 1),
        unreadable("name without AS", "FROM construct role\n", 1),
        unreadable("two names", "FROM construct AS a b\n", 1),
        unreadable("flag only", "FROM --platform=linux/amd64\n", 1),
        unreadable("unreadable before a good one", "FROM\nFROM construct\n", 1),
        none("empty", ""),
        none("blank lines", "\n  \n"),
        none("only a comment", "# only a comment\n"),
        none("only an arg", "ARG A=1\n"),
    ]
}

#[test]
fn final_stage_must_be_exactly_construct_and_no_stage_may_be_named_construct() -> TestResult {
    check(&accepted());
    check(&wrong_final_stage());
    check(&stage_names_and_syntax());
    Ok(())
}

fn noise_accepted() -> Vec<Row> {
    let ok = |label, text| row(label, text, Verdict::Accepted);
    vec![
        ok(
            "directive, comment and blank",
            "# syntax=docker/dockerfile:1.7\n# note\n\nFROM construct AS role\n# FROM ubuntu\n",
        ),
        ok("tabs and runs of blanks", "  FROM\tconstruct   AS   role\n"),
        ok(
            "crlf",
            "FROM ubuntu AS build\r\nRUN true\r\nFROM construct AS role\r\n",
        ),
        ok("continuation", "FROM \\\n  construct \\\n  AS role\n"),
        ok(
            "comment inside a continuation",
            "FROM construct \\\n# note\n  AS role\n",
        ),
        ok(
            "continued line that looks like a stage",
            "FROM construct AS role\nRUN echo one \\\nFROM ubuntu\n",
        ),
        ok(
            "blank after the escape character",
            "FROM construct AS role\nRUN echo one \\ \nFROM ubuntu\n",
        ),
        ok(
            "heredoc body",
            "FROM construct AS role\nRUN <<EOF\nFROM ubuntu\n\nEOF\n",
        ),
        ok(
            "crlf heredoc",
            "FROM ubuntu AS build\r\nRUN <<EOF\r\nx\r\nEOF\r\nFROM construct\r\n",
        ),
        ok(
            "chomped quoted heredoc",
            "FROM construct\nCOPY <<-\"EOT\" /etc/motd\n\tFROM ubuntu\n\tEOT\n",
        ),
        ok(
            "backtick escape",
            "# escape=`\nFROM construct `\n  AS role\n",
        ),
        ok(
            "backslash is ordinary under the backtick escape",
            "# escape=`\nFROM ubuntu AS build\nRUN echo \\\nFROM construct\n",
        ),
        ok(
            "a directive after a comment is a comment",
            "# note\n# escape=`\nFROM ubuntu AS build\nRUN echo `\nFROM construct\n",
        ),
    ]
}

fn noise_rejected() -> Vec<Row> {
    let bad = |label, text| row(label, text, not_construct(1, "ubuntu"));
    vec![
        bad("commented construct", "FROM ubuntu\n# FROM construct\n"),
        bad(
            "continued construct",
            "FROM ubuntu\nRUN echo \\\nFROM construct\n",
        ),
        bad(
            "heredoc construct",
            "FROM ubuntu\nRUN <<EOF\nFROM construct\nEOF\n",
        ),
        bad(
            "quoted construct",
            "FROM ubuntu\nRUN echo \"FROM construct\"\n",
        ),
        bad("crlf token", "FROM ubuntu\r\n"),
    ]
}

#[test]
fn parser_noise_never_changes_the_verdict() -> TestResult {
    check(&noise_accepted());
    check(&noise_rejected());
    Ok(())
}

fn directive_rows() -> Vec<Row> {
    let bypass = |label, text: &str| row(label, text, not_construct(2, "ubuntu"));
    let ok = |label, text: &str| row(label, text, Verdict::Accepted);
    vec![
        bypass(
            "indented escape bypass",
            "  # escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        bypass(
            "tab-indented",
            "\t# escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        bypass(
            "mixed indent",
            " \t # escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        bypass(
            "NBSP-indented",
            "\u{a0}# escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        bypass(
            "NBSP after the hash",
            "#\u{a0}escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        ok(
            "indented, valid join",
            "  # escape=`\nFROM construct `\n AS role\n",
        ),
        ok("BOM only", "\u{feff}FROM construct\n"),
        ok(
            "BOM then directive",
            "\u{feff}# escape=`\nFROM construct `\n AS role\n",
        ),
        bypass(
            "BOM bypass",
            "\u{feff}# escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        row(
            "BOM on line 2 is data",
            "# syntax=x\n\u{feff}FROM construct\n",
            Verdict::NoStages,
        ),
        ok(
            "blank first line ends the block",
            "\n# escape=`\nFROM ubuntu\nRUN echo hi `\nFROM construct\n",
        ),
        row(
            "NBSP-indented FROM is an instruction",
            "FROM construct\n\u{a0}FROM ubuntu\n",
            not_construct(2, "ubuntu"),
        ),
    ]
}

#[test]
fn directive_parsing_follows_buildkit_whitespace_and_bom_rules() -> TestResult {
    check(&directive_rows());
    Ok(())
}

fn request(dockerfile: &[u8]) -> Result<ImageRequest, Box<dyn Error>> {
    Ok(ImageRequest {
        role: RoleName::parse("sentinel")?,
        context: HostPath::parse(Path::new("/work/role"))?,
        context_files: vec![RepoPath::parse("Dockerfile")?],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: dockerfile.to_vec(),
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision::Clean {
            commit: CommitId::parse(&"1".repeat(40))?,
        },
        agent: Agent::Codex,
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        rebuild: Rebuild::No,
    })
}

#[test]
fn plan_new_applies_the_same_validation_to_the_request_bytes() -> TestResult {
    let texts = [
        ("final stage", "FROM ubuntu\n", not_construct(1, "ubuntu")),
        (
            "construct stage",
            "FROM ubuntu AS construct\nFROM construct\n",
            Verdict::ConstructStage(1),
        ),
        ("empty", "", Verdict::NoStages),
        ("valid", "FROM construct\n", Verdict::Accepted),
    ];
    for (label, text, want) in texts {
        let planned = plan_verdict(ImagePlan::new(&request(text.as_bytes())?));
        assert_eq!(planned, want, "{label}");
        assert_eq!(planned, verdict_of(validate_dockerfile(text)), "{label}");
    }
    let invalid = plan_verdict(ImagePlan::new(&request(&[0xFF, 0xFE, b'F'])?));
    assert_eq!(invalid, Verdict::NotUtf8);
    Ok(())
}

#[test]
fn dockerfile_errors_quote_what_they_found_as_plain_text() {
    let shown = |text: &str| {
        validate_dockerfile(text)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    };
    assert_eq!(
        shown("FROM ubuntu\n"),
        "line 1: the final stage must start FROM construct, found ubuntu"
    );
    assert_eq!(
        shown("FROM \"a\" b c\n"),
        "line 1: cannot read the FROM instruction FROM \"a\" b c"
    );
    assert_eq!(
        shown("FROM a\u{1b}[2Jb c d\n"),
        "line 1: cannot read the FROM instruction FROM a\\u{1b}[2Jb c d"
    );
    assert_eq!(
        shown("FROM ubuntu\u{1b}[2J\n"),
        "line 1: the final stage must start FROM construct, found ubuntu\\u{1b}[2J"
    );
}
