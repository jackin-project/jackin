// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ContextEntry;
use std::collections::HashSet;
use std::error::Error;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, ImageId, ImageTag, Platform, RepoPath, RoleName,
    Sha256,
};
use jackin_image::{HookFiles, ImagePlan, ImageRequest, Rebuild, RoleRevision, SourceFile};
use regex::Regex;

type TestResult = Result<(), Box<dyn Error>>;

const ORACLE: &str = "^[a-z0-9]+(?:(?:[._]|__|-+)[a-z0-9]+)*(?:/[a-z0-9]+(?:(?:[._]|__|-+)[a-z0-9]+)*)*:[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$";
const ALPHABET: [char; 6] = ['a', 'B', '7', '.', '_', '-'];
const HEX16: &str = "0123456789abcdef";

fn entry(path: &str) -> Result<ContextEntry, Box<dyn Error>> {
    Ok(ContextEntry::File {
        path: RepoPath::parse(path)?,
        bytes: path.as_bytes().to_vec(),
        executable: false,
    })
}

fn oracle(tag: &str) -> Result<bool, Box<dyn Error>> {
    let name_len = tag
        .rsplit_once(':')
        .map_or(usize::MAX, |(name, _)| name.len());
    Ok(Regex::new(ORACLE)?.is_match(tag) && name_len <= 255)
}

fn request(role: &str) -> Result<ImageRequest, Box<dyn Error>> {
    Ok(ImageRequest {
        role: RoleName::parse(role)?,
        context_files: vec![entry("Dockerfile")?],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: b"FROM construct AS role\n".to_vec(),
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
        agents: vec![Agent::Codex],
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        read_write: Vec::new(),
        rebuild: Rebuild::No,
    })
}

fn corpus() -> Vec<String> {
    let mut names = Vec::new();
    let mut level: Vec<String> = vec![String::new()];
    for _ in 0..5 {
        level = level
            .iter()
            .flat_map(|prefix| ALPHABET.iter().map(move |c| format!("{prefix}{c}")))
            .collect();
        names.extend(level.iter().filter(|n| RoleName::parse(n).is_ok()).cloned());
    }
    names.extend([
        "A".repeat(63),
        format!("a{}b", ".".repeat(61)),
        format!("a{}B", "_".repeat(61)),
        "construct".to_owned(),
        "role".to_owned(),
    ]);
    names
}

fn check_tag(tag: &ImageTag, role: &str) -> TestResult {
    let text = tag.to_string();
    assert!(oracle(&text)?, "{role}: {text} is not a Docker reference");
    assert!(text.starts_with("jackin.invalid/"), "{role}: {text}");
    assert_eq!(&ImageTag::parse(&text)?, tag, "{role}: {text}");
    Ok(())
}

#[test]
fn every_valid_role_name_gives_docker_valid_distinct_tags() -> TestResult {
    let names = corpus();
    assert_eq!(names.len(), 2334 + 5);
    let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    let mut finals = HashSet::new();
    let mut stages = HashSet::new();
    let mut constructs = HashSet::new();
    for role in &names {
        let plan = ImagePlan::new(&request(role)?)?;
        let built = plan.final_builds(&construct);
        let hex16 = &built.key.sha256().as_str()[..16];
        let construct_tag = &plan.construct_build().tag;
        for tag in [&built.derived.tag, &built.role.tag, construct_tag] {
            check_tag(tag, role)?;
        }
        assert!(
            built
                .derived
                .tag
                .to_string()
                .ends_with(&format!(":{hex16}")),
            "{role}"
        );
        assert!(
            built.role.tag.to_string().ends_with(&format!(":{hex16}")),
            "{role}"
        );
        assert_eq!(plan.final_tag(&construct), built.derived.tag, "{role}");
        finals.insert(built.derived.tag.to_string());
        stages.insert(built.role.tag.to_string());
        constructs.insert(construct_tag.to_string());
    }
    assert_eq!(finals.len(), names.len(), "final tags are distinct");
    assert_eq!(stages.len(), names.len(), "role-stage tags are distinct");
    assert_eq!(constructs.len(), 1);
    let all: HashSet<&String> = finals.iter().chain(&stages).chain(&constructs).collect();
    assert_eq!(all.len(), finals.len() + stages.len() + constructs.len());
    Ok(())
}

#[test]
fn role_names_map_to_documented_repositories() -> TestResult {
    let key = Sha256::of(b"abc");
    let rows = [
        ("sentinel", "sentinel"),
        ("Sentinel", "sentinel"),
        ("Agent-Smith_2", "agent-smith-2"),
        ("my.role", "my-role"),
        ("a..b", "a-b"),
        ("a._-b", "a-b"),
        ("X", "x"),
        ("0day", "0day"),
        ("The.Architect", "the-architect"),
        ("a_b.c-d", "a-b-c-d"),
    ];
    let long = ("A".repeat(63), "a".repeat(63));
    for (role, repo) in rows
        .iter()
        .chain(std::iter::once(&(long.0.as_str(), long.1.as_str())))
    {
        let role_name = RoleName::parse(role)?;
        assert_eq!(
            ImageTag::for_role(&role_name, &key).to_string(),
            format!("jackin.invalid/{repo}:ba7816bf8f01cfea"),
            "{role}"
        );
        assert_eq!(
            ImageTag::for_role_stage(&role_name, &key).to_string(),
            format!("jackin.invalid/role/{repo}:ba7816bf8f01cfea"),
            "{role}"
        );
    }
    assert_eq!(
        ImageTag::for_construct(&key).to_string(),
        "jackin.invalid/construct:ba7816bf8f01cfea"
    );
    assert_eq!(
        ImageTag::for_role(&RoleName::parse("Foo")?, &key),
        ImageTag::for_role(&RoleName::parse("foo")?, &key)
    );
    Ok(())
}

#[test]
fn image_tag_parse_enforces_the_docker_reference_grammar() -> TestResult {
    let longest = format!("jackin.invalid/{}", "a".repeat(240));
    let too_long = format!("jackin.invalid/{}", "a".repeat(241));
    let accept = [
        "jackin.invalid/a",
        "jackin.invalid/a-b",
        "jackin.invalid/a.b",
        "jackin.invalid/a_b",
        "jackin.invalid/a__b",
        "jackin.invalid/a---b",
        "jackin.invalid/role/a",
        "jackin.invalid/construct",
        longest.as_str(),
    ];
    let reject = [
        "jackin.invalid/Sentinel",
        "jackin.invalid/a..b",
        "jackin.invalid/a._b",
        "jackin.invalid/a-.b",
        "jackin.invalid/a___b",
        "jackin.invalid/-a",
        "jackin.invalid/a-",
        "jackin.invalid/.a",
        "jackin.invalid//a",
        "jackin.invalid/a/",
        "jackin.invalid/a b",
        "jackin.invalid/é",
        "jackin.invalid/a/B",
        too_long.as_str(),
    ];
    assert_eq!(longest.len(), 255);
    assert_eq!(too_long.len(), 256);
    for (names, label) in [(accept.as_slice(), true), (reject.as_slice(), false)] {
        for name in names {
            let text = format!("{name}:{HEX16}");
            assert_eq!(oracle(&text)?, label, "oracle: {name}");
            match ImageTag::parse(&text) {
                Ok(tag) => {
                    assert!(label, "parse accepted {name}");
                    assert_eq!(tag.to_string(), text);
                }
                Err(error) => {
                    assert!(!label, "parse rejected {name}: {error}");
                    assert!(error.to_string().contains("image tag"), "{name}: {error}");
                }
            }
        }
    }
    Ok(())
}
