// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `session`.

use super::SessionProvider;
use super::apply_account_env;
use super::{
    AgentSpawnSpec, AgentState, EXPLICIT_CAPABILITY_ENV_NAMES, OscPolicy, SESSION_ENV_PASSTHROUGH,
    Session, SessionEvent, SessionSpawnSpec, SessionTerminal, agent_model_args,
    build_agent_command, build_shell_command, child_exit_reason, emit_pty_exit, emit_pty_spawn,
    inject_status_env, isolated_wrapper_args, osc8_uri_is_safe, pty_exit_error_type,
    pty_exit_reason, validate_spawn_token_syntax,
};

use std::path::Path;

use std::sync::{Arc, Mutex};

use crate::agent_status::evidence::{AuthorityEvidence, AuthorityGrade, RawAgentState};

use crate::agent_status::process::{
    ForegroundGroup, ProcessCpuSample, ProcessInfo, ProcessSampler,
};

use crate::agent_status::rules::{RulePack, RulePackRegistry};

use anyhow::Result;

use jackin_core::Agent;

use jackin_protocol::agent_status::{AgentStatusConfidence, AgentStatusSource};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};

use tokio::sync::mpsc;

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
mod case_06;
