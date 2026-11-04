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
--  which lands in console.log (con_writeLog=1) where honrp.py
--  picks it up.  No game files are touched, no memory is patched.
----------------------------------------------------------

-- >>> HONRP BEGIN >>> (managed block, do not edit by hand)
local okMod, modErr = pcall(function()
    local G = _G
    local MARKER = '[HONRP]'
    local BUILD = 9
    local unpack = G.unpack or (G.table and G.table.unpack)

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
        local ok, v = pcall(function() return os and os.time and os.time() end)
        if ok and type(v) == 'number' then return v end
        return 0
    end

    -- console.log is stdio-buffered (the file can sit mid-line for minutes
    -- while in a match). CConsole::FlushLogs is a console command, reachable
    -- from Lua through the global Cmd()/UICmd("Cmd('FlushLogs')").
    local function flush_log(a)
        -- Cmd() is a Lua global in the UI environment; Cmd is also a UICmd.
        if G.Cmd then pcall(G.Cmd, 'FlushLogs') end
        if a then pcall(function() a:UICmd("Cmd('FlushLogs')") end) end
    end

    local function now_ms()
        local ok, v = pcall(function() return G.GetTime and G.GetTime() end)
        if ok and type(v) == 'number' then return v end
        return wall() * 1000
    end

    ----------------------------------------------------------
    -- UI access
    ----------------------------------------------------------
    local anchor = nil

    local function ui_manager()
        local E = iface_env or G
        return E.UIManager or G.UIManager
    end

    local function get_interface()
        local U = ui_manager()
        if U then
            local ok, w = pcall(function() return U:GetActiveInterface() end)
            if ok and w then return w end
            ok, w = pcall(function() return U:GetInterface('main') end)
            if ok and w then return w end
        end
        -- no UIManager global: use the widget obtained from Main:GetWidget
        return anchor
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

    ----------------------------------------------------------
    -- state emit
    ----------------------------------------------------------
    local lastEmit = 0
    local started = false
    local starting = false
    local emitCount = 0

    local function truthy(v)
        local s = tostring(v or ''):lower()
        return s == '1' or s == 'true'
    end

    -- Live in-game data comes from the game HUD's own triggers
    -- (ui/scripts/game/mid_bar.lua and bot_center.lua):
    --   MatchClock(seconds, valid) / PlayerScore(k,d,a,ck,nk,denies) / ActiveLevel(level,...)
    local hud = {}
    local hudRegistered = false
    local guarded_emit

    -- Reporting cadence: state changes are emitted immediately (coalesced to
    -- one per ~0.9 s), the MatchClock/lobby countdown heartbeat every 10 s.
    local HEARTBEAT_MS = 10000
    local MIN_GAP_MS = 900
    local function emit_now() if guarded_emit then guarded_emit(true) end end
    local function emit_beat() if guarded_emit then guarded_emit(false) end end

    -- Queue/lobby data comes from the same triggers matchmaking.lua and
    -- game_lobby.lua consume. Table reads through iface_env are a fallback.
    local watch = { in_queue = nil, found = nil,
                    amt = nil, world = nil, mode = nil,
                    name = nil, match_id = nil, panel = nil }
    local watchRegistered = false

    local function register_watches(w)
        if watchRegistered or not w then return end
        local ok = pcall(function()
            w:RegisterWatch('MatchmakingJoinQueue', function()
                watch.in_queue = true; watch.found = false
                emit_now()
            end)
            w:RegisterWatch('MatchmakingLeaveQueue', function()
                watch.in_queue = false
                emit_now()
            end)
            w:RegisterWatch('MatchmakingFoundMatch', function()
                watch.in_queue = false; watch.found = true
                emit_now()
            end)
            w:RegisterWatch('MatchmakingConnected', function()
                watch.in_queue = false
                emit_now()
            end)
            w:RegisterWatch('MatchmakingDisconnected', function()
                watch.in_queue = false
                emit_now()
            end)
            w:RegisterWatch('LobbyGameInfo', function(_, ...)
                watch.amt = select(17, ...)
                watch.world = select(4, ...)
                watch.mode = select(13, ...)
                watch.name = select(8, ...)
                watch.match_id = select(9, ...)
                emit_now()
            end)
            w:RegisterWatch('LobbyCountDown', function() emit_beat() end)
            w:RegisterWatch('HeroSelectInfo', function() emit_now() end)
            -- fired by Main:FSPanelToggle with the panel name ('' when closed)
            w:RegisterWatch('FSPanelTrigger', function(_, panelName)
                watch.panel = tostring(panelName or '')
                emit_now()
            end)
        end)
        if ok then watchRegistered = true end
    end

    local function hud_reset()
        hud.clock = nil; hud.clockValid = nil; hud.level = nil
        hud.kills = nil; hud.deaths = nil; hud.assists = nil
    end

    local hudHosts = {}

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

    local function register_hud()
        register_hud_on(anchor)
        local U = ui_manager()
        if U then
            local game = nil
            pcall(function() game = U:GetInterface('game') end)
            if game and game ~= anchor then register_hud_on(game) end
        end
    end

    local function emit(iface)
        local parts = {}
        local function add(k, v)
            parts[#parts + 1] = jstr(k) .. ':' .. jstr(v)
        end

        parts[#parts + 1] = '"v":1'
        parts[#parts + 1] = '"t":' .. wall()

        local in_game = ui(iface, 'IsInGame()')
        local inGame = truthy(in_game)
        add('in_game', inGame and '1' or '0')

        -- k2's IsInQueue UICmd is a stub that always returns "false", so use
        -- the front-end script's own state: Matchmaking.CurrentState
        -- (Enum.MatchmakingState: 0 Offline, 1 NotInGroup, 2 InGroup, 3 InQueue).
        local E = iface_env or G
        local mmstate = nil
        local M = E.Matchmaking
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
        if hero and tostring(hero) ~= '' then add('hero', hero) end

        local account = ui(iface, 'GetAccountName()')
        if account and tostring(account) ~= '' then add('account', account) end

        -- matchmade vs custom, and spectator/client view
        local arranged = ui(iface, 'IsArrangedMatch()')
        if arranged ~= nil and tostring(arranged) ~= '' then add('arranged', arranged) end

        local lc = ui(iface, 'GetLocalClientNumber()')
        if lc ~= nil and tostring(lc) ~= '' then add('local_client', lc) end
        local vc = ui(iface, 'GetViewClientNumber()')
        if vc ~= nil and tostring(vc) ~= '' then add('view_client', vc) end
        local vh = ui(iface, 'GetViewHeroName()')
        if vh and tostring(vh) ~= '' then add('view_hero', vh) end

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
        -- Lobby (game_lobby.lua) is read through the main interface's Lua
        -- environment; the LobbyGameInfo trigger is the fallback/primary.
        local L = E.Lobby
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

        -- which fullscreen menu panel is open. Main:FSPanelToggle fires
        -- Trigger('FSPanelTrigger', name); Main:IsPanelOpen(name) is its getter.
        local panelName = watch.panel
        if (panelName == nil or panelName == '') and type(G.Main) == 'table'
                and G.Main.IsPanelOpen then
            local names = { 'plinko', 'store', 'shop', 'ladder', 'profile',
                            'matchstats', 'motd', 'learn', 'game_lobby', 'play' }
            for i = 1, #names do
                local open = false
                pcall(function() open = G.Main:IsPanelOpen(names[i]) end)
                if open then panelName = names[i]; break end
            end
        end
        if panelName and panelName ~= '' then add('panel', panelName) end

        -- diagnostics: which environment/registry paths are actually alive
        parts[#parts + 1] = '"env":' .. (iface_env and 1 or 0)
        parts[#parts + 1] = '"uim":' .. (ui_manager() and 1 or 0)
        local hc = 0
        for _ in pairs(hudHosts) do hc = hc + 1 end
        parts[#parts + 1] = '"hudhosts":' .. hc

        parts[#parts + 1] = '"n":' .. emitCount
        raw_print('{' .. table.concat(parts, ',') .. '}')
        flush_log(iface)
    end

    guarded_emit = function(force)
        local t = now_ms()
        local min_gap = force and MIN_GAP_MS or HEARTBEAT_MS
        if lastEmit ~= 0 and (t - lastEmit) < min_gap then return end
        local iface = get_interface()
        local ok, err = pcall(emit, iface)
        if not ok then
            raw_print('{"v":1,"err":' .. jstr(tostring(err)) .. ',"n":' .. emitCount .. '}')
        end
        emitCount = emitCount + 1
        lastEmit = now_ms()
    end

    ----------------------------------------------------------
    -- scheduling: widget timer + per-frame callback
    ----------------------------------------------------------
    local function heartbeat(w)
        register_hud()
        if not watchRegistered then register_watches(w or anchor) end
        emit_beat()
    end

    local function reschedule(w)
        if w then
            local ok = pcall(function() w:Sleep(1000, sleeper) end)
            if ok then return end
        end
        local w2 = get_interface()
        if w2 and w2 ~= w then
            pcall(function() w2:Sleep(1000, sleeper) end)
        end
    end

    local function sleeper(w)
        heartbeat(w)
        reschedule(w)
    end

    local function onframe(w)
        heartbeat(w)
    end

    local function start(self)
        if started or starting then return end
        starting = true
        local w = get_interface()
        if not w and self and self.GetWidget then
            local ok, ww = pcall(function() return self:GetWidget('fe3_background_image') end)
            if ok then w = ww end
        end
        if not w and self and self.GetWidget then
            local ok, ww = pcall(function() return self:GetWidget('login_widgets') end)
            if ok then w = ww end
        end
        if not w then
            -- UI not up yet; retry on the next Main call
            starting = false
            return
        end
        started = true
        anchor = w
        register_hud()
        register_watches(w)
        raw_print('{"v":1,"boot":1,"build":' .. BUILD .. ',"has_widget":1}')
        guarded_emit()
        pcall(function() w:SetCallback('onframe', onframe) end)
        pcall(function() w:Sleep(1000, sleeper) end)
        -- The main interface may stop updating during a match; the game HUD
        -- interface keeps ticking there, so run a second timer on it.
        local game = nil
        local UIm = ui_manager()
        if UIm then
            pcall(function() game = UIm:GetInterface('game') end)
        end
        if game and game ~= w then
            pcall(function() game:SetCallback('onframe', onframe) end)
            pcall(function() game:Sleep(1000, sleeper) end)
        end
        starting = false
    end

    ----------------------------------------------------------
    -- hook Main (main.interface onloadlua calls Main:Init)
    --
    -- We seed the global `Main` table with a __newindex metamethod
    -- that wraps every function main.lua assigns.  The wrapper
    -- calls start() once (idempotent) and then the original.
    -- This makes the hook work no matter which Main method runs
    -- first, and keeps Main.Init itself working unchanged.
    ----------------------------------------------------------
    local iface_env = nil

    local function install_hook(t)
        local mt = getmetatable(t) or {}
        local oldnewindex = mt.__newindex
        mt.__newindex = function(tbl, k, v)
            if type(v) == 'function' then
                if not iface_env and G.getfenv then
                    local ok, env = pcall(G.getfenv, v)
                    if ok and type(env) == 'table' then iface_env = env end
                end
                local wrapped = function(self, ...)
                    if not started then pcall(start, self) end
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
