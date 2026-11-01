// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug)]
pub struct FakeClock {}

#[derive(Debug)]
pub struct MemFs {}

#[derive(Debug)]
pub struct TestRunner {}

#[derive(Debug)]
pub struct ScriptedProcessRunner {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScriptedProgram {
    Docker,
    Openshell,
    Ssh,
    Op,
}
