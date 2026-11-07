# honrp

<p align="center">
  <img src="assets/branding/readme-banner-1600x450.png" alt="honrp — Heroes of Newerth ↔ Discord Rich Presence" width="100%">
</p>

[![CI](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml/badge.svg)](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml)
[![AUR](https://img.shields.io/badge/AUR-honrp--bin-1793D1?logo=archlinux&logoColor=white)](https://aur.archlinux.org/packages/honrp-bin)

Discord Rich Presence for **Heroes of Newerth: Reborn** (the Juvio client) on Linux —
Steam/Proton, Lutris, Heroic, Faugus, Bottles, or plain Wine.

## States

<p align="center">
  <img src="assets/states.png" alt="The six honrp states: in match, in queue, hero select, spectating, main menu and Plinko" width="100%">
</p>

Illustrative render of the presence payloads (`build_activity()` output), not a
Discord screenshot: the client draws these lines on your profile next to the
Heroes of Newerth app art.

| state | card |
|---|---|
| Main menu | `Main Menu` · account |
| Queue | `Searching for a match` · `In queue` |
| Match | `Ranked • FoC • Banning Pick` · `Pyromancer • Lv 12 • 4/2/7` |
| Hero select | `Ranked • FoC • All Pick` · `Hero Select` |
| Spectating | `Spectating • FoC` · `Watching Master Of Arms` |
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

### Windows (alpha)

Portable, no installer and no autostart yet:

1. Download `honrp-vX.Y.Z-windows-x86_64.zip` from the latest release and extract it.
2. Run `honrp.exe install` — it writes the Lua bridge to
   `%USERPROFILE%\Documents\Juvio\Heroes of Newerth`.
3. Run `honrp.exe run` while playing (keep the window open), then restart HoN.

No admin required; nothing in the game install directory is modified. This
build has not been tested on real Windows hardware yet — feedback is very
welcome. See [README-WINDOWS.txt](README-WINDOWS.txt).

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
honrp                     # control window (Windows and Linux): status, paths, updates
honrp status [--json]     # current game state and the card it would send
honrp doctor              # prefix, bridge build, Discord handshake, service
honrp run -v              # foreground (systemd runs this normally)
honrp update [--check]    # fetch the latest release, verify and swap the binary
honrp stop                # stop the background daemon (window stays open)
honrp uninstall           # remove the service and the Lua bridge
honrp service status
```

The control window is the same egui UI on both platforms, a compact 480x192
panel (grows by 24 px for an action hint, and to 480x236 while Config or a
dialog is open): Rich Presence and Discord state (Rich Presence on/off;
Discord ready, Live on Discord, Can't connect, not running or unknown), the
card being sent (hover for hero/level/KDA), a compact **Stop Rich Presence /
Start Rich Presence** button with Config and Menu beside it, a "closing this
window keeps Rich Presence running" hint, and a neutral **Launch at startup**
row (on Linux it opens the copyable systemd commands). A faint separator sits
above the footer, which is a clickable update status. **Config** is a settings
screen for the game documents folder (**Browse...**, saved as `user_dir`) and
an optional Discord IPC socket override; no Discord install path is needed. X
closes the window only, never Rich Presence; there is no separate Close/Quit
button. For a headless build without the GUI dependency:
`cargo build --release --no-default-features`.

`honrp run` also keeps the installed Lua bridge in sync with the binary: if the
bundled bridge is newer (or was edited), the managed block in `startup.lua` is
rewritten on start-up and the daemon prints that the game needs one restart.
The same applies after an update, so `honrp install` only has to be run again
if you want to repair a deleted bridge.

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
| `short_maps` | `true` | `FoC` instead of `Forests of Caldavar (Phoenix)` |
| `check_updates` | `true` | daily GitHub check; prints `honrp update` when a newer release exists |
| `user_dir` | `""` | manual game documents folder (Config → Game folder) |
| `ipc_socket` | `""` | manual Discord IPC socket/pipe (Config → Discord) |

### Custom Discord app (optional)

1. Create an app at <https://discord.com/developers/applications>, copy its
   Application ID.
2. Put `{"client_id": "..."}` in the config (or `honrp install --client-id ...`).
3. Optional: upload an image as a Rich Presence asset named `hon-transparent`.

## Help / Troubleshooting

The same notes are in the app: **... → Help / Troubleshooting** opens this
section. If your problem isn't here, open an
[issue](https://github.com/NasirA1991/honrp/issues) with `honrp doctor` output.

<details>
<summary><b>Windows</b> — common problems</summary>

- **Discord shows nothing / "Discord not running"** — start the Discord desktop
  client; honrp only publishes while HoN is running. `honrp doctor` shows the
  handshake and which pipe was found. For a non-standard Discord build, set the
  socket in **Config → Discord**.
- **Two cards, or "juvio.exe" listed as a game** — Discord detects Heroes of
  Newerth on its own. Disable it once in *Discord → Settings → Game Activity →
  Registered Games*; honrp cannot change that list.
- **"Daemon stopped" / no card after closing the window** — the X button only
  closes the window; if the big button was used to stop the daemon, start it
  again with the same button or reopen honrp.
- **In-game card has no hero/level/KDA** — the Lua bridge must be loaded.
  Restart HoN after installing/updating; `honrp doctor` prints the bridge build
  from `Documents\Juvio\Heroes of Newerth\startup.lua`. Use **Config → Game
  folder** if your Documents folder lives somewhere else.
- **Map shows "Field Test"** — practice games run the `fieldtest` world by
  design; real matches show FoC.
- **Game folder not detected** — **Config → Browse** to the folder containing
  `console.log` (usually `Documents\Juvio\Heroes of Newerth`), then Save; the
  daemon restarts automatically.
- **SmartScreen/Defender blocks the exe or updater** — the binary is unsigned;
  "More info → Run anyway", or add an exclusion. Updates need `curl` and `tar`
  (both ship with Windows 10+).
- **Logs** — `%APPDATA%\honrp\daemon.log`.

</details>

<details>
<summary><b>Linux</b> — common problems</summary>

- **Game/prefix not found** — start HoN once and run `honrp doctor`; override
  with `honrp install --prefix ...` / `--user-dir` or `"user_dir"` in
  `~/.config/honrp/config.json`.
- **Card stuck on Main Menu** — bridge not loaded: restart HoN. `honrp doctor`
  shows the bridge build.
- **"Discord not running" / "Can't connect to Discord"** — Flatpak Discord often
  uses a different socket; set `ipc_socket` (for example
  `$XDG_RUNTIME_DIR/app/com.discordapp.Discord/discord-ipc-0`). `honrp doctor`
  lists the sockets it tries.
- **Two cards / Juvio detected by Discord** — same Registered Games fix as on
  Windows.
- **systemd service** — `systemctl --user status honrp` and
  `journalctl --user -u honrp`.
- **No window (Wayland/VM)** — the GUI needs a working Vulkan driver; use
  `honrp run` headless, the daemon doesn't need the window.
- **AUR vs updater** — `paru -Syu honrp-bin` for AUR installs; `honrp update`
  is for the standalone tarball.
- **No hero/KDA** — restart HoN after an update (the bridge is embedded in the
  binary and rewritten on daemon start).

</details>


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

## Brand assets

The banner above and the application icon come from the branding pack:

- `assets/branding/readme-banner-1600x450.png` — this header.
- `assets/branding/social-preview-1280x640.png` — upload under GitHub
  *Settings → Social preview*.
- `assets/branding/avatar-512.png` — repository avatar.
- `assets/icon.svg`, `assets/icon.png`, `assets/icon.ico` — app icon source
  and exports (the `.ico` contains 16–256 px variants and is embedded in the
  exe/window/installer).

Colors and usage: [docs/BRANDING.md](docs/BRANDING.md). The mark is original
(a gold hood/eyes, a three-arch bridge and a chat bubble); it does not
reproduce the Heroes of Newerth or Discord logos.

## License

MIT. honrp is an unofficial project, not affiliated with or endorsed by Juvio,
LLC or Discord Inc.; "Heroes of Newerth" and "Discord" are trademarks of their
respective owners. The icon is an original design and does not reproduce
either logo.
