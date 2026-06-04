#!/bin/bash
# /jackin/runtime/init-firewall.sh
#
# Installs an iptables OUTPUT allowlist for the `allowlist` network tier.
# Must run before the agent binary is exec'd — called from entrypoint.sh
# when JACKIN_NETWORK_MODE=allowlist.
#
# Required capabilities: CAP_NET_ADMIN, CAP_NET_RAW.
# Required packages: iptables, ipset, dnsutils (dig).
# Required env: JACKIN_ALLOWED_HOSTS (comma-separated domains/IPs/CIDRs).
#
# Enforcement quality:
#   full   — when sudo = false and dind = none (agent has no path to iptables)
#   partial — when sudo = true (agent can run `sudo iptables -F`)
#   partial — when dind is active (inner containers bypass host iptables)
# The session contract (JACKIN_NETWORK_ENFORCEMENT env var) carries the quality.
set -euo pipefail

ALLOWED_HOSTS="${JACKIN_ALLOWED_HOSTS:-}"

if [ -z "$ALLOWED_HOSTS" ]; then
    echo "[firewall] JACKIN_ALLOWED_HOSTS is empty; no allowlist installed" >&2
    exit 0
fi

# Create ipset for allowed hosts (hash:net accepts both host IPs and CIDRs).
ipset create jackin-allowed hash:net maxelem 65536 2>/dev/null || \
    ipset flush jackin-allowed

for entry in $(echo "$ALLOWED_HOSTS" | tr ',' '\n'); do
    entry="$(echo "$entry" | xargs)"   # trim whitespace
    [ -z "$entry" ] && continue

    # Strip optional port suffix (domain:8080 → domain).
    host="${entry%%:*}"

    # If the entry looks like an IPv4/IPv6 address or CIDR, add directly.
    if echo "$host" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]' || \
       echo "$host" | grep -qE '^[0-9a-fA-F:]+/'; then
        ipset add jackin-allowed "$entry" 2>/dev/null || true
        continue
    fi

    # Wildcard subdomain: *.example.com — resolve the apex domain.
    if echo "$host" | grep -q '^\*\.'; then
        host="${host#\*.}"
    fi

    # Resolve domain to IPs and add each.
    while IFS= read -r ip; do
        [ -z "$ip" ] && continue
        ipset add jackin-allowed "$ip" 2>/dev/null || true
    done < <(dig +short "$host" A "$host" AAAA 2>/dev/null | grep -vE '^;|^$')
done

# ── Apply OUTPUT policy ───────────────────────────────────────────────────────
# Default DROP; allow loopback, established, DNS, and the allowlist.
iptables -P OUTPUT DROP
iptables -A OUTPUT -o lo -j ACCEPT
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 53 -j ACCEPT
iptables -A OUTPUT -m set --match-set jackin-allowed dst -j ACCEPT

# Self-test: api.github.com is always in the default allowlist when gh auth
# is forwarded. Warn (but do not abort) if it's unreachable — DNS may still
# be resolving, so a soft warning avoids false-positive startup failures.
if ! curl -sf --max-time 5 --connect-timeout 3 https://api.github.com/zen > /dev/null 2>&1; then
    echo "[firewall] WARNING: api.github.com unreachable after allowlist install" \
         "— DNS may still be resolving; verify JACKIN_ALLOWED_HOSTS includes it" >&2
fi

echo "[firewall] OUTPUT allowlist active: $(ipset list jackin-allowed | grep -c '^[0-9]') entries"
