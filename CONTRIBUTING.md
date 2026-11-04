# Contributing

## Layout

| path | contents |
|---|---|
| `src/` | the daemon (Rust): detection, log tail, state, presence, Discord IPC |
| `lua/honrp_mod.lua` | the Lua bridge, installed as `startup.lua`; Lua 5.1, dependency-free |
| `assets/` | Rich Presence art (see `assets/README.md`) |
| `tests/` | mocked-K2 Lua harness |
| `docs/PLAN.md` | packaging and release notes |

## Development

```bash
cargo test
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
./tests/run_all.sh          # both
cargo fmt
./target/release/honrp status --json
```

The Rust tests cover activity building, state parsing and the Discord IPC
handshake/`SET_ACTIVITY` exchange over a real unix socket. The Lua harness runs
the bridge in a mocked engine and asserts the emitted `[HONRP]` lines.

## Changing the Lua bridge

- It runs inside the game's UI Lua VM (Lua 5.1): no `os.time`/`io`, no
  coroutines beyond what the UI provides, and `pcall` around everything that
  touches the engine.
- Bump `local BUILD = N` when behaviour changes; `honrp doctor` compares it with
  the running game's boot line.
- Do not rely on timers alone: emit from engine triggers (`MatchClock`,
  `PlayerScore`, `ActiveLevel`, `LobbyGameInfo`, `FSPanelTrigger`, ...).
- Keep the managed block markers intact for clean install/uninstall.

## Pull requests

- Run `./tests/run_all.sh` and `cargo fmt --check`.
- Keep the presence payload under Discord's 128-character fields.
- New state labels go through `build_activity()` and get a Rust test.
- Describe what you tested in-game (mode, map, queue) and paste the
  `[HONRP]` line if it changed.

## Commit style

Short imperative subject, body explains the "why". One logical change per
commit.
