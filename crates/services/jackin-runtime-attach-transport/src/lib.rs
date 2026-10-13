//! jackin-runtime-attach-transport: transport selection.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`transport::select_host_attach_transport`].
//!
//! Host attach transport selection and `attach-proxy` exec
//! args. Split out of `jackin-runtime` (S7 split 75); the
//! old `jackin_runtime::runtime::attach::transport::*`
//! paths keep working through a module re-export in
//! `attach.rs`.

pub mod transport;
