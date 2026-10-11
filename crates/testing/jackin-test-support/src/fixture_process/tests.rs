//! Self-tests for the [`FakeProcessHarness`](super::FakeProcessHarness).

use super::{FakeBinary, FakeProcessHarness, Invocation, ProcessScript};

use std::io::Result;

mod support;
use support::*;
mod case_01;
