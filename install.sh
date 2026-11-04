#!/usr/bin/env bash
# Download the latest static honrp binary, install it for this user and set up
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

if curl -fsSL "${base}/${bin}.sha256" -o "${tmp}/${bin}.sha256" 2>/dev/null; then
  (cd "$tmp" && sha256sum -c "${bin}.sha256")
fi

install -Dm755 "${tmp}/${bin}" "${dest}"
echo "installed ${dest}"

id=""
if [ -r /dev/tty ]; then
  printf "Discord application id (leave blank to configure later): "
  read -r id < /dev/tty || id=""
fi

if [ -n "${id}" ]; then
  "${dest}" install --client-id "${id}"
else
  "${dest}" install
fi
