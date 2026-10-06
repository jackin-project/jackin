//! Suite A: grant-failure ordering + mid-pipeline `FailedSetup` cleanup.

use super::*;

use crate::instance::{DockerResources, InstanceManifest, NewInstanceManifest};

use jackin_config::AppConfig;

use jackin_core::Agent;

use jackin_core::JackinPaths;

use jackin_core::RoleSelector;

use jackin_core::{ContainerHandle, ContainerState};

use jackin_test_support::FakeDockerClient;

use std::collections::{HashMap, VecDeque};

use tempfile::tempdir;

mod support;
use support::*;
mod case_01;
