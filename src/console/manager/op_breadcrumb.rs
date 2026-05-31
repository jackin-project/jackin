//! Parsed 1Password breadcrumb model shared by manager geometry and views.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PathBreadcrumb {
    pub(crate) vault: String,
    pub(crate) item: String,
    pub(crate) item_subtitle: Option<String>,
    pub(crate) section: Option<String>,
    pub(crate) field: String,
    pub(crate) attribute_query: Option<String>,
}

/// Parse an `OpRef.path` breadcrumb.
///
/// Grammar: `<Vault>/<Item>[<subtitle>?]/[<Section>/]<Field>[?<query>]`.
pub(crate) fn parse_path_breadcrumb(path: &str) -> Option<PathBreadcrumb> {
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

pub(crate) fn breadcrumb_display_width(parts: &PathBreadcrumb) -> usize {
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
    (s.to_string(), None)
}

fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[cfg(test)]
mod tests {
    use super::parse_path_breadcrumb;

    #[test]
    fn parse_path_breadcrumb_3_segment_no_subtitle() {
        let p = parse_path_breadcrumb("Private/Stripe/api key").unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Stripe");
        assert!(p.item_subtitle.is_none());
        assert!(p.section.is_none());
        assert_eq!(p.field, "api key");
        assert!(p.attribute_query.is_none());
    }

    #[test]
    fn parse_path_breadcrumb_3_segment_with_subtitle() {
        let p = parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/auth").unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Claude");
        assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
        assert!(p.section.is_none());
        assert_eq!(p.field, "auth");
    }

    #[test]
    fn parse_path_breadcrumb_4_segment_with_subtitle() {
        let p = parse_path_breadcrumb("Private/Claude[alexey@zhokhov.com]/security/auth token")
            .unwrap();
        assert_eq!(p.vault, "Private");
        assert_eq!(p.item, "Claude");
        assert_eq!(p.item_subtitle.as_deref(), Some("alexey@zhokhov.com"));
        assert_eq!(p.section.as_deref(), Some("security"));
        assert_eq!(p.field, "auth token");
    }

    #[test]
    fn parse_path_breadcrumb_with_attribute_query() {
        let p = parse_path_breadcrumb("Private/GitHub/one-time password?attribute=otp").unwrap();
        assert_eq!(p.field, "one-time password");
        assert_eq!(p.attribute_query.as_deref(), Some("?attribute=otp"));
    }

    #[test]
    fn parse_path_breadcrumb_subtitle_containing_brackets() {
        let p = parse_path_breadcrumb("Private/Claude[has [bracket]]/auth").unwrap();
        assert_eq!(p.item, "Claude[has ");
        assert_eq!(p.item_subtitle.as_deref(), Some("bracket]"));
    }

    #[test]
    fn parse_path_breadcrumb_invalid_too_few_segments() {
        assert!(parse_path_breadcrumb("Private/Item").is_none());
        assert!(parse_path_breadcrumb("Private").is_none());
        assert!(parse_path_breadcrumb("").is_none());
    }

    #[test]
    fn parse_path_breadcrumb_invalid_too_many_segments() {
        assert!(parse_path_breadcrumb("a/b/c/d/e").is_none());
    }
}
