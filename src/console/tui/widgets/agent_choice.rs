pub use jackin_console::widgets::agent_choice::{
    AgentChoice, AgentChoiceState as GenericAgentChoiceState, agent_picker_label, render,
};

impl AgentChoice for crate::agent::Agent {
    const ALL: &'static [Self] = Self::ALL;

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
        }
    }
}

pub type AgentChoiceState = GenericAgentChoiceState<crate::agent::Agent>;
