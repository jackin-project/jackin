// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Parsed 1Password breadcrumb model shared by console geometry and views.

#[derive(Debug, PartialEq, Eq)]
pub struct PathBreadcrumb {
    pub vault: String,
    pub item: String,
    pub item_subtitle: Option<String>,
    pub section: Option<String>,
    pub field: String,
    pub attribute_query: Option<String>,
}

/// Parse an `OpRef.path` breadcrumb.
///
/// Grammar: `<Vault>/<Item>[<subtitle>?]/[<Section>/]<Field>[?<query>]`.
#[must_use]
pub fn parse_path_breadcrumb(path: &str) -> Option<PathBreadcrumb> {
    if path.is_empty() {
        return None;
    }
    let (path_no_q, attr) = path
        .find('?')
        .map_or((path, None), |i| (&path[..i], Some(path[i..].to_string())));
    let segs: Vec<&str> = path_no_q.split('/').collect();
    let (item, item_subtitle, vault, section, field) = match segs.as_slice() {
        [vault, item_seg, field] => {
            let (item, sub) = split_bracket_subtitle(item_seg);
            (item, sub, vault.to_string(), None, field.to_string())
        }
        [vault, item_seg, section, field] => {
            let (item, sub) = split_bracket_subtitle(item_seg);
            (
                item,
                sub,
                vault.to_string(),
                Some(section.to_string()),
                field.to_string(),
            )
        }
        _ => return None,
    };
    Some(PathBreadcrumb {
        vault,
        item,
        item_subtitle,
        section,
        field,
        attribute_query: attr,
    })
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

fn split_bracket_subtitle(s: &str) -> (String, Option<String>) {
    if let Some(open) = s.rfind('[')
        && s.ends_with(']')
        && open < s.len() - 1
    {
        return (
            s[..open].to_string(),
            Some(s[open + 1..s.len() - 1].to_string()),
        );
    }
    (s.to_owned(), None)
}

fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[cfg(test)]
mod tests;
