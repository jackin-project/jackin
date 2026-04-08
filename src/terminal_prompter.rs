use crate::env_resolver::{EnvPrompter, PromptResult};
use dialoguer::{Input, Select};

pub struct TerminalPrompter;

impl EnvPrompter for TerminalPrompter {
    fn prompt_text(&self, title: &str, default: Option<&str>, skippable: bool) -> PromptResult {
        let mut input = Input::<String>::new().with_prompt(title);

        if let Some(d) = default {
            input = input.default(d.to_string());
        }

        if skippable {
            input = input.allow_empty(true);
        }

        match input.interact_text() {
            Ok(value) if value.is_empty() && skippable => PromptResult::Skipped,
            Ok(value) => PromptResult::Value(value),
            Err(_) => PromptResult::Skipped,
        }
    }

    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> PromptResult {
        let mut items: Vec<&str> = options.iter().map(String::as_str).collect();
        if skippable {
            items.push("(skip)");
        }

        let mut select = Select::new().with_prompt(title).items(&items);

        if let Some(d) = default
            && let Some(idx) = options.iter().position(|o| o == d)
        {
            select = select.default(idx);
        }

        match select.interact() {
            Ok(idx) if skippable && idx == options.len() => PromptResult::Skipped,
            Ok(idx) => PromptResult::Value(options[idx].clone()),
            Err(_) => PromptResult::Skipped,
        }
    }
}
