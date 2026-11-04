#!/usr/bin/env bash
# Install the honrp Lua state bridge into the running HoN (Wine/Proton) install.
set -euo pipefail
cd "$(dirname "$0")"
python3 honrp.py install "$@"
python3 honrp.py status || true
cat <<'EOF'

Next:
  1. restart Heroes of Newerth  (the mod is read by CHost::Init at launch)
  2. verify inside the game log:
       grep -a HONRP "<prefix>/drive_c/users/<user>/Documents/Juvio/Heroes of Newerth/console.log"
  3. start the daemon:
       python3 honrp.py run
EOF
