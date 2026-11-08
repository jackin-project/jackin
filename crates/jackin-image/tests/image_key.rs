// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ContextEntry;
use std::collections::HashSet;
use std::error::Error;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, ImageId, Platform, RepoPath, RoleName, Sha256,
};
use jackin_image::{
    HookFiles, ImageKey, ImagePlan, ImageRequest, Rebuild, RoleRevision, SourceFile,
};
use sha2::Digest;

type TestResult = Result<(), Box<dyn Error>>;
type Mutation = fn(&mut ImageRequest) -> TestResult;

const CONSTRUCT_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/Dockerfile"
));
const DERIVED_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/derived/Dockerfile"
));
const JACKIN_START: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/jackin-start"));
const SCAN: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/derived/scan"));
const WRITE_FILE_C: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/write-file.c"
));

const DOCKERFILE: &[u8] = b"FROM construct AS role\n";
const SETUP: &[u8] = b"#!/bin/sh\necho setup\n";

fn entry(path: &str) -> Result<ContextEntry, Box<dyn Error>> {
    Ok(ContextEntry::File {
        path: RepoPath::parse(path)?,
        bytes: path.as_bytes().to_vec(),
        executable: false,
    })
}

fn c1() -> String {
    format!("sha256:{}", "1".repeat(64))
}

fn c2() -> String {
    format!("sha256:{}", "2".repeat(64))
}

fn source(path: &str, bytes: &[u8]) -> Result<SourceFile, Box<dyn Error>> {
    Ok(SourceFile {
        path: RepoPath::parse(path)?,
        bytes: bytes.to_vec(),
    })
}

fn base() -> Result<ImageRequest, Box<dyn Error>> {
    Ok(ImageRequest {
        role: RoleName::parse("sentinel")?,
        context_files: vec![entry("Dockerfile")?],
        dockerfile: source("Dockerfile", DOCKERFILE)?,
        hooks: HookFiles {
            setup: Some(source("hooks/setup.sh", SETUP)?),
            preflight: None,
        },
        revision: RoleRevision {
            commit: CommitId::parse(&"1".repeat(40))?,
            digest: Sha256::of(b"clean"),
        },
        agent: Agent::Codex,
        agents: vec![Agent::Codex],
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        read_write: Vec::new(),
        rebuild: Rebuild::No,
    })
}

fn commit(digit: &str) -> Result<CommitId, Box<dyn Error>> {
    Ok(CommitId::parse(&digit.repeat(40))?)
}

fn derive(request: &ImageRequest, construct: &str) -> Result<ImageKey, Box<dyn Error>> {
    Ok(ImageKey::derive(request, &ImageId::parse(construct)?)?)
}

struct Changing {
    label: &'static str,
    edit: Mutation,
    construct: fn() -> String,
}

fn changing(label: &'static str, edit: Mutation) -> Changing {
    Changing {
        label,
        edit,
        construct: c1,
    }
}

fn changing_rows() -> Vec<Changing> {
    vec![
        changing("role", |r| {
            r.role = RoleName::parse("sentinel2")?;
            Ok(())
        }),
        changing("clean commit", |r| {
            r.revision = RoleRevision {
                commit: commit("2")?,
                digest: Sha256::of(b"clean"),
            };
            Ok(())
        }),
        changing("dirty with the same commit", |r| {
            r.revision = RoleRevision {
                commit: commit("1")?,
                digest: Sha256::of(b"d1"),
            };
            Ok(())
        }),
        changing("dirty with another digest", |r| {
            r.revision = RoleRevision {
                commit: commit("1")?,
                digest: Sha256::of(b"d2"),
            };
            Ok(())
        }),
        changing("dirty with another commit", |r| {
            r.revision = RoleRevision {
                commit: commit("2")?,
                digest: Sha256::of(b"d1"),
            };
            Ok(())
        }),
        changing("dockerfile text", |r| {
            r.dockerfile.bytes.extend_from_slice(b"RUN true\n");
            Ok(())
        }),
        changing("dockerfile newline", |r| {
            r.dockerfile.bytes.push(b'\n');
            Ok(())
        }),
        Changing {
            label: "construct image",
            edit: |_| Ok(()),
            construct: c2,
        },
        changing("platform", |r| {
            r.platform = Platform::LinuxAmd64;
            Ok(())
        }),
        changing("docker layer", |r| {
            r.docker = DockerLayer::Client;
            Ok(())
        }),
        changing("setup bytes", |r| {
            r.hooks.setup = Some(source("hooks/setup.sh", b"#!/bin/sh\necho other\n")?);
            Ok(())
        }),
        changing("setup absent", |r| {
            r.hooks.setup = None;
            Ok(())
        }),
        changing("preflight declared", |r| {
            r.hooks.preflight = Some(source("hooks/pre.sh", b"#!/bin/sh\necho pre\n")?);
            Ok(())
        }),
        changing("same file as preflight instead of setup", |r| {
            r.hooks.setup = None;
            r.hooks.preflight = Some(source("hooks/setup.sh", SETUP)?);
            Ok(())
        }),
    ]
}

fn equal_rows() -> Vec<(&'static str, Mutation)> {
    vec![
        ("clone", |_| Ok(())),
        ("builder", |r| {
            r.builder = BuilderName::parse("other")?;
            Ok(())
        }),
        ("rebuild", |r| {
            r.rebuild = Rebuild::Yes;
            Ok(())
        }),
        ("context files", |r| {
            r.context_files = vec![entry("src/other.rs")?, entry("src/more.rs")?];
            Ok(())
        }),
        ("dockerfile path", |r| {
            r.dockerfile.path = RepoPath::parse("docker/Dockerfile")?;
            Ok(())
        }),
    ]
}

#[test]
fn each_input_changes_the_key_and_unused_inputs_do_not() -> TestResult {
    let request = base()?;
    let key = derive(&request, &c1())?;
    let mut seen = HashSet::new();
    seen.insert(key.sha256().to_string());
    for row in changing_rows() {
        let mut edited = request.clone();
        (row.edit)(&mut edited)?;
        let other = derive(&edited, &(row.construct)())?;
        assert_ne!(other, key, "{}", row.label);
        seen.insert(other.sha256().to_string());
    }
    assert_eq!(seen.len(), 15, "the keys are pairwise distinct");
    for (label, edit) in equal_rows() {
        let mut edited = request.clone();
        edit(&mut edited)?;
        assert_eq!(derive(&edited, &c1())?, key, "{label}");
    }
    assert_eq!(derive(&request, &c1())?, key, "two derive calls");
    let planned = ImagePlan::new(&request)?.final_builds(&ImageId::parse(&c1())?);
    assert_eq!(planned.key, key, "the plan key");
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            out.push_str(&format!("{byte:02x}"));
            out
        })
}

fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend((bytes.len() as u64).to_be_bytes());
    out.extend(bytes);
}

struct Expect<'a> {
    role: &'a str,
    revision: (&'a str, &'a str),
    dockerfile: &'a [u8],
    construct: &'a str,
    build_args: &'a [String],
    platform: Platform,
    docker: &'a str,
    hooks: [Option<(&'a str, &'a [u8])>; 2],
}

fn expected_key(expect: &Expect<'_>) -> String {
    let mut out = Vec::new();
    field(&mut out, expect.role.as_bytes());
    field(&mut out, b"codex");
    field(&mut out, expect.revision.0.as_bytes());
    field(&mut out, expect.revision.1.as_bytes());
    field(&mut out, hex(expect.dockerfile).as_bytes());
    field(&mut out, expect.construct.as_bytes());
    field(&mut out, expect.build_args.len().to_string().as_bytes());
    for arg in expect.build_args {
        field(&mut out, arg.as_bytes());
    }
    field(&mut out, expect.platform.docker_arg().as_bytes());
    field(&mut out, expect.docker.as_bytes());
    field(&mut out, hex(JACKIN_START).as_bytes());
    field(&mut out, hex(DERIVED_DOCKERFILE).as_bytes());
    field(&mut out, hex(SCAN).as_bytes());
    for hook in expect.hooks {
        let (path, bytes) = hook.unwrap_or(("", b""));
        field(&mut out, path.as_bytes());
        field(
            &mut out,
            if hook.is_some() {
                hex(bytes)
            } else {
                String::new()
            }
            .as_bytes(),
        );
    }
    hex(&out)
}

fn planned_build_args(request: &ImageRequest) -> Result<Vec<String>, Box<dyn Error>> {
    let built = ImagePlan::new(request)?.final_builds(&ImageId::parse(&c1())?);
    Ok(built
        .derived
        .args
        .into_iter()
        .filter(|arg| arg.starts_with("--build-arg="))
        .collect())
}

fn second_request() -> Result<ImageRequest, Box<dyn Error>> {
    let mut request = base()?;
    request.role = RoleName::parse("Agent-Smith")?;
    request.revision = RoleRevision {
        commit: commit("2")?,
        digest: Sha256::of(b"dirty"),
    };
    request.platform = Platform::LinuxAmd64;
    request.docker = DockerLayer::Client;
    request.hooks.setup = Some(source("hooks/a.sh", b"setup bytes")?);
    request.hooks.preflight = Some(source("hooks/b.sh", b"preflight bytes")?);
    Ok(request)
}

fn check_tags(request: &ImageRequest, construct: &str, key: &str, repo: &str) -> TestResult {
    let hex16 = &key[..16];
    let built = ImagePlan::new(request)?.final_builds(&ImageId::parse(construct)?);
    assert_eq!(built.key.sha256().as_str(), key);
    assert_eq!(
        built.derived.tag.to_string(),
        format!("jackin.invalid/{repo}:{hex16}")
    );
    assert_eq!(
        built.role.tag.to_string(),
        format!("jackin.invalid/role/{repo}:{hex16}")
    );
    Ok(())
}

#[test]
fn key_is_the_documented_sha256_encoding_including_the_embedded_digests() -> TestResult {
    let first_args = planned_build_args(&base()?)?;
    assert_eq!(first_args.len(), 14, "derived build args");
    let first = Expect {
        role: "sentinel",
        revision: (&"1".repeat(40), &Sha256::of(b"clean").to_string()),
        dockerfile: DOCKERFILE,
        construct: &c1(),
        build_args: &first_args,
        platform: Platform::LinuxArm64,
        docker: "absent",
        hooks: [Some(("hooks/setup.sh", SETUP)), None],
    };
    let expected = expected_key(&first);
    assert_eq!(derive(&base()?, &c1())?.sha256().as_str(), expected);
    check_tags(&base()?, &c1(), &expected, "sentinel")?;

    let construct = format!("sha256:{}", "3".repeat(64));
    let dirty = Sha256::of(b"dirty").to_string();
    let second_args = planned_build_args(&second_request()?)?;
    assert_eq!(second_args.len(), 24, "derived build args with the client");
    assert!(second_args.contains(&"--build-arg=DOCKER_LAYER=client".to_owned()));
    let second = Expect {
        role: "Agent-Smith",
        revision: (&"2".repeat(40), &dirty),
        dockerfile: DOCKERFILE,
        construct: &construct,
        build_args: &second_args,
        platform: Platform::LinuxAmd64,
        docker: "client",
        hooks: [
            Some(("hooks/a.sh", b"setup bytes")),
            Some(("hooks/b.sh", b"preflight bytes")),
        ],
    };
    let expected = expected_key(&second);
    assert_eq!(
        derive(&second_request()?, &construct)?.sha256().as_str(),
        expected
    );
    check_tags(&second_request()?, &construct, &expected, "agent-smith")
}

fn construct_digest(platform: Platform, writer: &[u8]) -> String {
    let mut out = Vec::new();
    field(&mut out, hex(CONSTRUCT_DOCKERFILE).as_bytes());
    field(&mut out, hex(writer).as_bytes());
    field(&mut out, platform.docker_arg().as_bytes());
    hex(&out)
}

fn construct_hex(platform: Platform) -> String {
    construct_digest(platform, WRITE_FILE_C)[..16].to_owned()
}

fn construct_tag(request: &ImageRequest) -> Result<String, Box<dyn Error>> {
    Ok(ImagePlan::new(request)?.construct_build().tag.to_string())
}

#[test]
fn construct_tag_depends_on_the_platform_only() -> TestResult {
    let request = base()?;
    let arm_digest = construct_digest(Platform::LinuxArm64, WRITE_FILE_C);
    let amd_digest = construct_digest(Platform::LinuxAmd64, WRITE_FILE_C);
    assert_eq!(
        arm_digest,
        "b9b62c574059a5eb9e7c6c1e3908c517a5a4e4d6e84808f08fcc070a0d1e7c65"
    );
    assert_eq!(
        amd_digest,
        "cb353d46a3234563288a123437fc8bbdaeb7855890d79d782c63887546014524"
    );
    let mut changed_writer = WRITE_FILE_C.to_vec();
    changed_writer.push(b'\n');
    assert_ne!(
        construct_digest(Platform::LinuxArm64, &changed_writer),
        arm_digest
    );
    let arm = construct_tag(&request)?;
    assert_eq!(
        arm,
        format!(
            "jackin.invalid/construct:{}",
            construct_hex(Platform::LinuxArm64)
        )
    );
    let mut amd_request = request.clone();
    amd_request.platform = Platform::LinuxAmd64;
    let amd = construct_tag(&amd_request)?;
    assert_eq!(
        amd,
        format!(
            "jackin.invalid/construct:{}",
            construct_hex(Platform::LinuxAmd64)
        )
    );
    assert_ne!(arm, amd);
    let unchanged: [(&str, Mutation); 7] = [
        ("role", |r| {
            r.role = RoleName::parse("other")?;
            Ok(())
        }),
        ("revision", |r| {
            r.revision = RoleRevision {
                commit: commit("2")?,
                digest: Sha256::of(b"x"),
            };
            Ok(())
        }),
        ("dockerfile", |r| {
            r.dockerfile.bytes.extend_from_slice(b"RUN true\n");
            Ok(())
        }),
        ("hooks", |r| {
            r.hooks.setup = None;
            r.hooks.preflight = Some(source("hooks/p.sh", b"p")?);
            Ok(())
        }),
        ("docker", |r| {
            r.docker = DockerLayer::Client;
            Ok(())
        }),
        ("builder", |r| {
            r.builder = BuilderName::parse("other")?;
            Ok(())
        }),
        ("rebuild", |r| {
            r.rebuild = Rebuild::Yes;
            Ok(())
        }),
    ];
    for (label, edit) in unchanged {
        let mut edited = request.clone();
        edit(&mut edited)?;
        assert_eq!(construct_tag(&edited)?, arm, "{label}");
    }
    Ok(())
}

#[test]
fn construct_dockerfile_pins_its_base_image_by_digest() -> TestResult {
    let text = std::str::from_utf8(CONSTRUCT_DOCKERFILE)?;
    let mut count = 0;
    for line in text.lines().filter(|l| l.starts_with("ARG BASE_IMAGE=")) {
        count += 1;
        let value = &line["ARG BASE_IMAGE=".len()..];
        let (name, digest) = value
            .split_once("@sha256:")
            .ok_or_else(|| format!("occurrence {count} is not pinned: {value}"))?;
        assert!(
            name.starts_with("debian:") && name.len() > "debian:".len(),
            "occurrence {count} is not a tagged debian image: {name:?}"
        );
        assert!(
            digest.len() == 64
                && digest
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "occurrence {count} has a bad digest {digest:?}"
        );
    }
    assert!(count >= 1, "no ARG BASE_IMAGE line");
    Ok(())
}
