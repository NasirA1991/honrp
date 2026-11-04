# Contributing

## Layout

| path | contents |
|---|---|
| `honrp.py` | reference daemon (Python prototype; the Rust port will replace it) |
| `honrp_mod.lua` | Lua bridge installed as `startup.lua`; keep it Lua 5.1-compatible and dependency-free |
| `assets/` | Rich Presence art + generator (see `assets/README.md`) |
| `tests/` | Python unit/integration tests + mocked K2 Lua harness |
| `docs/PLAN.md` | Rust port and release plan |

## Development

```bash
python3 tests/test_honrp.py          # daemon: parsing, activity, Discord IPC
lua5.1 tests/test_lua_mod.lua honrp_mod.lua   # bridge under a mocked engine
./tests/run_all.sh                   # both
```

There are no third-party Python dependencies; the Lua harness needs `lua5.1`.

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

- Run `./tests/run_all.sh` and `python3 -m compileall -q .`.
- Keep the presence payload under Discord's 128-character fields.
- New state labels go through `build_activity()` and get a test.
- Describe what you tested in-game (mode, map, queue) and paste the
  `[HONRP]` line if it changed.

## Commit style

Short imperative subject, body explains the "why". One logical change per
commit.
