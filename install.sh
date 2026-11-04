#!/usr/bin/env bash
# Build honrp from a source checkout and install it for the current user.
# For release tarballs, run `honrp install` directly instead.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to build from source." >&2
  echo "Download a release binary from https://github.com/NasirA1991/honrp/releases" >&2
  exit 1
fi

cargo build --release
install -Dm755 target/release/honrp "$HOME/.local/bin/honrp"
echo "installed $HOME/.local/bin/honrp"
"$HOME/.local/bin/honrp" install
