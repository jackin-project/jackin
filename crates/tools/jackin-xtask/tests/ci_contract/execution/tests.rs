use super::*;
use serde_json::json;

const ALPHA: &str = "stack/rust/crates/alpha/clippy/default";
const BETA: &str = "stack/rust/crates/beta/nextest/default";
const DIGEST_ALPHA: &str = "b3-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_BETA: &str = "b3-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

type Fixture = (Map<String, Value>, Map<String, Value>, RustJobOwners);

fn fixture() -> Fixture {
    let task = |id: &str, digest: &str| json!({"task_id": id, "execution_digest": digest});
    let step = |digest: &str| {
        json!({
            "uses": "./.github/actions/declared-task-0", "with": {"digest": digest}
        })
    };
    let tasks = Map::from_iter([
        (ALPHA.to_owned(), task(ALPHA, DIGEST_ALPHA)),
        (BETA.to_owned(), task(BETA, DIGEST_BETA)),
    ]);
    let jobs = json!({
        "rust-alpha": {"steps": [step(DIGEST_ALPHA)]},
        "rust-beta": {"steps": [step(DIGEST_BETA)]},
        "rust-alpha-feature": {"steps": []},
        "check-unrelated": {"steps": []}
    })
    .as_object()
    .expect("fixture job map")
    .clone();
    let owners = RustJobOwners::from([
        (
            ("crates/alpha".into(), "default".into()),
            "rust-alpha".into(),
        ),
        (
            ("crates/alpha".into(), "feature".into()),
            "rust-alpha-feature".into(),
        ),
        (("crates/beta".into(), "default".into()), "rust-beta".into()),
    ]);
    (tasks, jobs, owners)
}

fn rejected(fixture: &Fixture, message: &str) {
    let error = assert_rust_execution_inventory(&fixture.0, &fixture.1, &fixture.2)
        .expect_err("mutated execution inventory must reject");
    assert!(error.to_string().contains(message), "{error}");
}

#[test]
fn complete_execution_inventory_without_per_step_gates_is_accepted() {
    let (tasks, jobs, owners) = fixture();
    assert_rust_execution_inventory(&tasks, &jobs, &owners).unwrap();
}

#[test]
fn manifest_record_removed_while_execution_remains_is_rejected() {
    let mut input = fixture();
    input.0.remove(ALPHA).unwrap();
    rejected(&input, "inventory count");
}

#[test]
fn extra_native_execution_is_rejected() {
    let mut input = fixture();
    let extra = json!({"uses": "./.github/actions/declared-task-0",
        "with": {"digest": "b3-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}});
    input.1["rust-alpha"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    rejected(&input, "inventory count");
}

#[test]
fn equal_count_but_different_execution_set_is_rejected() {
    let mut input = fixture();
    input.1["rust-alpha"]["steps"][0]["with"]["digest"] = json!(DIGEST_BETA);
    rejected(&input, "inventory set");
}

#[test]
fn duplicate_execution_outside_rust_jobs_is_rejected_globally() {
    let mut input = fixture();
    let duplicate = input.1["rust-alpha"]["steps"][0].clone();
    input.1["check-unrelated"]["steps"] = json!([duplicate]);
    rejected(&input, "one global execution owner");
}

#[test]
fn execution_moved_to_wrong_crate_is_rejected() {
    let mut input = fixture();
    let moved = input.1["rust-alpha"]["steps"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    input.1["rust-beta"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(moved);
    rejected(&input, "exact crate/configuration job");
}

#[test]
fn execution_moved_to_other_configuration_is_rejected() {
    let mut input = fixture();
    let moved = input.1["rust-alpha"]["steps"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    input.1["rust-alpha-feature"]["steps"] = json!([moved]);
    rejected(&input, "exact crate/configuration job");
}

#[test]
fn duplicate_manifest_execution_digest_is_rejected() {
    let mut input = fixture();
    input.0[BETA]["execution_digest"] = json!(DIGEST_ALPHA);
    rejected(&input, "unique Rust execution digest");
}
