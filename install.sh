#!/usr/bin/env bash
# Download the latest Linux honrp binary, install it for this user and set up
# the Lua bridge + systemd user service.
set -euo pipefail

repo="NasirA1991/honrp"
bin="honrp-linux-x86_64"
dest="${HOME}/.local/bin/honrp"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

case "$(uname -m)" in
  x86_64|amd64) ;;
  *) echo "unsupported architecture: $(uname -m) (only x86_64 builds are published)" >&2; exit 1 ;;
esac

base="https://github.com/${repo}/releases/latest/download"
echo "downloading ${base}/${bin}"
curl -fsSL "${base}/${bin}" -o "${tmp}/${bin}"
chmod +x "${tmp}/${bin}"

curl -fsSL "${base}/${bin}.sha256" -o "${tmp}/${bin}.sha256" || {
  echo "missing checksum for ${bin}; refusing to install" >&2
  exit 1
}
(cd "$tmp" && sha256sum -c "${bin}.sha256")

install -Dm755 "${tmp}/${bin}" "${dest}"
echo "installed ${dest}"
"${dest}" install
