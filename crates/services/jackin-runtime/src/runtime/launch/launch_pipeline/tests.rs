//! `run_launch_core` boundary harness (plan 016).
//!
//! Builds a fully-populated [`LaunchCore`] over `FakeDockerClient` /
//! `FakeRunner` + real grant/profile/config fixtures and drives the real
//! pipeline boundary (not helper-only substitutes).

use super::super::Backend;
use super::super::StepCounter;
use super::super::account_identity;
use super::launch_core::{self, LaunchCore};

use super::*;

use crate::runtime::docker_profile::DockerGrants;

use crate::runtime::identity::GitIdentity;

use crate::runtime::image::ImageDecision;

use jackin_config::AppConfig;

use jackin_core::Agent;

use jackin_core::ContainerState;

use jackin_core::JackinPaths;

use jackin_core::RoleSelector;

use jackin_env::ResolvedEnv;

use jackin_test_support::{FakeDockerClient, FakeRunner, seed_valid_role_repo};

use std::collections::{BTreeMap, VecDeque};

use std::path::PathBuf;

use std::sync::{Mutex, OnceLock};

use tempfile::TempDir;

mod restore_reuse_intent;

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod case_01;
mod case_02;
