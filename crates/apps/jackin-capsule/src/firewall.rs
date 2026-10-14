// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::runtime_setup::run_command;
use anyhow::{Context, Result, bail};
use jackin_process::{ExecRequest, StdioMode};
use std::collections::BTreeSet;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

const IPSET: &str = "jackin-allowed";

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Net(String),

    Domain(String),
}

fn parse_allowed_hosts(raw: &str) -> Vec<Entry> {
    raw.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(classify_entry)
        .collect()
}

fn classify_entry(entry: &str) -> Entry {
    let host = entry.strip_prefix("*.").unwrap_or(entry);

    if is_ip_or_cidr(host) {
        return Entry::Net(host.to_owned());
    }

    let domain = host.split_once(':').map_or(host, |(h, _)| h);
    Entry::Domain(domain.to_owned())
}

fn is_ip_or_cidr(host: &str) -> bool {
    let addr = host.split_once('/').map_or(host, |(a, _)| a);
    addr.parse::<IpAddr>().is_ok()
}

pub fn apply() -> Result<()> {
    let raw = std::env::var(jackin_core::JACKIN_ALLOWED_HOSTS_ENV_NAME).unwrap_or_default();
    let entries = parse_allowed_hosts(&raw);

    ensure_tool("iptables")?;
    ensure_tool("ip6tables")?;
    ensure_tool("ipset")?;

    for rule in BASE_DENY_RULES {
        iptables(rule)?;
    }

    for rule in BASE_DENY_RULES {
        ip6tables(rule)?;
    }

    if entries.is_empty() {
        return Ok(());
    }

    iptables(&["-A", "OUTPUT", "-p", "udp", "--dport", "53", "-j", "ACCEPT"])?;
    iptables(&["-A", "OUTPUT", "-p", "tcp", "--dport", "53", "-j", "ACCEPT"])?;

    let mut members: BTreeSet<String> = BTreeSet::new();
    for entry in entries {
        match entry {
            Entry::Net(net) => {
                if enforceable_ipv4(&net) {
                    members.insert(net);
                } else {
                    let _warning = jackin_telemetry::record_recovered_degradation();
                }
            }
            Entry::Domain(domain) => {
                let v4: Vec<String> = resolve(&domain)
                    .into_iter()
                    .filter(IpAddr::is_ipv4)
                    .map(|ip| ip.to_string())
                    .collect();
                if v4.is_empty() {
                    let _warning = jackin_telemetry::record_recovered_degradation();
                }
                members.extend(v4);
            }
        }
    }
    install_ipset(&members)?;

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

    Ok(())
}

fn enforceable_ipv4(member: &str) -> bool {
    match member.split_once('/') {
        Some((addr, prefix)) => {
            addr.parse::<Ipv4Addr>().is_ok() && prefix.parse::<u8>().is_ok_and(|p| p <= 32)
        }
        None => member.parse::<Ipv4Addr>().is_ok(),
    }
}

fn resolve(domain: &str) -> Vec<IpAddr> {
    (domain, 0u16)
        .to_socket_addrs()
        .into_iter()
        .flatten()
        .map(|sa| sa.ip())
        .collect()
}

fn ipset_restore_stream(members: &BTreeSet<String>) -> String {
    let mut stream = format!("create {IPSET} hash:net maxelem 65536\nflush {IPSET}\n");
    for member in members {
        stream.push_str(&format!("add {IPSET} {member}\n"));
    }
    stream
}

fn install_ipset(members: &BTreeSet<String>) -> Result<()> {
    let stream = ipset_restore_stream(members);

    let request = ExecRequest::new("ipset", ["restore", "-exist"])
        .stdin_mode(StdioMode::Capture)
        .stdout_mode(StdioMode::Inherit);
    let (operation, mut child) = crate::process_telemetry::spawn_sync(&request)
        .context("spawning firewall restore process")?;
    let Some(mut stdin) = child.stdin.take() else {
        operation.complete_io_failure();
        bail!("firewall restore stdin was unavailable");
    };
    if stdin.write_all(stream.as_bytes()).is_err() {
        operation.complete_io_failure();
        bail!("writing firewall restore policy failed");
    }
    drop(stdin);
    let Ok(output) = child.wait_with_output() else {
        operation.complete_io_failure();
        bail!("waiting for firewall restore process failed");
    };
    operation.complete_status(output.status, &[0]);
    if !output.status.success() {
        bail!("firewall restore process exited unsuccessfully");
    }
    Ok(())
}

fn iptables(args: &[&str]) -> Result<()> {
    run_command("iptables", args)
}

const BASE_DENY_RULES: &[&[&str]] = &[
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

fn ip6tables(args: &[&str]) -> Result<()> {
    run_command("ip6tables", args)
}

fn ensure_tool(tool: &str) -> Result<()> {
    let request = ExecRequest::new(tool, ["--version"])
        .stdout_mode(StdioMode::Null)
        .stderr_mode(StdioMode::Null);
    match crate::process_telemetry::exec_sync(&request) {
        Ok(_) => Ok(()),
        Err(_) => {
            bail!(
                "a required firewall tool is unavailable in this container image, but the `allowlist` network tier \
                 requires `iptables` and `ipset`. Rebuild the role image on a construct image that \
                 installs them (jackin❯ construct >= 0.17-trixie), or use a profile whose network \
                 tier does not enforce an egress allowlist."
            )
        }
    }
}

#[cfg(test)]
mod tests;
