// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256 as Sha256Hasher};
use thiserror::Error;

use crate::agent::Agent;
use crate::display::plain;

const HEX: &[u8; 16] = b"0123456789abcdef";
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
const BASE36: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const SUFFIX_MODULUS: u64 = 36 * 36 * 36 * 36;
const LAUNCH_ID_LEN: usize = 26;
const RESERVED_PREFIXES: [&str; 7] = [
    "JACKIN_",
    "OPENAI_",
    "CODEX_",
    "DOCKER_",
    "BUILDX_",
    "LD_",
    "OPENSHELL_",
];
const RESERVED_NAMES: [&str; 6] = [
    "PATH",
    "HOME",
    "BASH_ENV",
    "ENV",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];
const ATTEMPT_MAX: u64 = 999_999_999_999_999_999;
const ATTEMPT_MAX_DIGITS: usize = 18;
const SANDBOX_PREFIX: &str = "jackin-";
const IMAGE_ID_PREFIX: &str = "sha256:";
const IMAGE_TAG_PREFIX: &str = "jackin.invalid/";
const OP_PREFIX: &str = "op://";
const IMAGE_TAG_HEX_LEN: usize = 16;
const IMAGE_NAME_MAX: usize = 255;
const IMAGE_LABEL_HEX_LEN: usize = 32;
const SHA256_HEX_LEN: usize = 64;
const SLUG_FALLBACK: &str = "ws";

#[derive(Debug, Error)]
pub enum IdError {
    #[error("{kind} is empty")]
    Empty { kind: &'static str },
    #[error("{kind} is {len} characters long and the limit is {max}")]
    TooLong {
        kind: &'static str,
        max: usize,
        len: usize,
    },
    #[error("{kind} has the invalid character {} at index {index}", plain(.ch))]
    InvalidChar {
        kind: &'static str,
        ch: char,
        index: usize,
    },
    #[error("{kind} must start and end with an alphanumeric character")]
    BadEdge { kind: &'static str },
    #[error("{kind} must be {expected}")]
    BadFormat {
        kind: &'static str,
        expected: &'static str,
    },
}

#[derive(Debug, Error)]
#[error("the operating system could not provide random bytes: {0}")]
pub struct EntropyError(getrandom::Error);

#[derive(Clone, Copy)]
enum Edge {
    Open,
    First,
    Both,
}

#[derive(Clone, Copy)]
struct Rule {
    max: usize,
    accepts: fn(char) -> bool,
    edge: Edge,
    single_hyphen: bool,
    quiet: Option<&'static str>,
}

impl Rule {
    fn check(&self, kind: &'static str, text: &str) -> Result<(), IdError> {
        let len = text.chars().count();
        if len == 0 {
            return Err(IdError::Empty { kind });
        }
        if len > self.max {
            return Err(IdError::TooLong {
                kind,
                max: self.max,
                len,
            });
        }
        match self.quiet {
            Some(expected) if !text.chars().all(self.accepts) => {
                return Err(IdError::BadFormat { kind, expected });
            }
            Some(_) => {}
            None => first_invalid(kind, text, 0, self.accepts)?,
        }
        let first_ok = text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        let last_ok = text
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        let edge_ok = match self.edge {
            Edge::Open => true,
            Edge::First => first_ok,
            Edge::Both => first_ok && last_ok,
        };
        if !edge_ok {
            return Err(IdError::BadEdge { kind });
        }
        if self.single_hyphen && text.contains("--") {
            return Err(IdError::BadFormat {
                kind,
                expected: "free of consecutive hyphens",
            });
        }
        Ok(())
    }
}

fn first_invalid(
    kind: &'static str,
    text: &str,
    offset: usize,
    accepts: fn(char) -> bool,
) -> Result<(), IdError> {
    match text.chars().enumerate().find(|(_, c)| !accepts(*c)) {
        Some((index, ch)) => Err(IdError::InvalidChar {
            kind,
            ch,
            index: index + offset,
        }),
        None => Ok(()),
    }
}

fn label_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn dns_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

fn account_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

fn model_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-')
}

fn hex_char(c: char) -> bool {
    c.is_ascii_digit() || ('a'..='f').contains(&c)
}

fn base36_char(c: char) -> bool {
    c.is_ascii_digit() || c.is_ascii_lowercase()
}

const LABEL_VALUE: Rule = Rule {
    max: 63,
    accepts: label_char,
    edge: Edge::Both,
    single_hyphen: false,
    quiet: None,
};
const ACCOUNT: Rule = Rule {
    max: 32,
    accepts: dns_char,
    edge: Edge::Both,
    single_hyphen: true,
    quiet: None,
};
const DNS_19: Rule = Rule {
    max: 19,
    accepts: dns_char,
    edge: Edge::Both,
    single_hyphen: true,
    quiet: None,
};
const SLUG: Rule = Rule {
    max: 7,
    accepts: dns_char,
    edge: Edge::Both,
    single_hyphen: true,
    quiet: None,
};
const DNS_NAME_63: Rule = Rule {
    max: 63,
    accepts: dns_char,
    edge: Edge::Both,
    single_hyphen: true,
    quiet: None,
};
const GATEWAY_LIKE: Rule = Rule {
    max: 64,
    accepts: label_char,
    edge: Edge::First,
    single_hyphen: false,
    quiet: None,
};
const CHATGPT_ACCOUNT: Rule = Rule {
    max: 64,
    accepts: account_id_char,
    edge: Edge::Open,
    single_hyphen: false,
    quiet: Some("ASCII letters, digits and hyphens"),
};
const MODEL: Rule = Rule {
    max: 64,
    accepts: model_char,
    edge: Edge::First,
    single_hyphen: false,
    quiet: None,
};

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn non_empty(kind: &'static str, text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        Err(IdError::Empty { kind })
    } else {
        Ok(())
    }
}

fn hex_text(
    kind: &'static str,
    text: &str,
    offset: usize,
    lens: &[usize],
    expected: &'static str,
) -> Result<(), IdError> {
    first_invalid(kind, text, offset, hex_char)?;
    if lens.contains(&text.len()) {
        Ok(())
    } else {
        Err(IdError::BadFormat { kind, expected })
    }
}

fn check_env_name(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    let valid = text
        .chars()
        .enumerate()
        .all(|(index, c)| c.is_ascii_alphabetic() || c == '_' || (index > 0 && c.is_ascii_digit()));
    if valid {
        Ok(())
    } else {
        Err(IdError::BadFormat {
            kind,
            expected: "letters, digits and underscores, not starting with a digit",
        })
    }
}

fn check_name_suffix(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    first_invalid(kind, text, 0, base36_char)?;
    if text.len() == NameSuffix::LEN {
        Ok(())
    } else {
        Err(IdError::BadFormat {
            kind,
            expected: "exactly 4 base-36 digits",
        })
    }
}

fn check_sandbox_name(kind: &'static str, text: &str) -> Result<(), IdError> {
    DNS_19.check(kind, text)?;
    if !text.starts_with(SANDBOX_PREFIX) {
        return Err(IdError::BadFormat {
            kind,
            expected: "prefixed with jackin-",
        });
    }
    Ok(())
}

fn plain_text(text: &str) -> bool {
    !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn url_host(hostport: &str) -> bool {
    let host = match hostport.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return false,
        None => hostport,
    };
    !host.is_empty() && !host.starts_with('-')
}

fn ssh_authority(authority: &str) -> bool {
    match authority.rsplit_once('@') {
        Some((user, hostport)) => {
            !user.is_empty()
                && !user.starts_with('-')
                && !user.contains([':', '@'])
                && url_host(hostport)
        }
        None => url_host(authority),
    }
}

fn scp_form(text: &str) -> bool {
    let Some((target, path)) = text.split_once(':') else {
        return false;
    };
    let Some((user, host)) = target.split_once('@') else {
        return false;
    };
    !path.is_empty()
        && !target.contains('/')
        && !user.is_empty()
        && !user.starts_with('-')
        && !host.is_empty()
        && !host.starts_with('-')
        && !host.contains('@')
}

fn check_role_url(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    let accepted = plain_text(text)
        && !text.starts_with('-')
        && if let Some(rest) = text.strip_prefix("https://") {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            !authority.contains('@') && url_host(authority)
        } else if let Some(rest) = text.strip_prefix("ssh://") {
            ssh_authority(rest.split(['/', '?', '#']).next().unwrap_or_default())
        } else {
            !text.contains("://") && scp_form(text)
        };
    if accepted {
        Ok(())
    } else {
        Err(IdError::BadFormat {
            kind,
            expected: "an https URL without credentials, an ssh URL, or scp-style text, with a real host and no option, password or control character",
        })
    }
}

fn check_role_rev(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    let charset = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if charset && !text.starts_with('-') && !text.contains("..") {
        Ok(())
    } else {
        Err(IdError::BadFormat {
            kind,
            expected: "a Git ref name of letters, digits, . _ - and /, not starting with a hyphen or containing ..",
        })
    }
}

fn check_commit_id(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    hex_text(kind, text, 0, &[40, 64], "40 or 64 lowercase hex digits")
}

fn check_sha256(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    hex_text(kind, text, 0, &[SHA256_HEX_LEN], "64 lowercase hex digits")
}

fn check_op_reference(kind: &'static str, text: &str) -> Result<(), IdError> {
    let bad = IdError::BadFormat {
        kind,
        expected: "op:// followed by at least two non-empty segments",
    };
    let Some(rest) = text.strip_prefix(OP_PREFIX) else {
        return Err(bad);
    };
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.len() < 2 || segments.iter().any(|s| s.is_empty()) {
        return Err(bad);
    }
    Ok(())
}

fn path_separator(run: &[u8]) -> bool {
    matches!(run, b"." | b"_" | b"__") || run.iter().all(|byte| *byte == b'-')
}

fn path_component(text: &str) -> bool {
    let is_alnum = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let runs: Vec<&[u8]> = text
        .as_bytes()
        .chunk_by(|a, b| is_alnum(a) == is_alnum(b))
        .collect();
    runs.len() % 2 == 1
        && runs.iter().enumerate().all(|(index, run)| {
            if index % 2 == 0 {
                run.iter().all(is_alnum)
            } else {
                path_separator(run)
            }
        })
}

fn check_image_tag(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    let bad = IdError::BadFormat {
        kind,
        expected: "jackin.invalid/<name>:<16 lowercase hex digits>",
    };
    let Some((name, tag)) = text.rsplit_once(':') else {
        return Err(bad);
    };
    if !name.starts_with(IMAGE_TAG_PREFIX) {
        return Err(bad);
    }
    let offset = text.chars().count() - tag.chars().count();
    first_invalid(kind, tag, offset, hex_char)?;
    if tag.len() != IMAGE_TAG_HEX_LEN {
        return Err(bad);
    }
    let len = name.chars().count();
    if len > IMAGE_NAME_MAX {
        return Err(IdError::TooLong {
            kind,
            max: IMAGE_NAME_MAX,
            len,
        });
    }
    if name.split('/').all(path_component) {
        Ok(())
    } else {
        Err(IdError::BadFormat {
            kind,
            expected: "slash-separated lowercase path components",
        })
    }
}

fn check_image_id(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    let Some(digits) = text.strip_prefix(IMAGE_ID_PREFIX) else {
        return Err(IdError::BadFormat {
            kind,
            expected: "sha256: followed by 64 lowercase hex digits",
        });
    };
    hex_text(
        kind,
        digits,
        IMAGE_ID_PREFIX.len(),
        &[SHA256_HEX_LEN],
        "sha256: followed by 64 lowercase hex digits",
    )
}

fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

fn check_branch_name(kind: &'static str, text: &str) -> Result<(), IdError> {
    non_empty(kind, text)?;
    first_invalid(kind, text, 0, |c| {
        !(c.is_control()
            || is_format_char(c)
            || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    })?;
    let bad = |expected: &'static str| Err(IdError::BadFormat { kind, expected });
    if text.starts_with('-') {
        return bad("not starting with a hyphen");
    }
    if text.starts_with('/') || text.ends_with('/') || text.contains("//") {
        return bad("free of empty path components");
    }
    if text.ends_with('.') {
        return bad("not ending with a dot");
    }
    if text.contains("..") {
        return bad("free of two consecutive dots");
    }
    if text.contains("@{") {
        return bad("free of the sequence @{");
    }
    if text == "HEAD" {
        return bad("not HEAD");
    }
    if text
        .split('/')
        .any(|part| part.starts_with('.') || part.strip_suffix(".lock").is_some())
    {
        return bad("free of components that start with a dot or end with .lock");
    }
    if text.starts_with("jackin/") {
        return bad("outside the jackin/ namespace");
    }
    Ok(())
}

macro_rules! string_id {
    ($name:ident, $kind:literal, rule $rule:expr) => {
        string_id!(@define $name, $kind, |text: &str| $rule.check($kind, text));
    };
    ($name:ident, $kind:literal, with $check:path) => {
        string_id!(@define $name, $kind, |text: &str| $check($kind, text));
    };
    (@define $name:ident, $kind:literal, $check:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(text: &str) -> Result<$name, IdError> {
                let check = $check;
                check(text)?;
                Ok($name(text.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(text: String) -> Result<$name, IdError> {
                $name::parse(&text)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<$name, IdError> {
                $name::parse(text)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(WorkspaceName, "workspace name", rule LABEL_VALUE);
string_id!(RoleName, "role name", rule LABEL_VALUE);
string_id!(LaunchId, "launch ID", rule LABEL_VALUE);
string_id!(ImageLabel, "image label", rule LABEL_VALUE);
string_id!(AccountName, "account name", rule ACCOUNT);
string_id!(OpenShellWorkspace, "OpenShell workspace", rule DNS_19);
string_id!(SandboxName, "sandbox name", with check_sandbox_name);
string_id!(SandboxSlug, "sandbox slug", rule SLUG);
string_id!(NameSuffix, "name suffix", with check_name_suffix);
string_id!(EnvName, "environment variable name", with check_env_name);
string_id!(ProviderName, "provider name", rule DNS_NAME_63);
string_id!(ProfileId, "profile ID", rule DNS_NAME_63);
string_id!(GatewayName, "gateway name", rule GATEWAY_LIKE);
string_id!(BuilderName, "builder name", rule GATEWAY_LIKE);
string_id!(ChatgptAccountId, "ChatGPT account ID", rule CHATGPT_ACCOUNT);
string_id!(BranchName, "branch name", with check_branch_name);
string_id!(CommitId, "commit ID", with check_commit_id);
string_id!(Sha256, "SHA-256 digest", with check_sha256);
string_id!(OpReference, "1Password reference", with check_op_reference);
string_id!(ModelName, "model name", rule MODEL);
string_id!(ImageTag, "image tag", with check_image_tag);
string_id!(RoleUrl, "role URL", with check_role_url);
string_id!(RoleRev, "role revision", with check_role_rev);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct ImageId(Sha256);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64")]
pub struct AttemptId(NonZeroU64);

impl WorkspaceName {
    pub fn slug(&self) -> SandboxSlug {
        let mut slug = String::new();
        let mut pending_hyphen = false;
        for c in self.0.chars() {
            if c.is_ascii_alphanumeric() {
                if pending_hyphen && !slug.is_empty() {
                    slug.push('-');
                }
                pending_hyphen = false;
                slug.push(c.to_ascii_lowercase());
            } else {
                pending_hyphen = true;
            }
        }
        slug.truncate(SandboxSlug::MAX_LEN);
        let trimmed = slug.trim_end_matches('-');
        if trimmed.is_empty() {
            SandboxSlug(SLUG_FALLBACK.to_owned())
        } else {
            SandboxSlug(trimmed.to_owned())
        }
    }
}

impl SandboxSlug {
    pub const MAX_LEN: usize = 7;
}

impl NameSuffix {
    pub const LEN: usize = 4;

    pub fn for_launch(id: &LaunchId) -> NameSuffix {
        let digest = Sha256Hasher::digest(id.0.as_bytes());
        let number = digest
            .iter()
            .take(8)
            .fold(0_u64, |acc, byte| (acc << 8) | u64::from(*byte));
        let mut rest = number % SUFFIX_MODULUS;
        let mut digits = [b'0'; NameSuffix::LEN];
        for slot in digits.iter_mut().rev() {
            *slot = BASE36[(rest % 36) as usize];
            rest /= 36;
        }
        NameSuffix(digits.iter().copied().map(char::from).collect())
    }
}

impl SandboxName {
    pub const MAX_LEN: usize = 19;

    pub fn new(slug: &SandboxSlug, suffix: &NameSuffix) -> SandboxName {
        SandboxName(format!("{SANDBOX_PREFIX}{slug}-{suffix}"))
    }
}

impl LaunchId {
    pub fn generate() -> Result<LaunchId, EntropyError> {
        let mut entropy = [0_u8; LAUNCH_ID_LEN];
        getrandom::fill(&mut entropy).map_err(EntropyError)?;
        Ok(LaunchId::from_entropy(&entropy))
    }

    pub fn from_entropy(entropy: &[u8; LAUNCH_ID_LEN]) -> LaunchId {
        LaunchId(
            entropy
                .iter()
                .map(|byte| char::from(BASE32[usize::from(byte & 31)]))
                .collect(),
        )
    }
}

impl AttemptId {
    pub const FIRST: AttemptId = AttemptId(NonZeroU64::MIN);

    const KIND: &'static str = "attempt ID";

    pub fn parse(text: &str) -> Result<AttemptId, IdError> {
        let len = text.chars().count();
        if len == 0 {
            return Err(IdError::Empty { kind: Self::KIND });
        }
        if len > ATTEMPT_MAX_DIGITS {
            return Err(IdError::TooLong {
                kind: Self::KIND,
                max: ATTEMPT_MAX_DIGITS,
                len,
            });
        }
        first_invalid(Self::KIND, text, 0, |c| c.is_ascii_digit())?;
        text.parse::<u64>()
            .ok()
            .filter(|_| !text.starts_with('0'))
            .and_then(NonZeroU64::new)
            .map(AttemptId)
            .ok_or(IdError::BadFormat {
                kind: Self::KIND,
                expected: "a positive decimal integer without a leading zero",
            })
    }

    pub fn next(self) -> Option<AttemptId> {
        let next = self.0.get() + 1;
        if next > ATTEMPT_MAX {
            return None;
        }
        NonZeroU64::new(next).map(AttemptId)
    }
}

impl TryFrom<u64> for AttemptId {
    type Error = IdError;

    fn try_from(value: u64) -> Result<AttemptId, IdError> {
        if value > ATTEMPT_MAX {
            return Err(IdError::TooLong {
                kind: AttemptId::KIND,
                max: ATTEMPT_MAX_DIGITS,
                len: value.to_string().len(),
            });
        }
        NonZeroU64::new(value)
            .map(AttemptId)
            .ok_or(IdError::BadFormat {
                kind: AttemptId::KIND,
                expected: "a positive decimal integer without a leading zero",
            })
    }
}

impl FromStr for AttemptId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<AttemptId, IdError> {
        AttemptId::parse(text)
    }
}

impl fmt::Display for AttemptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ProviderName {
    pub fn for_account(account: &AccountName, agent: Agent) -> ProviderName {
        ProviderName(format!("jackin-{account}-{}", agent.as_str()))
    }
}

impl Sha256 {
    pub fn of(bytes: &[u8]) -> Sha256 {
        Sha256(hex_digest(&Sha256Hasher::digest(bytes)))
    }
}

impl ImageId {
    const KIND: &'static str = "image ID";

    pub fn parse(text: &str) -> Result<ImageId, IdError> {
        check_image_id(Self::KIND, text)?;
        Ok(ImageId(Sha256(text[IMAGE_ID_PREFIX.len()..].to_owned())))
    }

    pub fn label_value(&self) -> ImageLabel {
        ImageLabel(self.0.0[..IMAGE_LABEL_HEX_LEN].to_owned())
    }

    pub fn full(&self) -> &str {
        &self.0.0
    }
}

impl TryFrom<String> for ImageId {
    type Error = IdError;

    fn try_from(text: String) -> Result<ImageId, IdError> {
        ImageId::parse(&text)
    }
}

impl FromStr for ImageId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<ImageId, IdError> {
        ImageId::parse(text)
    }
}

impl fmt::Display for ImageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{IMAGE_ID_PREFIX}{}", self.0)
    }
}

impl Serialize for ImageId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl ImageTag {
    pub fn for_construct(key: &Sha256) -> ImageTag {
        ImageTag::build("construct", key)
    }

    pub fn for_role_stage(role: &RoleName, key: &Sha256) -> ImageTag {
        ImageTag::build(&format!("role/{}", role.repository()), key)
    }

    pub fn for_role(role: &RoleName, key: &Sha256) -> ImageTag {
        ImageTag::build(&role.repository(), key)
    }

    fn build(path: &str, key: &Sha256) -> ImageTag {
        ImageTag(format!(
            "{IMAGE_TAG_PREFIX}{path}:{}",
            &key.0[..IMAGE_TAG_HEX_LEN]
        ))
    }
}

impl RoleName {
    fn repository(&self) -> String {
        let mut repository = String::with_capacity(self.0.len());
        let mut pending_hyphen = false;
        for c in self.0.chars() {
            if c.is_ascii_alphanumeric() {
                if pending_hyphen {
                    repository.push('-');
                }
                pending_hyphen = false;
                repository.push(c.to_ascii_lowercase());
            } else {
                pending_hyphen = !repository.is_empty();
            }
        }
        repository
    }
}

impl EnvName {
    pub(crate) fn from_static(name: &'static str) -> EnvName {
        EnvName(name.to_owned())
    }

    pub fn is_reserved(&self) -> bool {
        let name = self.as_str();
        RESERVED_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
            || RESERVED_NAMES.contains(&name)
            || Agent::ALL.iter().any(|agent| {
                let spec = agent.spec();
                let keys = &spec.openshell.keys;
                spec.reserved_env.contains(&name)
                    || [
                        keys.access_token,
                        keys.refresh_token,
                        keys.account_id,
                        keys.id_token,
                    ]
                    .contains(&name)
            })
    }
}
