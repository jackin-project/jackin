// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const SESSION_START: &str = "session.start";
pub const SESSION_END: &str = "session.end";
pub const UI_SCREEN_ENTERED: &str = "ui.screen.entered";
pub const UI_SCREEN_EXITED: &str = "ui.screen.exited";
pub const UI_WIDGET_FOCUSED: &str = "ui.widget.focused";
pub const UI_WIDGET_UNFOCUSED: &str = "ui.widget.unfocused";
pub const APP_JANK: &str = "app.jank";
pub const APP_CRASH: &str = "app.crash";
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";

pub const ALL: &[&str] = &[
    SESSION_START,
    SESSION_END,
    UI_SCREEN_ENTERED,
    UI_SCREEN_EXITED,
    UI_WIDGET_FOCUSED,
    UI_WIDGET_UNFOCUSED,
    APP_JANK,
    APP_CRASH,
    TELEMETRY_VALIDATE,
];
