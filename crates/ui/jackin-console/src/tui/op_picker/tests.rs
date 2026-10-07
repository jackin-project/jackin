// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Most tests inject a no-op `StubRunner` and overwrite
//! `vaults`/`items`/`fields`/`load_state`/`stage`/selection
//! directly before driving `handle_key` — bypasses the worker
//! channel. The `*_uses_injected_runner_in_async_worker` tests at
//! the end exercise the worker path end-to-end.

use super::*;

use crate::tui::components::op_picker::{field_label_input_state, section_name_input_state};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use jackin_core::{FieldTarget, OpSection, OpSectionTarget};

use jackin_env::{
    OpAccount, OpCache, OpField, OpItem, OpStructRunner, OpVault, resolve_op_uri_to_ref,
};

use jackin_oppicker::ModalOutcome;

use std::cell::RefCell;

use std::rc::Rc;

use std::sync::{Arc, Mutex};

mod support_01;
mod support_01_helpers;
mod support_01_runners;
use support_01::*;
mod support_02;
use support_02::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
