// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::roles_origin::{Origin, cache};
use super::support::{TestResult, guard};

async fn two_origins() -> Result<(Origin, Origin), Box<dyn std::error::Error>> {
    let a = Origin::new().await?;
    let b = Origin::at("https://example.invalid/other.git").await?;
    let config = |origin: &Origin| origin.home.as_path().join(".gitconfig");
    let both = format!(
        "{}{}",
        std::fs::read_to_string(config(&a))?,
        std::fs::read_to_string(config(&b))?
    );
    std::fs::write(config(&a), &both)?;
    std::fs::write(config(&b), &both)?;
    Ok((a, b))
}

#[tokio::test]
async fn a_changed_role_url_never_reuses_the_old_origins_checkout() -> TestResult {
    guard(async {
        let (a, b) = two_origins().await?;
        let a_head = a.advance("glm-5.3").await?;
        let b_head = b.advance("glm-5.3").await?;
        let (cache, _keep) = cache()?;

        let loaded = a.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!((loaded.0, loaded.1.as_deref()), (a_head, Some("glm-5.3")));

        let loaded = b.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (loaded.0, loaded.1.as_deref()),
            (b_head.clone(), Some("glm-5.3"))
        );

        a.advance("glm-5.3").await?;
        let loaded = b.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!((loaded.0, loaded.1.as_deref()), (b_head, Some("glm-5.3")));

        let loaded = a.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(loaded.1.as_deref(), Some("glm-5.3"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_changed_role_url_drops_the_tags_and_branches_of_the_old_origin() -> TestResult {
    guard(async {
        let (a, b) = two_origins().await?;
        a.advance("glm-5.3").await?;
        super::roles_origin::git(a.role.root().as_path(), &["tag", "only-in-a"]).await?;
        let (cache, _keep) = cache()?;
        let loaded = a
            .load(&cache, Some("only-in-a"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(loaded.1.as_deref(), Some("glm-5.3"));
        let missing = b.load(&cache, Some("only-in-a")).await?;
        assert!(missing.is_err(), "{missing:?}");
        Ok(())
    })
    .await
}
