-- Mocked-environment test for honrp_mod.lua.
-- Run from the repo root:  lua5.1 tests/test_lua_mod.lua [path/to/honrp_mod.lua]
--
-- It emulates the parts of the K2 Lua environment the mod touches:
--   println, GetTime, UIManager, a widget with Sleep / SetCallback / UICmd
-- then loads the mod and plays the role of ui/scripts/fe3/main.lua.

local modpath = arg[1] or 'honrp_mod.lua'

local out = {}
function println(s) out[#out + 1] = tostring(s) end
local now = 1000
function GetTime() return now end

local widget = {}
local sleeps = {}
local mainWatch = {}
local inGameState = '1'
function widget:RegisterWatch(name, fn) mainWatch[name] = fn end
local gameWatch = {}
local gameIface = {}
function gameIface:RegisterWatch(name, fn) gameWatch[name] = fn end
function widget:Sleep(ms, fn) sleeps[#sleeps + 1] = fn end
function widget:SetCallback(ev, fn) self.cb = fn end
function widget:UICmd(expr)
  if expr == 'IsInGame()' then return inGameState end
  local vals = {
    ['IsInGame()'] = '1', ['IsInQueue()'] = '0',
    ['GetCurrentClientPhase()'] = '5',
    ['GetCurrentGamePhase()'] = '2',
    ['GetCurrentLobbyPhase()'] = '4',
    ['GetLocalHeroName()'] = 'Hero_Pyromancer',
    ['GetLocalTeam()'] = '1',
    ['GetAccountName()'] = 'evil"leader',
    ['IsArrangedMatch()'] = 'true',
    ['GetLocalClientNumber()'] = '0',
    ['GetViewClientNumber()'] = '0',
    ['GetViewHeroName()'] = 'Hero_Pyromancer',
  }
  return vals[expr] or ''
end
function widget:GetWidget(name)
  if name == 'fs_panelplinko' then return plinkoPanel end
  return nil
end
plinkoPanel = {}
function plinkoPanel:IsVisible() return 1 end
UIManager = {
  GetActiveInterface = function(self) return widget end,
  GetInterface = function(self, n)
    if n == 'main' then return widget end
    if n == 'game' then return gameIface end
  end,
}
Matchmaking = { CurrentState = 3 }  -- Enum.MatchmakingState.InQueue
Lobby = {
  lobbyGameInfo = {
    arrangedMatchType = '2',
    worldFancyName = 'caldavar',
    gameModeName = 'banningpick',
    matchID = '12345',
  },
}

dofile(modpath)

-- ui/scripts/fe3/main.lua would do this:
local G = getfenv(0)
Main = G['Main'] or {}
local inited = false
function Main:Init()
  inited = true
  return 'ret'
end
assert(inited == false, 'Init must not run at load')
local r = Main:Init()

-- the HUD triggers should now be registered on the "game" interface
assert(gameWatch['MatchClock'], 'MatchClock not registered')
assert(gameWatch['PlayerScore'], 'PlayerScore not registered')
assert(gameWatch['ActiveLevel'], 'ActiveLevel not registered')
assert(mainWatch['FSPanelTrigger'], 'FSPanelTrigger not registered')
gameWatch['MatchClock'](gameIface, '754', 'true')
gameWatch['PlayerScore'](gameIface, '4', '2', '7', '120', '0', '3')
gameWatch['ActiveLevel'](gameIface, '12', '1', 'false')
mainWatch['FSPanelTrigger'](widget, 'plinko')
assert(inited, 'original Main:Init did not run')
assert(r == 'ret', 'Main:Init return value lost')

assert(#sleeps >= 1, 'no Sleep timer scheduled')
-- event emits are coalesced (>=0.9 s); advance, then force one via PlayerScore
now = now + 1000
gameWatch['PlayerScore'](gameIface, '4', '2', '7', '120', '0', '3')
-- then 10 s heartbeats
for _ = 1, 3 do
  now = now + 10000
  sleeps[#sleeps](widget)
end

local inGameLine = out[#out]

-- queue while NOT in game, via the same trigger matchmaking.lua uses
inGameState = '0'
assert(mainWatch['MatchmakingJoinQueue'], 'MatchmakingJoinQueue not registered')
assert(mainWatch['LobbyGameInfo'], 'LobbyGameInfo not registered')
mainWatch['MatchmakingJoinQueue']()
mainWatch['LobbyGameInfo'](widget, 'banningpick', '', '', 'midwars', '10', '10', '0',
  'Ranked Test', '98765', 'host', '0', '1.0', 'banningpick', '0', '0', 'midwars', '3')
now = now + 10000
sleeps[#sleeps](widget)
local qline = out[#out]
assert(qline:find('"in_game":"0"'), 'expected menu line: ' .. qline)
assert(qline:find('"in_queue":"1"'), 'queue trigger not applied: ' .. qline)
assert(qline:find('"arranged_match":"3"'), 'lobby trigger amt missing: ' .. qline)
assert(qline:find('"world":"midwars"'), 'lobby trigger world missing: ' .. qline)

local emissions = 0
local saw_hero = false
local escaped = false
for _, line in ipairs(out) do
  if line:find('^%[HONRP%] {') then
    emissions = emissions + 1
    if line:find('"hero":"Hero_Pyromancer"') then saw_hero = true end
    if line:find('evil\\"leader') then escaped = true end
  end
end
assert(emissions >= 4, 'expected boot + 3 emissions, got ' .. emissions)
assert(saw_hero, 'hero field missing from emission')
assert(escaped, 'quote escaping broken')
local last = inGameLine
assert(last:find('"arranged_match":"2"'), 'arranged_match missing: ' .. last)
assert(last:find('"world":"caldavar"'), 'world missing: ' .. last)
assert(last:find('"lobby_mode":"banningpick"'), 'lobby_mode missing: ' .. last)
assert(last:find('"panel":"plinko"'), 'panel missing: ' .. last)
assert(last:find('"view_client":"0"'), 'view_client missing: ' .. last)
assert(last:find('"in_queue":"0"'), 'in-game queue must be 0: ' .. last)
assert(last:find('"mm_state":3'), 'mm_state missing: ' .. last)
assert(last:find('"clock":754'), 'clock missing: ' .. last)
assert(last:find('"level":12'), 'level missing: ' .. last)
assert(last:find('"kills":4'), 'kills missing: ' .. last)
assert(last:find('"assists":7'), 'assists missing: ' .. last)

print('LUA HARNESS OK, lines=' .. #out)

-- ---------------------------------------------------------------------------
-- scenario 2: no UIManager global; widget comes from Main:GetWidget
-- ---------------------------------------------------------------------------
local out2 = {}
local env = setmetatable({}, {__index = function(_, k)
  if k == 'Main' or k == 'UIManager' then return nil end
  return _G[k]
end})
env._G = env
env.println = function(s) out2[#out2 + 1] = tostring(s) end
env.GetTime = function() return now end

local chunk = assert(loadfile(modpath))
setfenv(chunk, env)
chunk()

local M = env.Main
assert(type(M) == 'table', 'Main table not seeded in fresh env')
local slept2 = {}
local w2 = {}
function w2:Sleep(ms, fn) slept2[#slept2 + 1] = fn end
function w2:SetCallback() end
function w2:UICmd(expr) return expr == 'IsInGame()' and '1' or '0' end
function M:GetWidget(name) return w2 end
function M:Init() end
M:Init()
assert(#slept2 >= 1, 'fallback widget path did not schedule a timer')
now = now + 10000
slept2[#slept2](w2)
local found2 = false
for _, line in ipairs(out2) do
  if line:find('"in_game":"1"') then found2 = true end
end
assert(found2, 'fallback widget path did not emit state')
print('LUA FALLBACK OK, lines=' .. #out2)
