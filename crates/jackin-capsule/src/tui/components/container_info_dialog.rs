#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfoDiagnostics {
    pub host_version: String,
    pub run_id: String,
    pub run_log_display: String,
    pub run_log_href: Option<String>,
}

impl Default for ContainerInfoDiagnostics {
    fn default() -> Self {
        Self {
            host_version: "unknown".to_string(),
            run_id: String::new(),
            run_log_display: "(not set)".to_string(),
            run_log_href: None,
        }
    }
}

pub(super) struct ContainerInfoRow<'a> {
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) emphasise: bool,
    pub(super) href: Option<&'a str>,
}

impl<'a> ContainerInfoRow<'a> {
    pub(super) fn new(label: &'static str, value: String) -> Self {
        Self {
            label,
            value,
            emphasise: false,
            href: None,
        }
    }

    pub(super) fn emphasised(mut self) -> Self {
        self.emphasise = true;
        self
    }

    pub(super) fn hyperlink(mut self, href: Option<&'a str>) -> Self {
        self.href = href;
        self
    }
}

/// Show `"(none)"` for empty role / agent strings so a missing value
/// is visibly missing rather than a confusingly empty gutter.
pub(super) fn non_empty_or_dim(s: &str) -> String {
    if s.is_empty() {
        "(none)".to_string()
    } else {
        s.to_string()
    }
}
