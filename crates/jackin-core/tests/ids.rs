// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::process::{Command, Stdio};
use std::str::FromStr;

use jackin_core::{
    AccountName, Agent, AttemptId, BranchName, BuilderName, ChatgptAccountId, CommitId, EnvName,
    GatewayName, IdError, ImageId, ImageLabel, ImageTag, LaunchId, ModelName, NameSuffix,
    OpReference, OpenShellWorkspace, ProfileId, ProviderName, RoleName, SandboxName, SandboxSlug,
    Sha256, WorkspaceName,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, PartialEq)]
enum Want {
    Empty,
    TooLong(usize, usize),
    CharAt(char, usize),
    Char(char),
    Edge,
    Format,
}

fn kind_of(error: &IdError) -> &'static str {
    match error {
        IdError::Empty { kind }
        | IdError::TooLong { kind, .. }
        | IdError::InvalidChar { kind, .. }
        | IdError::BadEdge { kind }
        | IdError::BadFormat { kind, .. } => kind,
    }
}

fn satisfies(want: &Want, error: &IdError) -> bool {
    match (want, error) {
        (Want::Empty, IdError::Empty { .. })
        | (Want::Edge, IdError::BadEdge { .. })
        | (Want::Format, IdError::BadFormat { .. }) => true,
        (Want::TooLong(max, len), IdError::TooLong { max: m, len: l, .. }) => max == m && len == l,
        (
            Want::CharAt(ch, index),
            IdError::InvalidChar {
                ch: c, index: i, ..
            },
        ) => ch == c && index == i,
        (Want::Char(ch), IdError::InvalidChar { ch: c, .. }) => ch == c,
        _ => false,
    }
}

fn grammar<T>(accept: &[&str], reject: &[(String, Want)]) -> Result<&'static str, Box<dyn Error>>
where
    T: FromStr<Err = IdError> + Display + Serialize + DeserializeOwned + Debug + PartialEq,
{
    for text in accept {
        let parsed: T = text.parse()?;
        assert_eq!(parsed.to_string(), *text);
        let json = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&parsed)?, json);
        assert_eq!(serde_json::from_str::<T>(&json)?, parsed);
    }
    let mut kind = "";
    for (text, want) in reject {
        let error = text
            .parse::<T>()
            .err()
            .ok_or_else(|| format!("{text:?} was accepted"))?;
        assert!(
            satisfies(want, &error),
            "{text:?}: wanted {want:?}, got {error:?}"
        );
        assert!(error.to_string().contains(kind_of(&error)), "{error}");
        kind = kind_of(&error);
        let json = serde_json::to_string(text)?;
        assert!(
            serde_json::from_str::<T>(&json).is_err(),
            "{text:?} deserialized"
        );
    }
    Ok(kind)
}

fn rep(ch: char, count: usize) -> String {
    ch.to_string().repeat(count)
}

fn rej(text: impl Into<String>, want: Want) -> (String, Want) {
    (text.into(), want)
}

fn distinct(kinds: &[&str]) {
    let unique: HashSet<&&str> = kinds.iter().collect();
    assert_eq!(unique.len(), kinds.len(), "{kinds:?}");
}

#[test]
fn label_value_ids_follow_the_gateway_grammar() -> TestResult {
    let max = rep('a', 63);
    let accept = [
        "a",
        "Z9",
        "my-project",
        "My.Project_1",
        "a-b.c_d",
        max.as_str(),
    ];
    let reject = vec![
        rej("", Want::Empty),
        rej(rep('a', 64), Want::TooLong(63, 64)),
        rej("-ab", Want::Edge),
        rej("_ab", Want::Edge),
        rej(".ab", Want::Edge),
        rej("ab-", Want::Edge),
        rej("ab.", Want::Edge),
        rej("ab_", Want::Edge),
        rej("a b", Want::CharAt(' ', 1)),
        rej("a/b", Want::CharAt('/', 1)),
        rej("a:b", Want::Char(':')),
        rej("a\u{e9}b", Want::CharAt('\u{e9}', 1)),
        rej("a\nb", Want::CharAt('\n', 1)),
    ];
    distinct(&[
        grammar::<WorkspaceName>(&accept, &reject)?,
        grammar::<RoleName>(&accept, &reject)?,
        grammar::<LaunchId>(&accept, &reject)?,
        grammar::<ImageLabel>(&accept, &reject)?,
    ]);
    Ok(())
}

#[test]
fn account_name_and_openshell_workspace_are_capped_dns_labels() -> TestResult {
    let account_max = rep('a', 32);
    let account_accept = ["a", "work", "team-1", "0day", account_max.as_str()];
    let account_reject = vec![
        rej("", Want::Empty),
        rej(rep('a', 33), Want::TooLong(32, 33)),
        rej("Work", Want::CharAt('W', 0)),
        rej("wo_rk", Want::CharAt('_', 2)),
        rej("wo.rk", Want::CharAt('.', 2)),
        rej("-work", Want::Edge),
        rej("work-", Want::Edge),
    ];
    let workspace_max = rep('a', 19);
    let workspace_accept = ["default", "a", "a-b", workspace_max.as_str()];
    let workspace_reject = vec![
        rej(rep('a', 20), Want::TooLong(19, 20)),
        rej("Default", Want::CharAt('D', 0)),
        rej("-x", Want::Edge),
        rej("a--b", Want::Format),
        rej("a---b", Want::Format),
        rej("aaaa--aaaa", Want::Format),
    ];
    distinct(&[
        grammar::<AccountName>(&account_accept, &account_reject)?,
        grammar::<OpenShellWorkspace>(&workspace_accept, &workspace_reject)?,
    ]);
    Ok(())
}

#[test]
fn sandbox_name_parse_enforces_prefix_length_and_single_hyphens() -> TestResult {
    let accept = ["jackin-ws-ab12", "jackin-a-0000", "jackin-abcdefg-wxyz"];
    let reject = vec![
        rej("", Want::Empty),
        rej("jackin-abcdefgh-wxyz", Want::TooLong(19, 20)),
        rej("jackin-WS-ab12", Want::CharAt('W', 7)),
        rej("jackin-w_s-ab1", Want::CharAt('_', 8)),
        rej("jackin-ws--ab12", Want::Format),
        rej("ws-ab12", Want::Format),
        rej("jackin-ws-ab12-", Want::Edge),
    ];
    grammar::<SandboxName>(&accept, &reject)?;
    Ok(())
}

#[test]
fn slug_and_suffix_have_exact_grammars() -> TestResult {
    let slug_accept = ["a", "ws", "my-proj", "abcdefg"];
    let slug_reject = vec![
        rej("", Want::Empty),
        rej("abcdefgh", Want::TooLong(7, 8)),
        rej("My", Want::CharAt('M', 0)),
        rej("-a", Want::Edge),
        rej("a-", Want::Edge),
        rej("a--b", Want::Format),
        rej("a_b", Want::CharAt('_', 1)),
    ];
    let suffix_accept = ["ab12", "0000", "zzzz"];
    let suffix_reject = vec![
        rej("", Want::Empty),
        rej("abc", Want::Format),
        rej("abcde", Want::Format),
        rej("AB12", Want::CharAt('A', 0)),
        rej("ab-1", Want::CharAt('-', 2)),
    ];
    distinct(&[
        grammar::<SandboxSlug>(&slug_accept, &slug_reject)?,
        grammar::<NameSuffix>(&suffix_accept, &suffix_reject)?,
    ]);
    Ok(())
}

#[test]
fn attempt_id_parses_one_to_eighteen_digits_and_steps_to_the_next() -> TestResult {
    for text in ["1", "9", "10", "123456789012345678", "999999999999999999"] {
        let parsed: AttemptId = text.parse()?;
        assert_eq!(parsed.to_string(), text);
    }
    let rejected = vec![
        rej("", Want::Empty),
        rej("0", Want::Format),
        rej("00", Want::Format),
        rej("01", Want::Format),
        rej("a1", Want::CharAt('a', 0)),
        rej("-1", Want::CharAt('-', 0)),
        rej("+1", Want::CharAt('+', 0)),
        rej("1 ", Want::CharAt(' ', 1)),
        rej("1000000000000000000", Want::TooLong(18, 19)),
        rej(rep('7', 20), Want::TooLong(18, 20)),
    ];
    for (text, want) in &rejected {
        let error = AttemptId::parse(text)
            .err()
            .ok_or_else(|| format!("{text:?} was accepted"))?;
        assert!(
            satisfies(want, &error),
            "{text:?}: wanted {want:?}, got {error:?}"
        );
        assert!(error.to_string().contains(kind_of(&error)));
    }
    assert_eq!(AttemptId::FIRST, AttemptId::parse("1")?);
    assert_eq!(AttemptId::FIRST.next(), Some(AttemptId::parse("2")?));
    let before_max = AttemptId::parse("999999999999999998")?;
    assert_eq!(
        before_max.next(),
        Some(AttemptId::parse("999999999999999999")?)
    );
    assert_eq!(AttemptId::parse("999999999999999999")?.next(), None);
    let copy = before_max;
    assert_eq!(copy, before_max);
    Ok(())
}

#[test]
fn env_name_is_a_posix_name_and_reserved_names_are_detected() -> TestResult {
    let accept = ["A", "_x", "FOO_BAR1", "lower_ok"];
    let reject = vec![
        rej("", Want::Empty),
        rej("1A", Want::Format),
        rej("A-B", Want::Format),
        rej("A B", Want::Format),
        rej("A=B", Want::Format),
        rej("\u{c9}", Want::Format),
    ];
    grammar::<EnvName>(&accept, &reject)?;
    let reserved = [
        "JACKIN_X",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_CONTEXT",
        "BUILDX_BUILDER",
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "CODEX_HOME",
        "CODEX_API_KEY",
        "CODEX_AUTH_ID_TOKEN",
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "OPENSHELL_GATEWAY",
        "PATH",
        "HOME",
        "BASH_ENV",
        "ENV",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ];
    for name in reserved {
        assert!(EnvName::parse(name)?.is_reserved(), "{name}");
    }
    let free = [
        "FOO",
        "JACKIN",
        "JACKINX",
        "XJACKIN_A",
        "DOCKER",
        "MY_DOCKER_HOST",
        "OPENAI",
        "MY_OPENAI_KEY",
        "CODEX",
        "LD",
        "LDX_A",
        "PATHS",
        "PATH_EXTRA",
        "MY_PATH",
        "HOMEDIR",
        "ENVX",
        "SSL_CERT",
        "SSL_CERT_FILES",
        "ANTHROPIC_API_KEY",
    ];
    for name in free {
        assert!(!EnvName::parse(name)?.is_reserved(), "{name}");
    }
    Ok(())
}

#[test]
fn other_ids_reject_one_input_of_each_violated_class() -> TestResult {
    let hex40 = "0123456789abcdef0123456789abcdef01234567";
    let hex64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let tag = "0123456789abcdef";
    let mut kinds = Vec::new();

    kinds.push(grammar::<ProviderName>(
        &["jackin-work-codex", "a"],
        &[
            rej("", Want::Empty),
            rej(rep('a', 64), Want::TooLong(63, 64)),
            rej("Codex", Want::CharAt('C', 0)),
            rej("-a", Want::Edge),
            rej("a--b", Want::Format),
            rej("a_b", Want::CharAt('_', 1)),
        ],
    )?);
    kinds.push(grammar::<ProfileId>(
        &["codex"],
        &[rej("co_dex", Want::CharAt('_', 2)), rej("", Want::Empty)],
    )?);
    kinds.push(grammar::<GatewayName>(
        &["jackin-local", "My.Gw_1", "a-b", "a-", "a."],
        &[
            rej("", Want::Empty),
            rej(rep('a', 65), Want::TooLong(64, 65)),
            rej("a b", Want::CharAt(' ', 1)),
            rej("-x", Want::Edge),
            rej("--help", Want::Edge),
            rej("-", Want::Edge),
            rej(".", Want::Edge),
        ],
    )?);
    kinds.push(grammar::<BuilderName>(
        &["orbstack", "a-b", "a-", "a."],
        &[
            rej("a/b", Want::CharAt('/', 1)),
            rej("-x", Want::Edge),
            rej("--help", Want::Edge),
            rej("-", Want::Edge),
            rej(".", Want::Edge),
        ],
    )?);
    kinds.push(grammar::<ChatgptAccountId>(
        &["67e55044-10b1-426f-9247-bb680e5fe0c8"],
        &[
            rej("a_b", Want::Format),
            rej(rep('a', 65), Want::TooLong(64, 65)),
        ],
    )?);
    kinds.push(grammar::<CommitId>(
        &[hex40, hex64],
        &[
            rej(&hex40[..39], Want::Format),
            rej(format!("{hex40}0"), Want::Format),
            rej(format!("{}g", &hex40[..39]), Want::CharAt('g', 39)),
        ],
    )?);
    kinds.push(grammar::<Sha256>(
        &[hex64],
        &[
            rej(&hex64[..63], Want::Format),
            rej(format!("{hex64}0"), Want::Format),
            rej(format!("A{}", &hex64[1..]), Want::Char('A')),
        ],
    )?);
    kinds.push(grammar::<OpReference>(
        &["op://vault/item", "op://vault/item/field"],
        &[
            rej("", Want::Format),
            rej("op://vault", Want::Format),
            rej("op://vault/", Want::Format),
            rej("op:///item", Want::Format),
            rej("op://vault//field", Want::Format),
            rej("http://vault/item", Want::Format),
        ],
    )?);
    kinds.push(grammar::<ModelName>(
        &["gpt-5.1-codex", "o3", "openai/gpt-4:latest"],
        &[
            rej("", Want::Empty),
            rej(rep('a', 65), Want::TooLong(64, 65)),
            rej("-gpt", Want::Edge),
            rej("gpt\"x", Want::CharAt('"', 3)),
            rej("gpt x", Want::CharAt(' ', 3)),
            rej("a\nb", Want::CharAt('\n', 1)),
            rej("a\"\nmodel_provider=\"x", Want::Char('"')),
        ],
    )?);
    let good_tag = format!("jackin.invalid/my-role:{tag}");
    let nested_tag = format!("jackin.invalid/role/my-role:{tag}");
    let construct_tag = format!("jackin.invalid/construct:{tag}");
    kinds.push(grammar::<ImageTag>(
        &[
            good_tag.as_str(),
            nested_tag.as_str(),
            construct_tag.as_str(),
        ],
        &[
            rej("", Want::Empty),
            rej(format!("other/my-role:{tag}"), Want::Format),
            rej(format!("jackin/my-role:{tag}"), Want::Format),
            rej(format!("docker.io/jackin/my-role:{tag}"), Want::Format),
            rej(
                format!("jackin.invalid.example/my-role:{tag}"),
                Want::Format,
            ),
            rej(format!("jackin.invalidx/my-role:{tag}"), Want::Format),
            rej(format!("xjackin.invalid/my-role:{tag}"), Want::Format),
            rej(format!("localhost/my-role:{tag}"), Want::Format),
            rej(
                format!("reg.example/jackin.invalid/my-role:{tag}"),
                Want::Format,
            ),
            rej(format!("jackin.invalid:{tag}"), Want::Format),
            rej(
                format!("jackin.invalid/my-role:{}", &tag[..15]),
                Want::Format,
            ),
            rej(
                format!("jackin.invalid/my-role:{}g", &tag[..15]),
                Want::Char('g'),
            ),
        ],
    )?);
    let good_id = format!("sha256:{hex64}");
    kinds.push(grammar::<ImageId>(
        &[good_id.as_str()],
        &[
            rej("", Want::Empty),
            rej(format!("sha512:{hex64}"), Want::Format),
            rej(hex64, Want::Format),
            rej(format!("sha256:{}", &hex64[..63]), Want::Format),
            rej(format!("sha256:A{}", &hex64[1..]), Want::Char('A')),
        ],
    )?);
    distinct(&kinds);
    Ok(())
}

fn git_accepts(candidate: &str, dir: &std::path::Path) -> Result<bool, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", candidate])
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?
        .wait_with_output()?;
    Ok(output.status.success())
}

#[test]
fn branch_name_agrees_with_git_check_ref_format() -> TestResult {
    let dir = tempfile::tempdir()?;
    let candidates = [
        "main",
        "feature/x-1",
        "release/1.2",
        "a/b/c",
        "x.y",
        "a..b",
        "x.lock",
        "a/x.lock",
        "a b",
        "a~b",
        "a^b",
        "a:b",
        "a?b",
        "a*b",
        "a[b",
        "/a",
        "a/",
        "a//b",
        "a.",
        ".a",
        "@",
        "a@{b",
        "a\\b",
        "-a",
    ];
    let mut accepted = 0;
    for candidate in candidates {
        let ours = BranchName::parse(candidate).is_ok();
        assert_eq!(ours, git_accepts(candidate, dir.path())?, "{candidate:?}");
        accepted += usize::from(ours);
    }
    assert!(accepted >= 5);
    let error = BranchName::parse("jackin/x").err().ok_or("accepted")?;
    assert!(satisfies(&Want::Format, &error), "{error:?}");
    let error = BranchName::parse("").err().ok_or("accepted")?;
    assert!(satisfies(&Want::Empty, &error), "{error:?}");
    let main: BranchName = "main".parse()?;
    assert_eq!(main.to_string(), "main");
    assert_eq!(serde_json::to_string(&main)?, "\"main\"");
    assert_eq!(serde_json::from_str::<BranchName>("\"main\"")?, main);
    Ok(())
}

const CF_UNICODE_17_0_0: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

fn is_format_17_0_0(c: char) -> bool {
    CF_UNICODE_17_0_0
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&u32::from(c)))
}

fn branch_rejects(name: &str) -> bool {
    BranchName::parse(name).is_err()
}

#[test]
fn branch_name_rejects_every_control_and_format_character_and_nothing_else() -> TestResult {
    for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
        let forbidden = c.is_control()
            || is_format_17_0_0(c)
            || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\');
        let name = format!("a{c}b");
        assert_eq!(branch_rejects(&name), forbidden, "U+{:04X}", u32::from(c));
    }
    for c in ('\u{80}'..='\u{9f}').chain(['\u{85}', '\u{202e}', '\u{2066}', '\u{2069}']) {
        let error = BranchName::parse(&format!("a{c}b"))
            .err()
            .ok_or("accepted")?;
        assert!(satisfies(&Want::CharAt(c, 1), &error), "{c:?}: {error:?}");
    }
    for accepted in [
        "caf\u{e9}",
        "\u{65e5}\u{672c}\u{8a9e}/x",
        "a\u{ac}b",
        "a\u{ae}b",
        "a\u{2065}b",
    ] {
        assert_eq!(BranchName::parse(accepted)?.as_str(), accepted);
    }
    Ok(())
}

#[test]
fn git_accepts_the_control_and_format_names_that_branch_name_deliberately_refuses() -> TestResult {
    let dir = tempfile::tempdir()?;
    for c in [
        '\u{9b}',
        '\u{85}',
        '\u{202e}',
        '\u{2066}',
        '\u{200b}',
        '\u{ad}',
        '\u{feff}',
        '\u{e0020}',
    ] {
        let name = format!("a{c}b");
        assert!(
            git_accepts(&name, dir.path())?,
            "git rejects U+{:04X}",
            u32::from(c)
        );
        assert!(branch_rejects(&name), "U+{:04X}", u32::from(c));
    }
    Ok(())
}

#[test]
fn launch_id_generate_yields_valid_distinct_ids() -> TestResult {
    let ids: Vec<LaunchId> = (0..64)
        .map(|_| LaunchId::generate())
        .collect::<Result<_, _>>()?;
    for id in &ids {
        let text = id.to_string();
        assert_eq!(text.len(), 26, "{text}");
        assert!(
            text.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "{text}"
        );
        assert_eq!(LaunchId::parse(&text)?, *id);
    }
    let distinct: HashSet<String> = ids.iter().map(ToString::to_string).collect();
    assert_eq!(distinct.len(), 64);
    Ok(())
}

#[test]
fn launch_id_encodes_entropy_one_byte_per_character_and_generate_uses_every_symbol_at_every_position()
-> TestResult {
    const SYMBOLS: &str = "abcdefghijklmnopqrstuvwxyz234567";
    for (value, symbol) in (0_u8..32).zip(SYMBOLS.chars()) {
        assert_eq!(
            LaunchId::from_entropy(&[value; 26]).to_string(),
            rep(symbol, 26),
            "{value}"
        );
    }
    assert_eq!(LaunchId::from_entropy(&[32; 26]).to_string(), rep('a', 26));
    let mut ramp = [0_u8; 26];
    for (slot, value) in ramp.iter_mut().zip(0_u8..) {
        *slot = value;
    }
    assert_eq!(
        LaunchId::from_entropy(&ramp).to_string(),
        "abcdefghijklmnopqrstuvwxyz"
    );
    let mut seen = vec![HashSet::new(); 26];
    let mut all = HashSet::new();
    for _ in 0..2048 {
        let text = LaunchId::generate()?.to_string();
        assert_eq!(text, LaunchId::parse(&text)?.to_string());
        assert_eq!(text.len(), 26);
        for (position, symbol) in text.chars().enumerate() {
            seen[position].insert(symbol);
        }
        assert!(all.insert(text));
    }
    for (position, symbols) in seen.iter().enumerate() {
        assert_eq!(symbols.len(), 32, "position {position}");
    }
    Ok(())
}

#[test]
fn sha256_image_tag_and_image_label_match_known_vectors() -> TestResult {
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(Sha256::of(b"abc").to_string(), abc);
    assert_eq!(
        Sha256::of(b"").to_string(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        ImageTag::for_role(&RoleName::parse("backend")?, &Sha256::of(b"abc")).to_string(),
        "jackin.invalid/backend:ba7816bf8f01cfea"
    );
    let input = format!("sha256:{abc}");
    let image = ImageId::parse(&input)?;
    assert_eq!(image.to_string(), input);
    let label = image.label_value();
    assert_eq!(label.to_string(), "ba7816bf8f01cfea414140de5dae2223");
    ImageLabel::parse(&label.to_string())?;
    Ok(())
}

#[test]
fn provider_name_for_every_valid_account_name_is_a_valid_provider_name() -> TestResult {
    let long = rep('a', 32);
    let candidates = ["work", "a", "x-y", "0day", "a--b", long.as_str(), "a-b-c"];
    let mut accepted = Vec::new();
    for candidate in candidates {
        let Ok(account) = AccountName::parse(candidate) else {
            continue;
        };
        accepted.push(candidate);
        let provider = ProviderName::for_account(&account, Agent::Codex);
        assert_eq!(provider.to_string(), format!("jackin-{candidate}-codex"));
        assert_eq!(ProviderName::parse(&provider.to_string())?, provider);
    }
    for control in ["work", "x-y", long.as_str()] {
        assert!(accepted.contains(&control), "{control}");
    }
    Ok(())
}

#[test]
fn an_invalid_character_is_shown_as_plain_text_even_when_it_is_a_control_character() {
    let shown = |text: &str| {
        ProviderName::parse(text)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    };
    assert_eq!(
        shown("a$b"),
        "provider name has the invalid character $ at index 1"
    );
    assert_eq!(
        shown("a\nb"),
        "provider name has the invalid character \\n at index 1"
    );
    assert_eq!(
        shown("a\u{1b}b"),
        "provider name has the invalid character \\u{1b} at index 1"
    );
    assert_eq!(
        shown("a\u{202e}b"),
        "provider name has the invalid character \\u{202e} at index 1"
    );
}
