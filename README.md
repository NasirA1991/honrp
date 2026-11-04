# honrp

[![CI](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml/badge.svg)](https://github.com/NasirA1991/honrp/actions/workflows/ci.yml)

Discord Rich Presence for **Heroes of Newerth** (the Juvio client, 0.13.x)
running through Steam Play / Proton on Linux.

It shows the game state on your Discord profile — not just "playing HoN":

| state | card |
|---|---|
| Main menu | `Main Menu` · account |
| Searching | `Searching for a match` · `In queue` |
| In a match | `Ranked • FoW • Banning Pick` · `Pyromancer • Lv 12 • 4/2/7` |
| Hero select | `Ranked • FoW • All Pick` · `Hero Select` |
| Spectating | `Spectating • FoW` · `Watching Master Of Arms` |
| Plinko | `Playing Plinko` · account |

## Quick install

```bash
curl -fsSL https://github.com/NasirA1991/honrp/releases/latest/download/install.sh | bash
```

That downloads the static binary to `~/.local/bin/honrp`, installs the Lua
bridge into your Proton prefix, and enables a systemd **user** service — no
root, no files touched in the game install directory. The card uses honrp's
built-in Discord application, so there is no Discord app to create. Then
restart Heroes of Newerth.

<details>
<summary>Manual / from source</summary>

```bash
# release binary
curl -fLO https://github.com/NasirA1991/honrp/releases/latest/download/honrp-linux-x86_64
chmod +x honrp-linux-x86_64 && ./honrp-linux-x86_64 install

# or build it
cargo build --release
install -Dm755 target/release/honrp ~/.local/bin/honrp
honrp install
```
</details>

## How it finds HoN

No hardcoded paths. `honrp install` and the daemon resolve the prefix in this
order:

1. **Running game (exact)** — scan `/proc` for `juvio.exe`, read its
   environment:
   - `STEAM_COMPAT_DATA_PATH` → the exact Proton prefix
     (`.../steamapps/compatdata/<appid>/pfx`),
   - `WINEPREFIX` as a fallback.

   This works for any Steam app id, including **non-Steam shortcuts** (Steam
   assigns those a random id), and is independent of where HoN lives or how
   many stale prefixes exist.
2. **Not running (best effort)** — scan:
   - the default Steam roots and every library in `libraryfolders.vdf`, looking
     inside **every** `steamapps/compatdata/*/pfx` (not a fixed app id); the
     Flatpak Steam path is included;
   - `~/.wine`, `$WINEPREFIX` and Lutris-style `~/Games/*` prefixes.

   Inside each prefix it looks at `drive_c/users/*/Documents/Juvio/Heroes of
   Newerth` (the Windows username can be anything), canonicalizes symlinks so
   duplicate library paths collapse to one, and picks the user dir with the
   newest `console.log`.
3. **Override when needed**:

   ```bash
   honrp install --prefix /path/to/pfx
   # or point straight at the docs dir:
   honrp install --user-dir "/path/to/pfx/drive_c/users/<user>/Documents/Juvio/Heroes of Newerth"
   ```

`honrp doctor` prints every candidate prefix/user dir it found, the
`console.log` age, and which one it picked — paste that into an issue if
detection fails.

There is no fixed Steam app id to rely on: a non-Steam shortcut gets a random
one, and a manually installed client has no `compatdata` entry at all. That's
why the running-process environment is preferred whenever it's available, and
why every `compatdata` prefix is scanned otherwise. If neither finds the game,
use `--prefix` / `--user-dir` (or start HoN once so the prefix exists).

## Discord app

honrp ships with a **built-in Discord application id**, so users don't need to
create their own Discord app. The card's name and icon come from that app.

If you want a custom name or artwork:

1. Create an application at <https://discord.com/developers/applications> and
   copy its **Application ID** (this id is public, not a token/secret).
2. Put it in `~/.config/honrp/config.json`:

   ```json
   {"client_id": "123456789012345678"}
   ```

   or run `honrp install --client-id <ID>`.
3. Optional: upload an image under **Rich Presence → Art Assets** with the
   name `hon-transparent` (or set `large_image` to whatever name you use); no
   artwork is shipped in this repo.

The built-in id is only a default: `client_id` / `HONRP_CLIENT_ID` overrides it.

## After install

Restart Heroes of Newerth once — the engine reads the bridge at startup. Then:

```bash
honrp status          # game/queue/panel + the card it would send
honrp doctor          # prefix, bridge build, Discord handshake, service
journalctl --user -u honrp -f
```

The daemon only connects to Discord while HoN is running. When the game exits
it clears the card and disconnects; when HoN is closed, `ss -xp | grep
discord-ipc` shows no connection from `honrp`.

## Troubleshooting

| symptom | fix |
|---|---|
| `no prefix found` | start HoN once, or use `--prefix` / `--user-dir` |
| wrong prefix chosen | same flags; `honrp doctor` lists all candidates |
| card stuck on `Main Menu` | bridge not loaded/old build: restart HoN, check `doctor` |
| no art on the card | upload an asset named `hon`, or clear `large_image` |
| Discord lists `honrp` as a game | Settings → Game Activity → Registered Games → remove it (the card comes from the app, not detection) |

## Configuration

`~/.config/honrp/config.json` (or `HONRP_CLIENT_ID` for a one-off run):

| key | default | meaning |
|---|---|---|
| `client_id` | — | Discord application id |
| `large_image` | `hon-transparent` | Rich Presence asset key; empty disables art |
| `large_text` | `Heroes of Newerth` | art hover text |
| `show_timer` | `false` | match/menu elapsed timer |
| `show_kda` | `true` | `hero • Lv • K/D/A` vs `hero • Lv` |
| `short_maps` | `true` | `FoW` instead of `Forests of Caldavar (Phoenix)` |
| `poll_interval` | `1.0` | seconds between `/proc` polls |
| `refresh_interval` | `300` | re-publish interval |

`honrp install`, `uninstall`, `status`, `doctor`, `run`, `service` are the
commands; `honrp --help` lists flags.

## Privacy

Local only. Reads `console.log`, `logs/game_*.log` and `/proc`; writes the
managed `startup.lua` block, `~/.config/honrp/config.json` and a systemd user
unit; talks only to Discord's local unix socket. No telemetry, no game memory,
no input, no changes to the game install. Full detail:
[PRIVACY.md](PRIVACY.md).

## Development

```bash
cargo test
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
./tests/run_all.sh
```

The Lua bridge is [lua/honrp_mod.lua](lua/honrp_mod.lua); the reverse-engineering
notes (how the state gets out of the engine) are in
[docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md).

## License

MIT. Not affiliated with Juvio or Discord.
