// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod build;
pub mod card;
pub mod detail;
pub mod model;
pub mod render;
pub mod report;
pub mod text;

pub use build::{ViewFilter, build};
pub use card::card;
pub use model::{
    AccountQuota, Activity, Attention, Currency, Fact, Meter, Note, PositiveAmount, ProviderGroup,
    Reading, ResetCredit, Role, Unit, UsageView, Used, UsedFraction, short_id,
};
pub use render::{Ctx, Depth, Line, Report, Span, Tone};
pub use report::report;
