use super::super::credential_resolver;
use std::sync::Mutex;

use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
