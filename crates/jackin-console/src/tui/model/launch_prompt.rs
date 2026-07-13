// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch-prompt trait definitions and `ConsoleApp` implementations.

use super::{ConsoleApp, ConsoleAppStage};

pub trait LaunchAgentPromptManagerState<RoleSelector, Agent>
where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(
        &mut self,
        role: RoleSelector,
        picker: crate::tui::components::agent_choice::AgentChoiceState<Agent>,
    );

    fn clear_launch_role_prompt(&mut self);
}

pub fn open_launch_agent_prompt_plan<RoleSelector, Agent>(
    state: &mut impl LaunchAgentPromptState<RoleSelector, Agent>,
    role: RoleSelector,
    choices: Vec<Agent>,
) where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    state.open_launch_agent_prompt(role, choices);
}

pub trait LaunchAgentPromptState<RoleSelector, Agent>
where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(&mut self, role: RoleSelector, choices: Vec<Agent>);
}

impl<Manager, LaunchInput, RoleSelector, OpCache, Agent> LaunchAgentPromptState<RoleSelector, Agent>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchAgentPromptManagerState<RoleSelector, Agent>,
    RoleSelector: Clone,
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(&mut self, role: RoleSelector, choices: Vec<Agent>) {
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_agent_prompt(
            role.clone(),
            crate::tui::components::agent_choice::AgentChoiceState::with_choices(choices),
        );
        manager.clear_launch_role_prompt();
        self.pending_launch_role = Some(role);
    }
}

pub trait LaunchRolePromptManagerState<RoleSelector>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<RoleSelector>,
    );
}

pub trait LaunchProviderPickerManagerState<RoleSelector, Agent, Provider>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_provider_picker(
        &mut self,
        picker: crate::tui::components::provider_picker::ProviderPickerState<
            RoleSelector,
            Agent,
            Provider,
        >,
    );
}

pub fn open_launch_role_prompt_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
    input: LaunchInput,
    roles: Vec<RoleSelector>,
    selected: Option<usize>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.open_launch_role_prompt(input, roles, selected);
}

pub fn clear_pending_launch_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.clear_pending_launch();
}

pub fn clear_pending_launch_role_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) {
    state.pending_launch_role = None;
}

pub fn take_pending_launch_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) -> Option<LaunchInput> {
    state.pending_launch.take()
}

pub fn take_pending_launch_and_role_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) -> Option<(LaunchInput, RoleSelector)> {
    Some((
        state.pending_launch.take()?,
        state.pending_launch_role.take()?,
    ))
}

pub fn store_pending_launch_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
    input: LaunchInput,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.store_pending_launch(input);
}

pub fn open_launch_provider_picker_plan<LaunchInput, RoleSelector, Agent, Provider>(
    state: &mut impl LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>,
    input: LaunchInput,
    role: RoleSelector,
    agent: Agent,
    providers: Vec<Provider>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    state.open_launch_provider_picker(input, role, agent, providers);
}

pub trait LaunchRolePromptState<LaunchInput, RoleSelector>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        input: LaunchInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    );

    fn clear_pending_launch(&mut self);

    fn store_pending_launch(&mut self, input: LaunchInput);
}

pub trait LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    fn open_launch_provider_picker(
        &mut self,
        input: LaunchInput,
        role: RoleSelector,
        agent: Agent,
        providers: Vec<Provider>,
    );
}

impl<Manager, LaunchInput, RoleSelector, OpCache> LaunchRolePromptState<LaunchInput, RoleSelector>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchRolePromptManagerState<RoleSelector>,
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        input: LaunchInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    ) {
        let mut picker = crate::tui::components::role_picker::RolePickerState::launch(roles);
        if let Some(selected) = selected {
            picker.list_state.select(Some(selected));
        }
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_role_prompt(picker);
        self.pending_launch = Some(input);
        self.pending_launch_role = None;
    }

    fn clear_pending_launch(&mut self) {
        self.pending_launch = None;
        self.pending_launch_role = None;
    }

    fn store_pending_launch(&mut self, input: LaunchInput) {
        self.pending_launch = Some(input);
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache, Agent, Provider>
    LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchProviderPickerManagerState<RoleSelector, Agent, Provider>,
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    fn open_launch_provider_picker(
        &mut self,
        input: LaunchInput,
        role: RoleSelector,
        agent: Agent,
        providers: Vec<Provider>,
    ) {
        let picker = crate::tui::components::provider_picker::ProviderPickerState::new(
            role.clone(),
            agent,
            providers,
        );
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_provider_picker(picker);
        self.pending_launch = Some(input);
        self.pending_launch_role = Some(role);
    }
}
