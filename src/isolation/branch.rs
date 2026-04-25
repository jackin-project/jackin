/// Build the scratch branch name for an isolated mount.
///
/// Selector namespace `/` is preserved; the optional `suffix` (used either
/// for clone-instance numbering or for multi-isolated-mount disambiguation)
/// is appended to the *final* segment with a leading `-`.
///
/// Examples:
/// - `branch_name("the-architect", None)` → `jackin/scratch/the-architect`
/// - `branch_name("the-architect", Some("clone-1"))` → `jackin/scratch/the-architect-clone-1`
/// - `branch_name("chainargos/the-architect", None)`
///   → `jackin/scratch/chainargos/the-architect`
/// - `branch_name("chainargos/the-architect", Some("workspace-jackin"))`
///   → `jackin/scratch/chainargos/the-architect-workspace-jackin`
pub fn branch_name(selector: &str, suffix: Option<&str>) -> String {
    suffix.map_or_else(
        || format!("jackin/scratch/{selector}"),
        |s| match selector.rsplit_once('/') {
            Some((ns, last)) => format!("jackin/scratch/{ns}/{last}-{s}"),
            None => format!("jackin/scratch/{selector}-{s}"),
        },
    )
}

/// Flatten a mount destination into a branch-suffix-safe string.
/// Strips leading/trailing `/` and replaces internal `/` with `-`.
/// e.g. `/workspace/jackin` → `workspace-jackin`.
pub fn dst_to_branch_suffix(dst: &str) -> String {
    dst.trim_matches('/').replace('/', "-")
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

    #[test]
    fn dst_suffix_appends_to_final_segment() {
        assert_eq!(
            branch_name("the-architect", Some("workspace-jackin")),
            "jackin/scratch/the-architect-workspace-jackin"
        );
    }

    #[test]
    fn dst_to_branch_suffix_strips_slashes_and_dashes() {
        assert_eq!(
            dst_to_branch_suffix("/workspace/jackin"),
            "workspace-jackin"
        );
        assert_eq!(
            dst_to_branch_suffix("/workspace/jackin/"),
            "workspace-jackin"
        );
        assert_eq!(dst_to_branch_suffix("/a/b/c"), "a-b-c");
    }
}
