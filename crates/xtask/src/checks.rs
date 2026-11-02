// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod comments;
mod containers;
mod errors;
mod layers;
mod pins;

pub use comments::*;
pub use containers::*;
pub use errors::*;
pub use layers::*;
pub use pins::*;

use crate::ci::CiStep;
use crate::repo::Repo;
use crate::violation::Violation;

pub trait Check {
    fn step(&self) -> CiStep;
    fn run(&self, repo: &Repo) -> Vec<Violation>;
}
