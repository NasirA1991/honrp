# Branch changes: `feat/compact-ui-branding` vs `main`

This document is the complete handoff for the work on
`feat/compact-ui-branding`, written so another agent can review it, port the
UI to Linux, and produce the next release without re-deriving anything.

- Base: `main` @ `f79baac` ("AUR: version the cached source filename")
- Branch head: `git log -1 --oneline` (live list: `git log main..HEAD`)
- Platform status: **Windows tested by hand; Linux compiles in CI but the GUI
  has not been run on Linux yet.** See `docs/PORTING-LINUX.md` for the port
  checklist.
- Version remains **0.3.3**; the next release (after Linux verification) is
  intended to be 0.3.4.

## 0. How to inspect the diff

```bash
git switch feat/compact-ui-branding
git log --oneline main..HEAD
git diff --stat main...HEAD
git diff main...HEAD -- src/gui.rs          # the new window
git diff main...HEAD -- src/run.rs src/ipc.rs src/detect/windows.rs
git diff main...HEAD -- lua/honrp_mod.lua
git diff main...HEAD -- packaging/ build.rs Cargo.toml
```

Everything below is the summary of that diff, plus the design decisions that
are not obvious from the code.

## 1. Architecture: GUI + daemon

`main` had one process. This branch splits it into two:

- `honrp` (no subcommand) opens the **control window** (egui/eframe).
- `honrp run` is the **daemon**: game detection, Discord IPC, Lua bridge sync,
  status publishing, control server, update notice, logging.
- The window is a front-end. It polls the daemon over loopback and never owns
  Discord or the bridge.
- Ports:
  - `127.0.0.1:47831` — GUI single-instance lock (the GUI binds it for its
    lifetime; a second GUI exits).
  - `127.0.0.1:47832` — daemon control protocol, newline commands `ping`,
    `status`, `stop`. `status` returns one JSON line (`DaemonStatus`).
- Lifecycle:
  - Opening the GUI calls `control::ensure_daemon()` (adopt a running daemon,
    otherwise spawn `honrp run`).
  - Closing the window (X) does **not** stop the daemon.
  - **Stop Rich Presence** stops the daemon after a confirmation; **Start Rich
    Presence** spawns it again.
  - There is no separate Close/Quit button.
- Spawn details:
  - Windows: `CREATE_NO_WINDOW`, plus **null stdio**. `FreeConsole()` in the
    GUI invalidated inherited std handles; without null stdio the spawn failed
    silently.
  - Unix: `process_group(0)`.
- Console: GUI and daemon call `ShowWindow(SW_HIDE)` + `FreeConsole()` when
  they own the console (launched from Explorer/Start Menu), so no hidden
  terminal remains.
- Autostart: HKCU `Run` value `"<exe>" run` on Windows; systemd user service
  via `honrp install` on Linux.

## 2. UI specification (`src/gui.rs`)

eframe/egui **0.37**, renderer **wgpu** (D3D12/Vulkan). glow rendered a blank
white window on the Windows AMD test machine while the UI loop ran, so wgpu is
hard-coded in `NativeOptions`.

### Window geometry

- Width 480.
- Base height **192**.
- `+24` when an **action hint** is visible (Discord problem states).
- `+24` when the **hero/level/KDA state line** is visible (in game).
- **236** while Config or any dialog/modal is open.
- Resized programmatically with `ViewportCommand::InnerSize`; not user
  resizable. `with_min_inner_size([480, 192])`.
- Title `honrp`, window icon from `assets/icon-64.rgba` (`ViewportBuilder::
  with_icon`).

### Top status line

Two independent indicators, dot + colored text (not pills):

| label | color | meaning |
|---|---|---|
| `Rich Presence on` | green | daemon alive |
| `Rich Presence off` | grey | daemon stopped |
| `Discord ready` | soft green | IPC endpoint exists, no game activity |
| `Live on Discord` | green | a valid card is being published |
| `Can't connect to Discord` | amber | game running, endpoint exists, connection failed |
| `Discord not running` | grey | no IPC endpoint found |
| `Discord state unknown` | grey | older daemon did not report availability |
| `Discord off` | grey | Rich Presence itself is off |

Every label has a tooltip. `Rich Presence on/off` is the daemon toggle;
“Live on Discord” is publishing. Problem states add a one-line hint below the
status:

- not running while game runs → `Start Discord to show your game status.`
- can't connect → `Discord is running, but honrp can't connect. Restart Discord.`
- unknown → `Restart honrp to refresh the Discord state.`

### Game area

- Big line: `Waiting for Heroes of Newerth`, or the activity details
  (`FoC • All Pick`, `Field Test • All Pick`, `In match`, ...).
- While **in game**, a second 13 px muted line shows the hero/level/KDA state
  (`Slither • Lv 1 • 0/0/0`). It used to be tooltip-only; it is now visible.
- Hovering the big line also shows the state tooltip.

### Primary action

- Content-sized (not full-width) neutral **Stop Rich Presence**, green
  **Start Rich Presence** when off. Height 32.
- Row also contains **Config** and **Menu** right-aligned.
- Stop opens a confirmation: Cancel first and focused; Escape cancels; Enter on
  the default cannot stop it; destructive button styled red separately.
- Below: hint `Closing this window keeps Rich Presence running in the
  background.` (or “Rich Presence is off…” when stopped).
- **Launch at startup** row:
  - Windows: toggle writing HKCU Run.
  - Linux: `Launch at startup...` button opening a dialog with copyable
    `honrp install` / `honrp uninstall`.
- Faint separator (muted color at alpha 46) above the footer, 10 px above / 3 px
  below.

### Footer

- `honrp 0.3.3 · Up to date` is a **link**; clicking it checks GitHub.
- When an update is known, an **Install x.y.z** button appears next to it.
- Messages (saved/stopped/failed) reuse this line. The stop message clears when
  the daemon transitions from stopped to running.
- `bridge 13` is no longer in the footer; it lives in Menu/Details.

### Menu (`...` is now **Menu**)

- **Help / Troubleshooting** → `https://github.com/NasirA1991/honrp#help--troubleshooting`
  via `ShellExecuteW` on Windows (the old `cmd /C start` failed after
  `FreeConsole()`).
- **Discord setup...** → one-time Registered Games notice, reopenable.
- **Details...** → version, bridge build, daemon/Discord/game state, game
  folder, log path.
- **Uninstall...** → warning; installed copies run the Inno uninstaller after a
  delay, portable copies remove bridge/settings/autostart and ask the user to
  delete the folder.
- Version/bridge small label at the bottom.

### Config screen (not a modal)

- Game documents folder text field + **Browse...** (rfd) + **Open folder**.
- Optional **Discord IPC socket** override; empty = auto-detect. honrp needs no
  Discord install path; it connects over local IPC.
- Save writes `user_dir` and `ipc_socket` to config.json; restarts the daemon
  if it is running.

### Theme

- `ThemePreference::System`; `HONRP_THEME=light|dark` forces one for
  screenshots.
- Palette in `gui::palette(dark)`:
  - ok / ok_soft / warn / danger_button / muted, tuned separately for light
    and dark. Muted is deliberately high-contrast (`#c2c2c2` dark,
    `#3c3f44` light).
- Toggle switch is custom-painted (not a checkbox) for visibility in both
  themes.
- Help/footer text sizes: 12 px; game state 13 px; status 14 px; game title
  16 px.

### Dialogs

- Discord setup: explains duplicate activity, real `→`, one-time + reopenable.
- Stop: see above.
- Details: grid; game folder shortened, full path on hover.
- Uninstall: warning; installed vs portable paths.
- Linux autostart: copyable commands.
- First-run Discord notice persisted at `%APPDATA%\honrp\.discord_notice_shown`.

## 3. New/changed Rust modules

### `src/gui.rs` (new)

The whole window. Key items:

- `HELP_URL`, `WINDOW_W/MAIN_BASE/EXTRA_ROW/OVERLAY_H`.
- `palette()`, `toggle_row()`, `faint_separator()`.
- `run()`: single instance, hide/free console, start poller, ensure daemon,
  `eframe::run_native`.
- `HonrpApp`: settings/dialog state, `window_h`, update state, message.
- `current_status()`: daemon status; `HONRP_DISCORD_STATE=...` dev override for
  screenshots (`ready|live|not-running|unavailable|unknown|off`).
- `check_updates` / `install_update`: background GitHub check.
- `shell_open()` on Windows (`ShellExecuteW`), `open_path`, `open_url`.
- `spawn_delayed_uninstall`, `uninstaller_path`.
- `autostart_enabled` / `set_autostart` (HKCU).
- `single_instance()` binds 47831.
- `hide_own_console()` hides + `FreeConsole()`.

### `src/control.rs` (new)

- `DaemonStatus` (serde, `#[serde(default)]`), `PORT = 47832`.
- `running()`, `status()`, `stop()`, `serve()`, `latest()`, `start_poller()`.
- `serve()` retries a TIME_WAIT bind for up to 2 min.
- `spawn_daemon()` with null stdio / CREATE_NO_WINDOW / process group.
- `ensure_daemon()` waits up to 15 s.

### `src/status.rs` (new)

`Snapshot` + global `Mutex`; daemon publishes, GUI reads. Fields include
`daemon`, `discord`, `discord_found: Option<bool>`, game pid/state/details,
`user_dir`, `update_text`.

### `src/discord_state.rs` (new)

Pure state derivation + tests. `derive(daemon, connected, found: Option<bool>,
game_running)`:

| daemon | connected | found | game | state |
|---|---|---|---|---|
| false | – | – | – | Off |
| true | true | – | – | Live |
| true | false | None | – | Unknown |
| true | false | false | – | NotRunning |
| true | false | true | false | Ready |
| true | false | true | true | Unavailable |

`needs_attention()` drives the inline hints.

### `src/update.rs` (new)

GitHub latest-release check, platform asset, SHA-256 against `checksums.txt`,
tar extraction, self-replace (Windows rename `.exe.old` + copy; Unix
copy/chmod/rename), `notify_if_newer`, `run(check_only)`. Uses `curl`/`tar`
with `CREATE_NO_WINDOW` on Windows.

### `src/run.rs` (changed)

- Publishes `status::Snapshot` every loop, including `discord_found` with a
  1.5 s probe + two-probe debounce.
- Honors `ipc_socket` from config as well as CLI.
- Syncs the managed Lua bridge on start and when `user_dir` first resolves.
- Prefers `fallback_map == "fieldtest"` while in a match: practice games load
  Field Test even though lobby metadata says `caldavarphoenix`.

### `src/ipc.rs` (changed)

- `available(override)` probe:
  - Windows `WaitNamedPipeW(..., 0)` over `discord-ipc-0..9`; busy/timeout
    (`ERROR_SEM_TIMEOUT`/`ERROR_PIPE_BUSY`) counts as found.
  - Unix: socket path exists + `UnixStream::connect()` succeeds (stale sockets
    rejected).
- Flatpak Stable/PTB/Canary, Snap Stable/Canary, Vesktop and native Canary
  paths added.

### `src/main.rs` (changed)

- New subcommands/lifecycle: `run`, `stop`, `status`, `doctor`, `install`,
  `uninstall`, `update`, `service` (systemd), GUI when no subcommand.
- GUI feature fallback help when built `--no-default-features`.
- `sync_bridge`, uninstall paths, HKCU autostart removal, console hide/free.
- `--minimized` kept hidden/ignored for old shortcuts.

### `src/config.rs` (changed)

New fields: `user_dir`, `ipc_socket`, `check_updates`.

### `src/activity.rs` (changed)

- Short map `FoC` (was `FoW`), full `Forests of Caldavar (Phoenix)`.
- `Field Test` mapping for `fieldtest`.
- Mode/rank/state formatting; tests updated.

### `src/detect/windows.rs` (changed)

- Process detection via **ToolHelp** (`CreateToolhelp32Snapshot` /
  `Process32FirstW` / `Process32NextW`), not `tasklist`.
- The old `tasklist` subprocess flashed a console every poll; ToolHelp is
  in-process and silent.

### `build.rs` (new)

- `winresource` (Windows-only build dependency) embeds `assets/icon.ico` into
  the exe. If the resource compiler is missing it logs a warning instead of
  failing the build. `cargo:rerun-if-changed=assets/icon.ico`.

### `Cargo.toml` / `Cargo.lock`

- `eframe` (wgpu-only features), `rfd`, `sha2`.
- Default feature `gui`; `gui = ["dep:eframe", "dep:rfd"]`.
- `winresource` under `[target.'cfg(windows)'.build-dependencies]`.
- windows-sys features: Foundation, System_Console, System_Pipes,
  System_Diagnostics_ToolHelp, UI_Shell, UI_WindowsAndMessaging.

## 4. Lua bridge (`lua/honrp_mod.lua`, build 13)

Injected via `startup.lua`; emits `[HONRP] {...}` JSON lines to UTF-16LE
`console.log`.

History relevant to the port:

- **Build 10** survives the Windows `main.interface` rebuild (D3D restart).
  The client re-executes the interface, destroys timers/watches and reassigns
  `Main.*`; the bridge re-arms on `Main:Init`/`OnReload`, the engine global
  `Trigger()` and interface changes.
- **Build 11** registers HUD triggers (`MatchClock`, `PlayerScore`,
  `ActiveLevel`) on the main interface object as well as active/game names.
  Build 10 only used `UIManager:GetInterface('game')`, unreachable from
  `startup.lua` (no level/KDA).
- **Build 12** preserves HUD values across an in-game re-arm. Build 11 cleared
  them; `PlayerScore` may not fire again until the next score change.
- **Build 13** seeds `0/0/0` while the hero is level 1. On a fresh match
  `PlayerScore` never fires at 0/0/0, so build 12 had nothing to preserve.
  The immediate emit after start still clears the values outside a game.

Panels: real `plinko_v2`, `FSPanelTrigger` capture, `Main:IsPanelOpen`
verification. Diagnostics: `env`, `uim`, `arm`, `gi`, `act`, `hudhosts`.
Tests: `tests/test_lua_mod.lua`, `tests/test_lua_reload.lua` (re-arm, HUD
registration, level-1 seed, KDA preservation, menu clearing).

## 5. Windows packaging

- `packaging/honrp.iss` (Inno Setup 6.2.2):
  - per-user install `{autopf}\honrp`, `PrivilegesRequired=lowest`,
    no admin.
  - Start Menu + optional desktop shortcut; shortcuts set
    `IconFilename: "{app}\honrp.exe"; IconIndex: 0` explicitly.
  - optional autostart task (`HKCU` Run `"<exe>" run`).
  - `SetupIconFile=..\assets\icon.ico`,
    `UninstallDisplayIcon={app}\honrp.exe`.
  - `PrepareToInstall` stops an existing daemon; `[UninstallRun]` stops it and
    strips the Lua bridge; `[UninstallDelete] {userappdata}\honrp`.
- `build.rs` embeds the exe icon; the installer, exe and window all use the
  branding pack icon.
- Local build:
  ```powershell
  cargo build --release
  ISCC.exe /DAppVersion=0.3.3 packaging\honrp.iss
  ```
  If the icon resource ever looks stale after replacing `assets/icon.ico`,
  delete `target\release\build\honrp-*` (or `cargo clean -p honrp`) and
  rebuild; copying an older-timestamped icon can otherwise skip the resource.
- Updater: `honrp update [--check]`, daily notice; checks GitHub API, verifies
  SHA-256, extracts, self-replaces. No signing; SmartScreen may warn.

## 6. Branding/assets

From the branding pack:

- `assets/icon.svg` (source), `assets/icon.png` (1024), `icon-256.png`,
  `icon.ico` (16–256), `icon-64.rgba` (window icon).
- `assets/branding/readme-banner-1600x450.png`, `social-preview-1280x640.png`,
  `avatar-512.png`.
- `assets/icons/hicolor/{16..512}/apps/honrp.png` (Linux).
- `packaging/honrp.desktop` (`Exec=honrp`, `Icon=honrp`).
- `docs/BRANDING.md` (colors/usage).
- README uses the banner; the social preview/avatar are for GitHub repo
  settings (not settable from files).

## 7. Tests / CI

- `cargo test`: **15 tests** (activity, update versions, bridge sync,
  Discord-state table, etc.).
- Lua: `lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua` and
  `lua5.1 tests/test_lua_reload.lua lua/honrp_mod.lua`; `tests/run_all.sh`
  runs both.
- `.github/workflows/ci.yml`:
  - `lua` job runs both Lua harnesses.
  - `rust` job (Ubuntu) installs GUI deps, `cargo test` + release build.
  - `windows` job builds release + zip **and the Inno Setup installer** on
    every push (artifact `honrp-windows-x86_64-ci` contains both).
- `.github/workflows/release.yml` on `v*` tags builds:
  - Linux static binary + tarball,
  - `.deb`/`.rpm` via nfpm, Arch package in an Arch container,
  - Windows exe/zip + Inno setup,
  - `checksums.txt`, publishes a GitHub release.

## 8. File inventory

Added:

| file | purpose |
|---|---|
| `src/gui.rs` | control window |
| `src/control.rs` | daemon control protocol / spawn / poller |
| `src/status.rs` | snapshot shared daemon→GUI |
| `src/discord_state.rs` | pure Discord state derivation + tests |
| `src/update.rs` | GitHub updater |
| `build.rs` | Windows icon resource |
| `packaging/honrp.iss` | Windows installer |
| `packaging/honrp.desktop` | Linux desktop entry |
| `tests/test_lua_reload.lua` | re-arm/HUD/KDA regression |
| `assets/icon.*`, `assets/icons/`, `assets/branding/` | branding |
| `docs/BRANDING.md`, `docs/PORTING-LINUX.md` | docs |

Modified:

| file | change |
|---|---|
| `src/main.rs` | subcommands, GUI entry, uninstall, autostart, console |
| `src/run.rs` | status publishing, ipc_socket, fieldtest, bridge sync |
| `src/ipc.rs` | endpoint probe + variants |
| `src/config.rs` | user_dir / ipc_socket / check_updates |
| `src/activity.rs` | FoC/Field Test labels |
| `src/detect/windows.rs` | ToolHelp process detection |
| `lua/honrp_mod.lua` | bridge 10–13 |
| `Cargo.toml`/`Cargo.lock` | eframe/rfd/sha2/winresource/features |
| `tests/run_all.sh` | runs the reload harness |
| workflows/README/CHANGELOG/docs | docs/build changes |

## 9. Linux porting (next agent)

Read `docs/PORTING-LINUX.md`. Summary:

1. `cargo test`, `cargo build --release`; run the GUI on X11 and Wayland.
2. Check wgpu rendering, 480×192 resize (+24 rows, 236 modals), icon in
   panel/dock, single instance, daemon spawn/detach, control port.
3. Verify Discord detection (native/Flatpak/Snap/Vesktop/Canary), 1.5 s probe
   and debounce, stale sockets.
4. Verify Proton game detection, `user_dir`, UTF-16 console.log, bridge
   install/update, panels, hero/level/KDA, Field Test practice.
5. Verify rfd portal picker, Help `xdg-open`, theme, autostart dialog.
6. Wire `packaging/honrp.desktop` and `assets/icons/hicolor/` into
   `nfpm.yaml`, the release tarball and the Arch PKGBUILD.
7. Bump `Cargo.toml` to `0.3.4`, update CHANGELOG, tag `v0.3.4`, push; the
   release workflow produces both platforms.

## 10. Known caveats

- Linux GUI runtime is untested.
- Windows binaries are unsigned.
- The updater has not been exercised against a real newer release.
- The Help anchor (`#help--troubleshooting`) only exists on this branch until
  it is merged to `main`; before that the link lands on the repo front page.
- The installed exe from the 11:46 build is older than these fixes; reinstall
  from `dist/honrp-setup-*.exe` to test.
- No tray icon by design.
