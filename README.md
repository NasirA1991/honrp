# honrp

<p align="center">
  <img src="assets/branding/readme-banner-1600x450.png" alt="honrp — Heroes of Newerth ↔ Discord Rich Presence" width="100%">
</p>

[![CI](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml/badge.svg)](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml)
[![AUR](https://img.shields.io/badge/AUR-honrp--bin-1793D1?logo=archlinux&logoColor=white)](https://aur.archlinux.org/packages/honrp-bin)

Discord Rich Presence for **Heroes of Newerth: Reborn** (the Juvio client) on
**Linux and Windows**. On Linux it works with Steam/Proton, Lutris, Heroic,
Faugus, Bottles or plain Wine; on Windows it uses the native client.

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
The package includes the desktop entry and hicolor icons:

```bash
honrp                 # control window
honrp run             # background daemon (systemd runs this for you)
```

Linux GUI requirements: a Wayland or X11 session, `libxkbcommon`,
`libwayland-client`, the X11 cursor/input/randr libraries, and a Vulkan-capable
driver (Mesa). The `.deb`, `.rpm` and AUR packages declare them; if you use the
standalone tarball, install your distro's equivalents. `cargo build --release
--no-default-features` builds the old headless CLI.

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
honrp uninstall --purge   # also removes ~/.config/honrp
paru -R honrp-bin         # or: yay -R honrp-bin
```

`~/.config/honrp/config.json` is left in place by default so reinstalling
keeps your settings.

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

## Implementation notes — everything changed in this branch

This branch (`feat/compact-ui-branding`) is based on commit `f79baac`
("AUR: version the cached source filename"). Everything below was added or
changed after that point. It is deliberately detailed so a Linux session can
review the work and port/verify the UI without re-deriving it.

**Status:** Windows build tested by hand (screenshots, installer, daemon, Lua
bridge, Discord states). Linux GUI has been verified on Wayland (KDE) and X11:
window opens, resizes for Config/dialogs (480×236), single-instance lock,
daemon spawn/adopt, Discord-ready state, bridge sync to build 13, and the
desktop/icons packaging wiring. The rfd folder picker, updater against a real
newer release, and an end-to-end in-game match with this UI build are still
unverified.

A standalone, file-by-file handoff for another agent is in
**[docs/BRANCH-CHANGES.md](docs/BRANCH-CHANGES.md)** — it covers the UI spec,
Discord state table, daemon/control protocol, Windows installer, updater,
bridge build 13, tests, the complete added/modified file inventory, and the
order to port/release it.

### 1. Architecture: GUI + daemon, on purpose

The old single process became two:

- `honrp` with no arguments opens the **control window** (egui/eframe).
- `honrp run` is the **daemon**: game detection, Discord IPC, Lua bridge sync,
  control server, logging. It runs without a window.
- The window is a front-end. It polls the daemon over loopback; it does not own
  Discord or the bridge.
- Closing the window (X) only closes the window. The daemon keeps running and
  Rich Presence keeps working.
- **Stop Rich Presence** stops the daemon after a confirmation; **Start Rich
  Presence** spawns it again. There is no separate Quit/Close button.
- Single instance: the GUI binds `127.0.0.1:47831`; a second launch exits.
- Control protocol: `127.0.0.1:47832`, one line per command (`ping`, `status`,
  `stop`); `status` returns one line of JSON (`DaemonStatus`).
- Spawn: `CREATE_NO_WINDOW` on Windows, `process_group(0)` on Unix. The GUI and
  daemon call `FreeConsole()` after hiding an owned console, so no hidden
  terminal is left behind (Windows bug fixed late in the branch).
- Autostart: HKCU `Run` value `"<exe>" run` on Windows; systemd user service on
  Linux (`honrp install`).

### 2. Control window (`src/gui.rs`)

- **eframe/egui 0.37 with the wgpu renderer.** glow produced a blank white
  window on the Windows AMD test machine while the UI loop ran at ~2.5 fps;
  wgpu (D3D12/Vulkan) fixed rendering. Linux uses wgpu too.
- **Size:** 480×192 base; +24 px for an action hint; 480×236 while Config or a
  dialog is open. The window resizes itself with `ViewportCommand::InnerSize`.
- **Status line:** Rich Presence on/off and a Discord state, each with its own
  dot/colour:
  - `Rich Presence on` (green) / `Rich Presence off` (grey)
  - `Discord ready` (soft green; endpoint exists, no game)
  - `Live on Discord` (green; a card is being published)
  - `Can't connect to Discord` (amber; game running, IPC failed)
  - `Discord not running` (grey; no endpoint)
  - `Discord state unknown` (grey; an older daemon did not report it)
  Problem states show an inline hint (`Start Discord to show your game
  status.` / `Discord is running, but honrp can't connect. Restart Discord.`).
  Every label has a tooltip.
- **Game line:** `Waiting for Heroes of Newerth`, the current card details, or
  `In match` / `Searching for a match`; hover shows hero/level/KDA.
- **Primary action:** neutral, content-sized **Stop Rich Presence** / green
  **Start Rich Presence**. Confirmation dialog: Cancel first and focused, Escape
  cancels, Enter on the default cannot stop it, destructive button styled
  separately.
- **Secondary controls:** Config and Menu sit on the right of the action row;
  `Launch at startup` (toggle on Windows; a command dialog on Linux) sits
  below; a faint separator separates the footer.
- **Footer:** `honrp 0.3.4 · Up to date` is a link; it checks GitHub. When an
  update exists, an **Install x.y.z** button appears. Messages (stopped,
  saved, failed) reuse this line, and clear when Rich Presence starts again
  (stale-message bug fixed).
- **Config screen:** game documents folder (text field + Browse + Open) and an
  optional Discord IPC socket/pipe override. Empty means auto-detect. Save
  restarts the daemon if it is running.
- **Menu:** Help / Troubleshooting (opens `#help--troubleshooting`), Discord
  setup… (the one-time Registered Games note, reopenable), Details… (version,
  bridge build, daemon/Discord/game state, game folder, log path), Uninstall….
  External links use `ShellExecuteW` on Windows; the old `cmd /C start` failed
  silently after `FreeConsole()` invalidated the GUI's std handles.
- **Theme:** follows the system (`ThemePreference::System`), with
  theme-aware ok/warn/danger/muted colours. Light mode was checked for text
  contrast. `HONRP_THEME=light|dark` forces one for screenshots.
- **Linux-specific branch:** no Windows registry toggle; **Launch at
  startup...** opens a dialog with copyable `honrp install` / `honrp uninstall`.

### 3. Discord state (`src/discord_state.rs`, `src/ipc.rs`, `src/status.rs`, `src/control.rs`)

- `discord_state::derive()` is a pure function of
  `(daemon, connected, discord_found: Option<bool>, game_running)`; it is
  table-tested.
- `discord_found` is `Option<bool>`, so a missing field from an older daemon
  means “unknown”, not a false “not detected”. `DaemonStatus` is
  `#[serde(default)]` for compatibility.
- `ipc::available()` probes the endpoint without handshaking:
  - Windows: `WaitNamedPipeW(..., 0)` over `discord-ipc-0..9`; busy/timeout
    (`ERROR_SEM_TIMEOUT`, `ERROR_PIPE_BUSY`) counts as found.
  - Unix: candidate socket exists **and** `UnixStream::connect()` succeeds, so
    stale socket files after a crash are not reported as ready.
  - Flatpak (Stable/PTB/Canary), Snap (Stable/Canary), Vesktop and native
    Canary paths are scanned.
- The probe is throttled to every **1.5 s** and debounced (two matching probes)
  so Discord restarts do not flicker the label and Unix does not see a
  connect/close on every poll.
- The daemon publishes the cached value in `status::Snapshot`; the GUI only
  derives labels from it.

### 4. Lua bridge build 13 (`lua/honrp_mod.lua`)

- Injected through `startup.lua`; emits `[HONRP] {...}` JSON lines to the
  UTF-16LE `console.log`.
- **Build 10** survives the Windows `main.interface` rebuild (the client
  restarts Direct3D and re-executes the interface, killing timers/watches and
  reassigning `Main.*`): re-arms on `Main:Init`/`Main:OnReload`, the engine
  global `Trigger()` and interface changes.
- **Build 11** registers HUD triggers (`MatchClock`, `PlayerScore`,
  `ActiveLevel`) on the main interface object as well as the active/game
  interfaces. Build 10 only registered them on
  `UIManager:GetInterface('game')`, which is unreachable from `startup.lua`,
  so a match emitted `"hudhosts":0` and no hero level/KDA.
- **Build 12** keeps the HUD values across an in-game re-arm. Build 11 cleared
  them in `start()`; when the client re-armed on entering a match,
  `PlayerScore` did not fire again until the first score change, so a fresh
  `0/0/0` disappeared and only the level (re-fired by `ActiveLevel`) came
  back. A re-arm outside a game still clears them (no stale stats between
  matches).
- **Build 13** seeds `0/0/0` while the hero is still level 1. On a fresh match
  `PlayerScore` does not fire until the first kill/death/assist, so even
  build 12 had no KDA to preserve and Discord showed only `Slither • Lv 1`.
  The app now also shows that hero/level/KDA line visibly under the map/mode
  instead of only in the hover tooltip.
- Practice games ("Learn" → play a hero) load `fieldtest` while the lobby
  metadata still says `caldavarphoenix`; the daemon now prefers the console
  log's `Client loading world "fieldtest"` during a match, so the card reads
  **Field Test**, not FoC.
- Windows process detection uses the ToolHelp API instead of `tasklist`; the
  old subprocess flashed a console window on every poll.
- Panels: includes the real `plinko_v2` name, captures `FSPanelTrigger`, and
  verifies remembered panels with `Main:IsPanelOpen` so a stale panel is not
  reported after a match.
- Diagnostics include `env`, `uim`, `arm`, `gi`, `act`, `hudhosts`.
- Tests: `tests/test_lua_mod.lua` and `tests/test_lua_reload.lua` (run under
  Lua 5.1 via `tools/run_lua51.py`).

### 5. Windows packaging, updates, bridge sync

- **Installer** (`packaging/honrp.iss`, Inno Setup 6.2.2): per-user, lowest
  privileges, `{autopf}\honrp`, Start Menu/desktop shortcuts with an explicit
  exe icon source, optional autostart task, uninstall entry, `[UninstallRun]`
  stops the daemon and strips the managed Lua block, `[UninstallDelete]`
  removes `%APPDATA%\honrp`. Existing daemons are stopped in
  `PrepareToInstall`.
- **Installer/icon**: `assets/icon.ico` is embedded by `build.rs` +
  `winresource` (Windows-only build dependency); the installer uses
  `SetupIconFile` and `UninstallDisplayIcon`.
- **Updater** (`src/update.rs`): GitHub latest-release API, platform asset,
  SHA-256 verification against `checksums.txt`, tar extraction, self-replace
  (rename running exe to `.exe.old` + copy on Windows), `honrp update
  [--check]`, daily startup notice (`check_updates`).
- **Bridge sync** (`sync_bridge`): rewrites the managed block when the on-disk
  `startup.lua` lacks the embedded build; runs before game launch (Windows
  Documents fallback) and when the user dir first resolves.
- **Uninstall**: in-app Uninstall… with warning; installed copies run the Inno
  uninstaller after a delay, portable copies remove autostart/settings/bridge
  and tell the user to delete the folder.
- **Bug fixes in the last commit:** explicit shortcut `IconFilename`, clean
  icon resource in the exe, `FreeConsole()` after hiding (no hidden terminal),
  stop notice cleared on restart.

### 6. Branding and build

- Icon/branding pack: gold hood + eyes, lavender three-arch bridge, chat bubble,
  purple-bordered tile. Original mark; no HoN/Discord logos.
- `assets/icon.svg` (source), `assets/icon.png` (1024), `icon-256.png`,
  `icon.ico` (16–256), `icon-64.rgba` (window icon, no PNG decoder needed),
  `assets/branding/` (README banner, social preview, avatar),
  `assets/icons/hicolor/` (Linux icon hierarchy), `docs/BRANDING.md`.
- `build.rs` embeds the `.ico` via `winresource`, gated to Windows targets.
- `README.md` uses `assets/branding/readme-banner-1600x450.png`.

### 7. Configuration additions

`config.example.json` / `src/config.rs`:

- `user_dir` — manual game documents folder (Config → Game folder).
- `ipc_socket` — manual Discord IPC socket/pipe (Config → Discord); the daemon
  now honors it (`run.rs`), not just `honrp doctor`.
- `check_updates` — daily update notice.

### 8. Files added or changed since `f79baac`

| file | why |
|---|---|
| `src/gui.rs` | control window, settings, dialogs, themes, state rendering |
| `src/control.rs` | daemon status/spawn/stop + loopback control server |
| `src/status.rs` | snapshot shared between daemon and window |
| `src/discord_state.rs` | pure Discord-state derivation + table tests |
| `src/update.rs` | GitHub update check/download/verify/self-replace |
| `src/main.rs` | subcommands, uninstall, bridge sync, autostart, console |
| `src/run.rs` | daemon loop, status publishing, `ipc_socket`, bridge sync |
| `src/ipc.rs` | endpoint probing + variants; config socket support |
| `src/activity.rs` | `FoC` short map (was `FoW`), activity text |
| `src/config.rs` | `user_dir`, `ipc_socket`, `check_updates` |
| `lua/honrp_mod.lua` | bridge build 10/11/12/13 |
| `tests/test_lua_reload.lua` | regression for interface reload/HUD |
| `tests/run_all.sh` | runs both Lua harnesses (mode kept executable) |
| `build.rs` | Windows icon resource |
| `packaging/honrp.iss` | Windows installer |
| `packaging/honrp.desktop` | Linux desktop entry (for the port) |
| `assets/icon.*`, `assets/icons/hicolor/`, `assets/branding/` | branding |
| `docs/BRANDING.md`, `docs/PORTING-LINUX.md` | brand guide, port checklist |
| `.github/workflows/ci.yml`, `release.yml` | Linux/Windows builds, tests |
| `README.md`, `README-WINDOWS.txt`, `CHANGELOG.md` | documentation |

### 9. Verification

- `cargo test` — **16 tests**: activity, config/update versions, bridge sync,
  Discord-state table, etc.
- Lua harnesses: `tests/test_lua_mod.lua`, `tests/test_lua_reload.lua`.
- CI run for this branch: ✓ Lua, ✓ Linux rust build, ✓ Windows build
  (the Windows job also produces the Inno Setup installer artifact on every
  push).
- Manual: dark/light screenshots, all simulated Discord states, stop/start
  confirmation, details/settings dialogs, installer icon, no leftover conhost,
  start/stop message transition.
- **Verified on Linux:** window and wgpu rendering on Wayland/KDE and X11,
  programmatic resize to 480×264 frame for Config (480×236 inner), single
  instance, daemon spawn/adopt, Discord-ready probe, bridge sync to build 13.
- **Still unverified:** rfd portal picker, updater against a real newer
  release, keyboard activation beyond the focused Cancel (Enter/Escape relied
  on egui’s focused-button behaviour), and an actual end-to-end in-game match
  on this UI build (the bridge was verified separately in game before the UI
  work).

### 10. Linux port and release

The Linux GUI is now runtime-verified on Wayland/KDE and X11 (see above);
`docs/PORTING-LINUX.md` is the original checklist. The desktop entry and
hicolor icon tree are wired into `nfpm.yaml`, the release tarball and the Arch
PKGBUILD, so the package installs a normal app-menu entry.

Release: `Cargo.toml` is already at `0.3.4`; commit, tag `v0.3.4`, push the
tag. `.github/workflows/release.yml`
then builds the Linux tarball, `.deb`, `.rpm`, Arch package and the Windows
zip/installer. The binaries in `dist/` and the local setup are deliberately not
committed; GitHub Releases is the distribution channel.

### 11. Known limitations

- The rfd folder picker and in-app updater have not been exercised on Linux yet.
- Windows binaries are unsigned (SmartScreen warning).
- No tray icon by design.
- `honrp update` is for the standalone tarball; AUR should be updated through
  the AUR package.
- `Help` points at the GitHub README; it only shows the troubleshooting section
  after this branch is merged/pushed (it is pushed on this branch).

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
