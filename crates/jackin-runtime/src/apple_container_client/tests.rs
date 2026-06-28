//! Tests for `apple_container_client`.
use super::*;

#[test]
fn parse_json_array_shape() {
    let json = r#"[{"name":"jk-a","status":"running"},{"name":"jk-b","status":"stopped"}]"#;
    let all = parse_all_containers_json(json);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "jk-a");
    assert!(all[0].is_running());
    assert!(!all[1].is_running());
}

#[test]
fn parse_ndjson_shape() {
    let json =
        "{\"name\":\"jk-a\",\"status\":\"running\"}\n{\"name\":\"jk-b\",\"status\":\"stopped\"}";
    let all = parse_all_containers_json(json);
    assert_eq!(all.len(), 2);
}

#[test]
fn parse_capitalized_keys_and_missing_status() {
    // apple/container's exact JSON shape is empirically determined; tolerate
    // capitalized keys and default a missing status to "unknown".
    let json = r#"[{"Name":"jk-a","State":"Running"},{"name":"jk-b"}]"#;
    let all = parse_all_containers_json(json);
    assert_eq!(all[0].name, "jk-a");
    assert!(all[0].is_running());
    assert_eq!(all[1].status, "unknown");
    assert!(!all[1].is_running());
}

#[test]
fn parse_empty_and_malformed() {
    assert!(parse_all_containers_json("").is_empty());
    assert!(parse_all_containers_json("   ").is_empty());
    // A malformed NDJSON line is skipped, not fatal.
    let json = "{\"name\":\"jk-a\",\"status\":\"running\"}\nnot json";
    assert_eq!(parse_all_containers_json(json).len(), 1);
}

#[tokio::test]
async fn fake_client_lifecycle_contract() {
    let client = FakeAppleContainerClient::new();
    let spec = AppleContainerSpec {
        image: "img".into(),
        env: vec![],
        mounts: vec![],
        caps_add: vec![],
    };
    client.run_container("jk-a", &spec).await.unwrap();
    assert!(
        client
            .inspect_container("jk-a")
            .await
            .unwrap()
            .unwrap()
            .is_running()
    );

    client.stop_container("jk-a").await.unwrap();
    assert!(
        !client
            .inspect_container("jk-a")
            .await
            .unwrap()
            .unwrap()
            .is_running()
    );

    // Prefix filtering matches the real client's list semantics.
    client.run_container("other", &spec).await.unwrap();
    let listed = client.list_containers("jk-").await.unwrap();
    assert_eq!(listed.len(), 1);

    client.remove_container("jk-a").await.unwrap();
    assert!(client.inspect_container("jk-a").await.unwrap().is_none());
}
