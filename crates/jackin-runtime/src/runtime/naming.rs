//! Naming conventions, Docker label/filter constants, and lightweight identifier helpers.

use crate::instance::runtime_slug;
use jackin_core::selector::RoleSelector;

/// Prefix for jackin-managed Docker image names.
///
/// Uses `_` as the separator so all structural boundaries in an image name
/// are `_`, visually distinct from container names which use `jk-{id}-…`.
pub(super) const IMAGE_PREFIX: &str = "jk_";

// ── Docker label keys ─────────────────────────────────────────────────────
//
// Used to tag and filter role containers and networks managed by jackin.

/// Applied to role containers, `DinD` sidecars, and networks.
pub const LABEL_MANAGED: &str = "jackin.managed=true";
/// Role containers only — distinguishes them from `DinD` sidecars.
pub(super) const LABEL_KIND_ROLE: &str = "jackin.kind=role";
/// `DinD` sidecars only — distinguishes them from role containers.
pub(super) const LABEL_KIND_DIND: &str = "jackin.kind=dind";
/// Explicit prewarmed `DinD` sidecars. These are not attached to a role
/// container yet, so orphan GC must not treat them as role-owned sidecars.
pub(super) const LABEL_KIND_PREWARM_DIND: &str = "jackin.kind=prewarm-dind";
/// Diagnostic label for reusable prewarm resources owned by jackin.
pub(super) const LABEL_PREWARM: &str = "jackin.prewarm=true";
/// Applied to role containers whose workspace opted into the
/// keep-awake reconciler. Read by `runtime::caffeinate::reconcile`
/// to decide whether to keep `caffeinate` running.
pub(super) const LABEL_KEEP_AWAKE: &str = "jackin.keep.awake=true";

/// Image label key recording which construct image was used to build a
/// derived image. Checked at load time: a mismatch between this label and
/// `JACKIN_CONSTRUCT_IMAGE` (or the canonical default) forces a rebuild so a
/// derived image built against a local construct is never reused by a standard
/// jackin invocation (and vice versa).
pub(super) const LABEL_IMAGE_CONSTRUCT: &str = "jackin.construct.image";

/// Image label key recording the construct version tag baked into a published
/// role image. Role CI gets this key from `jackin-role publish-labels` instead
/// of hardcoding it in workflow YAML. Checked at launch time: a mismatch
/// against the Dockerfile's pinned version means the published image pre-dates
/// a Renovate bump; jackin falls back to workspace mode so the role's workspace
/// Dockerfile — carrying the new pin — is used.
pub(super) const LABEL_IMAGE_CONSTRUCT_VERSION: &str =
    jackin_manifest::LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION;

/// Container label key storing the role container name. Applied to `DinD`
/// sidecars and managed networks so GC can map them back to their role.
pub(super) const LABEL_ROLE_KEY: &str = "jackin.role";

/// Container / image label key storing the derived image name. Applied to
/// role containers so image GC can skip images currently in use.
pub(super) const LABEL_IMAGE_KEY: &str = "jackin.image";

/// Image label key recording the git commit SHA of the role repo from which a
/// published image was built. Role CI gets this key from `jackin-role
/// publish-labels` instead of hardcoding it in workflow YAML. Checked at launch
/// time before the construct-version check: if the label matches the HEAD of the
/// cached role repo the image is current and the workspace rebuild is skipped.
/// Falls through to the construct-version check when this label is absent
/// (images predating this feature).
pub(super) const LABEL_IMAGE_ROLE_GIT_SHA: &str =
    jackin_manifest::LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA;

/// Image label key recording the complete launch-time recipe hash for the
/// derived image. This is the fast-path authority: when the local image's hash
/// matches the current recipe, launch can reuse it without invoking
/// `docker build`.
pub(super) const LABEL_IMAGE_RECIPE_HASH: &str = "jackin.image.recipe.hash";

/// Human-readable image label recording why the image recipe has the current
/// shape. The hash is authoritative; this version lets future recipe schema
/// changes invalidate old labels with a clear reason.
pub(super) const LABEL_IMAGE_RECIPE_VERSION: &str = "jackin.image.recipe.version";

/// Prefix for per-agent baked-binary version labels.
/// Full key: `jackin.agent.<slug>.version`.
/// Stamps which version of each agent binary was baked into the image (D3/D20).
/// Diagnostic — not part of the recipe hash.
pub(super) const LABEL_IMAGE_AGENT_VERSION_PREFIX: &str = "jackin.agent";

/// Format a human-friendly role name from a container name and its display label.
///
/// Examples:
///   - `("jk-k7p9m2xq-thearchitect", "The Architect")` → `"The Architect (k7p9m2xq)"`
///   - `("jk-k7p9m2xq-thearchitect", "")` → `"jk-k7p9m2xq-thearchitect"`
///
/// The instance-ID suffix is appended so two concurrent sessions of the
/// same role render as distinct rows in operator output.
pub(super) fn format_role_display(container_name: &str, display_name: &str) -> String {
    if display_name.is_empty() {
        return container_name.to_owned();
    }
    crate::instance::naming::instance_id_from_container_base(container_name).map_or_else(
        || display_name.to_owned(),
        |instance_id| format!("{display_name} ({instance_id})"),
    )
}

pub fn matching_family(selector: &RoleSelector, names: &[String]) -> Vec<String> {
    let role_slug = crate::instance::naming::compact_component(&selector.name, "role");
    names
        .iter()
        .filter(|name| crate::instance::naming::class_family_matches_with_slug(&role_slug, name))
        .cloned()
        .collect()
}

/// Derived image tag for a role.
///
/// The tag is agent-independent: the derived image installs every supported
/// agent (the container runs a multiplexer where any tab can launch any agent),
/// so its content does not depend on which agent was selected at launch. One
/// image per role (per branch) is reused across all agents — selecting a
/// different initial agent no longer forks a redundant, byte-identical image or
/// forces a rebuild. The selected agent survives only as non-identity runtime
/// metadata (the version probe), never in the tag or recipe.
pub(super) fn image_name(selector: &RoleSelector, role_git_sha: Option<&str>) -> String {
    tag_with_sha(
        format!("{IMAGE_PREFIX}{}", runtime_slug(selector)),
        role_git_sha,
    )
}

/// Number of leading hex chars of the role-repo commit SHA used in the image
/// tag. Matches the short SHA GitHub renders (e.g. `4f38b4f`).
const SHORT_GIT_SHA_LEN: usize = 7;

/// Shorten a role-repo commit SHA to its display form for an image tag.
pub(super) fn short_git_sha(sha: &str) -> &str {
    &sha[..sha.len().min(SHORT_GIT_SHA_LEN)]
}

/// Append `:<short-sha>` to a derived-image repository name so each role-repo
/// commit gets its own immutable tag (`jk_the-architect:4f38b4f`) instead of a
/// mutable `:latest` that silently overwrites prior builds. The recipe-hash /
/// construct labels still decide reuse-vs-rebuild *within* a tag; the SHA only
/// changes the name. When the SHA is unavailable (a role checkout with no
/// commits) the bare name is returned and Docker defaults it to `:latest`.
///
/// Only a real git commit SHA (all hex) is used as the tag — a non-hex value
/// could not be produced by `git rev-parse` and would yield an invalid Docker
/// reference, so it falls back to the bare name. This also keeps the name a
/// valid `FROM` target when it is used as the role base image.
fn tag_with_sha(name: String, role_git_sha: Option<&str>) -> String {
    match role_git_sha.filter(|sha| !sha.is_empty() && sha.bytes().all(|b| b.is_ascii_hexdigit())) {
        Some(sha) => format!("{name}:{}", short_git_sha(sha)),
        None => name,
    }
}

/// Image tag for a branch-specific local build. Branch slashes become dashes
/// so the tag is a valid Docker name and does not overwrite the stable image
/// (e.g. `jk_the-architect_feat-my-pr`). All structural separators in image
/// names are `_`. Role names and branch slugs contain only `[a-z0-9-]`, so
/// `_` marks every boundary. Agent-independent for the same reason as
/// [`image_name`].
pub(super) fn image_name_for_branch(
    selector: &RoleSelector,
    branch: &str,
    role_git_sha: Option<&str>,
) -> String {
    let slug = branch.replace('/', "-").to_ascii_lowercase();
    tag_with_sha(
        format!("{IMAGE_PREFIX}{}_{slug}", runtime_slug(selector)),
        role_git_sha,
    )
}

/// Local **base** image name for a role: the role content (architect layers)
/// either pulled from the published image and restamped, or rebuilt locally,
/// before the jackin overlay is derived on top. `__base` marks the boundary so
/// it never collides with the derived image name (which uses single `_`). The
/// derived image is `FROM` this base.
///
/// Examples: `jk_the-architect__base:4f38b4f`, `jk_chainargos_agent-brown__base:4f38b4f`,
/// branch: `jk_the-architect_feat-x__base:4f38b4f`.
pub(super) fn role_base_image_name(
    selector: &RoleSelector,
    branch: Option<&str>,
    role_git_sha: Option<&str>,
) -> String {
    let repo = match branch {
        Some(b) => {
            let slug = b.replace('/', "-").to_ascii_lowercase();
            format!("{IMAGE_PREFIX}{}_{slug}__base", runtime_slug(selector))
        }
        None => format!("{IMAGE_PREFIX}{}__base", runtime_slug(selector)),
    };
    tag_with_sha(repo, role_git_sha)
}

/// Docker volume name for the TLS client certificates shared between the
/// `DinD` sidecar (writer) and the role container (reader).
pub(crate) fn dind_certs_volume(container_name: &str) -> String {
    format!("{container_name}-dind-certs")
}

pub(crate) fn dind_container_name(container_name: &str) -> String {
    format!("{container_name}-dind")
}

pub(crate) fn role_network_name(container_name: &str) -> String {
    format!("{container_name}-net")
}

#[cfg(test)]
mod tests;
