// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Child-only descriptor-pinned working directories. Parent cwd never changes.
//! All setup runs before fork; the callback performs only fchdir and returns
//! an OS error. Descriptor ownership stays with the command until command drop.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::current_dir;

#[cfg(all(test, unix))]
mod tests;
