#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
. "${script_dir}/versions.env"
MISE_VERSION="$(python3 - "${script_dir}/../../mise.toml" <<'PYTHON'
import pathlib, re, sys, tomllib
config = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
version = config.get("min_version")
if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
    sys.exit("construct requires an exact numeric Mise min_version")
print(version)
PYTHON
)"
for version_name in TIRITH_VERSION SHELLFIRM_VERSION MISE_VERSION STARSHIP_VERSION; do
  if [ -z "${!version_name:-}" ]; then
    echo "::error::${version_name} is empty" >&2
    exit 1
  fi
done

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT
cd "${workdir}"

fetch() {
  curl --fail --silent --show-error --location \
    --retry 5 --retry-all-errors --retry-delay 2 \
    --connect-timeout 15 --max-time 120 --output "$2" "$1"
}

verify_listed_archive() {
  local archive="$1" checksums="$2" member="$3"
  awk -v archive="${archive}" '
    $2 == archive || $2 == "./" archive { print $1 "  " archive; count++ }
    END { if (count != 1) exit 1 }
  ' "${checksums}" > "${archive}.check"
  grep -Eq "^[a-f0-9]{64}  ${archive//./\\.}$" "${archive}.check"
  sha256sum -c "${archive}.check"
  tar -tzf "${archive}" | grep -Fx "${member}"
}

fetch "https://github.com/sheeki03/tirith/releases/download/v${TIRITH_VERSION}/checksums.txt" tirith-checksums.txt
fetch "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/SHASUMS256.txt" mise-SHASUMS256.txt

for arch in amd64 arm64; do
  case "${arch}" in
    amd64) tirith_triple=x86_64-unknown-linux-gnu; mise_arch=x64; starship_triple=x86_64-unknown-linux-gnu ;;
    arm64) tirith_triple=aarch64-unknown-linux-gnu; mise_arch=arm64; starship_triple=aarch64-unknown-linux-musl ;;
  esac

  archive="tirith-${tirith_triple}.tar.gz"
  fetch "https://github.com/sheeki03/tirith/releases/download/v${TIRITH_VERSION}/${archive}" "${archive}"
  verify_listed_archive "${archive}" tirith-checksums.txt tirith

  archive="mise-v${MISE_VERSION}-linux-${mise_arch}.tar.gz"
  fetch "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/${archive}" "${archive}"
  verify_listed_archive "${archive}" mise-SHASUMS256.txt mise/bin/mise

  archive="starship-${starship_triple}.tar.gz"
  base="https://github.com/starship/starship/releases/download/v${STARSHIP_VERSION}"
  fetch "${base}/${archive}" "${archive}"
  fetch "${base}/${archive}.sha256" "${archive}.sha256"
  checksum="$(tr -d '\r\n' < "${archive}.sha256")"
  [[ "${checksum}" =~ ^[a-f0-9]{64}$ ]]
  printf '%s  %s\n' "${checksum}" "${archive}" > "${archive}.check"
  sha256sum -c "${archive}.check"
  tar -tzf "${archive}" | grep -Fx starship
done
