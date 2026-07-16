// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded UI state tracking. Screens and widget focus are events plus metrics,
//! never lifetime spans.

use std::{cell::RefCell, collections::VecDeque, time::Instant};

use uuid::Uuid;

use crate::{Attr, FieldSet, Rejection, Value, counter, emit_event, histogram, metric, schema};

thread_local! {
    static PENDING_ACTION: RefCell<Option<ActionParent>> = const { RefCell::new(None) };
}

#[derive(Debug)]
pub struct ActionParent(Option<crate::operation::OperationGuard>);

impl ActionParent {
    pub fn in_scope<T>(&self, operation: impl FnOnce() -> T) -> T {
        match self.0.as_ref() {
            Some(guard) => guard.span().in_scope(operation),
            None => operation(),
        }
    }
}

impl Drop for ActionParent {
    fn drop(&mut self) {
        if let Some(guard) = self.0.take() {
            guard.complete(schema::enums::OutcomeValue::Success, None);
        }
    }
}

/// Retain a reducer-owned action through its synchronous follow-up effects and
/// the immediate action-triggered frame. Replacing or taking it completes the
/// prior action deterministically.
pub fn remember_action_parent(guard: crate::operation::OperationGuard) {
    PENDING_ACTION.with(|parent| {
        *parent.borrow_mut() = Some(ActionParent(Some(guard)));
    });
}

#[must_use]
pub fn take_action_parent() -> Option<ActionParent> {
    PENDING_ACTION.with(|parent| parent.borrow_mut().take())
}

/// Run reducer follow-up effects under the semantic action that requested
/// them, while leaving completion to the single action-triggered frame.
pub fn in_pending_action_scope<T>(operation: impl FnOnce() -> T) -> T {
    PENDING_ACTION.with(|parent| match parent.borrow().as_ref() {
        Some(parent) => parent.in_scope(operation),
        None => operation(),
    })
}

/// Record an immediate semantic action that does not own reducer follow-up.
///
/// Async launch and exit outcomes use this path so no operation guard can
/// survive across their await or loop-exit boundary.
pub fn record_action(
    action: schema::enums::UiActionName,
    screen: schema::enums::ScreenId,
    widget: Option<&'static str>,
) {
    let mut attrs = vec![Attr {
        key: schema::attrs::UI_ACTION_NAME,
        value: Value::Str(action.as_str()),
    }];
    attrs.push(Attr {
        key: schema::attrs::std_attrs::APP_SCREEN_ID,
        value: Value::Str(screen.as_str()),
    });
    attrs.push(Attr {
        key: schema::attrs::std_attrs::APP_SCREEN_NAME,
        value: Value::Str(screen.as_str()),
    });
    if let Some(widget) = widget {
        attrs.push(Attr {
            key: schema::attrs::std_attrs::APP_WIDGET_ID,
            value: Value::Str(widget),
        });
        attrs.push(Attr {
            key: schema::attrs::std_attrs::APP_WIDGET_NAME,
            value: Value::Str(widget),
        });
    }
    if let Ok(guard) = crate::root_operation(&crate::operation::UI_ACTION, &attrs) {
        guard.complete(schema::enums::OutcomeValue::Success, None);
    }
    let counter_attrs = [Attr {
        key: schema::attrs::UI_ACTION_NAME,
        value: Value::Str(action.as_str()),
    }];
    let _counter_result = counter(&metric::UI_ACTIONS).add(1, &counter_attrs);
}

/// Record one bounded UI frame. Continuous rendering remains metric-only.
pub fn record_render(screen: schema::enums::ScreenId, elapsed_seconds: f64) {
    let attrs = [Attr {
        key: schema::attrs::std_attrs::APP_SCREEN_ID,
        value: Value::Str(screen.as_str()),
    }];
    let _metric_result = histogram(&metric::UI_RENDER_DURATION).record(elapsed_seconds, &attrs);
}

#[derive(Debug, Default)]
pub struct JankMonitor {
    slow_frames: VecDeque<Instant>,
    crossing_active: bool,
}

impl JankMonitor {
    pub fn record_frame(&mut self, screen: schema::enums::ScreenId, elapsed_seconds: f64) {
        const WINDOW_SECONDS: f64 = 1.0;
        const THRESHOLD_SECONDS: f64 = 0.100;
        record_render(screen, elapsed_seconds);
        let now = Instant::now();
        while self
            .slow_frames
            .front()
            .is_some_and(|frame| now.duration_since(*frame).as_secs_f64() > WINDOW_SECONDS)
        {
            self.slow_frames.pop_front();
        }
        if elapsed_seconds >= THRESHOLD_SECONDS {
            self.slow_frames.push_back(now);
        }
        let crossing = !self.crossing_active && !self.slow_frames.is_empty();
        self.crossing_active = !self.slow_frames.is_empty();
        if crossing {
            let screen_attr = [Attr {
                key: schema::attrs::std_attrs::APP_SCREEN_ID,
                value: Value::Str(screen.as_str()),
            }];
            let _counter_result = counter(&metric::UI_JANK).add(1, &screen_attr);
            let jank_attrs = [
                Attr {
                    key: schema::attrs::std_attrs::APP_JANK_FRAME_COUNT,
                    value: Value::U64(self.slow_frames.len() as u64),
                },
                Attr {
                    key: schema::attrs::std_attrs::APP_JANK_PERIOD,
                    value: Value::F64(WINDOW_SECONDS),
                },
                Attr {
                    key: schema::attrs::std_attrs::APP_JANK_THRESHOLD,
                    value: Value::F64(THRESHOLD_SECONDS),
                },
            ];
            let _event_result =
                emit_event(&crate::event::APP_JANK, FieldSet::new(&jank_attrs, None));
        }
    }
}

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
        self.enter_new(screen)
    }

    pub fn transition(
        &mut self,
        screen: schema::enums::ScreenId,
        reason: schema::enums::TransitionReason,
        action_parent: Option<&ActionParent>,
    ) -> Result<(), Rejection> {
        let Some(previous) = self.current_screen() else {
            return self.enter_new(screen);
        };
        if previous == screen {
            return Ok(());
        }
        let transition_attrs = [
            Attr {
                key: schema::attrs::UI_SCREEN_PREVIOUS_ID,
                value: Value::Str(previous.as_str()),
            },
            Attr {
                key: schema::attrs::std_attrs::APP_SCREEN_ID,
                value: Value::Str(screen.as_str()),
            },
            Attr {
                key: schema::attrs::UI_TRANSITION_REASON,
                value: Value::Str(reason.as_str()),
            },
        ];
        let transition = action_parent.and_then(|parent| {
            parent.in_scope(|| {
                crate::operation(&crate::operation::UI_SCREEN_TRANSITION, &transition_attrs).ok()
            })
        });
        self.exit(reason)?;
        self.enter_new(screen)?;
        counter(&metric::UI_TRANSITIONS).add(
            1,
            &[
                Attr {
                    key: schema::attrs::std_attrs::APP_SCREEN_ID,
                    value: Value::Str(screen.as_str()),
                },
                Attr {
                    key: schema::attrs::UI_TRANSITION_REASON,
                    value: Value::Str(reason.as_str()),
                },
            ],
        )?;
        if let Some(transition) = transition {
            transition.complete(schema::enums::OutcomeValue::Success, None);
        }
        Ok(())
    }

    fn enter_new(&mut self, screen: schema::enums::ScreenId) -> Result<(), Rejection> {
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
        self.sequence = self.sequence.saturating_add(1);
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

    #[must_use]
    pub fn current_widget(&self) -> Option<&'static str> {
        self.current.as_ref().map(|focus| focus.widget)
    }
}

#[cfg(test)]
mod tests;
