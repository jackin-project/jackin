// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstanceError, AgentInstances, ModelName, Provider,
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
        model: ModelName::parse(model)?,
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
    assert_eq!(one.as_slice()[0].agent, Agent::Codex);
    assert_eq!(one.len(), 1);
    let three = AgentInstances::new(vec![
        inst(Agent::Codex, Provider::Codex, "work", "gpt-5.1-codex")?,
        inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::KimiCode, Provider::Kimi, "work", "kimi-k2")?,
    ])?;
    assert_eq!(three.len(), 3);
    assert_eq!(three.as_slice()[0].agent, Agent::Codex);
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
fn hash_and_json_are_canonical_without_encoding_the_lead() -> Result<(), Box<dyn std::error::Error>>
{
    let a = inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?;
    let b = inst(Agent::Codex, Provider::Codex, "work", "gpt-5.1-codex")?;
    let first = AgentInstances::new(vec![a.clone(), b.clone()])?;
    let second = AgentInstances::new(vec![b, a])?;
    assert_eq!(first.hash(), second.hash());
    assert_eq!(first.agents_json()?, second.agents_json()?);
    assert_eq!(first.as_slice()[0].agent, Agent::Omp);
    assert_eq!(second.as_slice()[0].agent, Agent::Codex);
    assert_eq!(first.hash().as_str(), "d078d2b7f62254df8287c7e52ba9bc5f");
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
    assert_eq!(one.hash().as_str(), "235f80276e0174fa411c641b8bc2f777");
    let json = one.agents_json()?;
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

#[test]
fn model_names_keep_hash_framing_unambiguous_and_preserve_colons()
-> Result<(), Box<dyn std::error::Error>> {
    for invalid in ["", "vendor,model", "vendor\nmodel"] {
        assert!(ModelName::parse(invalid).is_err(), "{invalid:?}");
    }
    let model = ModelName::parse("vendor:model-v2")?;
    let instances = AgentInstances::new(vec![AgentInstance::new(
        Agent::Codex,
        Provider::Codex,
        AccountName::parse("work")?,
        model,
    )])?;
    let json = instances.agents_json()?;
    let value: serde_json::Value = serde_json::from_str(&json)?;
    assert_eq!(value[0]["model"], "vendor:model-v2");
    assert_eq!(serde_json::from_str::<AgentInstances>(&json)?, instances);
    Ok(())
}
