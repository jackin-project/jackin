// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub trait BuildLogSink: Send + Sync + std::fmt::Debug {
    fn push_line(&self, line: &str);
}
