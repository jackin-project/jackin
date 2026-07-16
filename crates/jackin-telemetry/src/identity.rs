// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt,
    sync::{Mutex, OnceLock},
};

use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(Uuid);
        impl $name {
            #[must_use]
            pub fn mint() -> Self {
                Self(Uuid::new_v4())
            }
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
            pub fn parse(value: &str) -> Result<Self, uuid::Error> {
                Uuid::parse_str(value).map(Self)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

uuid_id!(InvocationId);
uuid_id!(SessionId);
uuid_id!(JobId);

static INVOCATION: OnceLock<InvocationId> = OnceLock::new();
static SESSIONS: Mutex<SessionRegistry> = Mutex::new(SessionRegistry {
    active: None,
    last_ended: None,
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionKind {
    Console,
    Attachment,
    Capsule,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionContext {
    pub current: SessionId,
    pub previous: Option<SessionId>,
    pub kind: SessionKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionOwnershipError {
    pub active: SessionContext,
    pub requested: SessionKind,
}

impl fmt::Display for SessionOwnershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a {:?} telemetry session already owns session {}",
            self.active.kind, self.active.current
        )
    }
}

impl std::error::Error for SessionOwnershipError {}

#[derive(Clone, Copy, Debug)]
struct SessionRegistry {
    active: Option<SessionContext>,
    last_ended: Option<SessionId>,
}

/// Install the process invocation once.
///
/// Repeating the same identity is idempotent. A conflicting reinitialization
/// returns the identity that already owns the process, so callers can adopt it
/// deterministically rather than silently replacing correlation.
pub fn set_current_invocation(id: InvocationId) -> Result<(), InvocationId> {
    if let Some(current) = INVOCATION.get().copied() {
        return if current == id { Ok(()) } else { Err(current) };
    }
    INVOCATION
        .set(id)
        .map_err(|attempted| INVOCATION.get().copied().unwrap_or(attempted))
}

#[must_use]
pub fn current_invocation() -> Option<InvocationId> {
    INVOCATION.get().copied()
}

fn claim_session(kind: SessionKind) -> Result<SessionContext, SessionOwnershipError> {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(active) = sessions.active {
        return Err(SessionOwnershipError {
            active,
            requested: kind,
        });
    }
    let context = SessionContext {
        current: SessionId::mint(),
        previous: sessions.last_ended,
        kind,
    };
    sessions.active = Some(context);
    Ok(context)
}

#[must_use]
pub fn current_session() -> Option<SessionContext> {
    SESSIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .active
}

fn end_session(id: SessionId) {
    let mut sessions = SESSIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if sessions.active.is_some_and(|value| value.current == id) {
        sessions.active = None;
        sessions.last_ended = Some(id);
    }
}

/// Exclusive interactive-session ownership with paired lifecycle events.
#[derive(Debug)]
pub struct SessionGuard {
    context: SessionContext,
    started: bool,
    owns_session: bool,
}

impl SessionGuard {
    /// Claim ownership without emitting the start event. Capsule startup uses
    /// this before fallible subscriber installation, then calls [`Self::start`].
    pub fn claim(kind: SessionKind) -> Result<Self, SessionOwnershipError> {
        Ok(Self {
            context: claim_session(kind)?,
            started: false,
            owns_session: true,
        })
    }

    /// Claim ownership and emit the paired start event immediately.
    pub fn begin(kind: SessionKind) -> Result<Self, SessionOwnershipError> {
        let mut guard = Self::claim(kind)?;
        guard.start();
        Ok(guard)
    }

    /// Begin a direct attachment, or continue the session already owned by
    /// the console that transferred its terminal into the attachment flow.
    /// Other active owners remain conflicts.
    pub fn begin_attachment() -> Result<Self, SessionOwnershipError> {
        if let Some(context) = current_session() {
            if context.kind == SessionKind::Console {
                return Ok(Self {
                    context,
                    started: false,
                    owns_session: false,
                });
            }
            return Err(SessionOwnershipError {
                active: context,
                requested: SessionKind::Attachment,
            });
        }
        Self::begin(SessionKind::Attachment)
    }

    pub fn start(&mut self) {
        if !self.started {
            emit_session_event(&crate::event::SESSION_START, self.context);
            self.started = true;
        }
    }

    #[must_use]
    pub const fn context(&self) -> SessionContext {
        self.context
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if !self.owns_session {
            return;
        }
        if self.started {
            emit_session_event(&crate::event::SESSION_END, self.context);
        }
        end_session(self.context.current);
    }
}

fn emit_session_event(def: &'static crate::event::EventDef, context: SessionContext) {
    let current = context.current.to_string();
    let previous = context.previous.map(|id| id.to_string());
    let mut attrs = vec![crate::Attr {
        key: crate::schema::attrs::std_attrs::SESSION_ID,
        value: crate::Value::Str(&current),
    }];
    if let Some(previous) = previous.as_deref() {
        attrs.push(crate::Attr {
            key: crate::schema::attrs::std_attrs::SESSION_PREVIOUS_ID,
            value: crate::Value::Str(previous),
        });
    }
    let _event_result = crate::emit_event(def, crate::FieldSet::new(&attrs, None));
}

#[cfg(test)]
mod tests;
