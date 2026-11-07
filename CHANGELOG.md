# Changelog

## 0.3.3 — 2026-11-07

- Added `docs/BRANCH-CHANGES.md`: a standalone handoff describing every change
  on this branch (UI spec, state table, daemon protocol, installer, updater,
  bridge 13, tests, file inventory, Linux port order).
- Help / Troubleshooting, Open folder and other external links now use
  `ShellExecuteW` instead of spawning `cmd.exe`/`explorer.exe`. After the GUI
  called `FreeConsole()`, the inherited std handles were invalid and the old
  `cmd /C start` silently did nothing.
- Practice games now report **Field Test**, not `FoC • All Pick`: the lobby
  metadata still says `caldavarphoenix`, but the console log's
  `Client loading world "fieldtest"` is authoritative during a match.
- Windows game detection no longer shells out to `tasklist` (which flashed a
  console window every poll, and `CREATE_NO_WINDOW` did not reliably suppress
  it). It enumerates processes with the ToolHelp API instead.
- Starting Rich Presence after stopping it is reliable: the daemon is spawned
  with null stdio (FreeConsole invalidated the GUI's inherited handles) and the
  control port retries a TIME_WAIT bind for up to two minutes.
- Lua bridge build 13: seed `0/0/0` while the hero is level 1. On a fresh
  match `PlayerScore` does not fire until the first kill/death/assist, so even
  build 12 had no KDA to preserve and the card read only `Slither • Lv 1`.
- The app now shows the hero/level/KDA line visibly under the map/mode while
  in game (it was previously only in the hover tooltip).
- Lua bridge build 12: keep hero level and KDA across an in-game interface
  re-arm. Build 11 cleared the HUD values in `start()`; PlayerScore does not
  fire again until the next score change, so a fresh `0/0/0` vanished and only
  the level returned. The immediate emit after `start()` still clears the
  values outside a game, so no stale stats leak between matches.
- Lua bridge build 11: in-game hero level and KDA on Windows. Build 10 only
  registered the HUD triggers (`MatchClock`, `PlayerScore`, `ActiveLevel`) on
  `UIManager:GetInterface('game')`, which is not reachable from the
  `startup.lua` environment, so a whole match emitted `"hudhosts":0`. The
  triggers are now registered on the `main` interface object as well — that is
  the registration build 9 used on Linux (the engine's watch registry is keyed
  by trigger name, not by interface) — plus the active interface and known
  game interface names.
- Lua bridge build 10: survive the client's `main.interface` rebuild. On
  Windows the client restarts Direct3D to apply the saved video mode shortly
  after launch and re-executes `ui/fe3/main.interface`; that destroys the
  widget the bridge's timer hung on and reassigns every `Main.*` function, so
  build 9 emitted four lines and then stayed silent for the rest of the
  session (main menu was still shown from the process fallback, but Learn,
  Store, Plinko, lobby, hero, level and KDA were all missing). The bridge now
  re-arms on `Main:Init`/`Main:OnReload`, on the engine's global `Trigger()`
  (which is not reassigned by the rebuild) and whenever the `main` interface
  object changes, re-registering watches and HUD triggers on the new
  interface.
- The bridge reads `iface_env` correctly now: build 9 declared it after the
  closures that used it, so they read a global that was always nil
  (`"env":0` in every line) and never consulted `Lobby`/`Matchmaking`.
- Panel detection uses the real panel name `plinko_v2`, captures
  `FSPanelTrigger` from the trigger call itself and verifies a remembered
  panel with `Main:IsPanelOpen`, so a stale panel is not reported after a
  match.
- `honrp run` now installs the bundled Lua bridge when the file on disk
  differs from the binary's copy. Updating honrp is enough — no second
  `honrp install` after a release, and Lua fixes only need one game restart.
  It also does this before the game has ever run (Windows falls back to
  `Documents\Juvio\Heroes of Newerth`) and again when the user dir first
  becomes known, so a daemon started at boot still prepares the bridge.
- New `honrp update` and `honrp update --check`: reads the latest GitHub
  release, downloads the platform asset with `curl`, verifies it against the
  release `checksums.txt` (SHA-256), unpacks it with `tar` and swaps the
  running binary. `honrp run` checks once a day and prints a notice (disable
  with `"check_updates": false` in the config).
- `honrp status` reports the installed bridge build next to the build the
  binary ships and says when they differ.
- Windows control window: double-clicking `honrp.exe` (no arguments) opens a
  small native window instead of a console. It shows daemon/Discord status,
  the game process and the card being sent, the version and installed bridge
  build, and has "Check for updates", "Open config" and "Start honrp when
  Windows starts" controls. The daemon runs inside the window process; a
  single-instance mutex focuses the existing window instead of starting a
  second daemon.
- Windows installer (`honrp-setup-<version>.exe`, Inno Setup): per-user, no
  admin prompt, installs to `%LOCALAPPDATA%\Programs\honrp`, optional desktop
  shortcut and autostart, and an Add/Remove Programs entry. Uninstalling also
  strips the managed Lua-bridge block and removes `%APPDATA%\honrp`.
- New branding pack: app icon (gold hood/eyes, lavender bridge, chat bubble on
  a purple-bordered tile), README banner, GitHub social preview and avatar.
  The Windows `.ico` (16–256 px) is embedded in the exe, window and installer;
  SVG/PNG sources and `docs/BRANDING.md` live in `assets/`. Original mark, no
  Heroes of Newerth or Discord logos.
- The control window is a compact 480x192 panel (grows by 24 px for an action
  hint, 480x236 while Config or a dialog is open): Rich Presence and Discord
  state with separate colours, the card line (hero/level/KDA on hover), a
  neutral **Stop Rich Presence / Start Rich Presence** button with Config and
  Menu beside it, a neutral **Launch at startup** row, a faint separator above
  the clickable update-status footer. The in-game card preview was removed:
  the card lives in Discord, and duplicating it made the panel feel cluttered.
- The footer link checks for updates and **Install x.y.z** appears when a newer
  release is known; the separate Check for updates button is gone.
- **Config** opens a settings screen where the game documents folder can be
  entered/browsed and an optional Discord IPC socket override set. honrp needs
  no Discord install path; it connects over Discord's local IPC.
- **Help** opens the README's `Help / Troubleshooting` section in the browser,
  with collapsible Windows and Linux lists of the common problems.
- The window follows the system light/dark theme, with palette-aware status
  colours and readable text in both.
- There is no separate **Close** or **Quit** button: X closes the window only
  (Rich Presence keeps running), and the primary button is the only stop/start
  control.
- The window no longer shows the card preview (it is in Discord already; the
  state is available as a tooltip on the game line).
- The control window and the daemon are separate processes: closing the
  window leaves the daemon (and Discord presence) running; the big button
  stops it after a warning that RPC stays off until it is started again.
  Opening the window adopts a running daemon or starts one automatically;
  `honrp stop` stops it from the CLI; the autostart entry starts the daemon
  only (no window), and the installer stops an old daemon before upgrading.
- The control window is cross-platform (egui/eframe): the same window runs on
  Windows and Linux. The daemon still runs headless with `honrp run`; build
  with `--no-default-features` for a GUI-less binary.
- First Windows start shows a one-time explanation of the Discord
  "juvio.exe / Heroes of Newerth" Registered Games conflict; the permanent
  Help menu links to the troubleshooting notes.
- The main window shrank to 480x192 with a compact-width Stop/Start button and
  a faint separator above the footer (it grows to 480x236 only while Config or
  a dialog is open), removing the dead space between the startup row and the
  footer.
- Discord state is derived from the real IPC endpoint instead of the connection
  alone: **Discord ready** (endpoint exists, no game), **Live on Discord**
  (publishing), **Can't connect to Discord** (endpoint exists but the
  connection failed while a game runs), **Discord not running** (no endpoint)
  and **Discord state unknown** (older daemon did not report it; `Option<bool>`
  instead of a misleading `false`). Problem states also show an inline action
  hint, and tooltips explain each.
- The endpoint probe is throttled (1.5 s) and debounced (two matching probes)
  so Discord restarts do not flicker the label and Unix does not see a
  connect/close every poll; stale sockets are rejected by connecting, and
  Windows busy-pipe errors (`ERROR_SEM_TIMEOUT`/`ERROR_PIPE_BUSY`) count as
  found. Flatpak/Snap/Canary/Vesktop socket locations were added.
- The Stop/Start control is content-sized, the status text is slightly larger,
  the Linux autostart row opens a dialog with copyable `honrp install` /
  `honrp uninstall` commands, and the footer sits in the normal layout flow so
  the vertical rhythm is consistent.
- UI review pass: clearer Discord setup copy ("duplicate activity" instead of
  "Two cards?", real arrow, reopenable from the menu), truthful status
  distinctions (Rich Presence on/off, Discord ready/live/not running/unknown,
  "Waiting for Heroes of Newerth" in normal text), a safer Stop dialog (Cancel
  first and focused, Enter on the default cannot stop Rich Presence, Escape
  cancels, destructive button styled separately), a neutral Stop Rich Presence
  button, the closing hint on the main screen, a condensed footer, bridge build
  moved to Details, a neutral **Launch at startup** row, tooltips on every
  status, and a Details dialog (version, bridge, states, game folder, log
  path).
- Linux port preparation: `packaging/honrp.desktop`, the hicolor icon set in
  `assets/icons/`, a `docs/PORTING-LINUX.md` handoff checklist, and a detailed
  "Implementation notes" section in the README covering everything changed on
  this branch.
- Windows fixes: the Start Menu/desktop shortcuts explicitly use the exe as
  their icon source (and the installer is built from a clean icon resource), the
  GUI and daemon detach/destroy their console instead of leaving a hidden
  terminal behind, and the "Rich Presence stopped" footer message clears as
  soon as Rich Presence is started again.
- The control window has an **Uninstall...** action with a warning. On an
  installed copy it stops the daemon and runs the Inno uninstaller (program,
  shortcuts, autostart, settings, bridge); for the portable zip it removes the
  autostart entry, settings and the Lua bridge, and tells you to delete the
  folder.
- The window renders with wgpu (D3D12/Vulkan) instead of OpenGL: on an
  AMD/Windows setup the glow renderer created a blank white window while the
  UI loop kept running. `honrp run` is unaffected (headless).
- `honrp` with no arguments now opens the control window on Windows; `honrp
  run` remains the headless daemon for scripts/systemd.
- The short map label for Forests of Caldavar is `FoC` (it was `FoW`).
- New Lua regression test `tests/test_lua_reload.lua` (fails on build 9,
  covers the rebuild and the missing HUD registration); CI runs it.
- Windows CI uploads the portable zip as a build artifact, so a branch build
  can be downloaded and tested without cutting a release.

## 0.3.2 — 2026-11-06

- Windows `install` works before the first game launch: if
  `Documents\Juvio\Heroes of Newerth` does not exist yet it is created (or
  pass `--user-dir` explicitly).

## 0.3.1 — 2026-11-06

- Windows zip is built with the static MSVC CRT, so it no longer needs
  `VCRUNTIME140.dll` / the VC++ redistributable on the target machine.

## 0.3.0 — 2026-11-06

- Windows alpha port (portable, no installer, no autostart yet):
  - process detection via `tasklist`, Documents/Known Folder paths
  - Discord named-pipe IPC (`\\.\pipe\discord-ipc-N`)
  - config in `%APPDATA%`, UTF-16LE log tail, cfg-gated file rotation
  - `honrp.exe install` writes the Lua bridge only; `honrp.exe run` starts it
  - Windows CI test/build job and a `windows-x86_64.zip` release asset
- Not code-signed; SmartScreen bypass instructions are in README-WINDOWS.txt.

## 0.2.7 — 2026-11-05

Rust fixes from an external review of the old Python prototype (the review
covered code that no longer exists; these points did carry over):

- Spectate detection no longer false-positives when `GetLocalHeroName()` is
  empty in a local/practice game: same local/view client now means playing.
- The daemon re-resolves the prefix/user dir when the game process changes,
  instead of keeping the previous prefix's `console.log`.
- Discord socket discovery also checks Vesktop and Snap Discord runtime paths.
- Complete `config.example.json`; Lua harness no longer assigns a stray global.
- The leftover Python working copy was deleted; the repo and releases are
  Rust-only.

## 0.2.6 — 2026-11-04

- Non-systemd support: `honrp install` falls back to
  `~/.config/autostart/honrp.desktop` when no systemd user session is available.
- Installer prints the Discord "Registered Games" hint for the competing
  `juvio.exe` detection.
- README: uninstall section, performance numbers, non-systemd note, Discord
  detection troubleshooting.

## 0.2.5 — 2026-11-04

- Release workflow fix: include the `tar.gz` and a `checksums.txt` covering all
  assets in the published release.

## 0.2.4 — 2026-11-04

- Release artifacts are now built entirely by GitHub Actions: tarball, static
  binary, `.deb`, `.rpm`, Arch `.pkg.tar.zst`, checksums and installer.
- AUR packaging stays source-based; `makepkg` is only used in CI (Arch
  container), not on the maintainer's machine.

## 0.2.3 — 2026-11-04

- Launcher-agnostic prefix detection: the running process's `WINEPREFIX`,
  `STEAM_COMPAT_DATA_PATH` or `/proc/<pid>/maps` `drive_c` path (works with
  Steam, Lutris, Heroic, Faugus, Bottles, plain Wine, custom scripts).
- When HoN isn't running, scan Steam (every compatdata prefix), Lutris and
  Heroic configs, Bottles, `~/Games`, `~/.wine`, `$WINEPREFIX`, and a bounded
  home-directory scan for any `drive_c/users/.../Documents/Juvio`.
- No fixed Steam app id anywhere (non-Steam shortcuts get random ids).

## 0.2.2 — 2026-11-04

- Default `large_image` is `hon-transparent`, matching the verified asset key
  for the built-in Discord application.

## 0.2.1 — 2026-11-04

- Detection no longer assumes a fixed Steam app id. Non-Steam shortcuts get a
  random id, so all `steamapps/compatdata/*/pfx` prefixes are scanned when the
  game isn't running; the running process's `STEAM_COMPAT_DATA_PATH` remains the
  exact source.
- Built-in Discord application id: no per-user Discord app required. Override
  with `client_id` / `HONRP_CLIENT_ID` / `honrp install --client-id`.
- Quick installer no longer prompts.

## 0.2.0 — 2026-11-04

- Discord Rich Presence for Heroes of Newerth (Juvio) on Linux/Steam Proton.
- Static Rust daemon: `/proc` + Proton prefix detection, UTF-16LE log tail,
  match/queue/lobby/hero-select/panel state, hero level and KDA.
- Discord IPC with clear-on-exit, send throttling and reconnect; the daemon is
  connected only while the game runs.
- Lua bridge (build 9) loaded through the client's `startup.lua`.
- Commands: `install`, `uninstall`, `status`, `doctor`, `run`, `service`.
- Configurable labels: `FoW` short map, timer off by default, optional KDA.
- Tests: 9 Rust unit tests + mocked-K2 Lua harness.
- `install.sh` downloads the release binary and sets up the user service.
