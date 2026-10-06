// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn clean_report(cache: &str, compiler_lines: usize, mbx: &str, origin: &str) -> String {
    format!(
        "VELNOR_CI_REPORT {{\"schema_version\":3,\"log_present\":true,\"cache_outcomes\":{{\"rustup\":\"exact\",\"mold\":\"exact\",\"cargo\":\"exact\",\"mbx\":\"{cache}\",\"docker_seed\":\"disabled\"}},\"origin_downloads\":{{\"updating_crates_io_index\":{origin},\"updating_git\":0,\"downloading_crates\":0,\"downloaded_lines\":[]}},\"compiler\":{{\"compiling_lines\":{compiler_lines},\"mbx_outcomes\":[\"mbx[cache]: object cache: {mbx} hits, 0 misses; 0 B downloaded\"]}}}}"
    )
}
