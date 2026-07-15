// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded UI state tracking. Screens and widget focus are events plus metrics,
//! never lifetime spans.

use std::time::Instant;

use uuid::Uuid;

use crate::{Attr, FieldSet, Rejection, Value, counter, emit_event, histogram, metric, schema};

#[derive(Debug)]
struct Visit {
    screen: schema::enums::ScreenId,
    id: String,
    entered: Instant,
}

#[derive(Debug, Default)]
pub struct ScreenVisitTracker {
    sequence: u64,
    current: Option<Visit>,
}

impl ScreenVisitTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            current: None,
        }
    }

    pub fn enter(&mut self, screen: schema::enums::ScreenId) -> Result<(), Rejection> {
        if self.current.is_some() {
            self.exit(schema::enums::TransitionReason::Action)?;
        }
        self.sequence = self.sequence.saturating_add(1);
        let id = Uuid::new_v4().to_string();
        let attrs = [
            Attr {
                key: schema::attrs::std_attrs::APP_SCREEN_ID,
                value: Value::Str(screen.as_str()),
            },
            Attr {
                key: schema::attrs::UI_SCREEN_VISIT_ID,
                value: Value::Str(&id),
            },
            Attr {
                key: schema::attrs::UI_NAVIGATION_SEQUENCE,
                value: Value::U64(self.sequence),
            },
        ];
        emit_event(
            &crate::event::UI_SCREEN_ENTERED,
            FieldSet::new(&attrs, None),
        )?;
        counter(&metric::UI_TRANSITIONS).add(
            1,
            &[Attr {
                key: schema::attrs::std_attrs::APP_SCREEN_ID,
                value: Value::Str(screen.as_str()),
            }],
        )?;
        self.current = Some(Visit {
            screen,
            id,
            entered: Instant::now(),
        });
        Ok(())
    }

    pub fn exit(&mut self, reason: schema::enums::TransitionReason) -> Result<(), Rejection> {
        let Some(visit) = self.current.take() else {
            return Ok(());
        };
        let attrs = [
            Attr {
                key: schema::attrs::std_attrs::APP_SCREEN_ID,
                value: Value::Str(visit.screen.as_str()),
            },
            Attr {
                key: schema::attrs::UI_SCREEN_VISIT_ID,
                value: Value::Str(&visit.id),
            },
            Attr {
                key: schema::attrs::UI_NAVIGATION_SEQUENCE,
                value: Value::U64(self.sequence),
            },
            Attr {
                key: schema::attrs::UI_TRANSITION_REASON,
                value: Value::Str(reason.as_str()),
            },
        ];
        emit_event(&crate::event::UI_SCREEN_EXITED, FieldSet::new(&attrs, None))?;
        histogram(&metric::UI_DWELL).record(
            visit.entered.elapsed().as_secs_f64(),
            &[
                Attr {
                    key: schema::attrs::std_attrs::APP_SCREEN_ID,
                    value: Value::Str(visit.screen.as_str()),
                },
                Attr {
                    key: schema::attrs::UI_TRANSITION_REASON,
                    value: Value::Str(reason.as_str()),
                },
            ],
        )
    }

    #[must_use]
    pub fn current_screen(&self) -> Option<schema::enums::ScreenId> {
        self.current.as_ref().map(|visit| visit.screen)
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Debug)]
struct Focus {
    widget: &'static str,
    focused: Instant,
}

#[derive(Debug, Default)]
pub struct WidgetFocusTracker {
    current: Option<Focus>,
}

impl WidgetFocusTracker {
    pub fn focus(&mut self, widget: &'static str) -> Result<(), Rejection> {
        self.unfocus()?;
        let attrs = [Attr {
            key: schema::attrs::std_attrs::APP_WIDGET_ID,
            value: Value::Str(widget),
        }];
        emit_event(
            &crate::event::UI_WIDGET_FOCUSED,
            FieldSet::new(&attrs, None),
        )?;
        self.current = Some(Focus {
            widget,
            focused: Instant::now(),
        });
        Ok(())
    }

    pub fn unfocus(&mut self) -> Result<(), Rejection> {
        let Some(focus) = self.current.take() else {
            return Ok(());
        };
        let attrs = [Attr {
            key: schema::attrs::std_attrs::APP_WIDGET_ID,
            value: Value::Str(focus.widget),
        }];
        emit_event(
            &crate::event::UI_WIDGET_UNFOCUSED,
            FieldSet::new(&attrs, None),
        )?;
        histogram(&metric::UI_FOCUS_DURATION).record(focus.focused.elapsed().as_secs_f64(), &attrs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_sequence_is_monotonic_and_visits_end() {
        let mut tracker = ScreenVisitTracker::new();
        tracker
            .enter(schema::enums::ScreenId::WorkspaceList)
            .unwrap();
        assert_eq!(tracker.sequence(), 1);
        tracker
            .enter(schema::enums::ScreenId::WorkspaceEditor)
            .unwrap();
        assert_eq!(tracker.sequence(), 2);
        tracker.exit(schema::enums::TransitionReason::Back).unwrap();
        assert_eq!(tracker.current_screen(), None);
    }

    #[test]
    fn widget_focus_replaces_prior_focus() {
        let mut tracker = WidgetFocusTracker::default();
        tracker.focus("general").unwrap();
        tracker.focus("mounts").unwrap();
        tracker.unfocus().unwrap();
        assert!(tracker.current.is_none());
    }
}
