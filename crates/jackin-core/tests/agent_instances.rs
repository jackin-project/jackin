// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstanceError, AgentInstances, Provider,
};

fn inst(
    agent: Agent,
    provider: Provider,
    account: &str,
    model: &str,
) -> Result<AgentInstance, Box<dyn std::error::Error>> {
    Ok(AgentInstance {
        agent,
        provider,
        account: AccountName::parse(account)?,
        model: model.to_owned(),
        branch: format!("agent/{}", agent.as_str()),
    })
}

#[test]
fn one_and_three_valid_and_repeated_and_empty() -> Result<(), Box<dyn std::error::Error>> {
    let one = AgentInstances::new(vec![inst(
        Agent::Codex,
        Provider::Codex,
        "work",
        "gpt-5.1-codex",
    )?])?;
    assert_eq!(one.lead().agent, Agent::Codex);
    assert_eq!(one.len(), 1);
    let three = AgentInstances::new(vec![
        inst(Agent::Codex, Provider::Codex, "work", "gpt-5.1-codex")?,
        inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::KimiCode, Provider::Kimi, "work", "kimi-k2")?,
    ])?;
    assert_eq!(three.len(), 3);
    assert_eq!(three.lead().agent, Agent::Codex);
    let repeated = AgentInstances::new(vec![
        inst(Agent::Codex, Provider::Codex, "work", "gpt-5.1-codex")?,
        inst(Agent::Codex, Provider::Codex, "other", "gpt-5.1-codex")?,
    ]);
    assert!(matches!(
        repeated,
        Err(AgentInstanceError::RepeatedAgent {
            agent: Agent::Codex
        })
    ));
    let empty = AgentInstances::new(vec![]);
    assert!(matches!(empty, Err(AgentInstanceError::Empty)));
    Ok(())
}

#[test]
fn label_and_json_order_invariant_lead_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let a = inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?;
    let b = inst(Agent::Codex, Provider::Codex, "work", "gpt-5.1-codex")?;
    let first = AgentInstances::new(vec![a.clone(), b.clone()])?;
    let second = AgentInstances::new(vec![b, a])?;
    assert_eq!(first.agents_label(), second.agents_label());
    assert_eq!(first.agents_json(), second.agents_json());
    assert_eq!(first.lead().agent, Agent::Omp);
    assert_eq!(second.lead().agent, Agent::Codex);
    assert_eq!(first.agents_label(), "d078d2b7f62254df8287c7e52ba9bc5f");
    Ok(())
}

#[test]
fn golden_label_and_exact_json_keys() -> Result<(), Box<dyn std::error::Error>> {
    let one = AgentInstances::new(vec![inst(
        Agent::Codex,
        Provider::Codex,
        "work",
        "gpt-5.1-codex",
    )?])?;
    assert_eq!(one.agents_label(), "235f80276e0174fa411c641b8bc2f777");
    let json = one.agents_json();
    let value: serde_json::Value = serde_json::from_str(&json)?;
    let keys: Vec<&str> = value[0]
        .as_object()
        .ok_or("object")?
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["account", "agent", "branch", "model", "provider"]);
    Ok(())
}
