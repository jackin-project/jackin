// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod baseline;
mod comments;
mod containers;
mod demo_gate;
mod errors;
mod layers;
mod pins;
mod screen;

pub use baseline::*;
pub use comments::*;
pub use containers::*;
pub use demo_gate::*;
pub use errors::*;
pub use layers::*;
pub use pins::*;
pub use screen::*;

use crate::ci::CiStep;
use crate::repo::Repo;
use crate::violation::Violation;

pub trait Check {
    fn step(&self) -> CiStep;
    fn run(&self, repo: &Repo) -> Vec<Violation>;
}
