//! OSC policy + osc8 + parse_osc7 extracted from session.
use crate as _;

/// reference operator-side files on click.
pub fn osc8_uri_is_safe(uri: &str) -> bool {
    if uri.is_empty() {
        return true;
    }
    // Byte-level case-insensitive prefix match so this stays allocation-free:
    // the frame hyperlink-region builder calls it per linked cell every frame,
    // and a `to_ascii_lowercase()` here would heap-allocate each call.
    let bytes = uri.trim().as_bytes();
    [b"http://".as_slice(), b"https://", b"mailto:"]
        .iter()
        .any(|scheme| {
            bytes
                .get(..scheme.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
        })
}

/// Parse an `OSC 7` payload into a local-filesystem path. `OSC 7`
/// canonically arrives as `file://<host>/<percent-encoded-path>`;
/// `url::Url` does the percent-decoding and host-stripping in one
/// pass. Returns `None` for any payload that does not parse as a
/// `file://` URL — silently trusting arbitrary text would let an
/// agent overwrite the pane title with whatever it pleased.
pub fn parse_osc7(payload: &str) -> Option<String> {
    let url = url::Url::parse(payload).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Per-OSC operator opt-out switches. All default to `allow`; the
/// values `deny`, `off`, `no` (case-sensitive) turn the matching
/// passthrough off when the operator runs an untrusted role. tmux
/// exposes the same family as `set-clipboard on|off` plus
/// `allow-passthrough` for OSC; jackin keeps the surface per-OSC so
/// the operator can leave the agent's terminal title alone but block
/// notification spam, or vice versa.
const ENV_OSC52: &str = "JACKIN_OSC52";
const ENV_OSC_TITLE: &str = "JACKIN_OSC_TITLE";
const ENV_OSC_NOTIFY: &str = "JACKIN_OSC_NOTIFY";
const ENV_OSC_HYPERLINK: &str = "JACKIN_OSC_HYPERLINK";

#[derive(Debug, Clone, Copy)]
pub struct OscPolicy {
    flags: u8,
}

const ALLOW_TITLE: u8 = 1 << 0;
const ALLOW_OSC52: u8 = 1 << 1;
const ALLOW_NOTIFY: u8 = 1 << 2;
const ALLOW_HYPERLINK: u8 = 1 << 3;

impl Default for OscPolicy {
    fn default() -> Self {
        Self {
            flags: ALLOW_TITLE | ALLOW_OSC52 | ALLOW_NOTIFY | ALLOW_HYPERLINK,
        }
    }
}

impl OscPolicy {
    /// Read policy from environment. Cached at `Session::spawn` time so a
    /// background pane cannot toggle the gate at runtime by `export`ing
    /// into a focused shell.
    pub fn from_env() -> Self {
        Self {
            flags: (if is_env_deny(ENV_OSC_TITLE) {
                0
            } else {
                ALLOW_TITLE
            }) | (if is_env_deny(ENV_OSC52) {
                0
            } else {
                ALLOW_OSC52
            }) | (if is_env_deny(ENV_OSC_NOTIFY) {
                0
            } else {
                ALLOW_NOTIFY
            }) | (if is_env_deny(ENV_OSC_HYPERLINK) {
                0
            } else {
                ALLOW_HYPERLINK
            }),
        }
    }

    pub fn allow_title(self) -> bool {
        self.flags & ALLOW_TITLE != 0
    }
    pub fn allow_osc52(self) -> bool {
        self.flags & ALLOW_OSC52 != 0
    }
    pub fn allow_notify(self) -> bool {
        self.flags & ALLOW_NOTIFY != 0
    }
    pub fn allow_hyperlink(self) -> bool {
        self.flags & ALLOW_HYPERLINK != 0
    }

    /// Test-only constructor with every passthrough gate closed.
    /// Production code must call `from_env()`; the `#[doc(hidden)]`
    /// attribute hides this from rustdoc and the `for_test_` prefix
    /// flags intent to readers. Cargo cannot list a crate in its own
    /// `[dev-dependencies]` with a feature flag, so a `#[cfg(feature
    /// = "test-helpers")]` gate would break the default `cargo test`
    /// invocation that integration tests rely on.
    #[doc(hidden)]
    pub fn for_test_deny_all() -> Self {
        Self { flags: 0 }
    }
}

fn is_env_deny(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("deny" | "off" | "no"))
}

