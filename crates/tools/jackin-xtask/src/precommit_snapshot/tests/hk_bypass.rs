use std::{env, fs, process::Command};

const CHILD: &str = "JACKIN_HK_BYPASS_CHILD";

#[test]
fn emergency_bypass_precedes_repository_and_tool_access() {
    if env::var_os(CHILD).is_some() {
        let result = crate::precommit_snapshot::run_from_hook();
        if env::var_os("HK").as_deref() == Some(std::ffi::OsStr::new("0")) {
            assert_eq!(result.expect("emergency bypass succeeds"), 0);
        } else {
            assert!(result.is_err(), "normal hook requires repository and tools");
        }
        return;
    }
    let root = tempfile::tempdir().expect("create isolated bypass fixture");
    let scratch = root.path().join("scratch");
    fs::create_dir(&scratch).expect("create empty scratch parent");
    for hk in ["0", "1"] {
        let output = crate::precommit_snapshot::process::capture(Command::new(env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                "precommit_snapshot::tests::hk_bypass::emergency_bypass_precedes_repository_and_tool_access",
                "--nocapture",
            ])
            .env_clear()
            .env(CHILD, "1")
            .env("HK", hk)
            .env("PATH", root.path().join("missing-tools"))
            .env("TMPDIR", &scratch)
            .current_dir(root.path()))
            .expect("run actual hook entry point in isolated process");
        assert!(output.status.success(), "{output:?}");
        if hk == "0" {
            assert!(
                crate::fs_util::read_dir_sorted(&scratch)
                    .expect("read scratch parent")
                    .is_empty()
            );
        }
    }
}
