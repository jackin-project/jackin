// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn ignores_irrelevant_variables() {
    let parsed = parse_zshrc_source("PATH=/usr/bin\nFOO=bar\nexport EDITOR=vim\n");
    assert!(parsed.values.is_empty());
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn parses_plain_and_export_assignments() {
    let parsed =
        parse_zshrc_source("ANTHROPIC_API_KEY=sk-ant-1\nexport OPENAI_API_KEY=sk-openai-2\n");
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("sk-ant-1")
    );
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("sk-openai-2")
    );
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn unquotes_simple_single_and_double_quotes() {
    let parsed =
        parse_zshrc_source("OPENAI_API_KEY=\"a\\\"b\\\\c\"\nANTHROPIC_API_KEY='lit $HOME `x`'\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("a\"b\\c")
    );
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("lit $HOME `x`")
    );
}

#[test]
fn dollar_expansions_are_unresolved() {
    let parsed = parse_zshrc_source(
        "CLAUDE_CONFIG_DIR=$HOME/.claude\nCODEX_HOME=${XDG_DATA_HOME}/codex\nOPENAI_API_KEY=a$Bc\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 3);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
    assert_eq!(parsed.unresolved[0].detail, "$HOME");
    assert_eq!(parsed.unresolved[1].detail, "${XDG_DATA_HOME}");
    assert_eq!(parsed.unresolved[2].detail, "$Bc");
}

#[test]
fn op_read_is_typed() {
    let parsed = parse_zshrc_source("KIMI_API_KEY=$(op read op://vault/item/field)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(parsed.unresolved[0].kind, UnresolvedKind::OpRead);
    assert_eq!(
        parsed.unresolved[0].detail,
        "$(op read op://vault/item/field)"
    );
}

#[test]
fn defined_function_calls_are_typed() {
    let parsed = parse_zshrc_source(
        "get_key() { echo hi; }\nfunction fetch_key { echo hi; }\nXAI_API_KEY=$(get_key xai)\nZHIPU_API_KEY=`fetch_key zai`\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::FunctionCall)
    );
}

#[test]
fn unknown_commands_are_command_substitutions() {
    let parsed =
        parse_zshrc_source("ZHIPU_API_KEY=$(cat ~/.key)\nOPENAI_API_KEY=`brew --prefix`/bin\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::CommandSubstitution)
    );
}

#[test]
fn semicolons_inside_substitutions_do_not_split() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=$(echo a; echo b)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(
        parsed.unresolved[0].kind,
        UnresolvedKind::CommandSubstitution
    );
    assert_eq!(parsed.unresolved[0].detail, "$(echo a; echo b)");
}

#[test]
fn arithmetic_tilde_and_globs_are_unresolved() {
    let parsed = parse_zshrc_source(
        "OPENAI_API_KEY=$((1 + 2))\nCODEX_HOME=~/.codex\nANTHROPIC_API_KEY=sk-*\n",
    );
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 3);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
    assert_eq!(parsed.unresolved[1].detail, "~");
}

#[test]
fn handles_comments_semicolons_and_continuations() {
    let parsed = parse_zshrc_source(
        "# leading comment\nOPENAI_API_KEY=sk # trailing\nANTHROPIC_API_KEY=\"a#b\";export KIMI_API_KEY=sk-\\\nabc\n",
    );
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("sk")
    );
    assert_eq!(
        parsed.values.get("ANTHROPIC_API_KEY").map(String::as_str),
        Some("a#b")
    );
    assert_eq!(
        parsed.values.get("KIMI_API_KEY").map(String::as_str),
        Some("sk-abc")
    );
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn handles_multiple_and_bare_exports() {
    let parsed = parse_zshrc_source(
        "export OPENAI_API_KEY=a ANTHROPIC_API_KEY=b\nexport FOO\ntypeset -x KIMI_API_KEY=c\n",
    );
    assert_eq!(parsed.values.len(), 3);
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn dynamic_irrelevant_variables_are_ignored() {
    let parsed = parse_zshrc_source("PATH=$(brew --prefix)/bin\nFOO=$HOME/x\n");
    assert!(parsed.values.is_empty());
    assert!(parsed.unresolved.is_empty());
}

#[test]
fn later_assignment_wins() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=first\nOPENAI_API_KEY=second\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("second")
    );
}

#[test]
fn append_and_array_assignments_are_unresolved() {
    let parsed = parse_zshrc_source("ANTHROPIC_API_KEY+=extra\nOPENAI_API_KEY=(a b)\n");
    assert!(parsed.values.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
    assert!(
        parsed
            .unresolved
            .iter()
            .all(|e| e.kind == UnresolvedKind::UnresolvableExpansion)
    );
}

#[test]
fn unescapes_backslashes() {
    let parsed = parse_zshrc_source("OPENAI_API_KEY=a\\ b\n");
    assert_eq!(
        parsed.values.get("OPENAI_API_KEY").map(String::as_str),
        Some("a b")
    );
}

#[test]
fn relevance_matrix() {
    for name in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "KIMI_API_KEY",
        "ANTHROPIC_BASE_URL",
        "OPENAI_BASE_URL",
        "KIMI_BASE_URL",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "AWS_PROFILE",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert!(is_account_relevant(name), "{name} should be relevant");
    }
    for name in ["PATH", "EDITOR", "FOO", "MY_API_KEYS", "BASE_URLS"] {
        assert!(!is_account_relevant(name), "{name} should be ignored");
    }
}

#[test]
fn reports_line_numbers() {
    let parsed = parse_zshrc_source("FOO=1\nOPENAI_API_KEY=$X\n");
    assert_eq!(parsed.unresolved.len(), 1);
    assert_eq!(parsed.unresolved[0].line, 2);
    assert_eq!(parsed.unresolved[0].name, "OPENAI_API_KEY");
}

#[test]
fn plan_extracts_custom_config_dirs_and_skips_relative() {
    let parsed =
        parse_zshrc_source("CLAUDE_CONFIG_DIR=/srv/work/claude\nCODEX_HOME=/srv/work/codex\n");
    let plan = import_plan(&parsed);
    assert_eq!(
        plan.directories,
        vec![
            DirectoryCandidate {
                agent: Agent::Claude,
                directory: PathBuf::from("/srv/work/claude"),
                source_var: "CLAUDE_CONFIG_DIR".into(),
            },
            DirectoryCandidate {
                agent: Agent::Codex,
                directory: PathBuf::from("/srv/work/codex"),
                source_var: "CODEX_HOME".into(),
            },
        ]
    );
    let parsed = parse_zshrc_source("CLAUDE_CONFIG_DIR=rel/path\n");
    assert!(import_plan(&parsed).directories.is_empty());
}

#[test]
fn plan_collects_complete_xdg_triple_only() {
    let parsed = parse_zshrc_source(
        "XDG_DATA_HOME=/srv/amp/data\nXDG_CONFIG_HOME=/srv/amp/config\nXDG_CACHE_HOME=/srv/amp/cache\n",
    );
    let plan = import_plan(&parsed);
    assert_eq!(
        plan.xdg_roots,
        Some(XdgRoots {
            data: PathBuf::from("/srv/amp/data"),
            config: PathBuf::from("/srv/amp/config"),
            cache: PathBuf::from("/srv/amp/cache"),
        })
    );
    let parsed =
        parse_zshrc_source("XDG_DATA_HOME=/srv/amp/data\nXDG_CONFIG_HOME=/srv/amp/config\n");
    assert!(import_plan(&parsed).xdg_roots.is_none());
}

#[test]
fn plan_parses_op_read_references() {
    let parsed = parse_zshrc_source(
        "KIMI_API_KEY=$(op read op://work/kimi/password)\nZHIPU_API_KEY=$(op read --account work \"op://work/zai/dev/field\")\n",
    );
    let plan = import_plan(&parsed);
    assert_eq!(plan.op_refs.len(), 2);
    assert_eq!(plan.op_refs[0].var, "KIMI_API_KEY");
    assert_eq!(plan.op_refs[0].line, 1);
    assert_eq!(plan.op_refs[0].reference.op, "op://work/kimi/password");
    assert_eq!(plan.op_refs[0].reference.path, "work/kimi/password");
    assert_eq!(plan.op_refs[0].reference.account, None);
    assert!(!plan.op_refs[0].reference.on_demand);
    assert_eq!(plan.op_refs[1].var, "ZHIPU_API_KEY");
    assert_eq!(plan.op_refs[1].reference.op, "op://work/zai/dev/field");
    assert_eq!(plan.op_refs[1].reference.path, "work/zai/dev/field");
    assert_eq!(plan.op_refs[1].reference.account.as_deref(), Some("work"));
    // Non-read op invocations and over-long (truncated) snippets stay unresolved-only.
    let long_arg = "x".repeat(80);
    let parsed = parse_zshrc_source(&format!(
        "A_API_KEY=$(op item get x)\nB_API_KEY=$(op read op://{long_arg}/i/f)\n"
    ));
    assert!(import_plan(&parsed).op_refs.is_empty());
    assert_eq!(parsed.unresolved.len(), 2);
}

#[test]
fn plan_parses_backquote_op_read_with_equals_account_flag() {
    let parsed =
        parse_zshrc_source("MINIMAX_API_KEY=`op read --account=ops op://work/minimax/password`\n");
    let plan = import_plan(&parsed);
    assert_eq!(plan.op_refs.len(), 1);
    assert_eq!(plan.op_refs[0].reference.op, "op://work/minimax/password");
    assert_eq!(plan.op_refs[0].reference.account.as_deref(), Some("ops"));
}

#[test]
fn plan_parses_wrapper_call_sites() {
    let parsed = parse_zshrc_source(
        "claude_key() { op read \"$1\"; }\nANTHROPIC_API_KEY=$(claude_key op://work/claude)\nXAI_API_KEY=`claude_key --profile \"work x\"`\n",
    );
    let plan = import_plan(&parsed);
    assert_eq!(plan.wrappers.len(), 2);
    assert_eq!(plan.wrappers[0].var, "ANTHROPIC_API_KEY");
    assert_eq!(plan.wrappers[0].line, 2);
    assert_eq!(
        plan.wrappers[0].spec,
        WrapperSpec {
            identity: "claude_key".into(),
            args: vec!["op://work/claude".into()],
        }
    );
    assert_eq!(
        plan.wrappers[1].spec,
        WrapperSpec {
            identity: "claude_key".into(),
            args: vec!["--profile".into(), "work x".into()],
        }
    );
}

#[test]
fn plan_groups_model_profiles_by_canonical_provider_stem() {
    let parsed = parse_zshrc_source(
        "MOONSHOT_MODEL=kimi-k2\nMOONSHOT_BASE_URL=https://api.kimi.com/coding/v1\nMOONSHOT_PROFILE=dev\nGOOGLE_MODEL=gemini-2.5-pro\nGOOGLE_BASE_URL=https://generativelanguage.example/v1\nZAI_MODEL=glm-4.6\nMINIMAX_MODEL=MiniMax-M2\nMINIMAX_BASE_URL=https://api.minimax.io/v1\nANTHROPIC_DEFAULT_OPUS_MODEL=opus-x\nANTHROPIC_MODEL=sonnet-y\nAWS_PROFILE=dev-only\n",
    );
    let plan = import_plan(&parsed);
    assert_eq!(
        plan.models,
        vec![
            ModelProfile {
                name: "anthropic".into(),
                model: Some("sonnet-y".into()),
                base_url: None,
            },
            ModelProfile {
                name: "google".into(),
                model: Some("gemini-2.5-pro".into()),
                base_url: Some("https://generativelanguage.example/v1".into()),
            },
            ModelProfile {
                name: "minimax".into(),
                model: Some("MiniMax-M2".into()),
                base_url: Some("https://api.minimax.io/v1".into()),
            },
            ModelProfile {
                name: "moonshot".into(),
                model: Some("kimi-k2".into()),
                base_url: Some("https://api.kimi.com/coding/v1".into()),
            },
            ModelProfile {
                name: "zai".into(),
                model: Some("glm-4.6".into()),
                base_url: None,
            },
        ]
    );
}

#[test]
fn plan_carries_no_secret_values() {
    let parsed = parse_zshrc_source(
        "CLAUDE_CONFIG_DIR=/srv/claude\nANTHROPIC_API_KEY=sk-ant-secret\nMOONSHOT_API_KEY=$(op read op://work/moonshot/password)\nMOONSHOT_MODEL=kimi-k2\n",
    );
    let plan = import_plan(&parsed);
    let rendered = format!("{plan:?}");
    assert!(!rendered.contains("sk-ant-secret"), "{rendered}");
    assert!(
        rendered.contains("op://work/moonshot/password"),
        "{rendered}"
    );
    assert!(rendered.contains("/srv/claude"), "{rendered}");
}
