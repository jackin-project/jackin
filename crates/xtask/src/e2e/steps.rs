// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod j00;
mod j01;
mod j02;
mod j03;
mod j04;
mod j05;
mod j06;
mod j07;
mod j08;
mod j09;
mod j10;
mod j11;
mod j12;
mod j13;
mod j14;
mod j15;
mod j16;
mod j17;

use super::run::Run;
use super::verdict::{StepId, Verdict};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Act(StepId),
    Settle(StepId),
}

pub(crate) const PLAN: [Phase; 21] = [
    Phase::Act(StepId::J00),
    Phase::Act(StepId::J01),
    Phase::Act(StepId::J02),
    Phase::Act(StepId::J03),
    Phase::Settle(StepId::J00),
    Phase::Act(StepId::J04),
    Phase::Act(StepId::J05),
    Phase::Act(StepId::J06),
    Phase::Act(StepId::J07),
    Phase::Act(StepId::J08),
    Phase::Act(StepId::J09),
    Phase::Act(StepId::J10),
    Phase::Act(StepId::J11),
    Phase::Act(StepId::J12),
    Phase::Act(StepId::J13),
    Phase::Settle(StepId::J05),
    Phase::Act(StepId::J14),
    Phase::Act(StepId::J15),
    Phase::Act(StepId::J16),
    Phase::Act(StepId::J17),
    Phase::Settle(StepId::J08),
];

#[derive(Debug)]
pub(crate) enum Outcome {
    Done(Verdict),
    Held(Verdict),
}

impl Run<'_> {
    pub(crate) async fn act(&mut self, id: StepId) -> Outcome {
        match id {
            StepId::J00 => Outcome::Held(self.j00().await),
            StepId::J01 => Outcome::Done(self.j01().await),
            StepId::J02 => Outcome::Done(self.j02().await),
            StepId::J03 => Outcome::Done(self.j03().await),
            StepId::J04 => Outcome::Done(self.j04().await),
            StepId::J05 => Outcome::Held(self.j05().await),
            StepId::J06 => Outcome::Done(self.j06().await),
            StepId::J07 => Outcome::Done(self.j07().await),
            StepId::J08 => Outcome::Held(self.j08().await),
            StepId::J09 => Outcome::Done(self.j09().await),
            StepId::J10 => Outcome::Done(self.j10().await),
            StepId::J11 => Outcome::Done(self.j11().await),
            StepId::J12 => Outcome::Done(self.j12().await),
            StepId::J13 => Outcome::Done(self.j13().await),
            StepId::J14 => Outcome::Done(self.j14().await),
            StepId::J15 => Outcome::Done(self.j15().await),
            StepId::J16 => Outcome::Done(self.j16().await),
            StepId::J17 => Outcome::Done(self.j17().await),
        }
    }

    pub(crate) async fn settle(&mut self, id: StepId, held: Verdict) -> Verdict {
        match id {
            StepId::J00 => self.j00_late(held).await,
            StepId::J05 => self.j05_late(held).await,
            StepId::J08 => self.j08_late(held),
            _ => held,
        }
    }

    pub(crate) fn blocked_by(&self, id: StepId) -> Option<StepId> {
        let failed = |step: StepId| {
            self.w
                .verdicts
                .get(&step)
                .is_some_and(|verdict| verdict.status() == super::verdict::Status::Fail)
        };
        if id >= StepId::J03 && self.w.build_failed {
            return Some(StepId::J00);
        }
        let needs_sandbox = id >= StepId::J04 && id <= StepId::J14 || id == StepId::J16;
        (needs_sandbox && failed(StepId::J03)).then_some(StepId::J03)
    }
}
