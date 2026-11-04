#!/usr/bin/env bash
# Run every honrp test (no external dependencies).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== python tests =="
python3 tests/test_honrp.py

if command -v lua5.1 >/dev/null 2>&1; then
  echo "== lua mod tests (mocked K2 environment) =="
  lua5.1 tests/test_lua_mod.lua honrp_mod.lua
else
  echo "lua5.1 not found - skipping mod test"
fi
