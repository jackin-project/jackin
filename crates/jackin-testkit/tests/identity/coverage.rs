// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_console::fixtures::Scenario;
use tuiscotti::check::{CheckOutcome, FrozenStore};

use crate::baseline;

const JOURNEYS: [&str; 3] = [
    include_str!("manager.rs"),
    include_str!("fixed.rs"),
    include_str!("capsule.rs"),
];

const WITHOUT_CAPTURE: [&str; 9] = [
    "jackin_cockpit_docker_names_key_and_egress_bypass",
    "jackin_roles_add_role",
    "jackin_fixed_dirty_exit_refuses_the_list_by_key_and_click",
    "jackin_fixed_working_directory_refuses_the_picker_by_key_and_click",
    "jackin_fixed_last_allowed_role_cannot_be_unchecked",
    "jackin_fixed_last_allowed_role_refuses_a_click",
    "jackin_fixed_env_form_refuses_a_credential_reference_and_notes_a_secret_name",
    "jackin_fixed_workspace_env_keeps_the_role_scope_live_and_refuses_the_sources",
    "jackin_launch_questions_end_the_launch_on_escape",
];

const WITHOUT_APPROVED_FILE: [&str; 4] = [
    "launch-running",
    "capsule-multi",
    "outro-last",
    "hard-cases",
];

fn tests_of(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    source
        .split("#[test]\nfn ")
        .skip(1)
        .filter_map(|chunk| {
            let (name, body) = chunk.split_once('(')?;
            let visuals = body
                .split(".visual(\"")
                .skip(1)
                .filter_map(|rest| rest.split_once('"').map(|(id, _)| id.to_owned()))
                .collect();
            Some((name.to_owned(), visuals))
        })
        .collect()
}

#[test]
fn every_baseline_journey_with_a_capture_is_driven_with_the_same_captures() {
    let baseline: BTreeMap<_, _> = baseline::journeys()
        .iter()
        .flat_map(|(_, source)| tests_of(source))
        .collect();
    let ours: BTreeMap<_, _> = JOURNEYS.iter().flat_map(|s| tests_of(s)).collect();
    let mut problems = Vec::new();
    for (name, visuals) in &baseline {
        match (visuals.is_empty(), ours.get(name)) {
            (true, None) if WITHOUT_CAPTURE.contains(&name.as_str()) => {}
            (true, _) => problems.push(format!("{name} has no capture and is not excluded")),
            (false, None) => problems.push(format!("{name} is not driven")),
            (false, Some(driven)) if driven != visuals => {
                problems.push(format!(
                    "{name} compares {driven:?}, the baseline {visuals:?}"
                ));
            }
            (false, Some(_)) => {}
        }
    }
    for name in ours.keys() {
        if !baseline.contains_key(name) {
            problems.push(format!("{name} is not a baseline journey"));
        }
    }
    for name in WITHOUT_CAPTURE {
        if baseline.get(name).is_none_or(|visuals| !visuals.is_empty()) {
            problems.push(format!("{name} is excluded but has a capture or is gone"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_approved_file_is_compared_by_one_journey() {
    let driven: BTreeSet<String> = JOURNEYS
        .iter()
        .flat_map(|source| tests_of(source).into_values().flatten())
        .collect();
    let directory = baseline::root().join("functional");
    let approved: BTreeSet<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("a directory entry").file_name())
        .filter_map(|name| {
            let name = name.to_string_lossy().into_owned();
            name.strip_suffix(".tui.json")
                .map(|stem| format!("functional/{stem}"))
        })
        .collect();
    assert_eq!(driven, approved);
}

#[test]
fn every_scenario_is_driven_or_has_no_approved_file() {
    let source = [JOURNEYS.concat(), include_str!("steps.rs").to_owned()].concat();
    let mut problems = Vec::new();
    for scenario in Scenario::ALL {
        let name = scenario.name();
        let driven = source.contains(&format!("\"{name}\""));
        let without = WITHOUT_APPROVED_FILE.contains(&name);
        if driven == without {
            problems.push(format!("{name}: driven {driven}, without a file {without}"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_one_cell_colour_change_fails_the_check() {
    let path = baseline::root().join("functional/jackin_launch_reconnect.tui.json");
    let approved = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let marker = "\"fg\":\"#";
    let last = approved.find(marker).expect("a foreground colour") + marker.len() + 5;
    let flipped = if approved.as_bytes()[last] == b'f' {
        "e"
    } else {
        "f"
    };
    let changed = format!("{}{flipped}{}", &approved[..last], &approved[last + 1..]);
    let artifacts = tempfile::tempdir().expect("an artifact directory");
    let store = FrozenStore::new(baseline::root(), artifacts.path()).expect("a frozen store");

    let same = store
        .check(approved.as_bytes())
        .expect("check the approved file");
    assert!(same.is_pass(), "{same:?}");
    match store
        .check(changed.as_bytes())
        .expect("check the changed file")
    {
        CheckOutcome::Failure(failure) => assert_eq!(failure.changed_cell_total(), 1),
        CheckOutcome::Pass(_) => panic!("a one-cell colour change passed"),
    }
}
