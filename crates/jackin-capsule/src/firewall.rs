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
//! IPv4 allowlist, IPv6 deny-all: the *allowlist* is an `iptables` (IPv4) OUTPUT
//! chain over an `inet` ipset, so only IPv4 addresses/CIDRs are allowlistable.
//! IPv6 entries and AAAA records are skipped (with a warning) rather than added —
//! a single IPv6 member would otherwise abort the whole `ipset restore` batch.
//! Because no IPv6 destination can be allowlisted, IPv6 egress is denied
//! wholesale via an `ip6tables` `-P OUTPUT DROP` (loopback + established only),
//! so a dual-stack agent cannot bypass the allowlist over IPv6.
//!
//! Fail-closed: the default-`DROP` policy plus the loopback/established accepts
//! are installed *first*, so any mid-apply error leaves egress denied rather than
//! open. The DNS accepts land next, before domain resolution, so `getaddrinfo`
//! can reach the resolver while the policy is already DROP.

use crate::runtime_setup::run_command;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::process::{Command, Stdio};

/// ipset name holding the allowed destination addresses/CIDRs.
const IPSET: &str = "jackin-allowed";

/// One parsed allowlist entry, classified by shape.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    /// A literal IP or CIDR (any family). Only IPv4 members are enforceable —
    /// see [`enforceable_ipv4`]; IPv6 is dropped with a warning at install time.
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
        return Entry::Net(host.to_owned());
    }

    // Domain, possibly `domain:port` — keep the host, drop the port.
    let domain = host.split_once(':').map_or(host, |(h, _)| h);
    Entry::Domain(domain.to_owned())
}

/// True when `host` is a bare IP address or `addr/prefix` CIDR (v4 or v6).
fn is_ip_or_cidr(host: &str) -> bool {
    let addr = host.split_once('/').map_or(host, |(a, _)| a);
    addr.parse::<IpAddr>().is_ok()
}

/// Entry point for the `firewall-apply` subcommand.
pub fn apply() -> Result<()> {
    let raw =
        std::env::var(jackin_core::env_model::JACKIN_ALLOWED_HOSTS_ENV_NAME).unwrap_or_default();
    let entries = parse_allowed_hosts(&raw);

    // Preflight the required binaries before touching the policy, so a missing
    // tool yields an actionable error rather than a bare "No such file" after a
    // partial install.
    ensure_tool("iptables")?;
    ensure_tool("ip6tables")?;
    ensure_tool("ipset")?;

    // Fail-closed: deny by default, then permit loopback and established flows
    // before anything fallible runs, so a mid-apply error cannot leave egress
    // open.
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

    // The allowlist ipset is IPv4-only (`inet`), so the allowlist cannot admit
    // any IPv6 destination. Deny all IPv6 egress fail-closed rather than leave
    // the v6 OUTPUT policy at its ACCEPT default — otherwise an agent on a
    // dual-stack network bypasses the entire allowlist over IPv6. Loopback and
    // established return traffic stay permitted, mirroring the IPv4 policy.
    for rule in IPV6_DENY_RULES {
        ip6tables(rule)?;
    }

    if entries.is_empty() {
        // network=allowlist with no hosts is fail-closed (no egress), not open.
        crate::clog!(
            "firewall: JACKIN_ALLOWED_HOSTS is empty; DROP-only policy (no IPv4/IPv6 egress)"
        );
        return Ok(());
    }

    // DNS accepts land before the resolve loop so `getaddrinfo` can reach the
    // resolver while the policy is already DROP.
    iptables(&["-A", "OUTPUT", "-p", "udp", "--dport", "53", "-j", "ACCEPT"])?;
    iptables(&["-A", "OUTPUT", "-p", "tcp", "--dport", "53", "-j", "ACCEPT"])?;

    // Resolve every entry to its IPv4 destinations, deduped. Non-IPv4 entries
    // and unresolvable hosts are skipped loudly: one bad/IPv6 member would abort
    // the whole `ipset restore` batch, and a silently-dropped host reads as a
    // mysterious connectivity failure later.
    let mut members: BTreeSet<String> = BTreeSet::new();
    for entry in entries {
        match entry {
            Entry::Net(net) => {
                if enforceable_ipv4(&net) {
                    members.insert(net);
                } else {
                    crate::clog!(
                        "firewall: WARNING: allowlist entry {net:?} is not an enforceable \
                         IPv4 address/CIDR; skipping (IPv6 egress is not filtered)"
                    );
                }
            }
            Entry::Domain(domain) => {
                let v4: Vec<String> = resolve(&domain)
                    .into_iter()
                    .filter(IpAddr::is_ipv4)
                    .map(|ip| ip.to_string())
                    .collect();
                if v4.is_empty() {
                    crate::clog!(
                        "firewall: WARNING: {domain} resolved to no IPv4 address; \
                         not allowlisted (host will be unreachable)"
                    );
                }
                members.extend(v4);
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

    crate::clog!(
        "firewall: OUTPUT allowlist active: {} IPv4 entries; IPv6 egress denied",
        members.len()
    );
    Ok(())
}

/// True when `member` is an IPv4 address or IPv4 CIDR with a valid prefix — the
/// only members an `inet` ipset over IPv4 `iptables` can enforce. Rejects IPv6
/// literals/CIDRs and malformed prefixes (e.g. `1.2.3.0/99`) so a single bad
/// entry is skipped rather than aborting the whole `ipset restore` batch.
fn enforceable_ipv4(member: &str) -> bool {
    match member.split_once('/') {
        Some((addr, prefix)) => {
            addr.parse::<Ipv4Addr>().is_ok() && prefix.parse::<u8>().is_ok_and(|p| p <= 32)
        }
        None => member.parse::<Ipv4Addr>().is_ok(),
    }
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

/// Build the `ipset restore` command stream: re-create and flush the set, then
/// one `add` per member. Flushing first gives a clean set on re-apply; with
/// `-exist` the `create` is idempotent and duplicate members are tolerated (the
/// old per-add `|| true`). Pure so the exact format stays unit-testable.
fn ipset_restore_stream(members: &BTreeSet<String>) -> String {
    let mut stream = format!("create {IPSET} hash:net maxelem 65536\nflush {IPSET}\n");
    for member in members {
        stream.push_str(&format!("add {IPSET} {member}\n"));
    }
    stream
}

/// Create the `hash:net` set and load every member in a single `ipset restore`,
/// instead of one `ipset add` process per address.
fn install_ipset(members: &BTreeSet<String>) -> Result<()> {
    let stream = ipset_restore_stream(members);

    let mut child = Command::new("ipset")
        .args(["restore", "-exist"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning ipset restore")?;
    let Some(mut stdin) = child.stdin.take() else {
        bail!("ipset restore stdin was not piped");
    };
    stdin
        .write_all(stream.as_bytes())
        .context("writing ipset restore stream")?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .context("waiting for ipset restore")?;
    if !output.status.success() {
        // Name the rejected member rather than reducing it to an exit code.
        bail!(
            "ipset restore failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Run one `iptables` invocation, erroring (fail-closed) on non-zero exit.
fn iptables(args: &[&str]) -> Result<()> {
    run_command("iptables", args)
}

/// IPv6 fail-closed program, ordered so the default-`DROP` policy lands before
/// the loopback/established accepts. No allowlist accepts — the ipset is
/// IPv4-only, so IPv6 is deny-all (loopback + established return traffic only).
const IPV6_DENY_RULES: &[&[&str]] = &[
    &["-P", "OUTPUT", "DROP"],
    &["-A", "OUTPUT", "-o", "lo", "-j", "ACCEPT"],
    &[
        "-A",
        "OUTPUT",
        "-m",
        "state",
        "--state",
        "ESTABLISHED,RELATED",
        "-j",
        "ACCEPT",
    ],
];

/// Run one `ip6tables` invocation, erroring (fail-closed) on non-zero exit.
fn ip6tables(args: &[&str]) -> Result<()> {
    run_command("ip6tables", args)
}

/// Verify a required firewall binary is present, with an actionable error.
///
/// Without this, a construct/role image that lacks `iptables`/`ipset` (e.g. an
/// older published construct image or a custom base) fails the allowlist install
/// with a bare `No such file or directory` and a torn-down container. Surface
/// the real cause and the fix instead.
fn ensure_tool(tool: &str) -> Result<()> {
    match Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "`{tool}` is not installed in this container image, but the `allowlist` network tier \
             requires `iptables` and `ipset`. Rebuild the role image on a construct image that \
             installs them (jackin' construct >= 0.17-trixie), or use a profile whose network \
             tier does not enforce an egress allowlist."
        ),
        Err(e) => Err(e).context(format!("checking for `{tool}`")),
    }
}

#[cfg(test)]
mod tests;
