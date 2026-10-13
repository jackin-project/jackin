// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Demand-activated host usage broker process, shipped alongside `jackin`.

fn main() {
    jackin_runtime::usage_broker::run_service_process();
}
