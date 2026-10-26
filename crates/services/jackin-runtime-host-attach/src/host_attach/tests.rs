// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_protocol::attach::{ClientFrame, ClipboardImageFormat, read_client_frame};

use tokio::io::{AsyncReadExt, duplex};

use super::*;

mod case_01;
