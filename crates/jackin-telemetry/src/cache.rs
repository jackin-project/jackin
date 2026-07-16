// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Governed cache-decision telemetry.

use crate::schema::enums::{CacheName, CacheResult};
use crate::{Attr, FieldSet, Value};

/// Emit the registered event and increment the matching cache-decision metric.
pub fn decision(name: CacheName, result: CacheResult) {
    let attrs = [
        Attr {
            key: crate::schema::attrs::CACHE_NAME,
            value: Value::Str(name.as_str()),
        },
        Attr {
            key: crate::schema::attrs::CACHE_RESULT,
            value: Value::Str(result.as_str()),
        },
    ];
    let _event = crate::emit_event(&crate::event::CACHE_DECISION, FieldSet::new(&attrs, None));
    let _count = crate::counter(&crate::metric::CACHE_DECISIONS).add(1, &attrs);
}
