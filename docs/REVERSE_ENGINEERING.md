# Reverse engineering notes

How `honrp` gets live game state out of the Juvio / Heroes of Newerth client
without touching its memory or injecting into the render loop: the engine's own
UI Lua VM writes a state line into the client's `console.log`, and the daemon
tails it.

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

the daemon parses it for the map/mode (the presence timer itself comes from the
HUD's `MatchClock` trigger, which is why it matches the real match clock).

Two caveats found on Windows (2026-11-07):

* Practice games — the "Learn → Practice" flow and `PlayerHosted:CreatePracticeGame`
  — set `gameVars.map = 'fieldtest'` (`ui/scripts/fe3/hero_list.lua:242`), so
  `Field Test • All Pick` is the *correct* presence for a practice match. A
  server-hosted match does not write a `logs/game_*.log` at all (only the local
  practice/custom game did in our captures); for those the map/mode has to come
  from the Lua bridge's `Lobby.lobbyGameInfo` / `LobbyGameInfo` watch.
* The log is a **local** match log, so `matchlog::newest` can show a stale map if
  the daemon joins mid-session and no new log is created.

### The `main.interface` rebuild (why build 9 went silent)

On Windows the client applies the saved video mode right after `CHost::Init`:
the console log shows `Wrote …/startup.cfg`, `Frame latency`, `Restarting
Direct3D`, the fontmaps reloading, and then a second

```
Loading ^mInterface^* '/ui/fe3/main.interface'
```

That re-execution does three things to a `startup.lua` mod:

1. every interface widget is destroyed, including the one the bridge put its
   `Sleep(1000, …)` timer and `SetCallback('onframe', …)` on;
2. `main.interface`'s `onloadlua` runs again (`Main:Init()`), but a one-shot
   `if started then return end` guard skips re-arming;
3. every `Main.*` key is reassigned.  Lua only calls a table's `__newindex`
   for keys that **do not exist yet**, so the wrapper installed on the first
   load is silently replaced by the plain function — the `Main` hook cannot
   re-arm itself even if it wanted to.

The engine's global `Trigger()` is *not* reassigned by the rebuild, and
`Main:FSPanelToggle` calls `Trigger('FSPanelTrigger', panelName)` for every
panel change, so bridge build 10 wraps `Trigger` and re-arms from there.

---

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
