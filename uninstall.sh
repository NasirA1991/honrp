#!/usr/bin/env bash
# Remove the honrp Lua state bridge.
set -euo pipefail
cd "$(dirname "$0")"
python3 honrp.py uninstall "$@"
