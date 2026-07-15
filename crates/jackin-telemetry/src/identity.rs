// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt,
    sync::{OnceLock, RwLock},
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
static SESSION: OnceLock<RwLock<Option<SessionContext>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionContext {
    pub current: SessionId,
    pub previous: Option<SessionId>,
}

pub fn set_current_invocation(id: InvocationId) -> Result<(), InvocationId> {
    INVOCATION.set(id)
}
#[must_use]
pub fn current_invocation() -> Option<InvocationId> {
    INVOCATION.get().copied()
}

pub fn begin_session() -> SessionContext {
    let slot = SESSION.get_or_init(|| RwLock::new(None));
    let mut active = slot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let context = SessionContext {
        current: SessionId::mint(),
        previous: active.map(|value| value.current),
    };
    *active = Some(context);
    context
}

#[must_use]
pub fn current_session() -> Option<SessionContext> {
    *SESSION
        .get_or_init(|| RwLock::new(None))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn end_session(id: SessionId) {
    let mut active = SESSION
        .get_or_init(|| RwLock::new(None))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if active.is_some_and(|value| value.current == id) {
        *active = None;
    }
}

/// Emits paired lifecycle events and clears ambient session correlation on drop.
#[derive(Debug)]
pub struct SessionGuard {
    context: SessionContext,
    owns: bool,
}

impl SessionGuard {
    #[must_use]
    pub fn begin() -> Self {
        let context = begin_session();
        emit_session_event(&crate::event::SESSION_START, context);
        Self {
            context,
            owns: true,
        }
    }

    /// Reuse an enclosing interactive session, or mint one when none exists.
    #[must_use]
    pub fn begin_or_reuse() -> Self {
        current_session().map_or_else(Self::begin, |context| Self {
            context,
            owns: false,
        })
    }

    #[must_use]
    pub const fn context(&self) -> SessionContext {
        self.context
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if self.owns {
            emit_session_event(&crate::event::SESSION_END, self.context);
            end_session(self.context.current);
        }
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
    let _ = crate::emit_event(def, crate::FieldSet::new(&attrs, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_values_are_uuid_unique_and_parseable() {
        let first = InvocationId::mint();
        let second = InvocationId::mint();
        assert_ne!(first, second);
        assert_eq!(InvocationId::parse(&first.to_string()).unwrap(), first);
    }

    #[test]
    fn session_tracks_previous_only_when_known() {
        let first = begin_session();
        assert_eq!(first.previous, None);
        let second = begin_session();
        assert_eq!(second.previous, Some(first.current));
        end_session(second.current);
        assert_eq!(current_session(), None);
    }
}
