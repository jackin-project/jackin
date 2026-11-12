// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt::{Debug, Display};
use std::str::FromStr;

use jackin_core::{IdError, RoleRev, RoleUrl};
use serde::Serialize;
use serde::de::DeserializeOwned;

type TestResult = Result<(), Box<dyn Error>>;

fn grammar<T>(accept: &[&str], reject: &[&str]) -> TestResult
where
    T: FromStr<Err = IdError> + Display + Serialize + DeserializeOwned + Debug + PartialEq,
{
    for text in accept {
        let parsed: T = text
            .parse()
            .map_err(|error| format!("{text:?} was rejected: {error}"))?;
        assert_eq!(parsed.to_string(), *text);
        let json = serde_json::to_string(text)?;
        assert_eq!(serde_json::to_string(&parsed)?, json);
        assert_eq!(serde_json::from_str::<T>(&json)?, parsed);
    }
    for text in reject {
        let error = text
            .parse::<T>()
            .err()
            .ok_or_else(|| format!("{text:?} was accepted"))?;
        let shown = format!("{error} {error:?}");
        if !text.is_empty() {
            assert!(!shown.contains(text), "{text:?} echoed in {shown}");
        }
        let json = serde_json::to_string(text)?;
        assert!(
            serde_json::from_str::<T>(&json).is_err(),
            "{text:?} deserialized"
        );
    }
    Ok(())
}

#[test]
fn role_url_accepts_only_https_ssh_and_scp_forms_without_secrets_or_options() -> TestResult {
    let accept = [
        "https://github.com/org/repo.git",
        "https://example.com:8443/a/b.git",
        "ssh://git@example.com/org/repo.git",
        "ssh://git@example.com:2222/org/repo.git",
        "ssh://example.com/org/repo",
        "git@github.com:org/repo.git",
        "deploy@git.example.com:team/sub/repo",
    ];
    let controlled: Vec<String> = ["a b", "a\tb", "a\nb", "a\0b", "a\u{7f}b", "a\u{a0}b"]
        .iter()
        .map(|tail| format!("https://example.com/{tail}"))
        .collect();
    let mut reject: Vec<&str> = vec![
        "",
        "--upload-pack=x",
        "-oProxyCommand=id@host:path",
        "--upload-pack=touch /tmp/pwn",
        "ssh://-oProxyCommand=id/x",
        "git@-oProxyCommand=id:x",
        "ext::sh",
        "fd::3",
        "file:///etc/passwd",
        "file:rel",
        "/srv/git/repo.git",
        "./repo",
        "../repo",
        "repo.git",
        "git://example.com/r.git",
        "http://example.com/r.git",
        "ftp://example.com/r",
        "https://user@example.com/org/repo.git",
        "https://ghp_Zq7Xv9Lm2@github.com/org/repo.git",
        "https://user:pw@example.com/r.git",
        "https://:pw@example.com/r",
        "ssh://git:pw@example.com/r",
        "https://",
        "https:///repo",
        "git@:path",
        "@host:path",
        "host:path",
        "ssh://-oProxyCommand=id@example.com/x",
        "ssh://git@[::1]/r.git",
        "ssh://git@example.com:abc/r",
        "ssh://git@example.com:/r",
        "https://example.com:abc/r",
        "https://example.com:/r",
        "https://example.com/org/repo?access_token=x",
        "https://example.com/org/repo.git?x=1",
        "https://example.com/org/repo#frag",
        "https://example.com?x=1",
        "https://example.com#frag",
        "https://example.com:8443/org/repo?x",
        "ssh://git@example.com/org/repo?x=1",
        "ssh://git@example.com/org/repo#frag",
        "ssh://example.com?x=1",
        "ssh://git@example.com:2222/org/repo?x",
    ];
    reject.extend(controlled.iter().map(String::as_str));
    grammar::<RoleUrl>(&accept, &reject)?;
    for secret in ["pw", "ghp_Zq7Xv9Lm2"] {
        for text in reject.iter().filter(|text| text.contains(secret)) {
            let error = text.parse::<RoleUrl>().err().ok_or("accepted")?;
            let shown = format!("{error} {error:?}");
            for window in secret.as_bytes().windows(3) {
                let window = std::str::from_utf8(window)?;
                assert!(!shown.contains(window), "{window} in {shown}");
            }
        }
    }
    Ok(())
}

#[test]
fn role_rev_accepts_ref_names_and_rejects_options_and_revspec_syntax() -> TestResult {
    let hex40 = "0123456789abcdef0123456789abcdef01234567";
    let hex64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let accept = [
        "main",
        "v1",
        "v1.2.3",
        "feature/x",
        "release/1.2",
        "refs/tags/v1",
        "a_b-c.d",
        hex40,
        hex64,
    ];
    let reject = [
        "",
        "-x",
        "--output=/tmp/x",
        "--",
        "a b",
        "a\tb",
        "a\nb",
        "a\0b",
        "a\u{7f}b",
        "a:b",
        "a^b",
        "a~1",
        "a?b",
        "a*b",
        "a[b",
        "a\\b",
        "a..b",
        "a@{b",
    ];
    grammar::<RoleRev>(&accept, &reject)
}
