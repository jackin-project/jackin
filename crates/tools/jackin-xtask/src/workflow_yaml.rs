use serde_json::Value;
use serde_saphyr::{Budget, Options};

const MAX_WORKFLOW_BYTES: usize = 1024 * 1024;

pub(crate) fn parse(bytes: &[u8]) -> anyhow::Result<Value> {
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

#[cfg(test)]
mod tests {
    use super::{MAX_WORKFLOW_BYTES, parse};

    #[test]
    fn accepts_current_ci_workflow_alias_reuse() {
        let parsed = parse(include_bytes!("../../../../.github/workflows/ci.yml"))
            .expect("real generated CI workflow fits bounded parser policy");
        assert!(parsed["jobs"].as_object().is_some());
    }

    #[test]
    fn rejects_expansion_bomb_under_replay_budget() {
        let mut yaml = String::from("a: &a [value]\nb: &b [");
        yaml.push_str(&["*a"; 100].join(", "));
        yaml.push_str("]\nc: &c [");
        yaml.push_str(&["*b"; 100].join(", "));
        yaml.push_str("]\nd: [");
        yaml.push_str(&["*c"; 100].join(", "));
        yaml.push_str("]\n");

        let error = parse(yaml.as_bytes()).expect_err("alias replay budget rejects expansion");
        assert!(error.to_string().contains("budget breached"), "{error}");
    }

    #[test]
    fn rejects_overlarge_workflow_before_parsing() {
        let error = parse(&vec![b' '; MAX_WORKFLOW_BYTES + 1])
            .expect_err("workflow input limit is checked before parser allocation");
        assert!(error.to_string().contains("1 MiB parse limit"), "{error}");
    }
}
