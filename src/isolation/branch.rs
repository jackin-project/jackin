/// Build the scratch branch name for an isolated mount.
///
/// Selector namespace `/` is preserved; an optional `suffix` is
/// appended to the *final* selector segment with a leading `-`. The
/// suffix is reserved for clone-instance disambiguation when clone
/// mode ships (V1.1) — V1 worktree mode always passes `None` because
/// `validate_isolation_layout` rejects multi-isolated-mounts on the
/// same host repo, so each container has at most one scratch branch
/// per host repo and the selector alone is unique.
///
/// Examples:
/// - `branch_name("the-architect", None)` → `jackin/scratch/the-architect`
/// - `branch_name("the-architect", Some("clone-1"))` → `jackin/scratch/the-architect-clone-1`
/// - `branch_name("chainargos/the-architect", None)`
///   → `jackin/scratch/chainargos/the-architect`
pub fn branch_name(selector: &str, suffix: Option<&str>) -> String {
    suffix.map_or_else(
        || format!("jackin/scratch/{selector}"),
        |s| match selector.rsplit_once('/') {
            Some((ns, last)) => format!("jackin/scratch/{ns}/{last}-{s}"),
            None => format!("jackin/scratch/{selector}-{s}"),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_suffix_no_namespace() {
        assert_eq!(
            branch_name("the-architect", None),
            "jackin/scratch/the-architect"
        );
    }

    #[test]
    fn no_suffix_with_namespace() {
        assert_eq!(
            branch_name("chainargos/the-architect", None),
            "jackin/scratch/chainargos/the-architect"
        );
    }

    #[test]
    fn clone_suffix_appends_to_final_segment_with_namespace() {
        assert_eq!(
            branch_name("chainargos/the-architect", Some("clone-1")),
            "jackin/scratch/chainargos/the-architect-clone-1"
        );
    }

    #[test]
    fn clone_suffix_appends_without_namespace() {
        assert_eq!(
            branch_name("the-architect", Some("clone-2")),
            "jackin/scratch/the-architect-clone-2"
        );
    }
}
