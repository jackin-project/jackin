// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn span_attr<'a>(
    span: &'a opentelemetry_sdk::trace::SpanData,
    key: &str,
) -> Option<std::borrow::Cow<'a, str>> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| attribute.value.as_str())
}
