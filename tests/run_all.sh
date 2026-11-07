#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
lua5.1 tests/test_lua_reload.lua lua/honrp_mod.lua
