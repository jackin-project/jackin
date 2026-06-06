//! Re-exports from `jackin-docker` for backward compatibility within the
//! root binary crate. New code should import directly from `jackin_docker`.

pub use jackin_docker::net::{USER_AGENT, download_parallel, fetch_text, get_text, http_client};
