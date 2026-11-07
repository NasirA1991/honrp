# Porting the UI to Linux

This branch is the Windows-tested version of the 0.3.3 UI/daemon work. CI
compiles it on Linux and Windows, but the Linux **GUI has not been run** yet.
This file is the handoff for the Linux session.

Read `README.md` → **Implementation notes — everything changed in this branch**
first. It lists every changed file and why.

## 1. Build and test

```bash
git switch feat/compact-ui-branding
cargo test
cargo build --release
./target/release/honrp          # GUI; auto-starts the daemon
./target/release/honrp run      # headless daemon
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
lua5.1 tests/test_lua_reload.lua lua/honrp_mod.lua
```

GUI build deps (same as CI):

```bash
sudo apt install pkg-config libxkbcommon-dev libwayland-dev \
  libegl1-mesa-dev libgl1-mesa-dev
```

The GUI uses **wgpu** (Vulkan/GL) because glow produced a blank white window on
the Windows AMD machine. On Linux, verify it renders under both X11 and
Wayland; if wgpu fails on a machine, consider a renderer fallback in
`src/gui.rs` (`eframe::Renderer`) rather than replacing it blindly.

## 2. Windows-only code to review

Search `cfg(windows)` / `cfg(not(windows))`:

- `src/gui.rs` — `hide_own_console` (`FreeConsole`), autostart toggle + HKCU
  registry, Windows `.ico` resource; Linux branch shows the
  **Launch at startup...** dialog (`systemd` commands).
- `src/control.rs` — `CREATE_NO_WINDOW` on Windows vs `process_group(0)` on
  Unix for the daemon spawn.
- `src/run.rs` — Windows `Documents/Juvio/Heroes of Newerth` fallback when the
  game has never run.
- `src/ipc.rs` — named pipes vs Unix sockets, and the platform-specific
  `available()` probe.
- `src/main.rs` — registry autostart, Windows uninstall path, console hiding.
- `src/update.rs` — platform asset name and self-replace (Windows renames the
  running exe, Unix copies/renames).
- `build.rs` — `winresource` only runs on Windows (target-gated build dep).
- `packaging/honrp.iss` — Windows installer only.

Everything else (`activity`, `detect`, `matchlog`, `modstate`, `tail`,
`config`, `status`, `discord_state`) is shared.

## 3. Runtime checklist

- [ ] Window opens at 480×192; grows by 24 px for an action hint and 236 px
      for Config/dialogs. Check `ViewportCommand::InnerSize` is honoured on
      Wayland and X11 (some compositors ignore programmatic resize).
- [ ] Window/taskbar icon comes from `assets/icon-64.rgba`
      (`ViewportBuilder::with_icon`); panel/dock icon should be the hood mark.
- [ ] Single instance on `127.0.0.1:47831`; second launch exits.
- [ ] Daemon spawns detached (`process_group(0)`), control port `47832`,
      `honrp stop` works, window close leaves it running.
- [ ] Discord detection: native, Flatpak, Snap, Vesktop, Canary. Check
      `ipc::available()` (path exists + connect, stale sockets rejected) and
      the 1.5 s probe / 2-probe debounce. Labels: ready, live, not running,
      can't connect, unknown.
- [ ] Game detection through Proton/Wine prefixes; `user_dir` override in
      Config; `console.log` is UTF-16LE.
- [ ] Lua bridge: `startup.lua` managed block is installed/updated before the
      game starts; in-game hero/level/KDA after `main.interface` reload
      (see README and `docs/REVERSE_ENGINEERING.md`).
- [ ] `rfd` folder picker under XDG portals (Config → Browse).
- [ ] Help → GitHub `#help--troubleshooting` opens in the browser.
- [ ] Theme follows the system; both palettes readable (`HONRP_THEME=light`
      and `dark` force one for screenshots).
- [ ] Linux autostart dialog: **Launch at startup...** shows copyable
      `honrp install` / `honrp uninstall`.
- [ ] `Details...` shows version, bridge build, daemon/Discord/game state,
      game folder.
- [ ] `honrp doctor` still works headless.

## 4. Linux packaging to add

The repo now contains:

- `assets/icons/hicolor/{16..512}/apps/honrp.png`
- `packaging/honrp.desktop` (`Exec=honrp`, `Icon=honrp`)

Wire them into:

- `packaging/nfpm.yaml` (`.deb`/`.rpm`): install the desktop file to
  `/usr/share/applications/honrp.desktop` and the icons to
  `/usr/share/icons/hicolor/<size>/apps/honrp.png`.
- `.github/workflows/release.yml`: copy `packaging/honrp.desktop` and the
  hicolor tree into `dist/` and the Linux tarball; update the Arch PKGBUILD
  `package()` to install them.
- `install.sh` stays the headless/systemd installer; decide whether the
  standalone tarball should install the desktop entry or leave that to the
  distro package.

## 5. Release the next version

1. Bump `Cargo.toml` (`version = "0.3.4"`), `Cargo.lock`, `CHANGELOG.md` and
   any version strings. Commit.
2. Push the branch and tag:
   ```bash
   git tag v0.3.4
   git push origin feat/compact-ui-branding v0.3.4
   ```
3. `.github/workflows/release.yml` builds:
   - Linux static binary + tarball
   - `.deb` / `.rpm` (nfpm)
   - Arch package
   - Windows exe/zip + Inno setup
   and publishes them to the GitHub release.
4. Verify the release assets, then announce both platforms.

## 6. What is deliberately still missing

- Runtime verification on Linux (this is the main task).
- A desktop-entry/tray story: **no tray icon by design**.
- Signing the Windows binaries (SmartScreen still warns).
- A Linux updater path: the AUR package is the recommended update channel;
  `honrp update` is for the standalone tarball.
