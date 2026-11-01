// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;
use std::time::Duration;

use jackin_core::{
    AccountName, AttemptId, BranchName, BuilderName, ChatgptAccountId, CommitId, EnvFile, EnvName,
    GatewayName, IdError, ImageId, ImageLabel, ImageTag, LaunchId, ModelName, NameSuffix,
    OpReference, OpenShellWorkspace, ProcessSpec, ProcessStdin, ProfileId, Program, ProviderName,
    Redactor, RoleName, SandboxName, SandboxSlug, Secret, SecretBytes, Sha256, WorkspaceName,
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
    }
    assert_eq!(secret.expose(), SENTINEL);
    assert_eq!(bytes.expose(), SENTINEL.as_bytes());
    assert!(format!("{spec:?}").contains("--flag"));
}

#[test]
fn secret_types_cannot_be_displayed_or_serialized() {
    assert_not_impl_any!(Secret: Serialize, Display);
    assert_not_impl_any!(SecretBytes: Serialize, Display);
}

#[test]
fn redactor_masks_every_occurrence_of_every_secret() {
    let a = Secret::new(SENTINEL.to_owned());
    let b = Secret::new("p\u{e4}ssw\u{f6}rd-\u{20ac}-9".to_owned());
    let c = Secret::new(
        "-----BEGIN PRIVATE KEY-----\nMIIE$(x)'\"\\z\n-----END PRIVATE KEY-----".to_owned(),
    );
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
    assert_eq!(checked, 22 * 3);

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
