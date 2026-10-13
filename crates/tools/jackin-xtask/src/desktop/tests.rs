use super::tempfile_dir;
use super::{
    DesktopCommand, MIN_OS, XunitTotals, assert_broker_version, assert_executable_file,
    assert_native_broker_archs, broker_path, minos_matches_target, normalize_generated_text,
    parse_dwarf_uuid, parse_swift_jobs, parse_xunit_totals, read_xunit_totals, swift_build_args,
    swift_test_args, tree_differences, validate_build, validate_test_totals, validate_version,
};

mod support;
use support::*;
mod case_01;
mod case_02;
