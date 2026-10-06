// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Parsed 1Password breadcrumb model shared by console geometry and views.

pub type PathBreadcrumb = jackin_core::OpBreadcrumbParts;

/// Parse an `OpRef.path` breadcrumb.
///
/// Grammar: `<Vault>/<Item>[<subtitle>?]/[<Section>/]<Field>[?<query>]`.
#[must_use]
pub fn parse_path_breadcrumb(path: &str) -> Option<PathBreadcrumb> {
    jackin_core::parse_op_breadcrumb_path(path)
}

#[must_use]
pub fn breadcrumb_display_width(parts: &PathBreadcrumb) -> usize {
    let mut width = text_width(&parts.vault) + text_width(" / ") + text_width(&parts.item);
    if let Some(subtitle) = &parts.item_subtitle {
        width += 1 + text_width(subtitle);
    }
    if let Some(section) = &parts.section {
        width += text_width(" / ") + text_width(section);
    }
    width += text_width(" \u{2192} ") + text_width(&parts.field);
    if let Some(query) = &parts.attribute_query {
        width += 1 + text_width(query);
    }
    width
}

fn text_width(text: &str) -> usize {
    termrock::text::display_cols(text)
}

#[cfg(test)]
mod tests;
