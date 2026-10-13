//! jackin-runtime-session-control: session.send and events control client.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`session_control::send_session_text`] — type into a session.
//!
//! Host-side client for the capsule's `session.send` and `events`
//! surface: types text into a running agent session and watches its
//! state transitions, exits, and activity. Split out of
//! `jackin-runtime` (S7 split 47); the old
//! `jackin_runtime::runtime::session_control::*` paths keep working
//! through a re-export shim.

pub mod session_control;
