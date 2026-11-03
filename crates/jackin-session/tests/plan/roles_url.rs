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
        let a_head = a.advance("model-FROM-A").await?;
        let b_head = b.advance("model-FROM-B").await?;
        let (cache, _keep) = cache()?;

        let loaded = a.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (loaded.0, loaded.1.as_deref()),
            (a_head, Some("model-FROM-A"))
        );

        let loaded = b.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (loaded.0, loaded.1.as_deref()),
            (b_head.clone(), Some("model-FROM-B"))
        );

        a.advance("model-EVIL").await?;
        let loaded = b.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (loaded.0, loaded.1.as_deref()),
            (b_head, Some("model-FROM-B"))
        );

        let loaded = a.load(&cache, None).await?.map_err(|e| format!("{e:?}"))?;
        assert_eq!(loaded.1.as_deref(), Some("model-EVIL"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_changed_role_url_drops_the_tags_and_branches_of_the_old_origin() -> TestResult {
    guard(async {
        let (a, b) = two_origins().await?;
        a.advance("model-FROM-A").await?;
        super::roles_origin::git(a.role.root().as_path(), &["tag", "only-in-a"]).await?;
        let (cache, _keep) = cache()?;
        let loaded = a
            .load(&cache, Some("only-in-a"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(loaded.1.as_deref(), Some("model-FROM-A"));
        let missing = b.load(&cache, Some("only-in-a")).await?;
        assert!(missing.is_err(), "{missing:?}");
        Ok(())
    })
    .await
}
