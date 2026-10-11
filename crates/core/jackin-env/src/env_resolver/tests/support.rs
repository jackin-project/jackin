// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) struct MockPrompter {
    responses: std::cell::RefCell<Vec<PromptResult>>,
    pub(super) captured_titles: std::cell::RefCell<Vec<String>>,
    pub(super) captured_defaults: std::cell::RefCell<Vec<Option<String>>>,
}

impl MockPrompter {
    pub(super) fn new(responses: Vec<PromptResult>) -> Self {
        Self {
            responses: std::cell::RefCell::new(responses),
            captured_titles: std::cell::RefCell::new(vec![]),
            captured_defaults: std::cell::RefCell::new(vec![]),
        }
    }
}

impl EnvPrompter for MockPrompter {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.captured_titles.borrow_mut().push(title.to_owned());
        self.captured_defaults
            .borrow_mut()
            .push(default.map(String::from));
        Ok(self.responses.borrow_mut().remove(0))
    }

    fn prompt_select(
        &self,
        title: &str,
        _options: &[String],
        default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.captured_titles.borrow_mut().push(title.to_owned());
        self.captured_defaults
            .borrow_mut()
            .push(default.map(String::from));
        Ok(self.responses.borrow_mut().remove(0))
    }
}

pub(super) struct ErrorPrompter;

impl EnvPrompter for ErrorPrompter {
    fn prompt_text(
        &self,
        _title: &str,
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        anyhow::bail!("prompt I/O failed")
    }

    fn prompt_select(
        &self,
        _title: &str,
        _options: &[String],
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        anyhow::bail!("prompt I/O failed")
    }
}

pub(super) fn static_var(default: &str) -> EnvVarDecl {
    EnvVarDecl {
        default_value: Some(default.to_owned()),
        interactive: false,
        skippable: false,
        prompt: None,
        options: vec![],
        depends_on: vec![],
    }
}

pub(super) fn interactive_text(prompt: &str) -> EnvVarDecl {
    EnvVarDecl {
        default_value: None,
        interactive: true,
        skippable: false,
        prompt: Some(prompt.to_owned()),
        options: vec![],
        depends_on: vec![],
    }
}

pub(super) fn interactive_select(prompt: &str, options: Vec<&str>) -> EnvVarDecl {
    EnvVarDecl {
        default_value: None,
        interactive: true,
        skippable: false,
        prompt: Some(prompt.to_owned()),
        options: options.into_iter().map(String::from).collect(),
        depends_on: vec![],
    }
}
