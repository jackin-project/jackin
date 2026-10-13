// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{io::Cursor, time::Duration};

use jackin_protocol::attach::{
    ClientFrame, ClientTerminal, ClipboardImageFormat, ServerFrame, SpawnRequest, encode_server,
    read_client_frame,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

use super::*;

mod case_01;
mod case_02;
mod case_03;
mod case_04;
