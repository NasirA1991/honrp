# Privacy & transparency

honrp is a local-only helper. This page is the complete list of what it
touches, including its loopback control ports and update check.

## What it reads

| path | why |
|---|---|
| `<prefix>/drive_c/users/<user>/Documents/Juvio/Heroes of Newerth/console.log` | the Lua bridge writes one `[HONRP]` state line per event / 10 s; the daemon tails it |
| `.../Documents/Juvio/Heroes of Newerth/logs/game_*.log` | map, mode and match start (the client's own match log) |
| `/proc/<pid>/comm`, `cmdline`, `environ` | find the running `juvio.exe` and its `STEAM_COMPAT_DATA_PATH` / `WINEPREFIX` |
| Steam `libraryfolders.vdf` and `steamapps/compatdata/*/pfx` | locate Proton prefixes when the game is not running |
| `~/.config/lutris/`, `~/.local/share/lutris/`, `~/.config/heroic/` and Flatpak equivalents | read launcher prefixes |
| `~/.local/share/bottles/` and Flatpak equivalent | read Bottles prefixes |
| `~/Games`, `~/games`, `~/.wine`, `WINEPREFIX` | common Wine/prefix locations |
| a bounded walk of `$HOME` (at most 20,000 directories) | last-resort `Documents/Juvio/Heroes of Newerth` search |
| `~/.config/honrp/config.json` | your settings (Discord app id, labels) |
| `$XDG_RUNTIME_DIR/discord-ipc-*` | Discord's local Rich Presence socket |

No game memory is opened, no keyboard/mouse input is read, and no other user's
processes are inspected (only processes whose comm is `juvio.exe`).

## What it writes

| path | why |
|---|---|
| `.../Documents/Juvio/Heroes of Newerth/startup.lua` | the Lua bridge, inside a `--- HONRP BEGIN/END ---` managed block; existing content is preserved and backed up |
| `~/.config/honrp/config.json` | configuration |
| `~/.config/systemd/user/honrp.service` | the user service (no root) |
| stderr / journal | status and errors |

The game install directory is never modified.

## Network

The daemon normally speaks only to the local Discord IPC unix socket
(`discord-ipc-0` … `discord-ipc-9`). There is no telemetry and no account login.

Two local/loopback connections exist:

- The GUI and daemon use TCP on `127.0.0.1:47831` and `127.0.0.1:47832` for
  single-instance detection and the control protocol. These are bound to
  loopback only.
- When update checks are enabled (`check_updates`, default `true`), honrp
  requests the GitHub release API and, if you install an update, the release
  asset over HTTPS. Set `check_updates` to `false` to disable the check.

Rich Presence content (activity name, details, state, art) is sent to Discord
and is visible according to your Discord activity-privacy settings. That is the
entire point of the tool; nothing else leaves the machine.

## Resource use (observed locally; no benchmark harness)

| piece | cost |
|---|---|
| daemon RSS | ~3–6 MiB (observed 3.0 MiB idle, no game attached) |
| daemon CPU | ~0.3 % of one core while idle, <0.5 % during play |
| bridge | one state line per event, 10 s heartbeat; ~40 B/s ≈ 144 KB/h of play |
| Discord traffic | one `SET_ACTIVITY` per change, at most every 5 s, only while a game runs |

## How to verify it yourself

```bash
honrp status                 # what it currently sees
honrp doctor                 # prefix/user dir, bridge build, Discord handshake
systemctl --user status honrp
strace -f -e trace=network -p "$(systemctl --user show honrp -p MainPID --value)"
#   -> only AF_UNIX connects to $XDG_RUNTIME_DIR/discord-ipc-*
ss -xp | grep discord-ipc   # no connection at all when HoN is closed
```

## Uninstall

```bash
honrp uninstall          # stops/disables the service, removes the Lua block
honrp uninstall --purge  # also removes ~/.config/honrp
```
