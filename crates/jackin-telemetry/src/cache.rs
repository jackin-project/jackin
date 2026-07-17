// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Governed cache-decision telemetry.

use std::time::Instant;

use crate::schema::enums::{CacheName, CacheResult};
use crate::{Attr, FieldSet, Value};

/// Emit the registered event and record the complete cache-decision metric family.
pub fn decision(name: CacheName, result: CacheResult) {
    let started_at = Instant::now();
    let name_attr = Attr {
        key: crate::schema::attrs::CACHE_NAME,
        value: Value::Str(name.as_str()),
    };
    let _active =
        crate::up_down_counter(&crate::metric::CACHE_DECISION_ACTIVE).add(1, &[name_attr]);
    let attrs = [
        name_attr,
        Attr {
            key: crate::schema::attrs::CACHE_RESULT,
            value: Value::Str(result.as_str()),
        },
    ];
    let _event = crate::emit_event(&crate::event::CACHE_DECISION, FieldSet::new(&attrs, None));
    let _count = crate::counter(&crate::metric::CACHE_DECISIONS).add(1, &attrs);
    let _duration = crate::histogram(&crate::metric::CACHE_DECISION_DURATION)
        .record(started_at.elapsed().as_secs_f64(), &attrs);
    let _active =
        crate::up_down_counter(&crate::metric::CACHE_DECISION_ACTIVE).add(-1, &[name_attr]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn decision_metric_family_has_only_bounded_cache_dimensions() {
        let name = crate::schema::attrs::CACHE_NAME;
        let result = crate::schema::attrs::CACHE_RESULT;
        assert_eq!(
            crate::metric::CACHE_DECISIONS
                .dimensions()
                .iter()
                .map(|attribute| attribute.name)
                .collect::<Vec<_>>(),
            [name, result]
        );
        assert_eq!(
            crate::metric::CACHE_DECISION_ACTIVE
                .dimensions()
                .iter()
                .map(|attribute| attribute.name)
                .collect::<Vec<_>>(),
            [name]
        );
        assert_eq!(
            crate::metric::CACHE_DECISION_DURATION
                .dimensions()
                .iter()
                .map(|attribute| attribute.name)
                .collect::<Vec<_>>(),
            [name, result]
        );
    }
}
