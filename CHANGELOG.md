# Changelog

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
