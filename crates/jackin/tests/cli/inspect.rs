// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{InspectView, Labels, Phase, ProviderName, SandboxName};
use jackin_openshell::{
    EndpointProtocol, HostPattern, NetworkRule, PowerAction, RuleEndpoint, RuleKey, TlsMode,
};
use jackin_testkit::{Rig, TestResult, guard};
use serde_json::json;

use super::support::{Failure, Harness, failed, keys, names};

fn codex_host_rule() -> Result<NetworkRule, Failure> {
    Ok(NetworkRule {
        key: RuleKey::provider_rule(&ProviderName::parse("other")?),
        endpoints: vec![RuleEndpoint {
            host: HostPattern::parse("api.openai.com")?,
            port: 443,
            protocol: EndpointProtocol::Tcp,
            tls: TlsMode::Default,
        }],
        binaries: Vec::new(),
    })
}

#[tokio::test]
async fn inspect_json_pins_every_key_set_and_reports_the_live_closure_violation() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;

        let clean = harness.run(&["inspect", name.as_str(), "--json"]).await?;
        clean.ok()?;
        let value = clean.value()?;
        assert_eq!(
            keys(&value)?,
            names(&[
                "schema",
                "summary",
                "providers",
                "policy",
                "closure_violations",
                "supervisor"
            ])
        );
        assert_eq!(value["schema"], 1);
        assert_eq!(
            keys(&value["summary"])?,
            names(&["name", "labels", "phase"])
        );
        assert_eq!(
            keys(&value["policy"])?,
            names(&["hash", "version", "loaded"])
        );
        assert_eq!(value["providers"], json!(["jackin-work-codex"]));
        assert_eq!(value["closure_violations"], json!([]));
        assert_eq!(
            value["supervisor"],
            json!({"Reported": {"Running": {"attempt": 1}}})
        );
        let typed: InspectView = clean.json()?;
        assert_eq!(typed.summary.name, name);

        rig.world.gateway.add_live_rule(&name, codex_host_rule()?);
        let flagged = harness.run(&["inspect", name.as_str(), "--json"]).await?;
        let value = flagged.value()?;
        let violations = value["closure_violations"].as_array().ok_or("no list")?;
        assert_eq!(violations.len(), 1);
        assert_eq!(
            keys(&violations[0])?,
            names(&["rule", "host", "provider_host"])
        );
        assert_eq!(violations[0]["rule"], "_provider_other");
        assert_eq!(violations[0]["host"], "api.openai.com");

        rig.world.session.power(&name, PowerAction::Stop).await?;
        rig.world.session.power(&name, PowerAction::Start).await?;
        let waiting = harness.run(&["inspect", name.as_str(), "--json"]).await?;
        assert_eq!(
            waiting.value()?["supervisor"],
            json!({"Reported": "Waiting"})
        );

        rig.world
            .gateway
            .force_phase(&name, Phase::Error { exit_code: Some(3) });
        let errored = harness.run(&["inspect", name.as_str(), "--json"]).await?;
        assert_eq!(errored.value()?["supervisor"], json!("Unreadable"));
        assert_eq!(
            errored.value()?["summary"]["phase"],
            json!({"Error": {"exit_code": 3}})
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_of_a_missing_or_unmanaged_sandbox_fails_with_empty_stdout() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let ghost = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&ghost, Labels::from_pairs([])?, Phase::Ready)?;
        let harness = Harness::new(&rig.world);
        harness.write_config(&rig.config.render())?;
        for name in ["jackin-absent-0009", ghost.as_str()] {
            let ran = harness.run(&["inspect", name, "--json"]).await?;
            failed(&ran, ExitClass::Failure, &[name])?;
        }
        Ok(())
    })
    .await
}
