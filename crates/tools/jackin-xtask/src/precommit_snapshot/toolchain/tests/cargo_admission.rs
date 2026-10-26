use super::*;
use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::OsString,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn host_inputs(
    root: &Path,
    isolation: &Path,
    cargo_home: &Path,
    controls: &SetupControls,
) -> Result<HostInputs> {
    let cwd = root.parent().expect("fixture root parent");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (OsString::from("HOME"), cwd.join("home").into_os_string()),
        (
            OsString::from("CARGO_HOME"),
            cargo_home.as_os_str().to_owned(),
        ),
    ]);
    HostInputs::from_environment_with_controls(&inherited, cwd, None, root, isolation, controls)
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("reviewed");
    let isolation = temporary.path().join("isolation");
    let cargo_home = temporary.path().join("cargo-home");
    fs::create_dir(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    fs::create_dir(&cargo_home).expect("Cargo home");
    (temporary, root, isolation, cargo_home)
}

fn controls() -> SetupControls {
    SetupControls::for_test(Duration::from_secs(30))
}

#[test]
fn bounded_directory_reader_stops_at_its_entry_limit() -> Result<()> {
    let temporary = tempfile::tempdir().expect("temporary directory");
    for name in ["one", "two", "three"] {
        fs::write(temporary.path().join(name), b"").expect("write entry");
    }
    let mut entries_seen = 0;
    let controls = controls();
    let result =
        fs_util::read_dir_sorted_controlled(temporary.path(), &mut entries_seen, 2, || {
            controls.check_start()
        });
    let error = result.expect_err("third entry exceeds the configured test limit");
    assert!(error.to_string().contains("entry limit"));
    assert_eq!(entries_seen, 2);
    Ok(())
}

#[test]
fn bounded_directory_reader_observes_cancellation_between_entries() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    for name in ["one", "two", "three", "four", "five"] {
        fs::write(temporary.path().join(name), b"").expect("write entry");
    }
    let cancellation = Arc::new(AtomicBool::new(false));
    let controls = SetupControls::new(Arc::clone(&cancellation), Duration::from_secs(30));
    let checks = Cell::new(0usize);
    let mut entries_seen = 0;
    let error =
        fs_util::read_dir_sorted_controlled(temporary.path(), &mut entries_seen, 10, || {
            let next_check = checks.get() + 1;
            checks.set(next_check);
            if next_check == 4 {
                cancellation.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            controls.check_start()
        })
        .expect_err("cancelled traversal stops between entries");
    assert!(format!("{error:#}").contains("setup was cancelled"));
    assert!(entries_seen > 0);
    assert!(entries_seen < 5);
}

#[test]
fn bounded_regular_reader_accepts_limit_and_rejects_larger_file() -> Result<()> {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("config.toml");
    let controls = controls();
    fs::write(&path, b"abc").expect("write bounded input");
    let contents = read_bounded_regular_file(
        &path,
        &controls,
        3,
        "fixture config",
        "fixture config exceeds its limit",
    )?;
    assert_eq!(contents, b"abc");

    fs::write(&path, b"abcd").expect("write oversized input");
    let error = read_bounded_regular_file(
        &path,
        &controls,
        3,
        "fixture config",
        "fixture config exceeds its limit",
    )
    .expect_err("file beyond limit is rejected before read");
    assert!(
        error
            .to_string()
            .contains("fixture config exceeds its limit")
    );
    Ok(())
}

#[test]
fn cargo_config_file_limit_applies_to_staged_and_external_roots() -> Result<()> {
    const SENTINEL: &[u8] = b"private-config-sentinel";
    let mut oversized = vec![b'\n'; usize::try_from(MAX_CARGO_CONFIG_FILE_BYTES + 1)?];
    oversized[..SENTINEL.len()].copy_from_slice(SENTINEL);

    for staged in [true, false] {
        let (temporary, root, isolation, cargo_home) = fixture();
        let config = if staged {
            let directory = root.join(".cargo");
            fs::create_dir(&directory).expect("staged Cargo config directory");
            directory.join("config.toml")
        } else {
            cargo_home.join("config.toml")
        };
        fs::write(&config, &oversized).expect("oversized Cargo config");
        let Err(error) = host_inputs(&root, &isolation, &cargo_home, &controls()) else {
            panic!("oversized config must be refused before tool setup");
        };
        let detail = format!("{error:#}");
        assert!(detail.contains("exceeds its byte limit"), "{detail}");
        assert!(!detail.contains(std::str::from_utf8(SENTINEL)?));
        drop(temporary);
    }
    Ok(())
}

#[test]
fn cargo_config_include_depth_allows_sixty_four_and_rejects_sixty_five() -> Result<()> {
    let (_temporary, root, isolation, cargo_home) = fixture();
    let directory = root.join(".cargo");
    fs::create_dir(&directory).expect("staged Cargo config directory");

    write_include_chain(&directory, 64);
    host_inputs(&root, &isolation, &cargo_home, &controls())
        .expect("include depth of 64 is admitted");

    write_include_chain(&directory, 65);
    let Err(error) = host_inputs(&root, &isolation, &cargo_home, &controls()) else {
        panic!("include depth of 65 is refused");
    };
    assert!(format!("{error:#}").contains("include depth exceeds its limit"));
    Ok(())
}

#[test]
fn cargo_config_aggregate_bytes_are_bounded_across_includes() -> Result<()> {
    let (_temporary, root, isolation, cargo_home) = fixture();
    let directory = root.join(".cargo");
    fs::create_dir(&directory).expect("staged Cargo config directory");
    let mut includes = Vec::new();
    let mut contents = vec![b'x'; usize::try_from(MAX_CARGO_CONFIG_FILE_BYTES)?];
    contents[0] = b'#';
    for index in 0..64 {
        let name = format!("included-{index:02}.toml");
        includes.push(format!("'{name}'"));
        fs::write(directory.join(name), &contents).expect("write bounded included config");
    }
    fs::write(
        directory.join("config.toml"),
        format!("include = [{}]\n", includes.join(", ")),
    )
    .expect("write include index");

    let Err(error) = host_inputs(&root, &isolation, &cargo_home, &controls()) else {
        panic!("included configs exceed the aggregate byte limit");
    };
    assert!(
        format!("{error:#}").contains("Cargo config aggregate exceeds its byte limit"),
        "{error:#}"
    );
    Ok(())
}

#[test]
fn cargo_config_distinct_file_count_is_bounded() -> Result<()> {
    let (_temporary, root, isolation, cargo_home) = fixture();
    let directory = root.join(".cargo");
    fs::create_dir(&directory).expect("staged Cargo config directory");
    let mut includes = Vec::new();
    for index in 0..MAX_CARGO_CONFIG_FILES {
        let name = format!("included-{index:04}.toml");
        includes.push(format!("'{name}'"));
        fs::write(directory.join(name), b"\n").expect("write included config");
    }
    fs::write(
        directory.join("config.toml"),
        format!("include = [{}]\n", includes.join(", ")),
    )
    .expect("write include index");

    let Err(error) = host_inputs(&root, &isolation, &cargo_home, &controls()) else {
        panic!("root plus included configs exceed the distinct file limit");
    };
    assert!(format!("{error:#}").contains("Cargo config file count exceeds its limit"));
    Ok(())
}

fn write_include_chain(directory: &Path, depth: usize) {
    for index in 0..depth {
        let name = if index == 0 {
            "config.toml".to_owned()
        } else {
            format!("config-{index}.toml")
        };
        let next = format!("config-{}.toml", index + 1);
        fs::write(directory.join(name), format!("include = ['{next}']\n"))
            .expect("write Cargo include");
    }
    fs::write(
        directory.join(format!("config-{depth}.toml")),
        "# terminal include\n",
    )
    .expect("write terminal Cargo include");
}

#[test]
fn cargo_config_cycles_remain_admitted_once() -> Result<()> {
    let (_temporary, root, isolation, cargo_home) = fixture();
    let directory = root.join(".cargo");
    fs::create_dir(&directory).expect("staged Cargo config directory");
    fs::write(directory.join("config.toml"), "include = ['other.toml']\n")
        .expect("write first config");
    fs::write(directory.join("other.toml"), "include = ['config.toml']\n")
        .expect("write cyclic config");

    host_inputs(&root, &isolation, &cargo_home, &controls())
        .expect("visited config files terminate include cycles");
    Ok(())
}

#[test]
fn cancelled_and_expired_host_input_capture_stop_before_config_io() -> Result<()> {
    let (_temporary, root, isolation, cargo_home) = fixture();
    let directory = root.join(".cargo");
    fs::create_dir(&directory).expect("staged Cargo config directory");
    fs::write(directory.join("config.toml"), "private-config-sentinel\n")
        .expect("malformed config sentinel");

    let cancelled = SetupControls::new(Arc::new(AtomicBool::new(true)), Duration::from_secs(30));
    let Err(error) = host_inputs(&root, &isolation, &cargo_home, &cancelled) else {
        panic!("pre-cancelled capture stops before reading config");
    };
    assert!(format!("{error:#}").contains("setup was cancelled"));
    assert!(!format!("{error:#}").contains("private-config-sentinel"));

    let expired = SetupControls::new(Arc::new(AtomicBool::new(false)), Duration::ZERO);
    let Err(error) = host_inputs(&root, &isolation, &cargo_home, &expired) else {
        panic!("expired capture stops before reading config");
    };
    assert!(format!("{error:#}").contains("phase deadline expired"));
    assert!(!format!("{error:#}").contains("private-config-sentinel"));
    Ok(())
}
