-- Mocked-environment test for the UI-rebuild path in honrp_mod.lua.
-- Run from the repo root:  lua5.1 tests/test_lua_reload.lua [path/to/honrp_mod.lua]
--
-- Reproduces what the Windows client does on start-up (console.log of
-- 2026-11-07 shows it explicitly):
--   * startup.lua runs, seeds Main, main.interface loads, Main:Init() arms
--     the bridge on interface A and the tick emits a few lines;
--   * the client restarts Direct3D for the saved video mode and *re-executes*
--     /ui/fe3/main.interface (new interface object B, new widgets);
--   * every `Main.*` key is reassigned.  Lua only calls __newindex for keys
--     that do not exist yet, so the per-function wrappers are replaced by
--     plain functions and the old widget (with its Sleep timer and onframe
--     callback) is gone.
-- build 9 emitted 4 lines and stayed silent for the rest of the session.
-- build 10 must re-arm on the next Trigger() and keep reporting.

local modpath = arg[1] or 'honrp_mod.lua'

local out = {}
function println(s) out[#out + 1] = tostring(s) end
local now = 1000
function GetTime() return now end

-- engine Trigger(): dispatch to the watches of the *current* interface
local current
function Trigger(name, ...)
  local watch = current and current.watch
  local fn = watch and watch[name]
  if fn then fn(name, ...) end
end

local function new_interface(label, state)
  local iface_state = state
  local iface = { label = label, watch = {}, sleeps = {} }
  local widget = { label = label }
  function iface:GetWidget(name)
    if name == 'fe3_sleeper' then return widget end
    return nil
  end
  function iface:RegisterWatch(name, fn) iface.watch[name] = fn end
  function iface:Sleep(ms, fn) iface.sleeps[#iface.sleeps + 1] = fn end
  function iface:SetCallback() end
  function iface:UICmd(expr) return iface_state[expr] or '' end
  function widget:RegisterWatch(name, fn) iface.watch[name] = fn end
  function widget:Sleep(ms, fn) iface.sleeps[#iface.sleeps + 1] = fn end
  function widget:SetCallback() end
  function widget:UICmd(expr) return iface_state[expr] or '' end
  current = { iface = iface, widget = widget, watch = iface.watch, sleeps = iface.sleeps }
  return iface
end

local function state(account, in_game, hero)
  return {
    ['IsInGame()'] = in_game,
    ['GetCurrentGamePhase()'] = in_game == '1' and '3' or '0',
    ['GetCurrentLobbyPhase()'] = '0',
    ['GetLocalHeroName()'] = hero or '',
    ['GetAccountName()'] = account,
    ['IsArrangedMatch()'] = 'false',
    ['GetLocalClientNumber()'] = in_game == '1' and '0' or '-1',
    ['GetViewClientNumber()'] = in_game == '1' and '0' or '-1',
    ['GetViewHeroName()'] = hero or '',
  }
end

local ifaceA = new_interface('A', state('PlayerA', '0'))
local gameA = {}
function gameA:RegisterWatch() end
UIManager = {
  GetInterface = function(self, n)
    if n == 'main' then return current.iface end
    if n == 'game' then return gameA end
  end,
  GetActiveInterface = function(self) return current.iface end,
}
Matchmaking = { CurrentState = 1 }
Lobby = { lobbyGameInfo = {} }

dofile(modpath)

-- first main.lua execution
local G = getfenv(0)
Main = G['Main'] or {}
function Main:GetWidget(name) return nil end
function Main:Init() return 'first' end
Main:Init()
assert(#ifaceA.sleeps >= 1, 'bridge did not arm on the first interface')
local first_boot = out[#out - 1]
assert(first_boot:find('"boot":1'), 'expected boot line: ' .. tostring(first_boot))
assert(first_boot:find('"has_widget":1'), 'no widget in boot line: ' .. tostring(first_boot))

now = now + 1000
ifaceA.sleeps[#ifaceA.sleeps](current.widget)
local first_state = out[#out]
assert(first_state:find('"account":"PlayerA"'), 'first emit wrong: ' .. tostring(first_state))

-- ---------------------------------------------------------------------------
-- Direct3D restart: main.interface is re-executed, new widgets, plain funcs
-- ---------------------------------------------------------------------------
local ifaceB = new_interface('B', state('PlayerB', '1', 'Hero_Chronos'))
local gameB = {}
function gameB:RegisterWatch() end
UIManager.GetInterface = function(self, n)
  if n == 'main' then return current.iface end
  if n == 'game' then return gameB end
end
UIManager.GetActiveInterface = function(self) return current.iface end

-- main.lua re-executed: every key already exists -> __newindex is NOT called
function Main:GetWidget(name) return nil end
function Main:Init() return 'second' end
Main:Init() -- plain function now; build 9 was dead from here on

-- The first thing the rebuilt UI does that we can observe: a panel change
-- goes through the engine's global Trigger().
now = now + 5000
Trigger('FSPanelTrigger', 'learn')

local reloaded = out[#out]
assert(reloaded:find('"arm":2'), 'did not re-arm after interface rebuild: ' .. tostring(reloaded))
assert(reloaded:find('"account":"PlayerB"'), 're-armed on the stale interface: ' .. tostring(reloaded))
assert(reloaded:find('"panel":"learn"'), 'panel lost on re-arm: ' .. tostring(reloaded))
assert(reloaded:find('"in_game":"1"'), 'in_game not read from the new interface: ' .. tostring(reloaded))
assert(reloaded:find('"hero":"Hero_Chronos"'), 'hero not read from the new interface: ' .. tostring(reloaded))

-- the new widget must own the heartbeat now
assert(#ifaceB.sleeps >= 1, 'new interface has no timer')

-- HUD triggers must be registered on the main interface: the game interface
-- is not reachable from startup.lua (build 10 regression, no level/KDA)
assert(ifaceB.watch['MatchClock'], 'MatchClock not registered on main interface')
assert(ifaceB.watch['PlayerScore'], 'PlayerScore not registered on main interface')
assert(ifaceB.watch['ActiveLevel'], 'ActiveLevel not registered on main interface')
now = now + 10000
ifaceB.watch['ActiveLevel'](nil, '14')
now = now + 1000
ifaceB.watch['PlayerScore'](nil, '6', '2', '9')
local hud = out[#out]
assert(hud:find('"level":14'), 'level missing: ' .. tostring(hud))
assert(hud:find('"kills":6'), 'kills missing: ' .. tostring(hud))
assert(hud:find('"deaths":2'), 'deaths missing: ' .. tostring(hud))
assert(hud:find('"assists":9'), 'assists missing: ' .. tostring(hud))

now = now + 10000
ifaceB.sleeps[#ifaceB.sleeps](current.widget)
local beat = out[#out]
assert(beat:find('"arm":2'), 'heartbeat not on the new interface: ' .. tostring(beat))
assert(beat:find('"account":"PlayerB"'), 'heartbeat used the stale interface: ' .. tostring(beat))

-- the dead interface's timer must not report anything any more
local before = #out
now = now + 10000
ifaceA.sleeps[#ifaceA.sleeps](ifaceA)
assert(#out == before, 'stale timer from interface A still emitted')

-- build 12: another in-game re-arm (entering a match) must not wipe the
-- level/KDA. PlayerScore does not fire again until the next score change.
local ifaceC = new_interface('C', state('PlayerB', '1', 'Hero_Chronos'))
local gameC = {}
function gameC:RegisterWatch() end
UIManager.GetInterface = function(self, n)
  if n == 'main' then return current.iface end
  if n == 'game' then return gameC end
end
now = now + 1000
Trigger('FSPanelTrigger', 'learn')
local kept = out[#out]
assert(kept:find('"arm":3'), 'third arm missing: ' .. tostring(kept))
assert(kept:find('"level":14'), 'level lost on in-game re-arm: ' .. tostring(kept))
assert(kept:find('"kills":6'), 'kills lost on in-game re-arm: ' .. tostring(kept))
assert(kept:find('"deaths":2'), 'deaths lost on in-game re-arm: ' .. tostring(kept))
assert(kept:find('"assists":9'), 'assists lost on in-game re-arm: ' .. tostring(kept))

-- ... but a re-arm outside a game must clear them (no stale stats leak).
new_interface('D', state('PlayerB', '0'))
now = now + 1000
Trigger('FSPanelTrigger', '')
local menu = out[#out]
assert(not menu:find('"kills":'), 'KDA leaked into the menu: ' .. tostring(menu))
assert(not menu:find('"level":'), 'level leaked into the menu: ' .. tostring(menu))

print('LUA RELOAD HARNESS OK, lines=' .. #out)
