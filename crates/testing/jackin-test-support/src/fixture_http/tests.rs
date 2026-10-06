//! Self-tests for the [`FixtureHttpServer`](super::FixtureHttpServer) harness.
//!
//! The client side speaks raw HTTP over `TcpStream`: the harness must stay
//! dependency-free, so its own tests cannot use an HTTP client crate either.

use super::{FixtureHttpServer, RecordedRequest, ScriptedResponse, redact_secrets};

use std::io::{Read, Result, Write};

use std::net::TcpStream;

use std::time::{Duration, Instant};

mod support;
use support::*;
mod case_01;
