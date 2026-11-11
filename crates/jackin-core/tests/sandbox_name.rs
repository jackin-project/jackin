// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use jackin_core::{LaunchId, NameSuffix, SandboxName, SandboxSlug, WorkspaceName};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn workspace(text: &str) -> Result<WorkspaceName, Box<dyn std::error::Error>> {
    Ok(WorkspaceName::parse(text)?)
}

fn n1_inputs() -> Vec<(String, &'static str)> {
    vec![
        ("jackin".to_owned(), "jackin"),
        ("My.Project_Name".to_owned(), "my-proj"),
        ("abcdef-ghi".to_owned(), "abcdef"),
        ("a__b".to_owned(), "a-b"),
        ("A".to_owned(), "a"),
        ("1.2.3".to_owned(), "1-2-3"),
        ("abcdefgh".to_owned(), "abcdefg"),
        ("ab-----cdefgh".to_owned(), "ab-cdef"),
        ("a.b.c.d.e.f.g.h".to_owned(), "a-b-c-d"),
        ("x._-.y".to_owned(), "x-y"),
        ("Z".repeat(63), "zzzzzzz"),
        ("9lives".to_owned(), "9lives"),
        ("ab.cdef.g".to_owned(), "ab-cdef"),
    ]
}

#[test]
fn workspace_slug_follows_the_d07_rules() -> TestResult {
    for (input, expected) in n1_inputs() {
        assert_eq!(workspace(&input)?.slug().to_string(), expected, "{input}");
    }
    Ok(())
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn generated_names() -> Vec<String> {
    const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const ALL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-";
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..200)
        .map(|_| {
            let len = 1 + (next(&mut state) % 63) as usize;
            let mut bytes: Vec<u8> = (0..len)
                .map(|_| ALL[(next(&mut state) % ALL.len() as u64) as usize])
                .collect();
            bytes[0] = ALNUM[(next(&mut state) % ALNUM.len() as u64) as usize];
            bytes[len - 1] = ALNUM[(next(&mut state) % ALNUM.len() as u64) as usize];
            bytes.into_iter().map(char::from).collect()
        })
        .collect()
}

fn check_fits(name: &SandboxName, slug: &SandboxSlug, suffix: &NameSuffix) {
    let text = name.to_string();
    assert!(text.len() <= 19, "{text}");
    assert!(text.starts_with("jackin-"), "{text}");
    assert!(
        text.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "{text}"
    );
    assert!(!text.contains("--"), "{text}");
    assert!(!text.ends_with('-'), "{text}");
    let tail = &text[text.len() - 5..];
    assert!(tail.starts_with('-'), "{text}");
    assert!(
        tail[1..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{text}"
    );
    assert_eq!(&text[7..text.len() - 5], slug.to_string(), "{text}");
    assert_eq!(&tail[1..], suffix.to_string());
}

#[test]
fn sandbox_name_always_fits_the_gateway() -> TestResult {
    let launches: Vec<LaunchId> = ["launch-1", "0123456789abcdefghijklmnop", "A.b_c-9"]
        .iter()
        .map(|text| LaunchId::parse(text))
        .collect::<Result<_, _>>()?;
    let mut corpus: Vec<String> = n1_inputs().into_iter().map(|(input, _)| input).collect();
    corpus.extend(generated_names());
    assert_eq!(corpus.len(), 213);
    for input in &corpus {
        let ws = workspace(input)?;
        let slug = ws.slug();
        assert!(SandboxSlug::parse(&slug.to_string()).is_ok(), "{input}");
        for launch in &launches {
            let suffix = NameSuffix::for_launch(launch);
            let name = SandboxName::new(&slug, &suffix);
            check_fits(&name, &slug, &suffix);
            assert_eq!(SandboxName::parse(&name.to_string())?, name);
        }
    }
    Ok(())
}

#[test]
fn name_suffix_is_stable_and_differs_across_launch_ids() -> TestResult {
    let launches: Vec<LaunchId> = (1..=8)
        .map(|i| LaunchId::parse(&format!("launch-{i}")))
        .collect::<Result<_, _>>()?;
    let suffixes: Vec<NameSuffix> = launches.iter().map(NameSuffix::for_launch).collect();
    let unique: HashSet<String> = suffixes.iter().map(ToString::to_string).collect();
    assert_eq!(unique.len(), 8);
    for (launch, suffix) in launches.iter().zip(&suffixes) {
        assert_eq!(&NameSuffix::for_launch(launch), suffix);
    }
    for (launch, golden) in [("launch-1", "3n9s"), ("abcdefghijklmnopqrstuvwxyz", "u8tr")] {
        let id = LaunchId::parse(launch)?;
        assert_eq!(NameSuffix::for_launch(&id).to_string(), golden, "{launch}");
    }
    let slug = workspace("backend")?.slug();
    let names: HashSet<String> = suffixes
        .iter()
        .map(|suffix| SandboxName::new(&slug, suffix).to_string())
        .collect();
    assert_eq!(names.len(), 8);
    Ok(())
}
