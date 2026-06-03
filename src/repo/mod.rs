//! Role repo validation re-exports — behavior now in `jackin-manifest`.

pub use jackin_manifest::repo::{
    CachedRepo, RoleRepoValidationError, ValidatedRoleRepo, validate_role_repo,
};

#[cfg(test)]
mod tests;
