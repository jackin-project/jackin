// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod layers;

pub use layers::*;

use crate::ci::CiStep;
use crate::repo::Repo;
use crate::violation::Violation;

pub trait Check {
    fn step(&self) -> CiStep;
    fn run(&self, repo: &Repo) -> Vec<Violation>;
}
