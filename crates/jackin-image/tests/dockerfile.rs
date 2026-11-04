// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Sha256;
use jackin_image::ContextEntry;
use std::error::Error;

use jackin_core::{Agent, BuilderName, CommitId, DockerLayer, Platform, RepoPath, RoleName};
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
    EngineClientEnv(usize, String),
    ExpandingEnvName(usize, String),
    ForeignFrontend(usize, String),
    NotUtf8,
    Other,
}

#[derive(Debug)]
struct Row {
    label: &'static str,
    text: String,
    want: Verdict,
}

fn entry(path: &str) -> Result<ContextEntry, Box<dyn Error>> {
    Ok(ContextEntry::File {
        path: RepoPath::parse(path)?,
        bytes: path.as_bytes().to_vec(),
        executable: false,
    })
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
        DockerfileError::EngineClientEnv { line, name } => {
            Verdict::EngineClientEnv(*line, name.clone())
        }
        DockerfileError::ExpandingEnvName { line, name } => {
            Verdict::ExpandingEnvName(*line, name.clone())
        }
        DockerfileError::ForeignFrontend { line, frontend } => {
            Verdict::ForeignFrontend(*line, frontend.clone())
        }
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
            | ImageError::Cancelled { .. },
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

fn not_a_continuation() -> Vec<Row> {
    let stage = |label, text| row(label, text, not_construct(3, "ubuntu:24.04"));
    vec![
        stage(
            "a doubled escape ends the instruction",
            "FROM construct\nRUN true \\\\\nFROM ubuntu:24.04\n",
        ),
        stage(
            "a vertical tab after the escape ends the instruction",
            "FROM construct\nRUN true \\\x0b\nFROM ubuntu:24.04\n",
        ),
        stage(
            "a form feed after the escape ends the instruction",
            "FROM construct\nRUN true \\\x0c\nFROM ubuntu:24.04\n",
        ),
        row(
            "a doubled backtick ends the instruction",
            "# escape=`\nFROM construct\nRUN true ``\nFROM ubuntu:24.04\n",
            not_construct(4, "ubuntu:24.04"),
        ),
    ]
}

#[test]
fn only_an_unescaped_escape_followed_by_blanks_continues_a_line() -> TestResult {
    check(&not_a_continuation());
    Ok(())
}

fn env_rows() -> Vec<Row> {
    let ok = |label, text: &str| row(label, text, Verdict::Accepted);
    let client = |label, text: &str, line, name: &str| {
        row(label, text, Verdict::EngineClientEnv(line, name.to_owned()))
    };
    let expands = |label, text: &str, line, name: &str| {
        row(
            label,
            text,
            Verdict::ExpandingEnvName(line, name.to_owned()),
        )
    };
    vec![
        ok("plain variable", "FROM construct\nENV FOO=1\n"),
        ok(
            "path and home are not engine client variables",
            "FROM construct\nENV PATH=/opt/bin:$PATH HOME=/sandbox\nENV LD_LIBRARY_PATH=/x\n",
        ),
        ok(
            "names that merely contain the text",
            "FROM construct\nENV MY_DOCKER_HOST=x DOCKERHOST=x BUILDX=x docker_host=x\n",
        ),
        ok(
            "the text only in a value",
            "FROM construct\nENV A=\"DOCKER_HOST=x\" B='BUILDX_BUILDER'\nENV C DOCKER_HOST\n",
        ),
        ok(
            "arg is not checked",
            "ARG DOCKER_HOST=x\nFROM construct\nARG BUILDX_BUILDER\n",
        ),
        ok(
            "an earlier stage does not reach the image",
            "FROM golang AS build\nENV DOCKER_HOST=x\nFROM construct AS role\nENV FOO=1\n",
        ),
        ok(
            "words that are not an instruction",
            "FROM construct\n# ENV DOCKER_HOST=x\nRUN echo ENV DOCKER_HOST=x\nLABEL ENV=DOCKER_HOST\n",
        ),
        ok(
            "a heredoc body line",
            "FROM construct\nRUN <<EOF\nENV DOCKER_HOST=x\nEOF\n",
        ),
        client(
            "docker host",
            "FROM construct\nENV DOCKER_HOST=tcp://x:2376\n",
            2,
            "DOCKER_HOST",
        ),
        client(
            "lower case keyword",
            "FROM construct\nenv DOCKER_TLS_VERIFY=1\n",
            2,
            "DOCKER_TLS_VERIFY",
        ),
        client(
            "buildx",
            "FROM construct\nENV BUILDX_BUILDER=x\n",
            2,
            "BUILDX_BUILDER",
        ),
        client(
            "the second pair",
            "FROM construct\nENV A=1 DOCKER_CERT_PATH=/x\n",
            2,
            "DOCKER_CERT_PATH",
        ),
        client(
            "legacy form",
            "FROM construct\nENV DOCKER_HOST tcp://x:2376\n",
            2,
            "DOCKER_HOST",
        ),
        client(
            "double quoted key",
            "FROM construct\nENV \"DOCKER_HOST\"=x\n",
            2,
            "DOCKER_HOST",
        ),
        client(
            "single quoted key",
            "FROM construct\nENV 'BUILDX_BUILDER'=x\n",
            2,
            "BUILDX_BUILDER",
        ),
        client(
            "continuation",
            "FROM construct\nENV A=1 \\\n    DOCKER_CONFIG=/x\n",
            2,
            "DOCKER_CONFIG",
        ),
        client(
            "escape directive",
            "# escape=`\nFROM construct\nENV A=1 `\n    DOCKER_HOST=x\n",
            3,
            "DOCKER_HOST",
        ),
        client(
            "final stage of several",
            "FROM golang AS build\nFROM construct AS role\nENV DOCKER_HOST=x\n",
            3,
            "DOCKER_HOST",
        ),
        client(
            "the final stage after a clean earlier one",
            "FROM construct AS a\nENV X=1\nFROM construct\nENV Y=1\nENV DOCKER_HOST=x\n",
            5,
            "DOCKER_HOST",
        ),
        client(
            "an escaped equals sign in the first word",
            "FROM construct\nENV D\\=X DOCKER_HOST=tcp://evil.example:2375\n",
            2,
            "DOCKER_HOST",
        ),
        client(
            "an escaped equals sign at the end of the first word",
            "FROM construct\nENV A\\= DOCKER_CONFIG=/sandbox/c\n",
            2,
            "DOCKER_CONFIG",
        ),
        client(
            "an escaped equals sign alone",
            "FROM construct\nENV \\=X BUILDX_BUILDER=x\n",
            2,
            "BUILDX_BUILDER",
        ),
        expands("expanding key", "FROM construct\nENV $NAME=x\n", 2, "$NAME"),
        expands(
            "braced key",
            "FROM construct\nENV A=1 ${NAME}=x\n",
            2,
            "${NAME}",
        ),
        expands(
            "quoted expanding key",
            "FROM construct\nENV \"$NAME\"=x\n",
            2,
            "\"$NAME\"",
        ),
        expands(
            "expanding legacy key",
            "FROM construct\nENV $NAME x\n",
            2,
            "$NAME",
        ),
    ]
}

#[test]
fn env_may_not_name_the_docker_engine_client_in_the_final_stage() -> TestResult {
    check(&env_rows());
    Ok(())
}

fn engine_client(label: &'static str, text: &str, line: usize, name: &str) -> Row {
    row(label, text, Verdict::EngineClientEnv(line, name.to_owned()))
}

fn onbuild_env_refused() -> Vec<Row> {
    let expands = |label, text: &str, line, name: &str| {
        row(
            label,
            text,
            Verdict::ExpandingEnvName(line, name.to_owned()),
        )
    };
    vec![
        engine_client(
            "deferred docker context",
            "FROM construct\nONBUILD ENV DOCKER_CONTEXT=evil\n",
            2,
            "DOCKER_CONTEXT",
        ),
        engine_client(
            "lower case",
            "FROM construct\nonbuild env BUILDX_BUILDER=evil\n",
            2,
            "BUILDX_BUILDER",
        ),
        engine_client(
            "mixed case",
            "FROM construct\nOnBuild Env DOCKER_HOST=x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "legacy form",
            "FROM construct\nONBUILD ENV DOCKER_HOST tcp://x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "nested onbuild",
            "FROM construct\nONBUILD ONBUILD ENV DOCKER_HOST=tcp://x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "tabs",
            "FROM construct\nONBUILD\tENV\tDOCKER_HOST=x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "second pair",
            "FROM construct\nONBUILD ENV A=1 DOCKER_CERT_PATH=/x\n",
            2,
            "DOCKER_CERT_PATH",
        ),
        engine_client(
            "quoted key",
            "FROM construct\nONBUILD ENV \"DOCKER_HOST\"=x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "continuation after onbuild",
            "FROM construct\nONBUILD \\\nENV DOCKER_HOST=x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "continuation inside the payload",
            "FROM construct\nONBUILD ENV A=1 \\\n    DOCKER_CONFIG=/x\n",
            2,
            "DOCKER_CONFIG",
        ),
        engine_client(
            "comment inside the continuation",
            "FROM construct\nONBUILD \\\n# note\nENV DOCKER_HOST=x\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "escape directive",
            "# escape=`\nFROM construct\nONBUILD ENV A=1 `\n    DOCKER_HOST=x\n",
            3,
            "DOCKER_HOST",
        ),
        engine_client(
            "final stage after a clean earlier one",
            "FROM construct AS a\nONBUILD ENV X=1\nFROM construct\nONBUILD ENV DOCKER_HOST=x\n",
            4,
            "DOCKER_HOST",
        ),
        engine_client(
            "after a deferred run with a heredoc",
            "FROM construct\nONBUILD RUN <<EOF\nx\nEOF\nONBUILD ENV DOCKER_HOST=x\n",
            5,
            "DOCKER_HOST",
        ),
        engine_client(
            "an escaped equals sign in the first word",
            "FROM construct\nONBUILD ENV D\\=X DOCKER_HOST=tcp://evil.example:2375\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "an escaped equals sign at the end of the first word",
            "FROM construct\nONBUILD ENV A\\= DOCKER_CONFIG=/sandbox/c\n",
            2,
            "DOCKER_CONFIG",
        ),
        engine_client(
            "an escaped equals sign alone",
            "FROM construct\nONBUILD ENV \\=X BUILDX_BUILDER=x\n",
            2,
            "BUILDX_BUILDER",
        ),
        expands(
            "expanding key",
            "FROM construct\nONBUILD ENV $NAME=x\n",
            2,
            "$NAME",
        ),
        expands(
            "braced key after a pair",
            "FROM construct\nONBUILD ENV A=1 ${NAME}=x\n",
            2,
            "${NAME}",
        ),
        expands(
            "expanding legacy key",
            "FROM construct\nONBUILD ENV $NAME x\n",
            2,
            "$NAME",
        ),
    ]
}

#[test]
fn an_onbuild_env_is_checked_as_the_env_it_defers() -> TestResult {
    check(&onbuild_env_refused());
    Ok(())
}

fn onbuild_heredoc_rows() -> Vec<Row> {
    let ok = |label, text: &str| row(label, text, Verdict::Accepted);
    vec![
        engine_client(
            "a deferred heredoc body cannot start a stage that hides an env",
            "FROM construct\nENV DOCKER_HOST=x\nONBUILD RUN <<EOF\nFROM construct\nEOF\n",
            2,
            "DOCKER_HOST",
        ),
        engine_client(
            "a deferred copy heredoc body cannot start a stage that hides an env",
            "FROM construct\nENV DOCKER_HOST=x\nONBUILD COPY <<-EOT /etc/motd\n\tFROM construct\n\tEOT\n",
            2,
            "DOCKER_HOST",
        ),
        ok(
            "a deferred heredoc body is not a stage",
            "FROM construct\nONBUILD RUN <<EOF\nFROM ubuntu\nEOF\n",
        ),
        ok(
            "a deferred heredoc body is not an env",
            "FROM construct\nONBUILD RUN <<EOF\nENV DOCKER_HOST=x\nEOF\n",
        ),
        ok(
            "a deferred add heredoc body is not an env",
            "FROM construct\nONBUILD ADD <<EOF /x\nENV DOCKER_HOST=x\nEOF\n",
        ),
    ]
}

#[test]
fn an_onbuild_heredoc_body_is_data_for_the_parser() -> TestResult {
    check(&onbuild_heredoc_rows());
    Ok(())
}

fn frontend_rows() -> Vec<Row> {
    let foreign = |label, text: &str, line, frontend: &str| {
        row(
            label,
            text,
            Verdict::ForeignFrontend(line, frontend.to_owned()),
        )
    };
    vec![
        foreign(
            "another image",
            "# syntax=evil/frontend:1\nFROM construct\n",
            1,
            "evil/frontend:1",
        ),
        foreign(
            "spaces around the equals sign",
            "# syntax = evil/frontend\nFROM construct\n",
            1,
            "evil/frontend",
        ),
        foreign(
            "upper case key",
            "# SYNTAX=evil/frontend\nFROM construct\n",
            1,
            "evil/frontend",
        ),
        foreign(
            "c style directive",
            "// syntax=evil/frontend\nFROM construct\n",
            1,
            "evil/frontend",
        ),
        foreign(
            "after a shebang line",
            "#!/bin/sh\n# syntax=evil/frontend\nFROM construct\n",
            2,
            "evil/frontend",
        ),
        foreign(
            "after another directive",
            "# escape=`\n# syntax=evil/frontend\nFROM construct\n",
            2,
            "evil/frontend",
        ),
        foreign(
            "after a byte order mark",
            "\u{feff}# syntax=evil/frontend\nFROM construct\n",
            1,
            "evil/frontend",
        ),
        foreign(
            "command line after the image",
            "# syntax=evil/frontend --flag\nFROM construct\n",
            1,
            "evil/frontend",
        ),
        foreign(
            "the second syntax directive",
            "# syntax=docker/dockerfile:1\n# syntax=evil/frontend\nFROM construct\n",
            2,
            "evil/frontend",
        ),
        foreign(
            "upstream frontend",
            "# syntax=docker/dockerfile-upstream:master\nFROM construct\n",
            1,
            "docker/dockerfile-upstream:master",
        ),
        foreign(
            "look alike name",
            "# syntax=docker/dockerfile.evil:1\nFROM construct\n",
            1,
            "docker/dockerfile.evil:1",
        ),
        foreign(
            "other registry",
            "# syntax=registry.example.com/docker/dockerfile:1\nFROM construct\n",
            1,
            "registry.example.com/docker/dockerfile:1",
        ),
        foreign(
            "registry with a port",
            "# syntax=localhost:5000/docker/dockerfile:1\nFROM construct\n",
            1,
            "localhost:5000/docker/dockerfile:1",
        ),
    ]
}

#[test]
fn a_role_dockerfile_may_name_only_the_docker_dockerfile_frontend() -> TestResult {
    check(&frontend_rows());
    Ok(())
}

fn onbuild_and_frontend_accepted() -> Vec<Row> {
    let ok = |label, text: &str| row(label, text, Verdict::Accepted);
    let digest = format!(
        "# syntax=docker/dockerfile:1.28.0@sha256:{}\nFROM construct\n",
        "0".repeat(64)
    );
    vec![
        ok(
            "onbuild in an earlier stage",
            "FROM golang AS build\nONBUILD ENV DOCKER_HOST=x\nFROM construct AS role\nENV FOO=1\n",
        ),
        ok(
            "a commented onbuild",
            "FROM construct\n# ONBUILD ENV DOCKER_HOST=x\n",
        ),
        ok(
            "onbuild text inside a run",
            "FROM construct\nRUN echo ONBUILD ENV DOCKER_HOST=x\n",
        ),
        ok(
            "onbuild as a variable name",
            "FROM construct\nENV ONBUILD=1\n",
        ),
        ok(
            "onbuild line inside a heredoc",
            "FROM construct\nRUN <<EOF\nONBUILD ENV DOCKER_HOST=x\nEOF\n",
        ),
        ok(
            "the text only in a deferred value",
            "FROM construct\nONBUILD ENV A=\"DOCKER_HOST=x\" B='BUILDX_BUILDER'\nONBUILD ENV C DOCKER_HOST\n",
        ),
        ok(
            "names that merely contain the text",
            "FROM construct\nONBUILD ENV MY_DOCKER_HOST=x DOCKERHOST=x BUILDX=x\n",
        ),
        ok(
            "deferred path",
            "FROM construct\nONBUILD ENV PATH=/opt/bin:$PATH\n",
        ),
        ok("deferred user", "FROM construct\nONBUILD USER root\n"),
        ok("deferred run", "FROM construct\nONBUILD RUN true\n"),
        ok(
            "a word that only starts with env",
            "FROM construct\nONBUILD ENVIRON DOCKER_HOST=x\n",
        ),
        ok(
            "a word that only starts with onbuild",
            "FROM construct\nONBUILDENV DOCKER_HOST=x\n",
        ),
        ok("official frontend with a digest", &digest),
        ok(
            "official frontend on its registry host",
            "# syntax=docker.io/docker/dockerfile:1\nFROM construct\n",
        ),
        ok(
            "official frontend without a tag",
            "# syntax=docker/dockerfile\n# check=skip=JSONArgsRecommended\nFROM construct\n",
        ),
        ok(
            "a syntax text after a comment is a comment",
            "# note\n# syntax=evil/frontend\nFROM construct\n",
        ),
        ok(
            "a syntax text after a blank line is a comment",
            "\n# syntax=evil/frontend\nFROM construct\n",
        ),
        ok(
            "a syntax text after the first instruction is a comment",
            "FROM construct\n# syntax=evil/frontend\n",
        ),
        ok(
            "a shebang that is not on the first line is a comment",
            "# note\n#!/bin/sh\n# syntax=evil/frontend\nFROM construct\n",
        ),
        row(
            "a json file has no stage",
            "{\"syntax\": \"evil/frontend\"}\n",
            Verdict::NoStages,
        ),
    ]
}

#[test]
fn onbuild_and_syntax_text_that_does_not_defer_or_select_anything_is_unchanged() -> TestResult {
    check(&onbuild_and_frontend_accepted());
    Ok(())
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
            "# syntax=docker/dockerfile:1\n\u{feff}FROM construct\n",
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
        context_files: vec![entry("Dockerfile")?],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: dockerfile.to_vec(),
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision {
            commit: CommitId::parse(&"1".repeat(40))?,
            digest: Sha256::of(b"clean"),
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
        (
            "deferred env",
            "FROM construct\nONBUILD ENV DOCKER_HOST=x\n",
            Verdict::EngineClientEnv(2, "DOCKER_HOST".to_owned()),
        ),
        (
            "foreign frontend",
            "# syntax=evil/frontend:1\nFROM construct\n",
            Verdict::ForeignFrontend(1, "evil/frontend:1".to_owned()),
        ),
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
    assert_eq!(
        shown("FROM construct\nENV DOCKER_\u{1b}[2J=x\n"),
        "line 2: ENV sets DOCKER_\\u{1b}[2J, which names the Docker engine client; jackin sets it from the Docker capability"
    );
    assert_eq!(
        shown("FROM construct\nENV $A\u{1b}[2J=x\n"),
        "line 2: the ENV name $A\\u{1b}[2J expands a variable, so it cannot be checked"
    );
}
