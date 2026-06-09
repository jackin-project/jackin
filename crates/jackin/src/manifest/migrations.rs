//! Role manifest migration re-exports — behavior now in `jackin-manifest`.

pub use jackin_manifest::migrations::{
    CURRENT_MANIFEST_VERSION, current_manifest_version, migrate_manifest_file,
    validate_manifest_version,
};
