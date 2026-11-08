// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod accounts;
mod agents_manifest;
mod allowed_roles;
mod codex;
mod env;
mod error_text;
mod errors;
mod files;
mod hook_fs;
mod identity;
mod image_request;
mod native;
mod omp;
mod omp_credentials;
mod role_engine;
mod roles;
mod roles_cancel;
mod roles_git;
mod roles_hidden;
mod roles_inputs;
mod roles_load;
mod roles_origin;
mod roles_path;
mod roles_race;
mod roles_url;
mod scoped_env;
mod support;
mod tuple;

#[path = "../support/trap.rs"]
mod trap;
