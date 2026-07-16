//! Shared modal size and placement registry.
//!
//! Surfaces pass the area that is allowed to be covered by the modal. The
//! registry centers within that area unless a spec explicitly says otherwise,
//! so callers keep owning footer/status reservation while modal sizing stays in
//! one place.

use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalRectSpec {
    TextInput,
    SourcePicker,
    ScopePicker,
    OpPicker,
    RolePicker {
        filtered_len: usize,
    },
    Confirm {
        width_pct: u16,
        height: u16,
    },
    MountChoice,
    AuthForm {
        required_height: u16,
    },
    Fixed {
        width_pct: u16,
        height: u16,
    },
    Exact {
        width: u16,
        height: u16,
    },
    MaxWidthMin {
        max_width: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
    PercentClamp {
        width_pct: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
    PercentClampWithMargin {
        width_pct: u16,
        min_width: u16,
        width_margin: u16,
        height_margin: u16,
        height: u16,
    },
    TopAligned {
        width: u16,
        height: u16,
    },
    TopAlignedMaxWidthMin {
        max_width: u16,
        min_width: u16,
        side_margin: u16,
        height: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalRectMode {
    TextInput,
    SourcePicker,
    ScopePicker,
    OpPicker,
    RolePicker { filtered_len: usize },
    Confirm { width_pct: u16, height: u16 },
    MountChoice,
    AuthForm { required_height: u16 },
    SaveDiscardCancel,
    FileBrowser,
    WorkdirPick,
    GithubPicker { choice_len: usize },
    ConfirmSave { required_height: u16 },
    ErrorPopup { required_height: u16 },
    ContainerInfo { required_height: u16 },
    StatusPopup,
}

impl ModalRectMode {
    fn spec(self, outer_height: u16) -> ModalRectSpec {
        match self {
            Self::TextInput => ModalRectSpec::TextInput,
            Self::SourcePicker => ModalRectSpec::SourcePicker,
            Self::ScopePicker => ModalRectSpec::ScopePicker,
            Self::OpPicker => ModalRectSpec::OpPicker,
            Self::RolePicker { filtered_len } => ModalRectSpec::RolePicker { filtered_len },
            Self::Confirm { width_pct, height } => ModalRectSpec::Confirm { width_pct, height },
            Self::MountChoice => ModalRectSpec::MountChoice,
            Self::AuthForm { required_height } => ModalRectSpec::AuthForm { required_height },
            Self::SaveDiscardCancel => ModalRectSpec::Fixed {
                width_pct: 70,
                height: 7,
            },
            Self::FileBrowser => ModalRectSpec::Fixed {
                width_pct: 70,
                height: 22,
            },
            Self::WorkdirPick => ModalRectSpec::Fixed {
                width_pct: 60,
                height: 12,
            },
            Self::GithubPicker { choice_len } => {
                let rows = (choice_len as u16).saturating_add(5).min(15);
                ModalRectSpec::Fixed {
                    width_pct: 60,
                    height: rows,
                }
            }
            Self::ConfirmSave { required_height } => ModalRectSpec::Fixed {
                width_pct: 80,
                height: required_height.min(outer_height),
            },
            Self::ErrorPopup { required_height } | Self::ContainerInfo { required_height } => {
                ModalRectSpec::Fixed {
                    width_pct: 60,
                    height: required_height,
                }
            }
            Self::StatusPopup => ModalRectSpec::Fixed {
                width_pct: 50,
                height: 7,
            },
        }
    }
}

#[must_use]
pub fn modal_rect_for_mode(outer: Rect, mode: ModalRectMode) -> Rect {
    modal_rect(outer, mode.spec(outer.height))
}

#[must_use]
pub fn modal_rect(outer: Rect, spec: ModalRectSpec) -> Rect {
    match spec {
        ModalRectSpec::TextInput => text_input_rect(outer),
        ModalRectSpec::SourcePicker => source_picker_rect(outer),
        ModalRectSpec::ScopePicker => scope_picker_rect(outer),
        ModalRectSpec::OpPicker => op_picker_rect(outer),
        ModalRectSpec::RolePicker { filtered_len } => {
            role_picker_rect_for_count(outer, filtered_len)
        }
        ModalRectSpec::Confirm { width_pct, height } => {
            centered_rect_fixed(outer, width_pct, height)
        }
        ModalRectSpec::MountChoice => mount_choice_rect(outer),
        ModalRectSpec::AuthForm { required_height } => {
            auth_form_rect_for_height(outer, required_height)
        }
        ModalRectSpec::Fixed { width_pct, height } => centered_rect_fixed(outer, width_pct, height),
        ModalRectSpec::Exact { width, height } => centered_rect_exact(outer, width, height),
        ModalRectSpec::MaxWidthMin {
            max_width,
            min_width,
            side_margin,
            height,
        } => {
            let width = max_width
                .min(outer.width.saturating_sub(side_margin))
                .max(min_width);
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::PercentClamp {
            width_pct,
            min_width,
            side_margin,
            height,
        } => {
            let max_width = outer.width.saturating_sub(side_margin).max(min_width);
            let width = (outer.width.saturating_mul(width_pct) / 100).clamp(min_width, max_width);
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::PercentClampWithMargin {
            width_pct,
            min_width,
            width_margin,
            height_margin,
            height,
        } => {
            let max_width = outer.width.saturating_sub(width_margin).max(min_width);
            let width = (outer.width.saturating_mul(width_pct) / 100).clamp(min_width, max_width);
            let height = height.min(outer.height.saturating_sub(height_margin));
            centered_rect_exact(outer, width, height)
        }
        ModalRectSpec::TopAligned { width, height } => Rect {
            x: outer.x + outer.width.saturating_sub(width) / 2,
            y: outer.y,
            width,
            height: height.min(outer.height),
        },
        ModalRectSpec::TopAlignedMaxWidthMin {
            max_width,
            min_width,
            side_margin,
            height,
        } => {
            let width = max_width
                .min(outer.width.saturating_sub(side_margin))
                .max(min_width);
            Rect {
                x: outer.x + outer.width.saturating_sub(width) / 2,
                y: outer.y,
                width,
                height: height.min(outer.height),
            }
        }
    }
}

#[must_use]
pub fn text_input_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 60, 5)
}

#[must_use]
pub fn source_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn scope_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn op_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 80, 22)
}

#[must_use]
pub fn role_picker_rect_for_count(outer: Rect, filtered_len: usize) -> Rect {
    let rows = (filtered_len as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

#[must_use]
pub fn mount_choice_rect(outer: Rect) -> Rect {
    // 2 borders + 1 leading + 1 question + 1 path + 1 spacer + 1 buttons + 1 trailing = 8
    let w = outer.width.min(80);
    let h = 8u16.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[must_use]
pub fn auth_form_rect_for_height(outer: Rect, required_height: u16) -> Rect {
    centered_rect_fixed(outer, 80, required_height)
}

/// Center a dialog at a stable preferred width derived from `pct_w` of a
/// 160-column reference terminal.
#[must_use]
pub fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
    const REFERENCE_COLS: u16 = 160;
    let preferred = REFERENCE_COLS.saturating_mul(pct_w) / 100;
    centered_rect_preferred(outer, preferred, rows)
}

/// Center a dialog at `preferred_w` columns, shrinking only when the outer area
/// is too narrow to fit `preferred_w` with a four-column side margin.
#[must_use]
pub fn centered_rect_preferred(outer: Rect, preferred_w: u16, rows: u16) -> Rect {
    let w = preferred_w.min(outer.width.saturating_sub(4));
    let h = rows.min(outer.height);
    centered_rect_exact(outer, w, h)
}

#[must_use]
pub fn centered_rect_exact(outer: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: outer.x + outer.width.saturating_sub(width) / 2,
        y: outer.y + outer.height.saturating_sub(height) / 2,
        width,
        height: height.min(outer.height),
    }
}
