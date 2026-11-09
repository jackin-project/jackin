A15MUT = r'''
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn raw() -> String {
    std::env::var("A15_MUT").unwrap_or_default()
}

pub(crate) fn on(id: &str) -> bool {
    raw().split(';').any(|item| item == id)
}

pub(crate) fn val(id: &str) -> Option<String> {
    raw().split(';').find_map(|item| {
        item.strip_prefix(id)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::to_owned)
    })
}

pub(crate) fn has(id: &str, value: &str) -> bool {
    val(id).is_some_and(|list| list.split(',').any(|entry| entry == value))
}

fn apply(value: &mut Value, env: &str) {
    let Ok(text) = std::env::var(env) else { return };
    let pairs: Vec<(String, Value)> = serde_json::from_str(&text).expect("A15 patch json");
    for (pointer, new) in pairs {
        if new == Value::String("@remove".to_owned()) {
            let (parent, key) = pointer.rsplit_once('/').expect("pointer");
            let target = value.pointer_mut(parent).expect("parent");
            if let Some(object) = target.as_object_mut() {
                object.remove(key);
            } else if let Some(array) = target.as_array_mut() {
                array.remove(key.parse::<usize>().expect("index"));
            }
        } else {
            *value.pointer_mut(&pointer).expect("pointer") = new;
        }
    }
}

pub(crate) fn patched<T: Serialize + DeserializeOwned>(value: T, env: &str) -> T {
    if std::env::var(env).is_err() {
        return value;
    }
    let mut json = serde_json::to_value(&value).expect("to value");
    apply(&mut json, env);
    serde_json::from_value(json).expect("from value")
}

pub(crate) fn canonical_bytes<T: Serialize>(value: &T, extra: Option<Value>) -> Vec<u8> {
    let omit = std::env::var("A15_OMIT").ok();
    if omit.is_none() && extra.is_none() {
        return serde_json::to_vec(value).expect("canonical");
    }
    let mut json = serde_json::to_value(value).expect("to value");
    if let Some(extra) = extra {
        json["credential"] = extra;
    }
    if let Some(list) = omit {
        for pointer in list.split(',').filter(|p| !p.is_empty()) {
            let (parent, key) = pointer.rsplit_once('/').expect("pointer");
            let target = if parent.is_empty() {
                &mut json
            } else {
                json.pointer_mut(parent).expect("parent")
            };
            if let Some(object) = target.as_object_mut() {
                object.remove(key);
            }
        }
    }
    serde_json::to_vec(&json).expect("canonical")
}

pub(crate) fn spec_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let touched = raw().split(';').any(|item| item.starts_with("spec."));
    if !touched {
        return serde_json::to_vec_pretty(value);
    }
    let mut json = serde_json::to_value(value)?;
    if on("spec.nondet") {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        json["nonce"] = Value::String(nanos.to_string());
    }
    if let Ok(leak) = std::env::var("A15_LEAK") {
        json["leak"] = Value::String(leak);
    }
    if on("spec.argv_string") {
        let joined = json["behavior"]["prompt"]["argv"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        json["behavior"]["prompt"]["argv"] = Value::String(joined);
    }
    if on("spec.answer_regex") {
        json["behavior"]["prompt"]["answer"] = serde_json::json!({"regex": "ready"});
    }
    if on("spec.target_wrong") {
        json["target"]["model"] = Value::String("other".to_owned());
    }
    serde_json::to_vec_pretty(&json)
}
'''
NEWFILES = [("crates/xtask/src/e2e/a15mut.rs", A15MUT)]
T = "crates/xtask/src/e2e/target.rs"
PATCHES = [
 ("crates/xtask/src/e2e.rs", "mod a1;\nmod adapter;\n", "mod a1;\nmod a15mut;\nmod adapter;\n"),
 (T,
  "        let agent = args.agent.as_deref().map(parse_agent).transpose()?;\n        let provider = args.provider.as_deref().map(parse_provider).transpose()?;\n        let agent = agent.unwrap_or(Agent::Codex);",
  "        if (super::a15mut::on(\"res.reject_agent\") && args.agent.is_some())\n            || (super::a15mut::on(\"res.reject_provider\") && args.provider.is_some())\n            || (super::a15mut::on(\"res.reject_pair\") && args.agent.is_some() && args.provider.is_some())\n        {\n            return Err(TargetError::SelectionName(\"mutation\".to_owned()));\n        }\n        let agent = args.agent.as_deref().map(parse_agent).transpose()?;\n        let provider = args.provider.as_deref().map(parse_provider).transpose()?;\n        let agent = agent.unwrap_or(Agent::Codex);"),
 (T,
  "        let tuple = lookup_tuple(agent, provider)?;\n        if tuple.state != TupleState::Supported {",
  "        let tuple = if super::a15mut::on(\"res.unknown_pair_ok\") {\n            lookup_tuple(agent, provider).or_else(|_| lookup_tuple(Agent::Codex, Provider::Codex))?\n        } else {\n            lookup_tuple(agent, provider)?\n        };\n        if tuple.state != TupleState::Supported && !super::a15mut::on(\"res.unsupported_ok\") {"),
 (T,
  "        if tuple.id != \"T01\" {\n            return Err(TargetError::AdapterUnavailable {\n                id: tuple.id.to_owned(),\n            });\n        }\n        let model = ModelName::parse(args.model.as_deref().unwrap_or(tuple.default_model))?;\n        tuple.check_model(model.as_str())?;",
  "        if tuple.id != \"T01\" && !super::a15mut::on(\"res.adapter_ok\") {\n            return Err(TargetError::AdapterUnavailable {\n                id: tuple.id.to_owned(),\n            });\n        }\n        let wanted_model = if super::a15mut::on(\"res.ignore_model\") {\n            tuple.default_model\n        } else if super::a15mut::on(\"res.default_model_wrong\") && args.model.is_none() {\n            \"gpt-5.1-codex\"\n        } else {\n            args.model.as_deref().unwrap_or(tuple.default_model)\n        };\n        let model = ModelName::parse(wanted_model)?;\n        if !super::a15mut::on(\"res.model_ok\") {\n            tuple.check_model(model.as_str())?;\n        }"),
 (T,
  "        let behavior = complete(tuple, behavior)?;\n        let credential = credential_for(args, home, tuple)?;",
  "        let behavior = complete(tuple, behavior)?;\n        let behavior = super::a15mut::patched(behavior, \"A15_BEH\");\n        let credential = credential_for(args, home, tuple)?;"),
 (T,
  "            if !provider_hosts.iter().any(|allowed| allowed == host) {\n                return Err(TargetError::OutsideProfile {",
  "            if !provider_hosts.iter().any(|allowed| allowed == host)\n                && !super::a15mut::has(\"host.skip\", kind)\n            {\n                return Err(TargetError::OutsideProfile {"),
 (T,
  "                adapter: \"t01-codex-v1\".to_owned(),\n                spec_sha256: String::new(),",
  "                adapter: if super::a15mut::on(\"res.adapter_id\") {\n                    \"t01-codex-v2\".to_owned()\n                } else {\n                    \"t01-codex-v1\".to_owned()\n                },\n                spec_sha256: String::new(),"),
 (T,
  "        let bytes = serde_json::to_vec(&canonical)?;\n        Ok(hex(&Sha256::digest(bytes)))",
  "        let extra = if super::a15mut::has(\"digest.cred\", self.credential.label()) {\n            Some(serde_json::to_value(&self.credential).expect(\"cred\"))\n        } else {\n            None\n        };\n        let bytes = super::a15mut::canonical_bytes(&canonical, extra);\n        Ok(hex(&Sha256::digest(bytes)))"),
 (T,
  "        let mut bytes = serde_json::to_vec_pretty(&self.probe_spec())?;\n        bytes.push(b'\\n');",
  "        let mut bytes = super::a15mut::spec_bytes(&self.probe_spec())?;\n        bytes.push(b'\\n');"),
 (T,
  "            \"--model\".to_owned(),\n            self.model.to_string(),\n        ]",
  "            \"--model\".to_owned(),\n            if super::a15mut::on(\"res.sel_model_ignored\") {\n                self.tuple.default_model.to_owned()\n            } else {\n                self.model.to_string()\n            },\n        ]"),
 (T,
  "    let missing = |category| TargetError::IncompleteBehavior {\n        id: tuple.id.to_owned(),\n        category,\n    };\n    Ok(CompleteBehaviorSpec {\n        version: raw\n            .version\n            .ok_or_else(|| missing(BehaviorCategory::Version))?,\n        process: raw\n            .process\n            .ok_or_else(|| missing(BehaviorCategory::Process))?,\n        prompt: raw\n            .prompt\n            .ok_or_else(|| missing(BehaviorCategory::Prompt))?,\n        terminal: raw\n            .terminal\n            .ok_or_else(|| missing(BehaviorCategory::Terminal))?,\n        credential: raw\n            .credential\n            .ok_or_else(|| missing(BehaviorCategory::Credential))?,\n        reflection: raw\n            .reflection\n            .ok_or_else(|| missing(BehaviorCategory::Reflection))?,\n        provider: raw\n            .provider\n            .ok_or_else(|| missing(BehaviorCategory::Provider))?,\n        usage: raw.usage.ok_or_else(|| missing(BehaviorCategory::Usage))?,\n    })",
  "    let missing = |category| TargetError::IncompleteBehavior {\n        id: tuple.id.to_owned(),\n        category,\n    };\n    let fill = super::adapter::t01_behavior();\n    let skip = |name: &str| super::a15mut::has(\"complete.fill\", name);\n    Ok(CompleteBehaviorSpec {\n        version: raw\n            .version\n            .or_else(|| skip(\"version\").then(|| fill.version.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Version))?,\n        process: raw\n            .process\n            .or_else(|| skip(\"process\").then(|| fill.process.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Process))?,\n        prompt: raw\n            .prompt\n            .or_else(|| skip(\"prompt\").then(|| fill.prompt.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Prompt))?,\n        terminal: raw\n            .terminal\n            .or_else(|| skip(\"terminal\").then(|| fill.terminal.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Terminal))?,\n        credential: raw\n            .credential\n            .or_else(|| skip(\"credential\").then(|| fill.credential.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Credential))?,\n        reflection: raw\n            .reflection\n            .or_else(|| skip(\"reflection\").then(|| fill.reflection.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Reflection))?,\n        provider: raw\n            .provider\n            .or_else(|| skip(\"provider\").then(|| fill.provider.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Provider))?,\n        usage: raw\n            .usage\n            .or_else(|| skip(\"usage\").then(|| fill.usage.clone()).flatten())\n            .ok_or_else(|| missing(BehaviorCategory::Usage))?,\n    })"),
 (T,
  "        let parsed = E2eCredential::parse(source)?;\n        return Err(TargetError::CredentialSource {\n            id: tuple.id.to_owned(),\n            kind: parsed.label().to_owned(),\n        });\n    }",
  "        let parsed = E2eCredential::parse(source)?;\n        if !super::a15mut::has(\"cred.accept\", parsed.label()) {\n            return Err(TargetError::CredentialSource {\n                id: tuple.id.to_owned(),\n                kind: parsed.label().to_owned(),\n            });\n        }\n    }"),
 (T,
  "    let home = args\n        .codex_home\n        .clone()\n        .or_else(|| home.map(|path| path.join(\".codex\")))\n        .ok_or(TargetError::CodexHome)?;\n    Ok(E2eCredential::CodexLogin { home })",
  "    let home = if super::a15mut::on(\"res.override_ignored\") {\n        home.map(|path| path.join(\".codex\"))\n    } else if super::a15mut::on(\"res.home_default_missing\") {\n        args.codex_home.clone()\n    } else {\n        args.codex_home\n            .clone()\n            .or_else(|| home.map(|path| path.join(\".codex\")))\n    }\n    .ok_or(TargetError::CodexHome)?;\n    if super::a15mut::on(\"res.cred_variant\") {\n        return Ok(E2eCredential::HostLogin { home });\n    }\n    Ok(E2eCredential::CodexLogin { home })"),
]
