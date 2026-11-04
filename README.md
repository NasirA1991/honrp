# honrp

[![CI](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml/badge.svg)](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml)

Discord Rich Presence for **Heroes of Newerth** (the Juvio client) on Linux —
Steam/Proton, Lutris, Heroic, Faugus, Bottles, or plain Wine.

| state | card |
|---|---|
| Main menu | `Main Menu` · account |
| Queue | `Searching for a match` · `In queue` |
| Match | `Ranked • FoW • Banning Pick` · `Pyromancer • Lv 12 • 4/2/7` |
| Hero select | `Ranked • FoW • All Pick` · `Hero Select` |
| Spectating | `Spectating • FoW` · `Watching Master Of Arms` |
| Plinko | `Playing Plinko` · account |

## Install

```bash
curl -fsSL https://github.com/NasirA1991/honrp/releases/latest/download/install.sh | bash
```

Arch Linux: `paru -S honrp-bin` (or `yay -S honrp-bin`), then run `honrp install`.

The AUR package installs the binary; `honrp install` is the **per-user** setup
step because pacman runs as root and cannot write into your Wine prefix (which
may not exist before the first launch).

From source:

```bash
cargo build --release
install -Dm755 target/release/honrp ~/.local/bin/honrp
honrp install
```

`honrp install` finds your Proton/Wine prefix, adds the Lua bridge to
`startup.lua`, and installs a systemd **user** service — no root, nothing in the
game install directory. On systems without a systemd user session it writes
`~/.config/autostart/honrp.desktop` instead. Restart Heroes of Newerth
afterwards. A Discord app id is built in, so you don't need to create a Discord
application.

### Uninstall

```bash
honrp uninstall           # removes the Lua bridge and the service/autostart
paru -R honrp-bin         # or: yay -R honrp-bin
```

`~/.config/honrp/config.json` is left in place so reinstalling keeps your
settings.

## Finding HoN

No hardcoded Steam library or app id:

- **Running:** read `WINEPREFIX` / `STEAM_COMPAT_DATA_PATH` from `juvio.exe`,
  or recover the prefix from the `drive_c` path in `/proc/<pid>/maps` — works
  for any launcher.
- **Not running:** scan every Steam `compatdata/*/pfx`, Lutris and Heroic game
  configs, Bottles, `~/Games`, `~/.wine`, `$WINEPREFIX`, plus a bounded scan of
  your home for `drive_c/users/*/Documents/Juvio/Heroes of Newerth`.
- **Override:** `honrp install --prefix /path/to/pfx` (or `--user-dir`).
  `honrp doctor` lists every candidate and the newest `console.log`.

## Commands

```bash
honrp status [--json]     # current game state and the card it would send
honrp doctor              # prefix, bridge build, Discord handshake, service
honrp run -v              # foreground (systemd runs this normally)
honrp uninstall           # remove the service and the Lua bridge
honrp service status
```

The daemon connects to Discord only while HoN is running; when the game exits
it clears the card and disconnects. It idles at ~1.6 MiB RSS with no Discord
connection.

## Configuration

`~/.config/honrp/config.json` (see `config.example.json`):

| key | default | |
|---|---|---|
| `client_id` | built-in | Discord application id (override if you want your own app) |
| `large_image` | `hon-transparent` | Rich Presence asset key; empty disables art |
| `show_kda` | `true` | `hero • Lv • K/D/A` vs `hero • Lv` |
| `show_timer` | `false` | match/menu timer |
| `short_maps` | `true` | `FoW` instead of `Forests of Caldavar (Phoenix)` |

### Custom Discord app (optional)

1. Create an app at <https://discord.com/developers/applications>, copy its
   Application ID.
2. Put `{"client_id": "..."}` in the config (or `honrp install --client-id ...`).
3. Optional: upload an image as a Rich Presence asset named `hon-transparent`.

## Troubleshooting

| symptom | fix |
|---|---|
| no prefix found | start HoN once, or use `--prefix` / `--user-dir` |
| wrong prefix | same flags; `honrp doctor` shows all candidates |
| card stuck on `Main Menu` | bridge not loaded: restart HoN, check `doctor` |
| no art | upload `hon-transparent` in the portal, or clear `large_image` |
| Discord shows a second `Juvio` game | disable it once in Settings → Game Activity → Registered Games; honrp cannot edit Discord's detection list |

## Performance

Small by design:

| piece | cost |
|---|---|
| daemon | ~1.7 MiB RSS, ~0.1–0.3% of one core; no Discord connection while HoN is closed |
| `/proc` poll | once per second (configurable); a directory read, not game state |
| Lua bridge | runs on the game's existing UI tick: a handful of `UICmd` lookups per state change and one console line per event / 10 s (~144 KB/h) |
| Discord | one `SET_ACTIVITY` per change, at most every 5 s |

The bridge does not hook the render loop, read game memory, or scan files during
a match; no measurable FPS impact in testing. Heavy `console.log` growth is
avoided by the 10 s heartbeat (the client itself logs more than this). If you
prefer fewer `/proc` reads, set `"poll_interval": 3.0`.

## Privacy

Reads `console.log`, `logs/game_*.log` and `/proc`; writes the managed
`startup.lua` block, config and a user unit; talks only to Discord's local unix
socket. No telemetry, no game memory, no game files changed. Details:
[PRIVACY.md](PRIVACY.md).

## Development

```bash
cargo test
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
```

Bridge: [lua/honrp_mod.lua](lua/honrp_mod.lua) · how it works:
[docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md) ·
[AUR packaging](packaging/aur).

## License

MIT — not affiliated with Juvio or Discord.
