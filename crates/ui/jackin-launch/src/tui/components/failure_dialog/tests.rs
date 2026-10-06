use crate::tui::model::{LaunchIdentity, LaunchTargetKind};

use crate::tui::update::initial_view;

use crate::tui::view::render_launch_frame;

use crate::{LaunchStage, tui::model::LaunchFailure};

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

mod support;
use support::*;
mod case_01;
