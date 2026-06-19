//! Agent and role prompting helpers — thin re-export shell.

pub(in crate::console) use jackin_console::tui::prompts::{
    ConcreteAgentPickerChoices as AgentPickerChoices,
    ConcreteLaunchPromptDispatch as LaunchPromptDispatch,
    ConcreteLaunchPromptRequest as LaunchPromptRequest,
    committed_role_prompt, dispatch_launch_prompt, draw_role_resolution_dialog,
    launch_with_committed_agent, prompt_agent_for_launch,
};
#[cfg(test)]
pub(in crate::console) use jackin_console::tui::prompts::show_role_resolution_error;
pub(in crate::console) use jackin_console::tui::message::PromptOutcome;
#[cfg(test)]
pub(in crate::console) use jackin_console::tui::message::OnPromptFailure;
