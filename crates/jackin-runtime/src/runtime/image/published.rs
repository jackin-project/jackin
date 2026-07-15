//! Published role image freshness checks.

use jackin_docker::docker_client::DockerApi;

use super::{LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, emit_compact_image_warning};

/// Published-image freshness relative to the current role repo state.
pub(super) enum PublishedImageFreshness {
    Fresh,
    Stale,
    NeedsRoleSha(String),
}

/// Returns freshness for a published image used as the role base.
///
/// Checks in order:
/// 1. `jackin.role.git.sha` label: if present and matches `head_sha`, the image
///    was built from the exact same commit. If present and different, stale.
/// 2. Fallback only when the current role SHA is not yet known:
///    `jackin.construct.version` label must match `dockerfile_version`.
///
/// If `docker pull` fails, treat the image as stale so launch falls back to the
/// workspace Dockerfile and reports a clearer error if the construct base is
/// also unreachable.
pub(super) async fn published_image_freshness(
    published: &str,
    dockerfile_version: &str,
    head_sha: Option<&str>,
    docker: &impl DockerApi,
) -> PublishedImageFreshness {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "published_image_pull",
        Some(published),
    );
    let pull_result = docker.pull_image(published).await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "published_image_pull",
        if pull_result.is_ok() {
            Some(published)
        } else {
            Some("error")
        },
    );
    if let Err(e) = pull_result {
        emit_compact_image_warning(&format!(
            "docker pull {published} failed ({e}); treating published image as stale and rebuilding from workspace Dockerfile"
        ));
        return PublishedImageFreshness::Stale;
    }

    let labels = match docker.inspect_image_labels(published).await {
        Err(e) => {
            emit_compact_image_warning(&format!(
                "could not read labels from {published} ({e}); treating published image as stale"
            ));
            return PublishedImageFreshness::Stale;
        }
        Ok(map) => map,
    };

    match (head_sha, labels.get(LABEL_IMAGE_ROLE_GIT_SHA)) {
        (Some(sha), Some(label_sha)) if label_sha == sha => return PublishedImageFreshness::Fresh,
        (Some(_), Some(_)) => return PublishedImageFreshness::Stale,
        (None, Some(label_sha)) => {
            return PublishedImageFreshness::NeedsRoleSha(label_sha.clone());
        }
        (Some(_), None) => return PublishedImageFreshness::Stale,
        _ => {}
    }

    if labels
        .get(LABEL_IMAGE_CONSTRUCT_VERSION)
        .is_some_and(|stored| stored != dockerfile_version)
    {
        PublishedImageFreshness::Stale
    } else {
        PublishedImageFreshness::Fresh
    }
}

pub(super) async fn published_image_is_stale(
    published: &str,
    dockerfile_version: &str,
    head_sha: Option<&str>,
    docker: &impl DockerApi,
) -> bool {
    !matches!(
        published_image_freshness(published, dockerfile_version, head_sha, docker).await,
        PublishedImageFreshness::Fresh
    )
}
