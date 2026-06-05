//! `firewall-apply` subcommand — installs the iptables OUTPUT egress allowlist
//! for the `allowlist` network tier.
//!
//! Run host-side via `docker exec --user root <container> jackin-capsule
//! firewall-apply` before the agent session starts. Running as root via
//! `docker exec` (daemon-granted, not setuid) is compatible with
//! `--security-opt no-new-privileges`. Replaces the former
//! `docker/runtime/init-firewall.sh`; the allowlist arrives via the
//! `JACKIN_ALLOWED_HOSTS` container env (comma-separated domains/IPs/CIDRs).
//!
//! Requires `CAP_NET_ADMIN` + `CAP_NET_RAW` and the `iptables`/`ipset` binaries
//! in the image. Domain resolution uses libc `getaddrinfo` (no `dig`).
//!
//! Fail-closed: the default-`DROP` policy plus loopback/established/DNS accepts
//! are installed *first*, so any mid-apply error leaves egress denied rather
//! than open. Domains resolve through the DNS accept window before the
//! allowlisted-destination rule lands.

use crate::runtime_setup::run_command;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::net::{IpAddr, ToSocketAddrs};
use std::process::{Command, Stdio};

/// ipset name holding the allowed destination addresses/CIDRs.
const IPSET: &str = "jackin-allowed";

/// One parsed allowlist entry, classified by shape.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    /// A literal IP or CIDR — added to the ipset verbatim (`hash:net` accepts
    /// both `1.2.3.4` and `1.2.3.0/24`, v4 and v6).
    Net(String),
    /// A domain name to resolve to addresses at apply time.
    Domain(String),
}

/// Parse `JACKIN_ALLOWED_HOSTS` into classified entries.
///
/// Pure (no DNS, no syscalls) so the classification is unit-testable. Handles
/// whitespace, `*.apex` wildcards (reduced to the apex), `host:port` suffixes
/// on domains, and bare IPv4/IPv6/CIDR literals. Empty/blank entries drop out.
fn parse_allowed_hosts(raw: &str) -> Vec<Entry> {
    raw.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(classify_entry)
        .collect()
}

/// Classify a single trimmed entry. See [`parse_allowed_hosts`].
fn classify_entry(entry: &str) -> Entry {
    // Wildcard subdomain `*.example.com` → allowlist the apex domain.
    let host = entry.strip_prefix("*.").unwrap_or(entry);

    // Bare IP or CIDR (covers IPv6 literals like `::1`, which the old shell
    // script's first-colon port strip mangled). Checked before port stripping
    // so an IPv6 literal is never split on its own colons.
    if is_ip_or_cidr(host) {
        return Entry::Net(host.to_string());
    }

    // Domain, possibly `domain:port` — keep the host, drop the port.
    let domain = host.split_once(':').map_or(host, |(h, _)| h);
    Entry::Domain(domain.to_string())
}

/// True when `host` is a bare IP address or `addr/prefix` CIDR (v4 or v6).
fn is_ip_or_cidr(host: &str) -> bool {
    let addr = host.split_once('/').map_or(host, |(a, _)| a);
    addr.parse::<IpAddr>().is_ok()
}

/// Entry point for the `firewall-apply` subcommand.
pub fn apply() -> Result<()> {
    let raw = std::env::var("JACKIN_ALLOWED_HOSTS").unwrap_or_default();
    let entries = parse_allowed_hosts(&raw);

    // Fail-closed: deny by default, then permit loopback, established flows,
    // and DNS — before anything else runs, so a mid-apply error cannot leave
    // egress open. The DNS accept also lets the domain resolution below reach
    // the resolver while the policy is already DROP.
    iptables(&["-P", "OUTPUT", "DROP"])?;
    iptables(&["-A", "OUTPUT", "-o", "lo", "-j", "ACCEPT"])?;
    iptables(&[
        "-A",
        "OUTPUT",
        "-m",
        "state",
        "--state",
        "ESTABLISHED,RELATED",
        "-j",
        "ACCEPT",
    ])?;

    if entries.is_empty() {
        // network=allowlist with no hosts is fail-closed (no egress), not open.
        eprintln!("[firewall] JACKIN_ALLOWED_HOSTS is empty; DROP-only policy (no egress)");
        return Ok(());
    }

    iptables(&["-A", "OUTPUT", "-p", "udp", "--dport", "53", "-j", "ACCEPT"])?;
    iptables(&["-A", "OUTPUT", "-p", "tcp", "--dport", "53", "-j", "ACCEPT"])?;

    // Resolve every entry to its destination addresses, deduped so the set and
    // the operator-facing count carry distinct members only.
    let mut members: BTreeSet<String> = BTreeSet::new();
    for entry in entries {
        match entry {
            Entry::Net(net) => {
                members.insert(net);
            }
            Entry::Domain(domain) => {
                members.extend(resolve(&domain).into_iter().map(|ip| ip.to_string()));
            }
        }
    }
    install_ipset(&members)?;

    // Permit the populated allowlist last; the set already has its members.
    iptables(&[
        "-A",
        "OUTPUT",
        "-m",
        "set",
        "--match-set",
        IPSET,
        "dst",
        "-j",
        "ACCEPT",
    ])?;

    eprintln!(
        "[firewall] OUTPUT allowlist active: {} entries",
        members.len()
    );
    Ok(())
}

/// Resolve a domain to its A/AAAA addresses via libc `getaddrinfo`. Best-effort:
/// resolution failure yields no addresses (the host is simply not allowlisted),
/// matching the old script's `dig … 2>/dev/null` behaviour.
fn resolve(domain: &str) -> Vec<IpAddr> {
    // Port 0 is irrelevant — only the addresses are used. `Result::into_iter`
    // drops the error arm, so a resolution failure yields an empty list.
    (domain, 0u16)
        .to_socket_addrs()
        .into_iter()
        .flatten()
        .map(|sa| sa.ip())
        .collect()
}

/// Create the `hash:net` set and load every member in a single `ipset restore`,
/// instead of one `ipset add` process per address. The stream re-creates and
/// flushes the set first so a re-apply starts clean; `-exist` makes the `create`
/// idempotent and tolerates duplicate members (the old per-add `|| true`).
fn install_ipset(members: &BTreeSet<String>) -> Result<()> {
    let mut stream = format!("create {IPSET} hash:net maxelem 65536\nflush {IPSET}\n");
    for member in members {
        stream.push_str(&format!("add {IPSET} {member}\n"));
    }

    let mut child = Command::new("ipset")
        .args(["restore", "-exist"])
        .stdin(Stdio::piped())
        .spawn()
        .context("spawning ipset restore")?;
    child
        .stdin
        .take()
        .expect("ipset restore stdin was piped")
        .write_all(stream.as_bytes())
        .context("writing ipset restore stream")?;
    let status = child.wait().context("waiting for ipset restore")?;
    if !status.success() {
        bail!("ipset restore exited with {status}");
    }
    Ok(())
}

/// Run one `iptables` invocation, erroring (fail-closed) on non-zero exit.
fn iptables(args: &[&str]) -> Result<()> {
    run_command("iptables", args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_ipv4_and_cidr_as_net() {
        assert_eq!(classify_entry("1.2.3.4"), Entry::Net("1.2.3.4".into()));
        assert_eq!(
            classify_entry("1.2.3.0/24"),
            Entry::Net("1.2.3.0/24".into())
        );
    }

    #[test]
    fn classifies_ipv6_literal_without_port_mangling() {
        // The old shell script's `${entry%%:*}` truncated this to `2001`.
        assert_eq!(classify_entry("::1"), Entry::Net("::1".into()));
        assert_eq!(
            classify_entry("2001:db8::1"),
            Entry::Net("2001:db8::1".into())
        );
        assert_eq!(
            classify_entry("2001:db8::/32"),
            Entry::Net("2001:db8::/32".into())
        );
    }

    #[test]
    fn strips_wildcard_to_apex() {
        assert_eq!(
            classify_entry("*.cdn.example.com"),
            Entry::Domain("cdn.example.com".into())
        );
    }

    #[test]
    fn strips_port_from_domain() {
        assert_eq!(
            classify_entry("registry.npmjs.org:443"),
            Entry::Domain("registry.npmjs.org".into())
        );
    }

    #[test]
    fn parses_list_with_whitespace_and_blanks() {
        let entries = parse_allowed_hosts(" api.anthropic.com , 1.2.3.0/24 ,, *.cdn.x.com ");
        assert_eq!(
            entries,
            vec![
                Entry::Domain("api.anthropic.com".into()),
                Entry::Net("1.2.3.0/24".into()),
                Entry::Domain("cdn.x.com".into()),
            ]
        );
    }

    #[test]
    fn empty_input_yields_no_entries() {
        assert!(parse_allowed_hosts("").is_empty());
        assert!(parse_allowed_hosts("  ,  , ").is_empty());
    }
}
