# honrp — Heroes of Newerth Discord Rich Presence for Linux

Discord Rich Presence for the current **Juvio / Heroes of Newerth** client
(0.13.x) running through Steam Play / Proton on Linux.

![The states honrp reports](demo_states.png)

```
┌──────────────────────────────────────────────┐
│  Heroes of Newerth Reborn                    │
│  Ranked • FoW • Banning Pick                 │
│  Pyromancer • Lv 12 • 4/2/7                  │
└──────────────────────────────────────────────┘
```

Two pieces:

| piece | what it does |
|---|---|
| `honrp` | a single static Rust binary. Finds `juvio.exe` in `/proc`, tails the game's `console.log`, drives Discord's native `discord-ipc-N` unix socket. |
| `lua/honrp_mod.lua` | the Lua bridge, installed as `<Documents>/Juvio/Heroes of Newerth/startup.lua`. Runs in the game's own UI Lua VM and prints one `[HONRP] {json}` line per state change plus a 10 s heartbeat. |

Nothing in the game install directory is modified. The bridge lives next to the
game's own settings files and is read by the engine at launch.

## Install

### From a release

```bash
tar -xzf honrp-v0.2.0-x86_64-linux.tar.gz
./honrp install
```

### From source

```bash
cargo build --release
install -Dm755 target/release/honrp ~/.local/bin/honrp
honrp install
```

`honrp install` detects the Proton prefix and Windows user directory, installs
the Lua bridge (managed block, backup kept), writes a **systemd user unit** and
starts it. No root, no files in the game install directory. Restart Heroes of
Newerth afterwards so the engine loads the bridge.

### Discord application

1. Create an application at <https://discord.com/developers/applications> and
   copy its **Application ID**.
2. Optional but recommended: **Rich Presence → Art Assets** → upload
   `assets/hon.png` with the asset name **`hon`**.
3. Put the ID in `~/.config/honrp/config.json`:

   ```json
   {"client_id": "123456789012345678"}
   ```

   or export `HONRP_CLIENT_ID` for a one-off run.

The card's application name is the name of your Discord application.

## Usage

```bash
honrp status [--json]     # what the daemon sees right now
honrp doctor              # detection, bridge build, Discord handshake, service
honrp run -v              # run in the foreground (systemd runs this for you)
honrp service status      # systemd user unit
honrp uninstall           # remove the service and the Lua bridge
```

Config lives in `~/.config/honrp/config.json` (see `config.example.json`):

| key | default | meaning |
|---|---|---|
| `client_id` | — | Discord application id (required) |
| `large_image` | `hon` | Rich Presence asset key (empty disables art) |
| `large_text` | `Heroes of Newerth` | art hover text |
| `show_timer` | `false` | match/menu elapsed timer |
| `show_kda` | `true` | `hero • Lv N • K/D/A` vs `hero • Lv N` |
| `short_maps` | `true` | `FoW` instead of `Forests of Caldavar (Phoenix)` |
| `poll_interval` | `1.0` | seconds between `/proc` polls |
| `refresh_interval` | `300` | re-publish interval (asset/name propagation) |

### Is it running?

```bash
systemctl --user status honrp     # active (running), journal included
honrp status                      # game/queue/panel + current card
journalctl --user -u honrp -f
```

There is no tray icon by default — a resident tray app would use more memory
than the daemon. `status`/`doctor` are the indicator; a StatusNotifierItem
indicator is on the roadmap.

## Privacy & transparency

honrp is local-only:

- reads `console.log`, the client's `logs/game_*.log`, and `/proc` for the
  running `juvio.exe` — nothing else;
- writes the managed `startup.lua` block, `~/.config/honrp/config.json` and a
  systemd **user** unit (no root);
- talks **only** to Discord's local unix socket — no HTTP, no telemetry;
- never opens game memory, never reads input, never modifies the game install.

Full list, measured resource use and verification commands (`strace`, `ss`,
`honrp doctor`): [PRIVACY.md](PRIVACY.md).

## Why this works (reverse engineering notes)

The short version of what the investigation found.

### Target layout

`juvio.exe` is a CEF-based launcher *and* the game entry point. It loads:

| file | size | role |
|---|---|---|
| `bin/k2.dll` | 39 MB | K2 engine — 91,925 `.pdata` functions, 4,440 exports with full C++ symbols |
| `bin/client.dll` | 7.7 MB | game client logic + game UICmds |
| `bin/origami.dll` | 91 MB | UI/WebKit layer (`[Origami …]` log channel) |
| `heroes of newerth/resources0.jz` | 9.5 GB | **plain ZIP**, 55,304 entries — including *source* Lua UI (`ui/scripts/fe3/*.lua`, `ui/scripts/game/*.lua`), `.interface` XML and string tables |

The UI Lua is not compiled: `ui/scripts/fe3/main.lua` and friends are shipped as
readable source, and `ui/scripts/shared/global_main.lua` even starts with
`-- ATTENTION MODDERS: DO NOT MODIFY THIS SCRIPT`.

### The hook: `startup.lua`

`k2.dll` exports `?Init@CHost@@QEAAXAEBV…` at `0x1805f7b00`. Near the end of
`CHost::Init` the engine does (disassembly at `0x1805f938d`):

```
mov  r8d, 0x0d                        ; length of "#/startup.lua"
lea  rdx, [rip+…]                     ; L"#/startup.lua"  (RVA 0x181fa5b08)
lea  rcx, [rsp+0x40]
call std::wstring::assign
mov  rcx, g_pFileManager
call CFileManager::Exists             ; "#/" = user documents dir
test al, al / testb $0x20, [host+0x3f0]
call g_pLuaManager->ExecuteFile       ; 0x1809c4820  CLuaManager::ExecuteFile
```

The `0x20` bit in the host flags is "config locked" (`-noconfig`); the live
console log does **not** contain *"Startup config is locked; using defaults for
this launch"*, and `Executing "#/startup.cfg"` is present, so user scripts are
enabled in the normal Steam path. `/server.lua` and `/client.lua` are executed
through the same `ExecuteFile` call just after it.

`#/` maps to `…/Documents/Juvio/Heroes of Newerth/`, the same directory the game
already writes `startup.cfg`, `game_settings_local.cfg` and `console.log` to.
`startup.lua` runs in the shared global Lua table, *before* the UI loads, which
is why the mod only seeds `Main` and starts its tick when `main.interface`
calls `Main:Init()`.

### Reading state from inside the UI

`CUICmd` is the engine's expression/command registry (`pUICmdRegistry`,
`CUICmdRegistry::GetInstance`). Every entry is a tiny static registration
function that constructs `? ?0CUICmd@@@…` with a name and a handler; they can
be enumerated statically. The useful ones:

| UICmd | handler | meaning |
|---|---|---|
| `IsInGame()` | `client!0x1802acdb0` | returns `"1"` / `"0"` |
| `IsInQueue()` | `k2!0x1801464a0` | matchmaking queue |
| `GetCurrentGamePhase()` | `client!0x1802acc00` | `CGameInfo+0xb4` (`EGameServerPhase`) as int |
| `GetCurrentLobbyPhase()` | `client!0x1802acd20` | `ELobbyPhase` |
| `GetCurrentClientPhase()` | `client!0x1802acc90` | chat/connection phase (`Enum.ClientPhase.CONNECTED = 5`) |
| `GetLocalHeroName()` | `client!0x1802ad400` | local hero |
| `GetLocalTeam()`, `GetNumClients()`, `GetAccountName()`, `GetPing()` | | |

`ui/scripts/shared/global_main.lua` wraps these as plain Lua helpers
(`IsInGame()`, `IsInQueue()`, `GetCurrentClientPhase()`, …) via
`interface:UICmd("…")`, and the phase enums live in
`ui/scripts/shared/enums.lua`.

Two scheduling primitives are used by the shipped UI and are available to a
mod:

* `widget:Sleep(ms, fn)` — one-shot timer on any widget, used by
  `ui/scripts/shared/widget_anims.lua` (`widget:Sleep(interval, Tick)`).
* `widget:SetCallback('onframe', fn)` — per-frame callback, used by
  `ui/test_suite/test_suite.lua`.

The mod uses both: state changes emit immediately, the timers provide a 10 s heartbeat.

### The log bridge

`console.log` is UTF-16LE and every Lua `println` / `print` is captured in it
(`con_writeLog`, `~/console.log`). That is verified in the shipped client: the
`^cROLEPICK …` debug lines in the current log come from
`ui/scripts/shared/role_pick_shared.lua:println`.

So the data path is just:

```
game Lua VM  --print-->  console.log  --tail-->  honrp  --unix socket-->  Discord
```

No memory reading, no ptrace, no DLL injection, no archive repacking, no
network protocol work.

### Extra data: map and mode

`~/Documents/Juvio/Heroes of Newerth/logs/game_NNNNN.log` is the per-match log
(UTF-16LE), created when a match starts:

```
INFO_DATE date:"2026/10/30" time:"00:13:57"
INFO_MATCH name:"Field Test" id:"4294967295"
INFO_MAP name:"fieldtest" version:"0.0.0"
INFO_SETTINGS mode:"Mode_Normal" options:"Option_None"
```

the daemon parses it for the map/mode and uses `INFO_DATE` as the presence
timestamp, so the elapsed clock matches the actual match.

---


## Tests

```bash
cargo test                                              # 9 Rust unit tests
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua         # bridge, mocked K2 engine
./tests/run_all.sh                                      # both
```

The Rust suite covers activity building (ranked/FoW/hero/KDA, timer off,
panels, spectator, long-map option), mod-state parsing and a real
handshake + `SET_ACTIVITY` exchange over a unix socket. The Lua harness boots
the bridge in a mocked engine, feeds HUD/panel/queue triggers and asserts the
emitted lines.

## How the presence maps

## How the presence maps

| detected | details | state | timestamp |
|---|---|---|---|
| in match | `Ranked • FoW • Banning Pick` | `Pyromancer • Lv 12 • 4/2/7` | none (optional) |
| hero select / pre-game (phase 2) | `Ranked • FoW • All Pick` | `Hero Select` | none |
| waiting for players (phase 1) | `Forests of Caldavar • All Pick` | `Waiting for players` | none |
| post-match (phase 4) | `Forests of Caldavar • All Pick` | `Post-Match` | none |
| spectating | `Spectating • Forests of Caldavar` | `Watching Master Of Arms` | match start |
| in queue | `Searching for a match` | `In queue` | session start |
| game lobby | `In a game lobby` | account name | session start |
| main menu | `Main Menu` | account name | session start |
| Plinko | `Playing Plinko` | account name | session start |
| Store / Shop | `Browsing the Store` | account name | session start |
| Ladder / Profile / Match stats / MOTD / Learn | `Viewing …` | account name | session start |
| game closed | *(activity cleared)* | | |

`demo_states.png` is a pre-rendered gallery of these states.

All labels are configurable — `short_maps` ("caldavar"/"caldavarphoenix" →
`FoW`, `midwars` → `Mid Wars`, `fieldtest` → `Field Test`), `show_timer` (off by
default; the hero level is the match-stage signal), `show_kda` (on by default;
turn it off for `hero • Lv N` only).

### What is detected, and where it comes from

| signal | source | confidence |
|---|---|---|
| in match | UICmd `IsInGame()` | exact |
| in queue | `Matchmaking.CurrentState == 3` and the `MatchmakingJoinQueue` / `MatchmakingLeaveQueue` triggers | exact (k2's `IsInQueue()` UICmd is a stub that always returns `"false"` — see below) |
| hero | UICmd `GetLocalHeroName()` | exact |
| hero level | HUD trigger `ActiveLevel(level, skillPoints, showLevelup)` | exact |
| KDA | HUD trigger `PlayerScore(kills, deaths, assists, …)` | exact |
| game clock | HUD trigger `MatchClock(seconds, valid)` | exact |
| pre-game / hero select | `GetCurrentGamePhase()` (`EGameServerPhase`: 0 IDLE, 1 WAITINGFORPLAYERS, 2 PREGAME, 3 INGAME, 4 POSTGAME, 5 FINISHED — documented in `game_shop_hd.lua`). `IsInGame()` is **false in pre-game**, so phase 2 is what identifies the hero-pick screen | exact |
| map | match log `INFO_MAP`, else lobby `worldFancyName` | exact when the match log is written |
| ranked / unranked | `Lobby.lobbyGameInfo.arrangedMatchType` (`EArrangedMatchType`: 2/3 ranked, 4/5 unranked) | exact in matchmade games; empty for practice/custom and when reconnecting with no lobby data |
| rules mode (All Pick, Banning Pick, …) | `Lobby.lobbyGameInfo.gameModeName` / match log `INFO_SETTINGS` | exact |
| game timer | match start from `INFO_DATE`, else first in-game tick | ±1 s |
| spectator | `GetLocalClientNumber()` vs `GetViewClientNumber()`, plus `GetViewHeroName()` | good; verify on a real spectate session |
| menu panel (Plinko, Store, Ladder, …) | visibility of `fs_panel{body}` widgets from `main.interface` | exact |

`IsInQueue()` is a stub: k2's handler at `0x1802e07a0` just returns the constant
string `"false"` (5 chars). The real queue state lives in
`Matchmaking.CurrentState` (`Enum.MatchmakingState`: 0 Offline, 1 NotInGroup,
2 InGroup, 3 InQueue), which the front-end sets from the
`MatchmakingJoinQueue` / `MatchmakingLeaveQueue` triggers. The mod reads both
and also watches those triggers directly, so queue state works.

Not exposed: selected queue modes while searching (kept in a `local` table
inside `matchmaking.lua`), replay-vs-spectate distinction. KDA and level are
read from the HUD's own triggers (`mid_bar.lua`, `bot_center.lua`), not from
memory.


## Files it touches

| path | why |
|---|---|
| `<prefix>/drive_c/users/<user>/Documents/Juvio/Heroes of Newerth/startup.lua` | the Lua bridge, inside a `--- HONRP BEGIN/END ---` block; existing content preserved and backed up |
| `<user dir>/console.log` | read-only state transport from the bridge |
| `<user dir>/logs/game_*.log` | read-only map/mode |
| `~/.config/honrp/config.json` | configuration |
| `~/.config/systemd/user/honrp.service` | user service |
| `/proc/<pid>/comm`, `/proc/<pid>/cmdline`, `/proc/<pid>/environ` | find `juvio.exe` and its prefix |
| `$XDG_RUNTIME_DIR/discord-ipc-*` | local Discord socket |

## Limitations / caveats

## Limitations / caveats

* **Restart required** after installing the Lua bridge.
* The bridge was developed against the 0.13.3.0 client; a client update can
  change or remove the `startup.lua` hook or UICmd names. `honrp status`
  shows what is still resolvable (`source: "mod"` vs `"fallback"`).
* `GetCurrentGamePhase()` returns a raw `EGameServerPhase` integer. It is
  logged but not currently rendered as text; the readable phase text comes from
  `GetCurrentLobbyPhase()` + `Enum.LobbyPhase`.
* No in-game KDA/score: the scoreboard data is trigger-driven inside the HUD
  interface, not exposed through the front-end UICmd registry. (Possible future
  work: load a tiny child interface next to `game_hd` and read the same
  triggers.)
* The mod is an observer. It does not read or write game memory, does not send
  input, and does not alter gameplay. It uses the same user-script path the
  engine has always offered for UI mods — keep that in mind if the game's
  integrity rules ever change.
* Discord must be running with its IPC socket available. Flatpak/Discord
  variants are covered by the socket search paths in `discord_sockets()`.


## Repository layout

```
Cargo.toml, src/      daemon (Rust)
lua/honrp_mod.lua     bridge, installed as startup.lua
assets/               Rich Presence art (see assets/README.md)
tests/                Lua harness
docs/PLAN.md          release/packaging notes
PRIVACY.md            exact read/write/network surface
```

## License

MIT — see [LICENSE](LICENSE). The art in `assets/` is derived from HoN/Juvio
artwork; see [assets/README.md](assets/README.md).

## Other injection points found (for future work)

* `#/override.cfg` and `#/startup.cfg` are executed at boot by
  `CConsole::ExecuteScript`, and the console scripting language has
  `exec` / `call` / `sleep` / `goto` / `if` / `else` / `repeat`.
* Command line: `-mod`, `-editpath`, `-execute`, `-autoexec`, `-noconfig`,
  `-forcetitle` (see the option table in `k2.dll` around `host_startupCfg`).
* `CFileManager` has a real "mod tree" (`ChangeModStack`, `GetModPath`,
  `GetTopModPath`, `fs_disablemods`) — the loading screen comments in
  `ui/fe3/loading.interface` mention it explicitly.
* Console: `UICall <widget> <uicmd>`, `LoadChildInterface`, `ReloadInterface`.
* For a native add-on instead of Lua: `k2.dll` exports `g_pLuaManager`,
  `g_pHost`, `g_pConsole`, and `CLuaManager::ExecuteFile`, so a proxy DLL could
  inject Lua on demand. That is heavier and unnecessary for presence.
