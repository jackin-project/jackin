use serde_json::Value;
use serde_saphyr::{Budget, Options};

const MAX_WORKFLOW_BYTES: usize = 1024 * 1024;

pub fn parse(bytes: &[u8]) -> anyhow::Result<Value> {
    anyhow::ensure!(
        bytes.len() <= MAX_WORKFLOW_BYTES,
        "workflow YAML exceeds the 1 MiB parse limit"
    );
    Ok(serde_saphyr::from_slice_with_options(bytes, options())?)
}

fn options() -> Options {
    let mut budget = Budget::default();
    budget.enforce_alias_anchor_ratio = false;
    budget.max_documents = 1;
    let mut options = Options::default();
    options.budget = Some(budget);
    options
}
