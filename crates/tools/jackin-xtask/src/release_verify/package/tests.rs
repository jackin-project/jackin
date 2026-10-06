// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use flate2::{Compression, write::GzEncoder};

use sha2::{Digest, Sha256};

use tar::{Builder as TarBuilder, EntryType, Header};

use super::*;

mod support;
use support::*;
mod case_01;
mod case_02;
