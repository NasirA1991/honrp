----------------------------------------------------------
--  honrp - Heroes of Newerth <-> Discord Rich Presence bridge
--
--  Installed as  <Documents>/Juvio/Heroes of Newerth/startup.lua
--  The engine executes that file from CHost::Init() through
--  CLuaManager::ExecuteFile() when it exists (k2.dll:
--  ?Init@CHost@@... -> "#/startup.lua" -> ExecuteFile), before
--  the UI is loaded.  We therefore only *seed* the global `Main`
--  table here and start the real tick once main.interface calls
--  Main:Init().
--
--  The tick prints one line per second to the console:
--      [HONRP] {"in_game":"1", ...}
--  which lands in console.log (con_writeLog=1) where honrp
--  picks it up.  No game files are touched, no memory is patched.
--
--  build 10: the client rebuilds its UI (main.interface is
--  re-executed) after the Direct3D/video-mode restart on start-up
--  and on a few in-game transitions.  That destroys every widget
--  the timer was attached to and replaces the functions on `Main`,
--  so build 9 emitted a few lines and then went silent for the rest
--  of the session.  The bridge now:
--    * re-arms on Main:Init()/Main:OnReload() and whenever the
--      active `main` interface object changes,
--    * wraps the engine's global Trigger() so a UI rebuild is
--      noticed before the engine dispatches the trigger, and the
--      full-screen panel name is captured directly,
--    * re-registers watches/HUD triggers on the new interface.
--
--  build 11: register the HUD triggers (MatchClock / PlayerScore /
--  ActiveLevel) on the main interface object as well as on the game
--  interface.  `UIManager:GetInterface('game')` is not reachable
--  from the startup.lua environment, so build 10 never registered
--  them and level/KDA stayed empty in a match.
--
--  build 12: keep the HUD values across an interface rebuild.  Build 11
--  cleared them in start(); when the client re-armed the bridge on entering
--  a match, PlayerScore did not fire again until the first score change, so
--  a fresh 0/0/0 card disappeared and only the level (re-fired by
--  ActiveLevel) came back.  The immediate emit after start() still clears
--  the values when the game is not running, so no stale stats leak between
--  matches.
----------------------------------------------------------

-- >>> HONRP BEGIN >>> (managed block, do not edit by hand)
local okMod, modErr = pcall(function()
    local G = _G
    local MARKER = '[HONRP]'
    local BUILD = 12
    local unpack = G.unpack or (G.table and G.table.unpack)

    ----------------------------------------------------------
    -- forward declarations
    --
    -- Lua locals are scoped from the point of declaration.  Build 9
    -- declared `iface_env` at the bottom of the file, so every closure
    -- above it read the *global* iface_env, which stayed nil forever
    -- ("env":0 in the emitted lines) and cost the Lobby table.
    ----------------------------------------------------------
    local iface_env = nil
    local main_iface = nil
    local anchor = nil
    local started = false
    local starting = false
    local arm_count = 0
    local trigger_hooked = false
    local trigger_busy = false
    local orig = {}
    local watchRegistered = false
    local hudHosts = {}

    local watch = {}
    local hud = {}

    local start, ensure, install_trigger_hook
    local emit, guarded_emit, emit_now, emit_beat
    local heartbeat, onframe, sleeper, reschedule
    local register_hud, register_watches

    ----------------------------------------------------------
    -- small helpers
    ----------------------------------------------------------
    local function raw_print(s)
        pcall(function()
            if G.println then
                G.println(MARKER .. ' ' .. s)
            elseif G.print then
                G.print(MARKER .. ' ' .. s .. '\n')
            end
        end)
    end

    local function jstr(v)
        local s = tostring(v == nil and '' or v)
        s = s:gsub('[%z\1-\31\\"]', function(c)
            if c == '"' then return '\\"' end
            if c == '\\' then return '\\\\' end
            if c == '\n' then return '\\n' end
            if c == '\r' then return '\\r' end
            if c == '\t' then return '\\t' end
            return string.format('\\u%04x', string.byte(c))
        end)
        return '"' .. s .. '"'
    end

    local function wall()
        local ok, v = pcall(function()
            local oslib = G.os or (iface_env and iface_env.os)
            return oslib and oslib.time and oslib.time()
        end)
        if ok and type(v) == 'number' then return v end
        return 0
    end

    -- console.log is stdio-buffered (the file can sit mid-line for minutes
    -- while in a match). CConsole::FlushLogs is a console command, reachable
    -- from Lua through the global Cmd()/UICmd("Cmd('FlushLogs')").
    local function flush_log(a)
        if G.Cmd then pcall(G.Cmd, 'FlushLogs') end
        if a then pcall(function() a:UICmd("Cmd('FlushLogs')") end) end
    end

    local function now_ms()
        local f = G.GetTime or (iface_env and iface_env.GetTime)
        if type(f) == 'function' then
            local ok, v = pcall(f)
            if ok and type(v) == 'number' and v >= 0 then return v end
        end
        local oslib = G.os or (iface_env and iface_env.os)
        if oslib and oslib.clock then
            local ok, v = pcall(oslib.clock)
            if ok and type(v) == 'number' then return math.floor(v * 1000) end
        end
        return nil
    end

    ----------------------------------------------------------
    -- UI access
    ----------------------------------------------------------
    local function ui_manager()
        local U
        if iface_env then U = iface_env.UIManager end
        return U or G.UIManager
    end

    local function get_main_interface()
        local U = ui_manager()
        if U then
            local ok, w = pcall(function() return U:GetInterface('main') end)
            if ok and w then return w end
            ok, w = pcall(function() return U:GetActiveInterface() end)
            if ok and w then return w end
        end
        return main_iface
    end

    -- A widget with Sleep()/SetCallback(): prefer the interface's own
    -- sleeper widget (the shipped UI uses `fe3_sleeper` the same way),
    -- then the always-updating background, then the interface itself.
    local function get_timer_widget(iface)
        if not iface then return nil end
        local names = { 'fe3_sleeper', 'fe3_background_image', 'login_widgets',
                        'system_bar_root' }
        for i = 1, #names do
            local ok, w = pcall(function() return iface:GetWidget(names[i]) end)
            if ok and w then return w end
        end
        local ok, sleep = pcall(function() return iface.Sleep end)
        if ok and type(sleep) == 'function' then return iface end
        return nil
    end

    local function ui(iface, expr)
        if not iface then return nil end
        local ok, r = pcall(function() return iface:UICmd(expr) end)
        if ok then return r end
        return nil
    end

    local function inum(v)
        local n = tonumber(tostring(v or ''))
        if not n then return 'null' end
        return tostring(math.floor(n))
    end

    local function truthy(v)
        local s = tostring(v or ''):lower()
        return s == '1' or s == 'true'
    end

    ----------------------------------------------------------
    -- live in-game data comes from the game HUD's own triggers
    -- (ui/scripts/game/mid_bar.lua and bot_center.lua):
    --   MatchClock(seconds, valid) / PlayerScore(k,d,a,ck,nk,denies) /
    --   ActiveLevel(level,...)
    ----------------------------------------------------------
    local function hud_reset()
        hud.clock = nil; hud.clockValid = nil; hud.level = nil
        hud.kills = nil; hud.deaths = nil; hud.assists = nil
    end

    local function register_hud_on(host)
        if not host or hudHosts[host] then return false end
        local ok = pcall(function()
            host:RegisterWatch('MatchClock', function(_, seconds, valid)
                hud.clock = tonumber(tostring(seconds or ''))
                hud.clockValid = not (tostring(valid or ''):lower() == 'false')
                emit_beat()
            end)
            host:RegisterWatch('PlayerScore', function(_, k, d, a)
                hud.kills = tonumber(tostring(k or ''))
                hud.deaths = tonumber(tostring(d or ''))
                hud.assists = tonumber(tostring(a or ''))
                emit_now()
            end)
            host:RegisterWatch('ActiveLevel', function(_, level)
                hud.level = tonumber(tostring(level or ''))
                emit_now()
            end)
        end)
        if ok then hudHosts[host] = true end
        return ok
    end

    -- The engine's watch registry is keyed by trigger name, not by interface:
    -- the shipped scripts register the same trigger from widgets of different
    -- interfaces.  Build 9 registered the HUD triggers on the *main* interface
    -- object and that is what delivered MatchClock/PlayerScore on Linux.
    -- `UIManager:GetInterface('game')` returns nothing from the startup.lua
    -- environment, so it must never be the only host (build 10 regression:
    -- "hudhosts":0 for a whole match, no level/KDA).
    local GAME_IFACES = { 'game', 'game_hd', 'game_arcade' }
    local hud_game = nil

    local function try_get_interface(name)
        local U = ui_manager()
        if not U then return nil end
        local ok, v = pcall(function() return U:GetInterface(name) end)
        if ok then return v end
        return nil
    end

    register_hud = function()
        register_hud_on(main_iface)
        register_hud_on(anchor)
        local U = ui_manager()
        if U then
            -- during a match the active interface is the game one
            local ok, act = pcall(function() return U:GetActiveInterface() end)
            if ok and act then register_hud_on(act) end
        end
        if not hud_game then
            for i = 1, #GAME_IFACES do
                local gi = try_get_interface(GAME_IFACES[i])
                if gi then
                    hud_game = gi
                    register_hud_on(gi)
                    break
                end
            end
        end
    end

    ----------------------------------------------------------
    -- queue / lobby watches
    ----------------------------------------------------------
    register_watches = function(host)
        if watchRegistered or not host then return false end
        local ok = pcall(function()
            host:RegisterWatch('MatchmakingJoinQueue', function()
                watch.in_queue = true; watch.found = false
                emit_now()
            end)
            host:RegisterWatch('MatchmakingLeaveQueue', function()
                watch.in_queue = false
                emit_now()
            end)
            host:RegisterWatch('MatchmakingFoundMatch', function()
                watch.in_queue = false; watch.found = true
                emit_now()
            end)
            host:RegisterWatch('MatchmakingConnected', function()
                watch.in_queue = false
                emit_now()
            end)
            host:RegisterWatch('MatchmakingDisconnected', function()
                watch.in_queue = false
                emit_now()
            end)
            -- game_lobby.lua: params[1]="gameModeString" ... params[17]="arrangedMatchType"
            host:RegisterWatch('LobbyGameInfo', function(_, ...)
                watch.amt = select(17, ...)
                watch.world = select(4, ...)
                watch.mode = select(13, ...)
                watch.name = select(8, ...)
                watch.match_id = select(9, ...)
                emit_now()
            end)
            host:RegisterWatch('LobbyCountDown', function() emit_beat() end)
            host:RegisterWatch('HeroSelectInfo', function() emit_now() end)
            -- fired by Main:FSPanelToggle with the panel name ('' when closed)
            host:RegisterWatch('FSPanelTrigger', function(_, panelName)
                watch.panel = tostring(panelName or '')
                emit_now()
            end)
        end)
        if ok then watchRegistered = true end
        return ok
    end

    ----------------------------------------------------------
    -- state emit
    ----------------------------------------------------------
    local lastEmit = nil
    local emitCount = 0

    -- Reporting cadence: state changes are emitted immediately (coalesced to
    -- one per ~0.9 s), the MatchClock/lobby countdown heartbeat every 10 s.
    local HEARTBEAT_MS = 10000
    local MIN_GAP_MS = 900

    -- Full-screen panel names, in the order they are checked.  `plinko_v2` is
    -- the real name (main.lua registers it via OnFSPanelInstantiate), `play`
    -- is the home screen.
    local PANELS = { 'plinko_v2', 'store', 'learn', 'ladder', 'profile',
                     'matchstats', 'game_lobby', 'play', 'motd' }

    local function is_panel_open(mainTbl, name)
        -- use the *current* function: after a UI rebuild `orig` still points
        -- at the old interface's closure
        local f = mainTbl.IsPanelOpen or orig.IsPanelOpen
        if type(f) ~= 'function' then return false end
        local open = false
        pcall(function() open = f(mainTbl, name) end)
        return open and true or false
    end

    emit = function(iface)
        if not iface then iface = get_main_interface() end
        local E = iface_env or G
        local parts = {}
        local function add(k, v)
            parts[#parts + 1] = jstr(k) .. ':' .. jstr(v)
        end

        parts[#parts + 1] = '"v":1'
        parts[#parts + 1] = '"t":' .. wall()

        local inGame = truthy(ui(iface, 'IsInGame()'))
        add('in_game', inGame and '1' or '0')

        -- k2's IsInQueue UICmd is a stub that always returns "false", so use
        -- the front-end script's own state: Matchmaking.CurrentState
        -- (Enum.MatchmakingState: 0 Offline, 1 NotInGroup, 2 InGroup, 3 InQueue).
        local mmstate = nil
        local M = E.Matchmaking or G.Matchmaking
        if type(M) == 'table' and M.CurrentState ~= nil then
            mmstate = tonumber(tostring(M.CurrentState))
        end
        local inQueue = watch.in_queue
        if inQueue == nil then inQueue = (mmstate == 3) end
        if inGame then inQueue = false end
        add('in_queue', inQueue and '1' or '0')
        if mmstate then parts[#parts + 1] = '"mm_state":' .. math.floor(mmstate) end

        local gp = ui(iface, 'GetCurrentGamePhase()')
        if gp ~= nil then parts[#parts + 1] = '"game_phase":' .. inum(gp) end

        local lp = ui(iface, 'GetCurrentLobbyPhase()')
        if lp ~= nil then parts[#parts + 1] = '"lobby_phase":' .. inum(lp) end

        local hero = ui(iface, 'GetLocalHeroName()')
        if type(hero) == 'string' and hero ~= '' then add('hero', hero) end

        local account = ui(iface, 'GetAccountName()')
        if type(account) == 'string' and account ~= '' then add('account', account) end

        -- matchmade vs custom, and spectator/client view
        local arranged = ui(iface, 'IsArrangedMatch()')
        if arranged ~= nil and tostring(arranged) ~= '' then add('arranged', arranged) end

        local lc = ui(iface, 'GetLocalClientNumber()')
        if lc ~= nil and tostring(lc) ~= '' then add('local_client', lc) end
        local vc = ui(iface, 'GetViewClientNumber()')
        if vc ~= nil and tostring(vc) ~= '' then add('view_client', vc) end
        local vh = ui(iface, 'GetViewHeroName()')
        if type(vh) == 'string' and vh ~= '' then add('view_hero', vh) end

        if inGame then
            if hud.clock ~= nil then
                parts[#parts + 1] = '"clock":' .. math.floor(hud.clock)
            end
            if hud.level then
                parts[#parts + 1] = '"level":' .. math.floor(hud.level)
            end
            if hud.kills and hud.deaths and hud.assists then
                parts[#parts + 1] = '"kills":' .. math.floor(hud.kills)
                parts[#parts + 1] = '"deaths":' .. math.floor(hud.deaths)
                parts[#parts + 1] = '"assists":' .. math.floor(hud.assists)
            end
        else
            hud_reset()
        end

        -- lobby / matchmade info (EArrangedMatchType + world + rules mode).
        -- Lobby (game_lobby.lua) is read through the UI's Lua environment; the
        -- LobbyGameInfo trigger is the fallback/primary.
        local L = E.Lobby or G.Lobby
        local info = (type(L) == 'table' and type(L.lobbyGameInfo) == 'table')
                     and L.lobbyGameInfo or {}
        local amt = info.arrangedMatchType
        local world = info.worldFancyName
        local lmode = info.gameModeName
        local lname = info.lobbyGameName
        local mid = info.matchID
        local function override(v, w)
            if w ~= nil and tostring(w) ~= '' then return w end
            return v
        end
        amt = override(amt, watch.amt)
        world = override(world, watch.world)
        lmode = override(lmode, watch.mode)
        lname = override(lname, watch.name)
        mid = override(mid, watch.match_id)
        local function addfield(key, value)
            if value ~= nil and tostring(value) ~= '' then add(key, value) end
        end
        addfield('arranged_match', amt)
        addfield('world', world)
        addfield('lobby_mode', lmode)
        addfield('lobby_name', lname)
        addfield('match_id', mid)

        -- which full-screen menu panel is open. Main:FSPanelToggle fires
        -- Trigger('FSPanelTrigger', name) (captured by our Trigger hook) and
        -- Main:IsPanelOpen(name) is the getter fallback.  A stale last event
        -- (e.g. "learn" from before a match) is only trusted while the panel
        -- is really open.
        local mainTbl = E.Main or G.Main
        local canVerify = type(mainTbl) == 'table'
            and type(mainTbl.IsPanelOpen) == 'function'
        local panelName = watch.panel
        if canVerify and panelName and panelName ~= ''
                and not is_panel_open(mainTbl, panelName) then
            panelName = ''
            watch.panel = ''
        end
        if canVerify and (panelName == nil or panelName == '') then
            for i = 1, #PANELS do
                if is_panel_open(mainTbl, PANELS[i]) then
                    panelName = PANELS[i]
                    break
                end
            end
        end
        if panelName and panelName ~= '' then add('panel', panelName) end

        -- diagnostics: which environment/registry paths are actually alive
        parts[#parts + 1] = '"env":' .. (iface_env and 1 or 0)
        parts[#parts + 1] = '"uim":' .. (ui_manager() and 1 or 0)
        parts[#parts + 1] = '"arm":' .. arm_count
        parts[#parts + 1] = '"gi":' .. (hud_game and 1 or 0)
        local U2 = ui_manager()
        if U2 then
            local okA, act = pcall(function() return U2:GetActiveInterface() end)
            if okA and act then
                local okN, nm = pcall(function() return act:GetName() end)
                if okN and type(nm) == 'string' and nm ~= '' then add('act', nm) end
            end
        end
        local hc = 0
        for _ in pairs(hudHosts) do hc = hc + 1 end
        parts[#parts + 1] = '"hudhosts":' .. hc

        parts[#parts + 1] = '"n":' .. emitCount
        raw_print('{' .. table.concat(parts, ',') .. '}')
        flush_log(iface)
    end

    guarded_emit = function(force)
        local t = now_ms()
        if t ~= nil and lastEmit ~= nil then
            local min_gap = force and MIN_GAP_MS or HEARTBEAT_MS
            if t >= lastEmit and (t - lastEmit) < min_gap then return end
        end
        local iface = main_iface or get_main_interface()
        local ok, err = pcall(emit, iface)
        if not ok then
            raw_print('{"v":1,"err":' .. jstr(tostring(err)) .. ',"n":' .. emitCount .. '}')
        end
        emitCount = emitCount + 1
        lastEmit = now_ms()
    end

    emit_now = function() guarded_emit(true) end
    emit_beat = function() guarded_emit(false) end

    ----------------------------------------------------------
    -- scheduling: widget timer + per-frame callback
    ----------------------------------------------------------
    heartbeat = function(w)
        register_hud()
        if not watchRegistered then
            register_watches(main_iface or w or anchor)
        end
        emit_beat()
    end

    onframe = function(w)
        -- frames from a widget of a replaced interface must not report
        if w and anchor and w ~= anchor then return end
        heartbeat(w)
    end

    reschedule = function()
        if not anchor then return end
        pcall(function() anchor:Sleep(1000, sleeper) end)
    end

    sleeper = function(w)
        if w and anchor and w ~= anchor then
            -- a timer from a previous interface: re-arm instead of dying
            ensure()
            return
        end
        heartbeat(w)
        reschedule()
    end

    ----------------------------------------------------------
    -- arming / re-arming
    --
    -- start() used to be one-shot ("if started then return").  The client
    -- re-executes main.interface after the Direct3D restart on launch, which
    -- destroys the widget the timer hangs on; the bridge then stayed silent
    -- for the whole session.  Now every entry point re-checks which interface
    -- is current and moves the timer onto the new one.
    ----------------------------------------------------------
    start = function(self, force)
        if starting then return end
        starting = true
        install_trigger_hook()

        local iface = get_main_interface()
        local w = get_timer_widget(iface)

        if not w and self and type(self.GetWidget) == 'function' then
            pcall(function() w = self:GetWidget('fe3_background_image') end)
            if not w then
                pcall(function() w = self:GetWidget('login_widgets') end)
            end
            if w and not iface then iface = w end
        end

        if not w then
            if not anchor then started = false end
            starting = false
            return
        end

        if started and not force and w == anchor and iface == main_iface then
            starting = false
            return
        end

        if iface then main_iface = iface end
        anchor = w
        started = true
        arm_count = arm_count + 1

        -- the watches/HUD handlers belonged to the previous widget: every
        -- interface object has its own registry.  Keep what we already knew
        -- about the open panel/queue so a rebuild does not blank the presence.
        watchRegistered = false
        hudHosts = {}
        local keepPanel = watch.panel
        local keepQueue = watch.in_queue
        watch = { in_queue = keepQueue, found = nil, amt = nil, world = nil,
                  mode = nil, name = nil, match_id = nil, panel = keepPanel }
        -- Keep hud.* here: PlayerScore may not fire again after a re-arm until
        -- the next kill/death/assist, which made the KDA disappear mid-match.
        -- guarded_emit() below resets it when the game is not running.

        pcall(function() w:SetCallback('onframe', onframe) end)
        pcall(function() w:Sleep(1000, sleeper) end)
        register_hud()
        register_watches(main_iface or w)

        raw_print('{"v":1,"boot":1,"build":' .. BUILD .. ',"arm":' ..
            arm_count .. ',"has_widget":1}')
        guarded_emit(true)
        starting = false
    end

    ensure = function()
        start(nil, false)
    end

    ----------------------------------------------------------
    -- hook the engine's global Trigger()
    --
    -- main.interface is re-executed on a UI rebuild and reassigns every
    -- `Main.*` key; Lua only honours a table's __newindex for *new* keys, so
    -- the per-function wrapper installed on the first load is replaced and
    -- never runs again.  Trigger() is an engine global that is not
    -- reassigned, and Main:FSPanelToggle calls it for every panel change, so
    -- wrapping it gives the bridge a hook that survives the rebuild.
    ----------------------------------------------------------
    install_trigger_hook = function()
        if trigger_hooked then return end
        local orig_trigger = G.Trigger
        if type(orig_trigger) ~= 'function' then return end
        trigger_hooked = true
        G.Trigger = function(name, ...)
            if trigger_busy then return orig_trigger(name, ...) end
            trigger_busy = true
            local isPanel = (name == 'FSPanelTrigger')
            if isPanel then
                local p = ...
                if type(p) ~= 'string' then p = tostring(p or '') end
                watch.panel = p
            end
            -- adopt a rebuilt interface *before* the engine dispatches this
            -- trigger to the watch list of the (now stale) one
            pcall(start, nil, false)
            local res = { pcall(orig_trigger, name, ...) }
            trigger_busy = false
            if isPanel then pcall(emit_now) end
            if not res[1] then error(res[2], 0) end
            if unpack then return unpack(res, 2) end
            return res[2]
        end
    end

    ----------------------------------------------------------
    -- hook Main (main.interface onloadlua calls Main:Init)
    --
    -- We seed the global `Main` table with a __newindex metamethod
    -- that wraps every function main.lua assigns.  The wrapper
    -- calls start() once and then the original.  This makes the hook
    -- work no matter which Main method runs first, and keeps
    -- Main.Init itself working unchanged.
    ----------------------------------------------------------
    local function install_hook(t)
        local mt = getmetatable(t) or {}
        local oldnewindex = rawget(mt, '__newindex')
        mt.__newindex = function(tbl, k, v)
            if type(v) == 'function' then
                if G.getfenv then
                    local ok, env = pcall(G.getfenv, v)
                    -- only the environment that owns this `Main` table is the
                    -- UI environment (main.lua does `Main = _G['Main']`)
                    if ok and type(env) == 'table' and env.Main == tbl then
                        iface_env = env
                    end
                end
                orig[k] = v
                local wrapped = function(self, ...)
                    if not started then
                        pcall(start, self, true)
                    elseif k == 'Init' or k == 'OnReload' then
                        pcall(start, self, true)
                    elseif k == 'FSPanelToggle' or k == 'ToggleUI'
                            or k == 'InstantUnlock' then
                        pcall(start, self, false)
                    end
                    local res = { v(self, ...) }
                    if unpack then return unpack(res) end
                    return res[1]
                end
                rawset(tbl, k, wrapped)
            elseif oldnewindex then
                oldnewindex(tbl, k, v)
            else
                rawset(tbl, k, v)
            end
        end
        setmetatable(t, mt)
        return true
    end

    local hooked = false
    if type(G.Main) ~= 'table' then
        G.Main = {}
        hooked = install_hook(G.Main)
    elseif G.Main.Init == nil then
        hooked = install_hook(G.Main)
    end
    install_trigger_hook()
    raw_print('{"v":1,"boot":1,"build":' .. BUILD .. ',"hook":' ..
        jstr(hooked and 'armed' or 'unavailable') .. '}')
end)

if not okMod then
    pcall(function()
        local G = _G
        local msg = '[HONRP] {"v":1,"err":"' ..
            tostring(modErr):gsub('["\\]', "'") .. '"}\n'
        if G.print then G.print(msg) end
    end)
end
-- <<< HONRP END <<< (managed block, do not edit by hand)
