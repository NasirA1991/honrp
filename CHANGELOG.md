# Changelog

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
