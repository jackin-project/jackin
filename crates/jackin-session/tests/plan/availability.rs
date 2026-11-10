// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, Provider, TUPLES, TupleState, lookup_tuple,
};
use jackin_session::{HeadState, PlanError, plan_multi};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

#[derive(Clone, Copy)]
struct Case {
    id: &'static str,
    agent: Agent,
    provider: Provider,
    model: &'static str,
}

const SUPPORTED: [Case; 15] = [
    Case {
        id: "T01",
        agent: Agent::Codex,
        provider: Provider::Codex,
        model: "gpt-6.1-sol",
    },
    Case {
        id: "T03",
        agent: Agent::GrokBuild,
        provider: Provider::Grok,
        model: "grok-4.7",
    },
    Case {
        id: "T09",
        agent: Agent::Omp,
        provider: Provider::Zai,
        model: "glm-5.3",
    },
    Case {
        id: "T10",
        agent: Agent::Omp,
        provider: Provider::Kimi,
        model: "kimi-for-coding",
    },
    Case {
        id: "T11",
        agent: Agent::Omp,
        provider: Provider::Minimax,
        model: "minimax-m3",
    },
    Case {
        id: "T12",
        agent: Agent::Omp,
        provider: Provider::Mimo,
        model: "mimo-v2.6-pro",
    },
    Case {
        id: "T13",
        agent: Agent::ClaudeCode,
        provider: Provider::Kimi,
        model: "kimi-for-coding",
    },
    Case {
        id: "T14",
        agent: Agent::ClaudeCode,
        provider: Provider::Zai,
        model: "glm-5.3",
    },
    Case {
        id: "T15",
        agent: Agent::ClaudeCode,
        provider: Provider::Minimax,
        model: "MiniMax-M3",
    },
    Case {
        id: "T16",
        agent: Agent::ClaudeCode,
        provider: Provider::Mimo,
        model: "mimo-v2.6-pro",
    },
    Case {
        id: "T17",
        agent: Agent::Codex,
        provider: Provider::Kimi,
        model: "kimi-for-coding",
    },
    Case {
        id: "T18",
        agent: Agent::Codex,
        provider: Provider::Zai,
        model: "glm-5.3",
    },
    Case {
        id: "T19",
        agent: Agent::Codex,
        provider: Provider::Minimax,
        model: "MiniMax-M3",
    },
    Case {
        id: "T20",
        agent: Agent::Codex,
        provider: Provider::Mimo,
        model: "mimo-v2.6-pro",
    },
    Case {
        id: "T21",
        agent: Agent::Omp,
        provider: Provider::Claude,
        model: "claude-haiku-5-5",
    },
];

struct Refusal {
    case: Case,
    state: TupleState,
    reason: &'static str,
}

const UNPROVEN: &str = "not yet proven on a real gateway";
const REFUSALS: [Refusal; 7] = [
    Refusal {
        case: Case {
            id: "T02",
            agent: Agent::ClaudeCode,
            provider: Provider::Claude,
            model: "haiku",
        },
        state: TupleState::BlockedVendor,
        reason: "vendor clarification pending",
    },
    Refusal {
        case: Case {
            id: "T04",
            agent: Agent::MuseCode,
            provider: Provider::Muse,
            model: "muse-spark-1.2",
        },
        state: TupleState::Unavailable,
        reason: UNPROVEN,
    },
    Refusal {
        case: Case {
            id: "T05",
            agent: Agent::Antigravity,
            provider: Provider::Antigravity,
            model: "gemini-4-argon-medium",
        },
        state: TupleState::Unavailable,
        reason: "subscription requires a desktop keyring in the sandbox",
    },
    Refusal {
        case: Case {
            id: "T06",
            agent: Agent::MinimaxCode,
            provider: Provider::Minimax,
            model: "minimax-m3",
        },
        state: TupleState::Unavailable,
        reason: "no stable Linux artifact",
    },
    Refusal {
        case: Case {
            id: "T07",
            agent: Agent::MimoCode,
            provider: Provider::Mimo,
            model: "mimo-v2.6-pro",
        },
        state: TupleState::Unavailable,
        reason: "native login has no refresh grant",
    },
    Refusal {
        case: Case {
            id: "T08",
            agent: Agent::KimiCode,
            provider: Provider::Kimi,
            model: "kimi-for-coding",
        },
        state: TupleState::Unavailable,
        reason: "native refresh and final live proof pending",
    },
    Refusal {
        case: Case {
            id: "T22",
            agent: Agent::Omp,
            provider: Provider::Codex,
            model: "gpt-5.1-codex",
        },
        state: TupleState::Unavailable,
        reason: "no API key on this host",
    },
];

fn account(case: &Case) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(&case.id.to_lowercase())?)
}

fn role() -> Result<RoleFixture, Box<dyn std::error::Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(
        "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[agents.grok]\ninstructions = \"instructions/grok.md\"\n".to_owned(),
    ))
}

async fn setup() -> Result<super::support::Setup, Box<dyn std::error::Error>> {
    let mut accounts = String::new();
    for tuple in TUPLES {
        let name = tuple.id.to_lowercase();
        let credential = if tuple.id == "T01" {
            "source = \"codex-login\", home = \"/tmp/codex-home\", chatgpt_account_id = \"3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f\"".to_owned()
        } else if tuple.id == "T03" {
            "source = \"host-login\", path = \"/tmp/grok-home/auth.json\"".to_owned()
        } else {
            "source = \"file\", path = \"/tmp/credential\"".to_owned()
        };
        accounts.push_str(&format!(
            "[accounts.{name}]\nprovider = \"{}\"\ncredential = {{ {credential} }}\n\n",
            tuple.provider.as_str()
        ));
    }
    accounts.push_str("[accounts.zai-control]\nprovider = \"zai\"\ncredential = { source = \"file\", path = \"/tmp/credential\" }\n");
    let mut spec = Spec::new(role()?);
    spec.accounts = accounts;
    spec.build().await
}

fn instance(case: &Case) -> Result<AgentInstance, Box<dyn std::error::Error>> {
    Ok(AgentInstance::new(
        case.agent,
        case.provider,
        account(case)?,
        case.model.to_owned(),
    ))
}

fn control(case: &Case) -> Case {
    if case.agent == Agent::Codex {
        Case {
            id: "T14",
            agent: Agent::ClaudeCode,
            provider: Provider::Zai,
            model: "glm-5.3",
        }
    } else {
        Case {
            id: "T18",
            agent: Agent::Codex,
            provider: Provider::Zai,
            model: "glm-5.3",
        }
    }
}

fn control_instance(case: &Case) -> Result<AgentInstance, Box<dyn std::error::Error>> {
    let control = control(case);
    Ok(AgentInstance::new(
        control.agent,
        control.provider,
        AccountName::parse("zai-control")?,
        control.model.to_owned(),
    ))
}

fn secondary_instances(case: &Case) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        control_instance(case)?,
        instance(case)?,
    ])?)
}

fn assert_refusal(error: PlanError, refusal: &Refusal) {
    let case = &refusal.case;
    assert!(
        matches!(
            error,
            PlanError::TupleNotSupported { agent, provider, ref reason }
                if agent == case.agent && provider == case.provider && reason == refusal.reason
        ),
        "{}: {error}",
        case.id
    );
}

#[tokio::test]
async fn every_supported_tuple_plans_as_lead_with_its_exact_identity_and_default_model()
-> TestResult {
    guard(async {
        let setup = setup().await?;
        for case in SUPPORTED {
            let plan = setup.plan(
                setup
                    .request(1)?
                    .tuple(case.agent, case.provider)
                    .account(Some(account(&case)?))
                    .model(Some(case.model.to_owned())),
            )?;
            let lead = plan.instances().lead();
            assert_eq!(lead.agent, case.agent, "{}", case.id);
            assert_eq!(lead.provider, case.provider, "{}", case.id);
            assert_eq!(lead.account, account(&case)?, "{}", case.id);
            assert_eq!(lead.model, case.model, "{}", case.id);
            assert_eq!(plan.account().account, account(&case)?, "{}", case.id);
            assert_eq!(
                plan.draft().labels().agents,
                *plan.instances(),
                "{}",
                case.id
            );
            assert_eq!(
                plan.files().layout.exec[0].provider,
                case.provider.as_str(),
                "{}",
                case.id
            );
            assert_eq!(
                plan.files().layout.exec[0].account,
                case.id.to_lowercase(),
                "{}",
                case.id
            );
            assert_eq!(plan.files().layout.exec[0].model, case.model, "{}", case.id);

            let defaulted = setup.plan(
                setup
                    .request(2)?
                    .tuple(case.agent, case.provider)
                    .account(Some(account(&case)?))
                    .model(None),
            )?;
            assert_eq!(
                defaulted.instances().lead().model,
                case.model,
                "{}",
                case.id
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_supported_tuple_plans_as_a_secondary_in_canonical_lead_first_layouts() -> TestResult
{
    guard(async {
        let setup = setup().await?;
        for case in SUPPORTED {
            let instances = secondary_instances(&case)?;
            let layout = plan_multi(&instances, None, HeadState::Born)?;
            let plan = setup.plan(setup.request(3)?.instances(instances.clone()))?;
            let control = control(&case);
            for found in [plan.instances(), &instances] {
                let items: Vec<_> = found.iter().collect();
                assert_eq!(items.len(), 2, "{}", case.id);
                assert_eq!(items[0].agent, control.agent, "{}", case.id);
                assert_eq!(items[1].agent, case.agent, "{}", case.id);
                assert_eq!(items[1].provider, case.provider, "{}", case.id);
                assert_eq!(items[1].account, account(&case)?, "{}", case.id);
                assert_eq!(items[1].model, case.model, "{}", case.id);
            }
            assert_eq!(layout.worktrees[0].agent, control.agent, "{}", case.id);
            assert_eq!(layout.worktrees[1].agent, case.agent, "{}", case.id);
            assert_eq!(
                layout.exec[1].provider,
                case.provider.as_str(),
                "{}",
                case.id
            );
            assert_eq!(
                layout.exec[1].account,
                case.id.to_lowercase(),
                "{}",
                case.id
            );
            assert_eq!(layout.exec[1].model, case.model, "{}", case.id);
            assert_eq!(plan.files().layout, layout, "{}", case.id);
            assert_eq!(plan.draft().labels().agents, instances, "{}", case.id);
            assert_eq!(plan.draft().providers().len(), 2, "{}", case.id);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_remaining_tuple_refuses_lead_launch_with_its_exact_typed_reason() -> TestResult {
    guard(async {
        let setup = setup().await?;
        for refusal in REFUSALS {
            let case = &refusal.case;
            let tuple = lookup_tuple(case.agent, case.provider)?;
            assert_eq!(tuple.id, case.id);
            assert_eq!(tuple.state, refusal.state, "{}", case.id);
            assert_eq!(tuple.reason, Some(refusal.reason), "{}", case.id);
            let error = setup
                .plan(
                    setup
                        .request(1)?
                        .tuple(case.agent, case.provider)
                        .account(Some(account(case)?))
                        .model(Some(case.model.to_owned())),
                )
                .err()
                .ok_or("expected lead refusal")?;
            assert_refusal(error, &refusal);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_remaining_tuple_refuses_secondary_launch_and_multi_with_the_same_reason()
-> TestResult {
    guard(async {
        let setup = setup().await?;
        for refusal in REFUSALS {
            let instances = secondary_instances(&refusal.case)?;
            let error = plan_multi(&instances, None, HeadState::Born)
                .err()
                .ok_or("expected secondary plan_multi refusal")?;
            assert_refusal(error, &refusal);
            let error = setup
                .plan(setup.request(1)?.instances(instances))
                .err()
                .ok_or("expected secondary plan_launch refusal")?;
            assert_refusal(error, &refusal);
        }
        Ok(())
    })
    .await
}

#[test]
fn t13_plans_multi_with_the_exact_kimi_identity_and_model() -> TestResult {
    let case = SUPPORTED
        .iter()
        .find(|case| case.id == "T13")
        .ok_or("missing T13 supported case")?;
    let instances = AgentInstances::single(instance(case)?);
    let layout = plan_multi(&instances, None, HeadState::Born)?;
    assert_eq!(layout.exec[0].provider, "kimi");
    assert_eq!(layout.exec[0].account, "t13");
    assert_eq!(layout.exec[0].model, "kimi-for-coding");
    assert_eq!(instances.lead().agent, Agent::ClaudeCode);
    assert_eq!(instances.lead().provider, Provider::Kimi);
    Ok(())
}

#[tokio::test]
async fn every_remaining_tuple_refuses_lead_before_account_selection() -> TestResult {
    guard(async {
        let setup = Spec::new(role()?).build().await?;
        for refusal in REFUSALS {
            let case = &refusal.case;
            let error = setup
                .plan(
                    setup
                        .request(1)?
                        .tuple(case.agent, case.provider)
                        .model(Some(case.model.to_owned())),
                )
                .err()
                .ok_or("expected lead refusal before account selection")?;
            assert_refusal(error, &refusal);
        }
        Ok(())
    })
    .await
}
