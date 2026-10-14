// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::current_dir;

#[cfg(test)]
mod tests;
