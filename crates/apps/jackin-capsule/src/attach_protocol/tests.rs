// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{Semaphore, mpsc},
};

use super::{ClientFrame, handle_attach_client, initial_spawn_requests};

use crate::protocol::attach::SpawnRequest;


mod support;
use support::*;
mod case_01;
