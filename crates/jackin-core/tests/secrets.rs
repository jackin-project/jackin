// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;
use std::time::Duration;

use jackin_core::{
    AccountName, AttemptId, BranchName, BuilderName, ChatgptAccountId, CommitId, EnvFile, EnvName,
    EnvSnapshot, EnvValue, GatewayName, IdError, ImageId, ImageLabel, ImageTag, LaunchId,
    ModelName, NameSuffix, OpReference, OpenShellWorkspace, OutputLine, OutputStream, ProcessError,
    ProcessOutput, ProcessSpec, ProcessStatus, ProcessStdin, ProfileId, Program, ProviderName,
    Redactor, RoleName, RoleRev, RoleUrl, SandboxName, SandboxSlug, Secret, SecretBytes,
    SecretOutput, Sha256, WorkspaceName,
};
use serde::Serialize;
use static_assertions::assert_not_impl_any;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SENTINEL: &str = "Zq7#Xv9!Lm2@Hk4wP";

fn windows(secret: &str) -> Vec<String> {
    let chars: Vec<char> = secret.chars().collect();
    chars.windows(3).map(|w| w.iter().collect()).collect()
}

fn assert_no_leak(secret: &str, output: &str) {
    for window in windows(secret) {
        assert!(!output.contains(&window), "leaked {window:?} in {output:?}");
    }
}

fn assert_no_byte_leak(output: &str) {
    let numbers: Vec<&str> = output
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect();
    let bytes: Vec<String> = SENTINEL.bytes().map(|b| b.to_string()).collect();
    for run in numbers.windows(3) {
        assert!(
            !bytes.windows(3).any(|known| known == run),
            "leaked bytes {run:?} in {output:?}"
        );
    }
}

#[test]
fn secret_debug_output_hides_the_value_wherever_it_is_nested() {
    let secret = Secret::new(SENTINEL.to_owned());
    let bytes = SecretBytes::new(SENTINEL.as_bytes().to_vec());
    let optional = Some(Secret::new(SENTINEL.to_owned()));
    let listed = vec![
        Secret::new(SENTINEL.to_owned()),
        Secret::new(SENTINEL.to_owned()),
    ];
    let redactor = Redactor::new([&secret]);
    let spec = ProcessSpec {
        program: Program::Git,
        args: vec!["--flag".to_owned()],
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Bytes(SecretBytes::new(SENTINEL.as_bytes().to_vec())),
        timeout: Duration::from_secs(1),
    };
    for rendered in [
        format!("{secret:?}"),
        format!("{secret:#?}"),
        format!("{bytes:?}"),
        format!("{bytes:#?}"),
        format!("{optional:?}"),
        format!("{optional:#?}"),
        format!("{listed:?}"),
        format!("{listed:#?}"),
        format!("{redactor:?}"),
        format!("{redactor:#?}"),
        format!("{spec:?}"),
        format!("{spec:#?}"),
    ] {
        assert_no_leak(SENTINEL, &rendered);
        assert_no_byte_leak(&rendered);
    }
    assert_eq!(secret.expose(), SENTINEL);
    assert_eq!(bytes.expose(), SENTINEL.as_bytes());
    assert!(format!("{spec:?}").contains("--flag"));
}

#[test]
fn secret_types_cannot_be_displayed_or_serialized() {
    assert_not_impl_any!(Secret: Serialize, Display);
    assert_not_impl_any!(SecretBytes: Serialize, Display);
    assert_not_impl_any!(EnvValue: Serialize, Display);
    assert_not_impl_any!(SecretOutput: Serialize, Display, Clone);
}

#[test]
fn redactor_masks_every_occurrence_of_every_secret() {
    let a = Secret::new(SENTINEL.to_owned());
    let b = Secret::new("p\u{e4}ssw\u{f6}rd-\u{20ac}-9".to_owned());
    let label = "PRIVATE KEY";
    let c = Secret::new(format!(
        "-----BEGIN {label}-----\nMIIE$(x)'\"\\z\n-----END {label}-----"
    ));
    let redactor = Redactor::new([&a, &b, &c]);
    let mask = redactor.apply(a.expose());
    assert!(!mask.is_empty());
    let (a_text, b_text, c_text) = (a.expose(), b.expose(), c.expose());
    let cases = [
        (
            format!("error: {a_text} rejected"),
            format!("error: {mask} rejected"),
        ),
        (
            format!("{a_text} {a_text} {a_text}"),
            format!("{mask} {mask} {mask}"),
        ),
        (format!("{a_text}{a_text}"), format!("{mask}{mask}")),
        (
            format!("Bearer {a_text}.suffix"),
            format!("Bearer {mask}.suffix"),
        ),
        (format!("pre-{a_text}-post"), format!("pre-{mask}-post")),
        (
            format!("\u{e9} {a_text} \u{20ac}"),
            format!("\u{e9} {mask} \u{20ac}"),
        ),
        (format!("x {b_text} y"), format!("x {mask} y")),
        (format!("key:\n{c_text}\nend"), format!("key:\n{mask}\nend")),
        (
            format!("a\nb {a_text}\nc {b_text}\n"),
            format!("a\nb {mask}\nc {mask}\n"),
        ),
    ];
    for (input, expected) in cases {
        let output = redactor.apply(&input);
        assert_eq!(output, expected);
        for secret in [a_text, b_text, c_text] {
            assert_no_leak(secret, &output);
        }
    }
}

#[test]
fn redactor_handles_overlap_empty_and_absent_secrets() {
    let short = Secret::new("abcd1234".to_owned());
    let long = Secret::new("abcd1234efgh".to_owned());
    let redactor = Redactor::new([&short, &long]);
    let mask = redactor.apply("abcd1234efgh");
    let output = redactor.apply("x abcd1234efgh y");
    assert_eq!(output, format!("x {mask} y"));
    assert!(!output.contains("efgh"));

    let empty = Secret::new(String::new());
    assert_eq!(Redactor::new([&empty]).apply("hello"), "hello");
    assert_eq!(
        Redactor::new([]).apply("any text at all"),
        "any text at all"
    );
    assert_eq!(redactor.apply("nothing to hide"), "nothing to hide");
}

type Probe = fn(&str) -> Option<String>;

fn rejection<T: FromStr<Err = IdError>>(text: &str) -> Option<String> {
    text.parse::<T>()
        .err()
        .map(|error| format!("{error} {error:?}"))
}

#[test]
fn errors_never_echo_a_secret_bearing_input() -> TestResult {
    let inputs = [
        SENTINEL.to_owned(),
        SENTINEL.repeat(8),
        format!("sk-{SENTINEL}"),
    ];
    let probes: Vec<(&str, Probe)> = vec![
        ("WorkspaceName", rejection::<WorkspaceName>),
        ("RoleName", rejection::<RoleName>),
        ("LaunchId", rejection::<LaunchId>),
        ("ImageLabel", rejection::<ImageLabel>),
        ("AccountName", rejection::<AccountName>),
        ("OpenShellWorkspace", rejection::<OpenShellWorkspace>),
        ("SandboxName", rejection::<SandboxName>),
        ("SandboxSlug", rejection::<SandboxSlug>),
        ("NameSuffix", rejection::<NameSuffix>),
        ("EnvName", rejection::<EnvName>),
        ("ProviderName", rejection::<ProviderName>),
        ("ProfileId", rejection::<ProfileId>),
        ("GatewayName", rejection::<GatewayName>),
        ("BuilderName", rejection::<BuilderName>),
        ("ChatgptAccountId", rejection::<ChatgptAccountId>),
        ("CommitId", rejection::<CommitId>),
        ("Sha256", rejection::<Sha256>),
        ("OpReference", rejection::<OpReference>),
        ("ModelName", rejection::<ModelName>),
        ("ImageTag", rejection::<ImageTag>),
        ("RoleUrl", rejection::<RoleUrl>),
        ("RoleRev", rejection::<RoleRev>),
        ("ImageId", rejection::<ImageId>),
        ("AttemptId", rejection::<AttemptId>),
        ("BranchName", rejection::<BranchName>),
    ];
    let mut checked = 0;
    for (name, probe) in &probes {
        for input in &inputs {
            match probe(input) {
                Some(rendered) => {
                    assert_no_leak(SENTINEL, &rendered);
                    checked += 1;
                }
                None => assert_eq!(*name, "BranchName", "{name} accepted {input:?}"),
            }
        }
    }
    assert_eq!(checked, 24 * 3);

    let name = EnvName::parse("A")?;
    let invalid = EnvFile::new(BTreeMap::from([(name, format!("{SENTINEL}\n"))]))
        .err()
        .ok_or("accepted")?;
    assert_no_leak(SENTINEL, &format!("{invalid} {invalid:?}"));
    let syntax = EnvFile::parse(&format!("export A='{SENTINEL}"))
        .err()
        .ok_or("accepted")?;
    assert_no_leak(SENTINEL, &format!("{syntax} {syntax:?}"));
    Ok(())
}

#[derive(Debug)]
struct Chain {
    env: EnvSnapshot,
    spec: ProcessSpec,
    output: Result<ProcessOutput, ProcessError>,
    lines: Vec<OutputLine>,
    secret: SecretOutput,
}

fn chain() -> Result<Chain, Box<dyn std::error::Error>> {
    Ok(Chain {
        env: EnvSnapshot::from_pairs([
            ("ANTHROPIC_API_KEY".to_owned(), SENTINEL.to_owned()),
            ("HOME".to_owned(), "/home/probe-user".to_owned()),
        ]),
        spec: ProcessSpec {
            program: Program::Op,
            args: vec!["read".to_owned(), "op://vault/item".to_owned()],
            cwd: None,
            env: vec![
                (
                    "OP_SERVICE_ACCOUNT_TOKEN".to_owned(),
                    EnvValue::Secret(Secret::new(SENTINEL.to_owned())),
                ),
                ("LANG".to_owned(), EnvValue::Plain("en_US.UTF-8".to_owned())),
            ],
            env_remove_prefixes: vec!["OPENSHELL_".to_owned()],
            stdin: ProcessStdin::Bytes(SecretBytes::new(SENTINEL.as_bytes().to_vec())),
            timeout: Duration::from_secs(1),
        },
        output: Ok(ProcessOutput {
            status: ProcessStatus::Exited(0),
            stdout: SENTINEL.as_bytes().to_vec(),
            stderr: format!("boom: {SENTINEL}\n").into_bytes(),
        }),
        lines: vec![OutputLine {
            stream: OutputStream::Stdout,
            text: SENTINEL.to_owned(),
        }],
        secret: SecretOutput {
            status: ProcessStatus::Exited(0),
            stdout: SecretBytes::new(SENTINEL.as_bytes().to_vec()),
            stderr: SENTINEL.as_bytes().to_vec(),
        },
    })
}

#[test]
fn process_boundary_debug_prints_names_and_lengths_never_values() -> TestResult {
    let chain = chain()?;
    for (rendered, status) in [
        (format!("{chain:?}"), "Exited(0)"),
        (format!("{chain:#?}"), "Exited("),
    ] {
        assert_no_leak(SENTINEL, &rendered);
        assert_no_byte_leak(&rendered);
        assert!(!rendered.contains("/home/probe-user"), "{rendered}");
        for control in [
            "ANTHROPIC_API_KEY",
            "OP_SERVICE_ACCOUNT_TOKEN",
            "en_US.UTF-8",
            "op://vault/item",
            "OPENSHELL_",
            status,
            "17",
            "24",
        ] {
            assert!(
                rendered.contains(control),
                "{control} missing in {rendered}"
            );
        }
    }
    assert_eq!(
        chain.env.get(&EnvName::parse("HOME")?),
        Some("/home/probe-user")
    );
    assert_eq!(chain.spec.args.len(), 2);
    assert_eq!(chain.lines[0].text, SENTINEL);
    assert_eq!(chain.secret.stdout.expose(), SENTINEL.as_bytes());
    assert_eq!(chain.output?.stderr.len(), 24);
    Ok(())
}

#[test]
fn redactor_masks_each_line_of_a_multi_line_secret_and_merges_overlapping_matches() {
    let secret = "-----BEGIN KEY-----\nAAAAAAAA\nBBBBBBBB\n-----END KEY-----";
    let redact = |secrets: &[&str], text: &str| {
        let secrets: Vec<Secret> = secrets
            .iter()
            .map(|secret| Secret::new((*secret).to_owned()))
            .collect();
        Redactor::new(secrets.iter()).apply(text)
    };
    let mask = redact(&["abc"], "abc");
    assert!(!mask.is_empty());
    for line in ["AAAAAAAA", "BBBBBBBB", "-----BEGIN KEY-----"] {
        assert_eq!(
            redact(&[secret], &format!("line: {line}")),
            format!("line: {mask}")
        );
    }
    assert_eq!(
        redact(&[secret], &format!("x\n{secret}\ny")),
        format!("x\n{mask}\ny")
    );
    let blank = "AAAAAAAA\n\nBBBBBBBB";
    assert_eq!(redact(&[blank], ""), "");
    assert_eq!(redact(&[blank], "\n"), "\n");
    assert_eq!(redact(&["AAAAAAAA\r\nBBBBBBBB"], "AAAAAAAA"), mask);
    assert_eq!(redact(&["AAAAAAAA\r\nBBBBBBBB"], "BBBBBBBB"), mask);
    let rows: [(&[&str], &str, String); 4] = [
        (&["abc", "bcd"], "abcd", mask.clone()),
        (&["abc", "bcd", "cde"], "xabcdey", format!("x{mask}y")),
        (&["abc", "def"], "abcdef", format!("{mask}{mask}")),
        (
            &["AAAAAAAA\nBBBBBBBB", "BBBBBBBBcccc"],
            "AAAAAAAA\nBBBBBBBBcccc",
            mask.clone(),
        ),
    ];
    for (secrets, text, expected) in rows {
        assert_eq!(redact(secrets, text), expected, "{secrets:?} {text:?}");
    }
}

#[test]
fn credential_adjacent_ids_never_echo_a_rejected_character() {
    let probes: [(&str, Probe); 3] = [
        ("EnvName", rejection::<EnvName>),
        ("ChatgptAccountId", rejection::<ChatgptAccountId>),
        ("OpReference", rejection::<OpReference>),
    ];
    let inputs = [
        SENTINEL.to_owned(),
        format!("sk-{SENTINEL}"),
        "\u{e9}\u{20ac}".to_owned(),
        "a#b".to_owned(),
    ];
    for (name, probe) in probes {
        for input in &inputs {
            let rendered = probe(input).unwrap_or_default();
            assert!(!rendered.is_empty(), "{name} accepted {input:?}");
            for echoed in ['#', '!', '@', '\u{20ac}', '\u{e9}'] {
                assert!(!rendered.contains(echoed), "{name} {input:?}: {rendered}");
            }
        }
    }
}
