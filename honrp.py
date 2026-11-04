#!/usr/bin/env python3
"""
honrp - Discord Rich Presence for Heroes of Newerth on Linux (Wine / Proton).

Two layers:

1. A tiny Lua mod (honrp_mod.lua -> <userdir>/startup.lua) runs inside the
   game's own UI Lua VM and emits one JSON line per second into
   <userdir>/console.log.  That gives us the *real* client state
   (IsInGame / IsInQueue / hero / phases) with no binary patching.

2. This daemon finds the running juvio.exe, tails console.log, and talks
   Discord IPC over the native $XDG_RUNTIME_DIR/discord-ipc-N socket.

If the mod is not installed the daemon degrades gracefully to log/file
heuristics (match log presence, world-load lines in console.log).

Usage:
    honrp.py run                     # start the presence daemon
    honrp.py status                  # print the detected state as JSON
    honrp.py install [--user-dir D]  # install the Lua mod
    honrp.py uninstall               # remove the Lua mod
    honrp.py mock-discord PATH       # test helper: fake Discord IPC server
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import signal
import socket
import struct
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass, field
from typing import Optional

# ---------------------------------------------------------------------------
# constants / pretty names
# ---------------------------------------------------------------------------

APP_NAME = "Heroes of Newerth"
MOD_MARKER = "[HONRP]"
MOD_BEGIN = "--- HONRP BEGIN ---"
MOD_END = "--- HONRP END ---"
MOD_VERSION = 1

# Enum.LobbyPhase in ui/scripts/shared/enums.lua (both the front-end enum and
# the server CGameInfo::ELobbyPhase use the same small ordering).
LOBBY_PHASE = {
    0: "Lobby",
    1: "In lobby",
    2: "Starting",
    3: "Waiting for players",
    4: "Draft",
    5: "Final phase",
}

# Map ids seen in match logs / string tables -> display name.
MAP_NAMES = {
    "caldavar": "Forests of Caldavar",
    "caldavarphoenix": "Forests of Caldavar (Phoenix)",
    "midwars": "Mid Wars",
    "midwars_extended": "Mid Wars Extended",
    "fieldtest": "Field Test",
    "showdown": "Showdown",
    "blankworld": "Blank World",
}

GAME_MODE_NAMES = {
    "mode_normal": "All Pick",
    "mode_banningpick": "Banning Pick",
    "mode_banningdraft": "Banning Draft",
    "mode_banningdraftblitz": "Blitz Banning Draft",
    "mode_blindbanningpick": "Blind Banning",
    "mode_blindbanningpickblitz": "Blitz Blind Banning",
    "mode_randomdraft": "Random Draft",
    "mode_singledraft": "Single Draft",
    "mode_sameherodraft": "Same Hero Draft",
    "mode_captainsmode": "Captains Mode",
    "mode_lockpick": "Lock Pick",
    "mode_shufflepick": "Shuffle Pick",
    "mode_allrandom": "All Random",
    "mode_balancedrandom": "Balanced Random",
    "mode_heroban": "Hero Ban",
    "mode_allpick": "All Pick",
    "mode_allpick": "All Pick",
    "mode_all_pick": "All Pick",
    "mode_randomdraft": "Random Draft",
    "mode_random_draft": "Random Draft",
    "mode_captainsmode": "Captains Mode",
    "mode_captains_mode": "Captains Mode",
    "mode_blindpick": "Blind Pick",
    "mode_blind_pick": "Blind Pick",
    "mode_banningpick": "Banning Pick",
    "mode_banningdraft": "Banning Draft",
    "mode_lockpick": "Lock Pick",
    "mode_shufflepick": "Shuffle Pick",
    "mode_allrandom": "All Random",
    "mode_balancedrandom": "Balanced Random",
    "mode_single_draft": "Single Draft",
    "mode_heroban": "Hero Ban",
    "mode_none": "",
}


def pretty_map(map_id: str, short: bool = True) -> str:
    key = (map_id or "").strip().lower()
    if not key:
        return ""
    if short:
        if key in ("caldavar", "caldavarphoenix"):
            return "FoW"
        if key == "midwars":
            return "Mid Wars"
        if key == "fieldtest":
            return "Field Test"
    if key in MAP_NAMES:
        return MAP_NAMES[key]
    # strip "map_" / "world_" prefixes if the caller passed a resource id
    key = re.sub(r"^(map|world)_", "", key)
    return MAP_NAMES.get(key) or key.replace("_", " ").title()


def pretty_hero(name: str) -> str:
    """Hero_Pyromancer -> Pyromancer, Hero_DrRepulsor -> Dr Repulsor."""
    h = (name or "").strip()
    if h.lower().startswith("hero_"):
        h = h[5:]
    if h and " " not in h and "_" not in h:
        h = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", h)
    return h


# EArrangedMatchType (e_arrangedmatchtype.h, mirrored in game_lobby.lua)
ARRANGED_MATCH_TYPE = {
    2: "Ranked",
    3: "Ranked",
    4: "Unranked",
    5: "Unranked",
}

# full-screen menu panels from ui/fe3/main.interface (fs_panel{body})
PANEL_LABELS = {
    "plinko": "Playing Plinko",
    "plinko_v2": "Playing Plinko",
    "store": "Browsing the Store",
    "shop": "Browsing the Shop",
    "ladder": "Viewing the Ladder",
    "profile": "Viewing a Profile",
    "matchstats": "Viewing Match Stats",
    "motd": "",
    "learn": "In the Learn tab",
    "game_lobby": "In a game lobby",
    "play": "",
}


# client's own comment in ui/scripts/game/game_shop_hd.lua:
#   0 IDLE, 1 WAITINGFORPLAYERS, 2 PREGAME, 3 INGAME, 4 POSTGAME, 5 FINISHED
GAME_SERVER_PHASE = {
    1: "Waiting for players",
    2: "Hero Select",
    4: "Post-Match",
    5: "Match Over",
}


def in_match(s: "State") -> bool:
    """IsInGame is false during PREGAME (hero pick); phase 3 is the real match."""
    return s.in_game or s.game_phase == 3


def in_pregame(s: "State") -> bool:
    if in_match(s):
        return False
    if s.game_phase in (1, 2):
        return True
    # the lobby panel is authoritative; stale lobby data can linger for ages,
    # so never infer a lobby from world/lobby_name alone
    return (s.panel == "game_lobby"
            and s.local_client is not None and s.local_client >= 0)


def is_spectating(s: "State") -> bool:
    if not in_match(s):
        return False
    if (s.local_client is not None and s.view_client is not None
            and s.local_client != s.view_client):
        return True
    # spectators have no local hero but do have a viewed hero
    return not s.hero and bool(s.view_hero)


def ranked_label(s: "State") -> str:
    return ARRANGED_MATCH_TYPE.get(s.arranged_match, "")


def pretty_mode(mode: str) -> str:
    key = (mode or "").strip().lower()
    if not key:
        return ""
    if key in GAME_MODE_NAMES:
        return GAME_MODE_NAMES[key]
    key = re.sub(r"^mode_", "", key)
    return GAME_MODE_NAMES.get("mode_" + key) or key.replace("_", " ").title()


# ---------------------------------------------------------------------------
# process / path discovery
# ---------------------------------------------------------------------------


def read_file(path: str) -> str:
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def read_text(path: str, limit: int = 1 << 20) -> str:
    try:
        with open(path, "rb") as f:
            return f.read(limit).decode("utf-8", "replace")
    except OSError:
        return ""


def read_environ(pid: int) -> dict:
    env = {}
    try:
        with open(f"/proc/{pid}/environ", "rb") as f:
            for item in f.read().split(b"\0"):
                if b"=" in item:
                    k, v = item.split(b"=", 1)
                    env[k.decode("utf-8", "replace")] = v.decode("utf-8", "replace")
    except OSError:
        pass
    return env


def proc_start_time(pid: int) -> Optional[float]:
    """Wall-clock start time of a process (Linux, /proc/pid/stat field 22)."""
    try:
        stat = read_text(f"/proc/{pid}/stat")
        # comm may contain spaces/parens: cut at the last ')'
        rest = stat[stat.rindex(")") + 2:].split()
        start_ticks = int(rest[19])
        with open("/proc/stat", "r") as f:
            btime = None
            for line in f:
                if line.startswith("btime "):
                    btime = int(line.split()[1])
                    break
        if btime is None:
            return None
        return btime + start_ticks / os.sysconf("SC_CLK_TCK")
    except (OSError, ValueError, IndexError):
        return None


def is_game_process(pid: int) -> bool:
    try:
        with open(f"/proc/{pid}/comm", "rb") as f:
            comm = f.read().strip().lower()
        if comm == b"juvio.exe":
            return True
        cmd = read_text(f"/proc/{pid}/cmdline").lower()
        return "juvio.exe" in cmd and "bash" not in cmd.split("\0")[0]
    except OSError:
        return False


@dataclass
class GameProcess:
    pid: int
    prefix: Optional[str]
    cmdline: str
    start_time: Optional[float]


def find_games() -> list[GameProcess]:
    out = []
    try:
        pids = [int(x) for x in os.listdir("/proc") if x.isdigit()]
    except OSError:
        return out
    for pid in pids:
        if not is_game_process(pid):
            continue
        env = read_environ(pid)
        prefix = env.get("WINEPREFIX") or None
        if not prefix and env.get("STEAM_COMPAT_DATA_PATH"):
            prefix = os.path.join(env["STEAM_COMPAT_DATA_PATH"], "pfx")
        cmdline = read_text(f"/proc/{pid}/cmdline").replace("\0", " ").strip()
        out.append(GameProcess(pid, prefix, cmdline, proc_start_time(pid)))
    out.sort(key=lambda g: g.start_time or 0)
    return out


def user_dir_candidates(prefix: Optional[str]) -> list[str]:
    cands = []
    if prefix:
        cands += glob.glob(
            os.path.join(prefix, "drive_c", "users", "*",
                         "Documents", "Juvio", "Heroes of Newerth"))
    # fall back to any Proton prefix on the machine
    for base in ("~/.steam/steam/steamapps/compatdata",
                 "~/.local/share/Steam/steamapps/compatdata",
                 "~/.wine"):
        base = os.path.expanduser(base)
        cands += glob.glob(os.path.join(
            base, "*", "pfx", "drive_c", "users", "*",
            "Documents", "Juvio", "Heroes of Newerth"))
        cands += glob.glob(os.path.join(
            base, "drive_c", "users", "*",
            "Documents", "Juvio", "Heroes of Newerth"))
    cands.append(os.path.expanduser("~/Documents/Juvio/Heroes of Newerth"))
    seen, out = set(), []
    for c in cands:
        if c not in seen and os.path.isdir(c):
            seen.add(c)
            out.append(c)
    return out


def pick_user_dir(prefix: Optional[str], explicit: Optional[str] = None) -> Optional[str]:
    if explicit:
        return explicit if os.path.isdir(explicit) else None
    cands = user_dir_candidates(prefix)
    if not cands:
        return None
    # prefer the directory with the most recently touched console.log
    def score(d):
        p = os.path.join(d, "console.log")
        try:
            return os.path.getmtime(p)
        except OSError:
            return 0
    return max(cands, key=score)


# ---------------------------------------------------------------------------
# console.log tailing (UTF-16LE)
# ---------------------------------------------------------------------------


class ConsoleTail:
    """Incrementally read complete UTF-16LE lines from a growing file."""

    def __init__(self, path: str):
        self.path = path
        self.offset = 0
        self.residual = b""
        self._ino = None

    def reset(self):
        self.offset = 0
        self.residual = b""

    def poll(self) -> list[str]:
        try:
            st = os.stat(self.path)
        except OSError:
            return []
        if self._ino != st.st_ino or st.st_size < self.offset:
            self.reset()
            self._ino = st.st_ino
        if st.st_size == self.offset:
            return []
        try:
            with open(self.path, "rb") as f:
                f.seek(self.offset)
                chunk = f.read(st.st_size - self.offset)
        except OSError:
            return []
        self.offset += len(chunk)
        data = self.residual + chunk
        cut = data.rfind(b"\n\x00")
        if cut < 0:
            self.residual = data[-1:] if len(data) % 2 else data
            return []
        head, self.residual = data[:cut + 2], data[cut + 2:]
        if self.offset <= 2 + len(chunk) and head.startswith(b"\xff\xfe"):
            head = head[2:]
        text = head.decode("utf-16-le", "replace")
        return [ln.rstrip("\r") for ln in text.split("\n") if ln]


# ---------------------------------------------------------------------------
# match log (the per-match text log the client writes)
# ---------------------------------------------------------------------------


@dataclass
class MatchLog:
    path: str
    mtime: float
    map_id: str = ""
    mode: str = ""
    match_name: str = ""
    started: Optional[float] = None


INFO_RE = {
    "date": re.compile(r'INFO_DATE\s+date:"([^"]*)"\s+time:"([^"]*)"'),
    "map": re.compile(r'INFO_MAP\s+name:"([^"]*)"'),
    "mode": re.compile(r'INFO_SETTINGS\s+mode:"([^"]*)"'),
    "match": re.compile(r'INFO_MATCH\s+name:"([^"]*)"'),
}


def parse_match_log(path: str) -> Optional[MatchLog]:
    try:
        with open(path, "rb") as f:
            raw = f.read(8192)
        text = raw.decode("utf-16-le", "replace")
    except OSError:
        return None
    out = MatchLog(path=path, mtime=os.path.getmtime(path))
    m = INFO_RE["map"].search(text)
    if m:
        out.map_id = m.group(1)
    m = INFO_RE["mode"].search(text)
    if m:
        out.mode = m.group(1)
    m = INFO_RE["match"].search(text)
    if m:
        out.match_name = m.group(1)
    m = INFO_RE["date"].search(text)
    if m:
        try:
            out.started = time.mktime(time.strptime(
                m.group(1) + " " + m.group(2), "%Y/%m/%d %H:%M:%S"))
        except ValueError:
            pass
    return out


def newest_match_log(user_dir: str) -> Optional[MatchLog]:
    best = None
    for p in glob.glob(os.path.join(user_dir, "logs", "game_*.log")):
        try:
            m = os.path.getmtime(p)
        except OSError:
            continue
        if best is None or m > best:
            best = m
    if best is None:
        return None
    for p in glob.glob(os.path.join(user_dir, "logs", "game_*.log")):
        try:
            if abs(os.path.getmtime(p) - best) < 1e-6:
                return parse_match_log(p)
        except OSError:
            continue
    return None


# ---------------------------------------------------------------------------
# state
# ---------------------------------------------------------------------------

WORLD_LOAD_RE = re.compile(r"Client (?:loading|resuming load of) world\s*(\S*)")
LEFT_GAME_RE = re.compile(
    r"(Disconnected|disconnect_|CMD\(Disconnect\)|MainMenu)")


@dataclass
class State:
    running: bool = False
    pid: Optional[int] = None
    session_start: Optional[float] = None
    source: str = "none"                 # "mod" | "fallback"
    mod_seen_at: float = 0.0             # monotonic
    in_game: bool = False
    in_queue: bool = False
    client_phase: Optional[int] = None
    game_phase: Optional[int] = None
    lobby_phase: Optional[int] = None
    hero: str = ""
    account: str = ""
    panel: str = ""
    arranged_match: Optional[int] = None
    arranged: Optional[bool] = None
    world: str = ""
    lobby_mode: str = ""
    lobby_name: str = ""
    match_id: str = ""
    local_client: Optional[int] = None
    view_client: Optional[int] = None
    view_hero: str = ""
    mm_state: Optional[int] = None
    kills: Optional[int] = None
    deaths: Optional[int] = None
    assists: Optional[int] = None
    level: Optional[int] = None
    clock: Optional[int] = None
    map_id: str = ""
    mode: str = ""
    match_started: Optional[float] = None
    match_source: str = ""
    mod_error: str = ""
    raw: dict = field(default_factory=dict)


def _int_or_none(v) -> Optional[int]:
    try:
        return int(str(v).strip().strip('"'))
    except (TypeError, ValueError):
        return None


def _truthy(v) -> bool:
    return str(v).strip().strip('"').lower() in ("1", "true", "yes")


class StateMachine:
    def __init__(self, user_dir: Optional[str], verbose: bool = False):
        self.user_dir = user_dir
        self.verbose = verbose
        self.tail = ConsoleTail(os.path.join(user_dir, "console.log")) if user_dir else None
        self.state = State()
        self._fallback_in_game = False
        self._fallback_map = ""
        self._fallback_match_start = None
        self._last_world_line = 0.0
        self._match_log_key = None
        self._pid = None
        self._saw_boot = False
        self._boot_time = 0.0
        self._warned_no_state = False
        # On startup the tail replays the whole console.log. Mod lines are
        # still useful (latest state), historical "Client loading world" /
        # "MainMenu" lines are not - they used to flip the fallback state.
        self._first_poll = True
        self._games = []
        self._games_at = 0.0

    # -- mod -----------------------------------------------------------
    def _consume_line(self, line: str, now_wall: float, now_mono: float):
        if MOD_MARKER not in line:
            if self.verbose and ("world" in line.lower() or "Disconnect" in line):
                pass
            m = WORLD_LOAD_RE.search(line)
            if m:
                self._fallback_in_game = True
                self._last_world_line = now_mono
                self._fallback_map = m.group(1).strip('"') or self._fallback_map
                if not self._fallback_match_start:
                    self._fallback_match_start = now_wall
            elif LEFT_GAME_RE.search(line):
                if self._fallback_in_game:
                    self._fallback_in_game = False
                    self._fallback_match_start = None
            return

        payload = line.split(MOD_MARKER, 1)[1].strip()
        try:
            data = json.loads(payload)
        except json.JSONDecodeError:
            return
        if not isinstance(data, dict):
            return

        s = self.state
        if MOD_MARKER not in line and self._first_poll:
            return
        s.mod_seen_at = now_mono
        if data.get("boot"):
            # The mod is loaded; the UI tick starts at Main:Init.
            if not self._saw_boot:
                self._saw_boot = True
                self._boot_time = now_mono
                print(f"honrp: Lua bridge loaded ({data})", file=sys.stderr)
                if data.get("hook") == "unavailable":
                    print("honrp: warning: could not arm the Main hook; "
                          "state lines will not be produced", file=sys.stderr)
            elif self.verbose:
                print(f"mod: {data}", file=sys.stderr)
            return
        s.source = "mod"
        s.mod_seen_at = now_mono
        s.raw = data
        s.in_game = _truthy(data.get("in_game", "0"))
        s.in_queue = _truthy(data.get("in_queue", "0"))
        s.client_phase = _int_or_none(data.get("client_phase"))
        s.game_phase = _int_or_none(data.get("game_phase"))
        s.lobby_phase = _int_or_none(data.get("lobby_phase"))
        hero = str(data.get("hero") or "").strip()
        s.hero = "" if hero.lower() in ("", "nil", "none", "0") else hero
        s.account = str(data.get("account") or "").strip()
        s.panel = str(data.get("panel") or "").strip()
        s.arranged_match = _int_or_none(data.get("arranged_match"))
        av = data.get("arranged")
        s.arranged = None if av in (None, "") else _truthy(av)
        s.world = str(data.get("world") or "").strip()
        s.lobby_mode = str(data.get("lobby_mode") or "").strip()
        s.lobby_name = str(data.get("lobby_name") or "").strip()
        s.match_id = str(data.get("match_id") or "").strip()
        s.local_client = _int_or_none(data.get("local_client"))
        s.view_client = _int_or_none(data.get("view_client"))
        s.mm_state = _int_or_none(data.get("mm_state"))
        s.kills = _int_or_none(data.get("kills"))
        s.deaths = _int_or_none(data.get("deaths"))
        s.assists = _int_or_none(data.get("assists"))
        s.level = _int_or_none(data.get("level"))
        s.clock = _int_or_none(data.get("clock"))
        if s.mm_state == 3:            # Enum.MatchmakingState.InQueue
            s.in_queue = True
        vh = str(data.get("view_hero") or "").strip()
        s.view_hero = "" if vh.lower() in ("", "nil", "none", "0") else vh
        s.mod_error = str(data.get("err") or "")
        if s.in_game and not s.match_started:
            s.match_started = now_wall
            s.match_source = "mod"

    def games(self) -> list:
        """find_games() is a /proc scan; cache it so a 1 Hz poll stays cheap."""
        now = time.monotonic()
        if not self._games or (now - self._games_at) > 3.0:
            self._games = find_games()
            self._games_at = now
        return self._games

    # -- poll ----------------------------------------------------------
    def poll(self) -> State:
        now_wall = time.time()
        now_mono = time.monotonic()
        s = self.state

        games = self.games()
        if not games:
            self.state = State()
            return self.state
        g = games[-1]
        if self._pid != g.pid:
            # new game instance: console.log is recreated, drop all state
            self._pid = g.pid
            self._fallback_in_game = False
            self._fallback_map = ""
            self._fallback_match_start = None
            self._match_log_key = None
            self._saw_boot = False
            self._warned_no_state = False
            if self.tail:
                self.tail.reset()
        s.running = True
        s.pid = g.pid
        s.session_start = g.start_time

        if self.tail is None or not os.path.exists(self.tail.path):
            ud = pick_user_dir(g.prefix)
            if ud:
                self.user_dir = ud
                self.tail = ConsoleTail(os.path.join(ud, "console.log"))

        if self.tail:
            for line in self.tail.poll():
                try:
                    self._consume_line(line, now_wall, now_mono)
                except Exception as exc:  # never let a log line kill the daemon
                    if self.verbose:
                        print(f"line parse error: {exc}", file=sys.stderr)
            self._first_poll = False

        # match log (map / mode / start time) - only meaningful in a match
        if self.user_dir and in_match(s):
            key = None
            ml = newest_match_log(self.user_dir)
            if ml:
                key = (ml.path, ml.mtime)
            if ml and key != self._match_log_key:
                self._match_log_key = key
                if ml.map_id:
                    s.map_id = ml.map_id
                if ml.mode:
                    s.mode = ml.mode
                # trust INFO_DATE only for a log that belongs to this match
                # (recently written and not stale from a previous game)
                if (ml.started and ml.started <= now_wall + 120
                        and (now_wall - ml.started) <= 6 * 3600
                        and (now_wall - ml.mtime) <= 300):
                    s.match_started = ml.started
                    s.match_source = "matchlog"
        if not s.in_game:
            self._match_log_key = None

        # the HUD match clock is the authoritative match start
        if (in_match(s) and s.clock is not None and s.clock >= 0
                and s.match_source != "clock"):
            s.match_started = now_wall - s.clock
            s.match_source = "clock"

        # mod silence -> stale; fall back to log heuristics
        if s.source == "mod" and (now_mono - s.mod_seen_at) > 40:
            s.source = "fallback"
            s.raw = {}
            s.in_game = self._fallback_in_game
            s.in_queue = False
            s.hero = ""
            if s.in_game:
                s.match_started = self._fallback_match_start
                s.match_source = "console"
                if not s.map_id:
                    s.map_id = self._fallback_map

        if s.source == "none" and self._fallback_in_game:
            s.source = "fallback"
            s.map_id = s.map_id or self._fallback_map

        if s.source == "fallback":
            s.in_game = s.in_game or self._fallback_in_game
            if s.in_game and not s.match_started:
                s.match_started = self._fallback_match_start or now_wall
                s.match_source = "console"

        if (self._saw_boot and s.source != "mod" and not self._warned_no_state
                and (now_mono - self._boot_time) > 30):
            self._warned_no_state = True
            print("honrp: warning: Lua bridge is loaded but has not emitted state "
                  "lines yet (UI not ready, or the Main hook did not fire)",
                  file=sys.stderr)

        # menus: reset match clock once we have left the match
        if not in_match(s):
            s.match_started = None
            s.match_source = ""
            s.map_id = ""
            s.mode = ""
            if not s.in_queue and not in_pregame(s):
                s.hero = ""

        return s


# ---------------------------------------------------------------------------
# activity
# ---------------------------------------------------------------------------


def phase_label(s: State) -> str:
    if s.lobby_phase is not None:
        return LOBBY_PHASE.get(s.lobby_phase, "Lobby")
    return ""


def build_activity(s: State, cfg: dict) -> Optional[dict]:
    if not s.running:
        return None
    act: dict = {}
    large = cfg.get("large_image", "hon")
    if large:
        act["assets"] = {
            "large_image": large,
            "large_text": cfg.get("large_text", APP_NAME),
        }
    if cfg.get("small_image"):
        act["assets"]["small_image"] = cfg["small_image"]
        act["assets"]["small_text"] = cfg.get("small_text", "")

    if in_match(s):
        start = s.match_started
    elif in_pregame(s):
        start = None          # no meaningful clock while picking
    else:
        start = s.session_start
    if start and cfg.get("show_timer", False):
        act["timestamps"] = {"start": int(start * 1000)}

    short_maps = bool(cfg.get("short_maps", True))
    if in_pregame(s):
        mapname = pretty_map(s.world, short_maps)
        mode = pretty_mode(s.lobby_mode)
        ranked = ranked_label(s)
        act["details"] = (" • ".join(x for x in (ranked, mapname, mode) if x)
                          or "In a game lobby")
        bits = []
        if s.hero:
            bits.append(pretty_hero(s.hero))
        phase = GAME_SERVER_PHASE.get(s.game_phase or 0)
        if phase:
            bits.append(phase)
        elif s.lobby_name:
            bits.append("In a game lobby")
        act["state"] = " • ".join(bits) if bits else "In a game lobby"
    elif in_match(s):
        mapname = pretty_map(s.world or s.map_id, short_maps)
        mode = pretty_mode(s.lobby_mode or s.mode)
        ranked = ranked_label(s)
        if is_spectating(s):
            act["details"] = " • ".join(x for x in ("Spectating", mapname) if x)
            act["state"] = ("Watching " + pretty_hero(s.view_hero)
                            if s.view_hero else "Spectating")
        else:
            act["details"] = (" • ".join(x for x in (ranked, mapname, mode) if x)
                              or "In a match")
            bits = []
            hero_name = s.hero
            if not hero_name and not is_spectating(s):
                # GetLocalHeroName can be empty in some local/practice games;
                # the view hero is your own hero in normal play.
                hero_name = s.view_hero
            if hero_name:
                bits.append(pretty_hero(hero_name))
                if s.level:
                    bits.append(f"Lv {s.level}")
                if (cfg.get("show_kda", True)
                        and s.kills is not None and s.deaths is not None
                        and s.assists is not None):
                    bits.append(f"{s.kills}/{s.deaths}/{s.assists}")
            elif s.lobby_phase == 4:
                bits.append("Picking Phase")
            elif mode:
                bits.append(mode)
            act["state"] = " • ".join(bits) if bits else "In a match"
    elif s.in_queue:
        act["details"] = "Searching for a match"
        act["state"] = "In queue"
    else:
        act["details"] = PANEL_LABELS.get(s.panel, "") or "Main Menu"
        if s.account:
            act["state"] = s.account
        elif s.lobby_phase not in (None, 0) and phase_label(s):
            act["state"] = phase_label(s)

    # Discord caps these at 128 characters
    for k in ("details", "state"):
        if k in act:
            act[k] = act[k][:128]
    act["instance"] = False
    return act


# ---------------------------------------------------------------------------
# Discord IPC
# ---------------------------------------------------------------------------


def discord_sockets() -> list[str]:
    dirs = []
    for var in ("XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"):
        v = os.environ.get(var)
        if v:
            dirs.append(v)
    dirs += [f"/run/user/{os.getuid()}", "/tmp",
             os.path.expanduser("~/.var/app/com.discordapp.Discord/config/discord"),
             os.path.expanduser("~/.var/app/com.discordapp.Discord/tmp")]
    out = []
    for d in dict.fromkeys(dirs):
        out += sorted(glob.glob(os.path.join(d, "discord-ipc-*")))
    return out


class DiscordIPC:
    def __init__(self, client_id: str, ipc_path: Optional[str] = None, verbose=False):
        self.client_id = str(client_id)
        self.ipc_path = ipc_path
        self.verbose = verbose
        self.sock: Optional[socket.socket] = None
        self.ready = False
        self.last_error = None

    def connect(self) -> bool:
        paths = [self.ipc_path] if self.ipc_path else discord_sockets()
        for path in paths:
            if not path or not os.path.exists(path):
                continue
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.settimeout(1.0)
                s.connect(path)
                self.sock = s
                self._send(0, {"v": 1, "client_id": self.client_id})
                # Discord's RPC utility process can be slow right after a
                # client restart; 3 s was not enough on this machine.
                msg = self._recv_json(timeout=8.0)
                if msg and msg.get("cmd") == "DISPATCH" and msg.get("evt") == "READY":
                    self.ready = True
                    if self.verbose:
                        print(f"discord: connected via {path}", file=sys.stderr)
                    return True
                if msg and msg.get("evt") == "ERROR":
                    self.last_error = msg.get("data", {}).get("message", "error")
                elif msg is None:
                    self.last_error = "handshake timeout"
                    if self.verbose:
                        print(f"discord: handshake rejected: {self.last_error}",
                              file=sys.stderr)
                self.close()
            except OSError as exc:
                self.last_error = str(exc)
                self.close()
        return False

    # -- framing -------------------------------------------------------
    def _send(self, op: int, payload: dict):
        data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.sock.sendall(struct.pack("<II", op, len(data)) + data)

    def _recv_exact(self, n: int) -> Optional[bytes]:
        buf = b""
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                return None
            buf += chunk
        return buf

    def _recv_json(self, timeout: float = 1.0) -> Optional[dict]:
        if not self.sock:
            return None
        self.sock.settimeout(timeout)
        try:
            hdr = self._recv_exact(8)
            if not hdr:
                return None
            op, ln = struct.unpack("<II", hdr)
            body = self._recv_exact(ln) if ln else b""
            if body is None:
                return None
            if op == 3:  # ping
                self._send(4, json.loads(body.decode("utf-8", "replace")))
                return None
            return json.loads(body.decode("utf-8", "replace"))
        except (OSError, ValueError):
            return None

    def drain(self):
        """Read any pending replies without blocking for long."""
        if not self.sock:
            return
        try:
            while True:
                self.sock.settimeout(0.0)
                hdr = self._recv_exact(8)
                if not hdr:
                    return
                op, ln = struct.unpack("<II", hdr)
                body = self._recv_exact(ln) if ln else b""
                if self.verbose and body:
                    try:
                        msg = json.loads(body.decode("utf-8", "replace"))
                        if msg.get("evt") == "ERROR" or msg.get("cmd") == "SET_ACTIVITY":
                            print(f"discord <- {json.dumps(msg)[:400]}", file=sys.stderr)
                    except ValueError:
                        pass
        except (OSError, ValueError):
            return

    def set_activity(self, activity: dict, pid: int) -> bool:
        if not self.sock:
            return False
        payload = {
            "cmd": "SET_ACTIVITY",
            "args": {"pid": pid, "activity": activity},
            "nonce": str(uuid.uuid4()),
        }
        try:
            self._send(1, payload)
            return True
        except OSError as exc:
            self.last_error = str(exc)
            self.close()
            return False

    def close(self):
        self.ready = False
        if self.sock:
            try:
                self.sock.close()
            except OSError:
                pass
            self.sock = None


# ---------------------------------------------------------------------------
# config
# ---------------------------------------------------------------------------


def load_config(path: Optional[str] = None) -> dict:
    cfg = {
        "client_id": os.environ.get("HONRP_CLIENT_ID", ""),
        "large_image": "hon",
        "large_text": APP_NAME,
        "poll_interval": 1.0,
        "refresh_interval": 300,
        "show_timer": False,
        "show_kda": True,
        "short_maps": True,
        "ipc_socket": "",
    }
    candidates = [path] if path else [
        os.environ.get("HONRP_CONFIG"),
        os.path.expanduser("~/.config/honrp/config.json"),
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json"),
    ]
    for c in candidates:
        if c and os.path.exists(c):
            try:
                cfg.update(json.load(open(c)))
                break
            except (OSError, ValueError):
                pass
    return cfg


# ---------------------------------------------------------------------------
# install / uninstall
# ---------------------------------------------------------------------------


def mod_source() -> str:
    here = os.path.dirname(os.path.abspath(__file__))
    return open(os.path.join(here, "honrp_mod.lua"), encoding="utf-8").read()


def strip_existing_mod(text: str) -> str:
    while MOD_BEGIN in text:
        a = text.index(MOD_BEGIN)
        b = text.find(MOD_END, a)
        if b < 0:
            text = text[:a]
            break
        text = text[:a] + text[b + len(MOD_END):]
    return text


def cmd_install(args) -> int:
    games = find_games()
    ud = getattr(args, "user_dir", None) or pick_user_dir(games[-1].prefix if games else None)
    if not ud:
        print("error: could not find the Heroes of Newerth user directory.\n"
              "Pass --user-dir '/path/to/pfx/drive_c/users/<user>/Documents/"
              "Juvio/Heroes of Newerth'.", file=sys.stderr)
        return 1
    target = os.path.join(ud, "startup.lua")
    block = (MOD_BEGIN + "\n" + mod_source() + "\n" + MOD_END + "\n")
    existing = ""
    if os.path.exists(target):
        existing = open(target, encoding="utf-8", errors="replace").read()
        user_part = strip_existing_mod(existing)
        # only back up content the user actually wrote (never our own block)
        backup = target + ".honrp.bak"
        if user_part.strip() and not os.path.exists(backup):
            open(backup, "w", encoding="utf-8").write(user_part)
        existing = user_part
    open(target, "w", encoding="utf-8").write(existing.rstrip() + "\n\n" + block)
    print(f"installed Lua bridge -> {target}")
    print("")
    print("What this does:")
    print("  - adds a managed block to the game's startup.lua (backup kept)")
    print("  - the bridge prints one state line per event/10 s into console.log")
    print("  - the daemon reads that log and talks only to Discord's local socket")
    print("  - no internet, no game memory access, no input, no game files changed")
    print(f"  - ~0.4 KB/s of log while playing; daemon ~15 MiB RAM (Python prototype)")
    print("")
    print("Remove with: python3 honrp.py uninstall")
    print("Restart Heroes of Newerth to load the bridge.")
    return 0


def cmd_uninstall(args) -> int:
    games = find_games()
    ud = getattr(args, "user_dir", None) or pick_user_dir(games[-1].prefix if games else None)
    if not ud:
        print("error: user directory not found", file=sys.stderr)
        return 1
    target = os.path.join(ud, "startup.lua")
    if not os.path.exists(target):
        print("nothing to uninstall")
        return 0
    text = strip_existing_mod(open(target, encoding="utf-8", errors="replace").read())
    text = text.strip()
    if text:
        open(target, "w", encoding="utf-8").write(text + "\n")
    else:
        os.unlink(target)
    backup = target + ".honrp.bak"
    if os.path.exists(backup) and os.path.getsize(backup) == 0:
        os.unlink(backup)
    print(f"removed HONRP block from {target}")
    return 0


# ---------------------------------------------------------------------------
# run / status
# ---------------------------------------------------------------------------


def mod_state(ud: Optional[str], game: Optional[GameProcess]) -> dict:
    """Whether the Lua bridge is installed and whether it predates this run."""
    path = os.path.join(ud, "startup.lua") if ud else None
    installed = bool(path and MOD_MARKER in read_file(path))
    restart = False
    if installed and game and game.start_time:
        try:
            restart = os.path.getmtime(path) > game.start_time
        except OSError:
            pass
    return {"installed": installed, "restart_needed": restart}


def cmd_status(args) -> int:
    cfg = load_config(getattr(args, "config", None))
    games = find_games()
    prefix = games[-1].prefix if games else None
    ud = pick_user_dir(prefix, getattr(args, "user_dir", None))
    modinfo = mod_state(ud, games[-1] if games else None)
    sm = StateMachine(ud, verbose=True)
    st = sm.poll()
    out = {
        "game_process": None if not games else {
            "pid": games[-1].pid,
            "start_time": games[-1].start_time,
            "prefix": games[-1].prefix,
        },
        "user_dir": ud,
        "console_log": os.path.join(ud, "console.log") if ud else None,
        "mod_installed": modinfo["installed"],
        "restart_needed": modinfo["restart_needed"],
        "state": {
            "source": st.source,
            "in_game": st.in_game,
            "in_queue": st.in_queue,
            "client_phase": st.client_phase,
            "game_phase": st.game_phase,
            "lobby_phase": st.lobby_phase,
            "hero": st.hero,
            "account": st.account,
            "panel": st.panel,
            "arranged_match": st.arranged_match,
            "arranged": st.arranged,
            "world": st.world,
            "lobby_mode": st.lobby_mode,
            "view_client": st.view_client,
            "local_client": st.local_client,
            "view_hero": st.view_hero,
            "spectating": is_spectating(st),
            "mm_state": st.mm_state,
            "level": st.level,
            "kills": st.kills,
            "deaths": st.deaths,
            "assists": st.assists,
            "clock": st.clock,
            "map_id": st.map_id,
            "mode": st.mode,
            "match_started": st.match_started,
            "mod_last_seen_age": (round(time.monotonic() - st.mod_seen_at, 1)
                                  if st.mod_seen_at else None),
            "mod_error": st.mod_error,
        },
        "activity": build_activity(st, cfg),
    }
    print(json.dumps(out, indent=2))
    return 0


def cmd_run(args) -> int:
    cfg = load_config(getattr(args, "config", None))
    client_id = args.client_id or cfg.get("client_id") or ""
    ipc = args.ipc_socket or cfg.get("ipc_socket") or None
    if getattr(args, "dry_run", False):
        print("dry-run: no Discord connection", file=sys.stderr)
    elif not client_id:
        print("error: no Discord application id configured.\n"
              "Create an app at https://discord.com/developers/applications and put its\n"
              "Application ID in ~/.config/honrp/config.json ({\"client_id\": \"...\"})\n"
              "or export HONRP_CLIENT_ID=...", file=sys.stderr)
        return 2

    rpc = None if getattr(args, "dry_run", False) else DiscordIPC(client_id, ipc, verbose=getattr(args, "verbose", False))
    last_activity = None
    last_sent = 0.0
    last_warn = 0.0
    running = True

    def stop(*_):
        nonlocal running
        running = False
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)

    games = find_games()
    ud = pick_user_dir(games[-1].prefix if games else None, getattr(args, "user_dir", None))
    if not ud:
        print("warning: HoN user directory not found; will keep looking",
              file=sys.stderr)
    modinfo = mod_state(ud, games[-1] if games else None)
    if modinfo["installed"] and modinfo["restart_needed"]:
        print("honrp: note: the Lua bridge was installed after this game session "
              "started - restart HoN to activate it", file=sys.stderr)
    sm = StateMachine(ud, verbose=getattr(args, "verbose", False))
    print(f"honrp: watching {'pid ' + str(games[-1].pid) if games else 'for juvio.exe'}"
          f", user dir: {ud}", file=sys.stderr)

    interval = float(cfg.get("poll_interval", 1.0))
    refresh = float(cfg.get("refresh_interval", 300))
    last_pid = None          # process the current activity was published for
    while running:
        try:
            st = sm.poll()
            activity = build_activity(st, cfg)
            now = time.monotonic()
            if rpc is not None:
                if activity is None:
                    # no game: clear with the pid we published for, then drop
                    # the connection - a disconnect always clears the card
                    if rpc.ready:
                        if last_pid is not None:
                            rpc.set_activity({}, last_pid)
                        rpc.close()
                        if getattr(args, "verbose", False):
                            print("discord: game closed, presence cleared",
                                  file=sys.stderr)
                    last_activity = None
                    last_sent = 0.0
                elif not rpc.ready:
                    if not rpc.connect():
                        if now - last_warn > 30:
                            last_warn = now
                            print(f"honrp: waiting for Discord ({rpc.last_error or 'no ipc socket'})",
                                  file=sys.stderr)
                        time.sleep(min(5.0, interval * 5))
                        continue
                    # fresh connection: Discord has no activity for us, republish
                    last_activity = None
                    last_sent = 0.0
                else:
                    rpc.drain()
            changed = activity != last_activity
            due = (now - last_sent) >= 5.0
            if getattr(args, "dry_run", False):
                if changed:
                    print(json.dumps({"t": round(time.time(), 2),
                                      "activity": activity,
                                      "state": st.raw or {
                                          "in_game": st.in_game,
                                          "in_queue": st.in_queue,
                                          "source": st.source}},
                                     sort_keys=True), flush=True)
                    last_activity = activity
            elif (activity is not None
                  and (changed or (now - last_sent) >= refresh) and due):
                pid = st.pid or os.getpid()
                if rpc.set_activity(activity, pid):
                    last_pid = pid
                    last_activity = activity
                    last_sent = now
                    if getattr(args, "verbose", False):
                        print(f"discord: {json.dumps(activity)[:300]}", file=sys.stderr)
                        if st.raw:
                            print(f"  mod: {json.dumps(st.raw)[:400]}", file=sys.stderr)
            time.sleep(interval)
        except Exception as exc:  # keep the daemon alive
            print(f"honrp: error: {exc}", file=sys.stderr)
            time.sleep(2)
    if rpc is not None:
        if last_pid is not None:
            rpc.set_activity({}, last_pid)
        rpc.close()
    return 0


# ---------------------------------------------------------------------------
# mock discord server (test helper)
# ---------------------------------------------------------------------------


def cmd_mock_discord(args) -> int:
    path = args.path
    if os.path.exists(path):
        os.unlink(path)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(path)
    os.chmod(path, 0o600)
    srv.listen(1)
    print(f"mock discord listening on {path}", flush=True)

    def serve():
        conn, _ = srv.accept()
        buf = b""
        while True:
            chunk = conn.recv(65536)
            if not chunk:
                break
            buf += chunk
            while len(buf) >= 8:
                op, ln = struct.unpack("<II", buf[:8])
                if len(buf) < 8 + ln:
                    break
                body, buf = buf[8:8 + ln], buf[8 + ln:]
                msg = json.loads(body.decode("utf-8", "replace"))
                print("REQ", op, json.dumps(msg), flush=True)
                if op == 0:
                    resp = {"cmd": "DISPATCH", "evt": "READY", "data": {
                        "v": 1, "config": {"cdn_host": "", "api_endpoint": "",
                                           "environment": "production"},
                        "user": {"id": "0", "username": "mock", "discriminator": "0",
                                 "avatar": None}}}
                else:
                    resp = {"cmd": "SET_ACTIVITY", "evt": None,
                            "data": None, "nonce": msg.get("nonce")}
                out = json.dumps(resp).encode()
                try:
                    conn.sendall(struct.pack("<II", 1, len(out)) + out)
                except OSError:
                    break
        conn.close()

    try:
        serve()
    except KeyboardInterrupt:
        pass
    finally:
        srv.close()
        try:
            os.unlink(path)
        except OSError:
            pass
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv=None) -> int:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--user-dir", default=argparse.SUPPRESS,
                        help="override the HoN user directory")
    common.add_argument("--config", default=argparse.SUPPRESS,
                        help="config.json path")
    common.add_argument("-v", "--verbose", action="store_true",
                        default=argparse.SUPPRESS)

    ap = argparse.ArgumentParser(prog="honrp", description=__doc__,
                                 parents=[common],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd")

    p_run = sub.add_parser("run", help="run the presence daemon", parents=[common])
    p_run.add_argument("--client-id")
    p_run.add_argument("--ipc-socket")
    p_run.add_argument("--dry-run", action="store_true",
                       help="print presence updates instead of talking to Discord")
    p_run.set_defaults(func=cmd_run)

    p_status = sub.add_parser("status", help="show detected state", parents=[common])
    p_status.set_defaults(func=cmd_status)

    p_install = sub.add_parser("install", help="install the Lua mod", parents=[common])
    p_install.set_defaults(func=cmd_install)

    p_uninstall = sub.add_parser("uninstall", help="remove the Lua mod", parents=[common])
    p_uninstall.set_defaults(func=cmd_uninstall)

    p_mock = sub.add_parser("mock-discord", help="fake Discord IPC server")
    p_mock.add_argument("path")
    p_mock.set_defaults(func=cmd_mock_discord)

    args = ap.parse_args(argv)
    if not getattr(args, "func", None):
        ap.print_help()
        return 1
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
