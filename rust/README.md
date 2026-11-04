# honrp — Rust port

Same Lua bridge, same presence format, single static binary instead of the
Python daemon.

```bash
cargo build --release --manifest-path rust/honrp/Cargo.toml
./rust/honrp/target/release/honrp status        # what it sees
./rust/honrp/target/release/honrp run           # daemon (needs config client_id)
./rust/honrp/target/release/honrp doctor        # detection/bridge/Discord checks
./rust/honrp/target/release/honrp install       # Lua bridge + systemd user unit
```

A `config.json` in `~/.config/honrp/` (same format as the Python version) or
`HONRP_CLIENT_ID` supplies the Discord application id.

## State of the port

Implemented and tested:

- `/proc` discovery of `juvio.exe` + `STEAM_COMPAT_DATA_PATH` / `WINEPREFIX`
- user-dir detection across Steam libraries (canonicalized: symlinked
  compatdata dirs collapse to one), with `--user-dir` override
- incremental UTF-16LE `console.log` tail (BOM, partial lines, rotation),
  starting at EOF so a previous session is not replayed
- mod state parsing, match-log map/mode, match clock
- presence: ranked/unranked, `FoW`/`Mid Wars`/`Field Test`, `hero • Lv • KDA`,
  hero select, queue, panels, spectating, configurable timer/KDA/short maps
- Discord IPC: socket discovery, 8 s handshake timeout, ping/pong, 5 s send
  throttle, refresh, clear-on-exit with the game's pid + disconnect
- `status [--json]`, `doctor`, `install`/`uninstall`, `service install/status`
- 9 unit tests (activity, parsing, IPC handshake/activity over a unix socket)

Still to do before the release: musl release packaging (workflow added), a
`honrp setup` wizard, and the Python↔Rust parity pass in `docs/PLAN.md`.
