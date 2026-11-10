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
    let one = AgentInstances::new(vec![inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?])?;
    assert_eq!(one.lead().agent, Agent::Codex);
    assert_eq!(one.len(), 1);
    let three = AgentInstances::new(vec![
        inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::Omp, Provider::Minimax, "work", "minimax-m3")?,
        inst(Agent::KimiCode, Provider::Kimi, "work", "kimi-k2")?,
    ])?;
    assert_eq!(three.len(), 3);
    assert_eq!(three.lead().agent, Agent::Codex);
    let repeated = AgentInstances::new(vec![
        inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::Codex, Provider::Zai, "other", "glm-5.3")?,
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
fn persisted_identity_preserves_the_lead_and_distinguishes_a_lead_swap()
-> Result<(), Box<dyn std::error::Error>> {
    let omp = inst(Agent::Omp, Provider::Kimi, "work", "kimi-for-coding")?;
    let codex = inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?;
    let first = AgentInstances::new(vec![omp.clone(), codex.clone()])?;
    let second = AgentInstances::new(vec![codex, omp])?;
    let persisted: AgentInstances = serde_json::from_str(&first.agents_json())?;
    assert_eq!(persisted.lead().agent, Agent::Omp);
    assert_ne!(first.agents_label(), second.agents_label());
    assert_ne!(first.agents_json(), second.agents_json());
    let old = jackin_core::Sha256::of(b"codex:codex:work:glm-5.3,omp:kimi:work:kimi-for-coding");
    assert_ne!(second.agents_label(), &old.as_str()[..32]);
    Ok(())
}

#[test]
fn canonical_identity_sorts_only_the_secondary_instances() -> Result<(), Box<dyn std::error::Error>>
{
    let omp = inst(Agent::Omp, Provider::Kimi, "work", "kimi-for-coding")?;
    let codex = inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?;
    let grok = inst(Agent::GrokBuild, Provider::Grok, "work", "grok-code")?;
    let first = AgentInstances::new(vec![omp.clone(), grok.clone(), codex.clone()])?;
    let second = AgentInstances::new(vec![omp, codex, grok])?;
    assert_eq!(first.agents_label(), second.agents_label());
    assert_eq!(first.agents_json(), second.agents_json());
    let persisted: AgentInstances = serde_json::from_str(&first.agents_json())?;
    assert_eq!(persisted.lead().agent, Agent::Omp);
    assert_eq!(persisted.as_slice()[1].agent, Agent::Codex);
    assert_eq!(persisted.as_slice()[2].agent, Agent::GrokBuild);
    Ok(())
}

#[test]
fn golden_label_and_exact_json_keys() -> Result<(), Box<dyn std::error::Error>> {
    let one = AgentInstances::new(vec![inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?])?;
    assert_eq!(one.agents_label(), "428e0189bf6f23cb88404e18a8342fe9");
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

#[test]
fn credential_environment_collisions_are_rejected_with_tuple_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (
            Agent::KimiCode,
            Provider::Kimi,
            Agent::Omp,
            Provider::Kimi,
            "kimi-for-coding",
            "KIMI_API_KEY",
            "T08",
            "T10",
        ),
        (
            Agent::KimiCode,
            Provider::Kimi,
            Agent::Codex,
            Provider::Kimi,
            "kimi-for-coding",
            "KIMI_API_KEY",
            "T08",
            "T17",
        ),
        (
            Agent::Omp,
            Provider::Kimi,
            Agent::Codex,
            Provider::Kimi,
            "kimi-for-coding",
            "KIMI_API_KEY",
            "T10",
            "T17",
        ),
        (
            Agent::Omp,
            Provider::Zai,
            Agent::Codex,
            Provider::Zai,
            "glm-5.3",
            "ZAI_API_KEY",
            "T09",
            "T18",
        ),
        (
            Agent::Omp,
            Provider::Minimax,
            Agent::Codex,
            Provider::Minimax,
            "MiniMax-M3",
            "MINIMAX_CODE_API_KEY",
            "T11",
            "T19",
        ),
        (
            Agent::Omp,
            Provider::Mimo,
            Agent::Codex,
            Provider::Mimo,
            "mimo-v2.6-pro",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "T12",
            "T20",
        ),
        (
            Agent::ClaudeCode,
            Provider::Kimi,
            Agent::Omp,
            Provider::Claude,
            "kimi-for-coding",
            "ANTHROPIC_API_KEY",
            "T13",
            "T21",
        ),
    ];
    for (
        first_agent,
        first_provider,
        second_agent,
        second_provider,
        model,
        env,
        first_id,
        second_id,
    ) in cases
    {
        let first_model = if first_agent == Agent::Omp && first_provider == Provider::Claude {
            "claude-haiku-5-5"
        } else if first_agent == Agent::Omp && first_provider == Provider::Minimax {
            "minimax-m3"
        } else {
            model
        };
        let error = AgentInstances::new(vec![
            inst(first_agent, first_provider, "work", first_model)?,
            inst(second_agent, second_provider, "work", model)?,
        ])
        .err()
        .ok_or("expected a credential collision")?;
        assert!(
            matches!(
                error,
                AgentInstanceError::CredentialEnvCollision {
                    env: found_env,
                    first,
                    second,
                } if found_env == env && first == first_id && second == second_id
            ),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn repeated_agent_refusal_precedes_environment_collision_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let error = AgentInstances::new(vec![
        inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::Omp, Provider::Zai, "other", "glm-5.3")?,
    ])
    .err()
    .ok_or("expected repeated-agent refusal")?;
    assert!(matches!(
        error,
        AgentInstanceError::RepeatedAgent { agent: Agent::Omp }
    ));
    Ok(())
}

#[test]
fn noncolliding_supported_pairs_construct_and_deserialization_rejects_collisions()
-> Result<(), Box<dyn std::error::Error>> {
    for second in [
        inst(Agent::Codex, Provider::Zai, "work", "glm-5.3")?,
        inst(Agent::Omp, Provider::Zai, "work", "glm-5.3")?,
    ] {
        let instances = AgentInstances::new(vec![
            inst(Agent::ClaudeCode, Provider::Zai, "work", "glm-5.3")?,
            second,
        ])?;
        assert_eq!(instances.lead().agent, Agent::ClaudeCode);
        assert_eq!(instances.len(), 2);
    }
    let value = serde_json::json!([
        {
            "agent": "omp",
            "provider": "zai",
            "account": "work",
            "model": "glm-5.3",
            "branch": "agent/omp"
        },
        {
            "agent": "codex",
            "provider": "zai",
            "account": "work",
            "model": "glm-5.3",
            "branch": "agent/codex"
        }
    ]);
    let error = serde_json::from_value::<AgentInstances>(value)
        .err()
        .ok_or("expected deserialization refusal")?;
    let text = error.to_string();
    assert!(text.contains("ZAI_API_KEY"), "{text}");
    assert!(text.contains("T09"), "{text}");
    assert!(text.contains("T18"), "{text}");
    Ok(())
}
