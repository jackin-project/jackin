// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Thin `jackin workflow` handlers: validate a definition, or compile
//! it into an immutable plan. Live agent selection is not wired in
//! this step, so `plan` reports that gap as a typed `missing_input`
//! diagnostic after the definition itself validates.

use anyhow::{Context as _, Result, bail};

use crate::cli::workflow::{WorkflowCommand, WorkflowFileArgs, WorkflowPlanArgs};

pub(super) fn handle(command: WorkflowCommand) -> Result<()> {
    match command {
        WorkflowCommand::Validate(args) => validate(&args),
        WorkflowCommand::Plan(args) => plan(&args),
    }
}

fn load_definition(
    file: &std::path::Path,
) -> Result<(jackin_workflow_spec::rules::CheckedDefinition, String)> {
    use jackin_workflow_spec::eval::{EvalLimits, FrozenBundle, evaluate_bounded};
    use jackin_workflow_spec::rules::validate_definition;

    let file_label = file.display().to_string();
    let dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let schema_path = dir.join("Workflow.pkl");
    let schema = std::fs::read_to_string(&schema_path)
        .with_context(|| format!("reading {}", schema_path.display()))?;
    let workflow =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let bundle = FrozenBundle::from_texts(
        "/bundle/repair.pkl",
        [
            ("/bundle/Workflow.pkl", schema),
            ("/bundle/repair.pkl", workflow),
        ],
    )
    .map_err(render)?;
    let Some(helper) = jackin_workflow_spec::eval::resolve_helper_binary() else {
        bail!("evaluation helper not installed next to this jackin binary");
    };
    let evaluated =
        evaluate_bounded(&bundle, &file_label, &helper, &EvalLimits::defaults()).map_err(render)?;
    validate_definition(&evaluated.json, &file_label)
        .map(|checked| (checked, file_label))
        .map_err(render)
}

fn render(error: jackin_workflow_spec::error::WorkflowError) -> anyhow::Error {
    let lines: Vec<String> = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{} [{}] {} (action: {})",
                diagnostic.file,
                diagnostic.field,
                diagnostic.code,
                diagnostic.message,
                diagnostic.action
            )
        })
        .collect();
    anyhow::anyhow!("{}", lines.join("\n"))
}

fn validate(args: &WorkflowFileArgs) -> Result<()> {
    match load_definition(&args.file) {
        Ok(_) => {
            println!("valid: {}", args.file.display());
            Ok(())
        }
        Err(error) => {
            println!("invalid: {}", args.file.display());
            Err(error)
        }
    }
}

fn plan(args: &WorkflowPlanArgs) -> Result<()> {
    let (_checked, file_label) = load_definition(&args.file)?;
    match jackin_workflow_spec::resolve::resolve_live_selections(&file_label) {
        Ok(_) => Ok(()),
        Err(error) => {
            if args.json {
                print_diagnostics_json(&error);
            }
            Err(render(error))
        }
    }
}

fn print_diagnostics_json(error: &jackin_workflow_spec::error::WorkflowError) {
    let diagnostics: Vec<serde_json::Value> = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "file": diagnostic.file,
                "field": diagnostic.field,
                "code": diagnostic.code.as_str(),
                "message": diagnostic.message,
                "action": diagnostic.action,
            })
        })
        .collect();
    println!("{}", serde_json::json!({ "diagnostics": diagnostics }));
}
