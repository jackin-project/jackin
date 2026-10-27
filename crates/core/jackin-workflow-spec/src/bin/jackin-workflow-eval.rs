// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::alloc::System;
use std::io::Read as _;

use cap::Cap;
use jackin_workflow_spec::eval::{HelperRequest, evaluate_request};

#[global_allocator]
static ALLOCATOR: Cap<System> = Cap::new(System, usize::MAX);

#[expect(
    clippy::print_stderr,
    reason = "the helper reports startup failures on stderr; the parent captures it"
)]
fn fail(message: &str) -> std::process::ExitCode {
    eprintln!("jackin-workflow-eval: {message}");
    std::process::ExitCode::from(2)
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args();
    let memory_bytes = args.nth(1).and_then(|text| text.parse::<u64>().ok());
    let Some(memory_bytes) = memory_bytes else {
        return fail("usage: jackin-workflow-eval MEMORY_BYTES < request.json");
    };
    if ALLOCATOR.set_limit(memory_bytes as usize).is_err() {
        return fail("cannot set the memory cap");
    }
    let mut stdin = Vec::new();
    if std::io::stdin().read_to_end(&mut stdin).is_err() {
        return fail("cannot read the request");
    }
    let request: HelperRequest = match serde_json::from_slice(&stdin) {
        Ok(request) => request,
        Err(error) => return fail(&format!("malformed request: {error}")),
    };
    let response = evaluate_request(&request);
    match serde_json::to_vec(&response) {
        Ok(bytes) => {
            use std::io::Write as _;
            let _unused = std::io::stdout().write_all(&bytes);
            std::process::ExitCode::SUCCESS
        }
        Err(error) => fail(&format!("cannot encode the response: {error}")),
    }
}
