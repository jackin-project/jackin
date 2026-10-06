#[test]
fn construct_installs_linux_clipboard_helpers_for_agent_compatibility() {
    let dockerfile = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docker/construct/Dockerfile"),
    )
    .expect("read construct Dockerfile");
    for package in ["wl-clipboard", "xauth", "xclip"] {
        assert!(
            dockerfile.contains(package),
            "construct image must install {package} for Linux agent clipboard helper compatibility"
        );
    }
}
