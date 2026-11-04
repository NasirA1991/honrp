#!/usr/bin/env python3
"""Self-tests for honrp. Run: python3 tests/test_honrp.py"""

import json
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

import honrp  # noqa: E402


def write_utf16(path, text):
    with open(path, "wb") as f:
        f.write(b"\xff\xfe")
        f.write(text.encode("utf-16-le"))


class TestLogParsing(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="honrp-test-")
        os.makedirs(os.path.join(self.tmp, "logs"), exist_ok=True)
        self.console = os.path.join(self.tmp, "console.log")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_console_tail_and_mod_lines(self):
        write_utf16(self.console, "Engine start up...\n")
        tail = honrp.ConsoleTail(self.console)
        lines = tail.poll()
        self.assertEqual(lines, ["Engine start up..."])

        with open(self.console, "ab") as f:
            f.write('^cROLEPICK build RolePick_Play\n'.encode("utf-16-le"))
            f.write(
                '[HONRP] {"v":1,"in_game":"1","in_queue":"0",'
                '"game_phase":3,"lobby_phase":2,"hero":"Hero_Pyromancer",'
                '"team":"1","account":"evilleader"}\n'.encode("utf-16-le"))
        lines = tail.poll()
        self.assertEqual(len(lines), 2)
        self.assertIn("ROLEPICK", lines[0])

        sm = honrp.StateMachine(self.tmp)
        sm.tail = tail
        for ln in lines:
            sm._consume_line(ln, time.time(), time.monotonic())
        st = sm.state
        self.assertTrue(st.in_game)
        self.assertFalse(st.in_queue)
        self.assertEqual(st.hero, "Hero_Pyromancer")
        self.assertEqual(st.lobby_phase, 2)

    def test_boot_message_does_not_clobber(self):
        sm = honrp.StateMachine(self.tmp)
        sm._consume_line('[HONRP] {"v":1,"boot":1,"hook":"armed"}',
                         time.time(), time.monotonic())
        self.assertFalse(sm.state.in_game)

    def test_match_log_parse(self):
        body = (
            'INFO_DATE date:"2026/10/30" time:"00:13:57"\r\n'
            'INFO_SERVER name:"Unnamed Server"\r\n'
            'INFO_GAME name:"Juvio" version:"0.13.3.0"\r\n'
            'INFO_MATCH name:"Field Test" id:"4294967295"\r\n'
            'INFO_MAP name:"fieldtest" version:"0.0.0"\r\n'
            'INFO_SETTINGS mode:"Mode_Normal" options:"Option_None"\r\n')
        p = os.path.join(self.tmp, "logs", "game_00001.log")
        write_utf16(p, body)
        ml = honrp.parse_match_log(p)
        self.assertEqual(ml.map_id, "fieldtest")
        self.assertEqual(ml.mode, "Mode_Normal")
        self.assertIsNotNone(ml.started)
        self.assertEqual(honrp.pretty_map(ml.map_id), "Field Test")
        self.assertEqual(honrp.pretty_mode(ml.mode), "All Pick")

    def test_fallback_world_load(self):
        sm = honrp.StateMachine(self.tmp)
        sm._consume_line("Client loading world caldavar", time.time(), time.monotonic())
        self.assertTrue(sm._fallback_in_game)
        self.assertEqual(sm._fallback_map, "caldavar")


class TestActivity(unittest.TestCase):
    def test_in_game_activity(self):
        st = honrp.State()
        st.running = True
        st.pid = 42
        st.in_game = True
        st.hero = "Hero_Pyromancer"
        st.map_id = "midwars"
        st.mode = "Mode_Normal"
        st.match_started = 1000.0
        act = honrp.build_activity(st, {"large_image": "hon"})
        self.assertIn("Mid Wars", act["details"])
        self.assertIn("Pyromancer", act["state"])
        self.assertNotIn("timestamps", act)   # timer off by default
        self.assertLessEqual(len(act["details"]), 128)

    def test_timer_and_long_maps_are_optional(self):
        st = honrp.State(running=True, in_game=True, hero="Hero_Pyromancer",
                         arranged_match=2, map_id="caldavar",
                         lobby_mode="banningpick", match_started=1000.0)
        act = honrp.build_activity(st, {"show_timer": True, "short_maps": False})
        self.assertEqual(act["timestamps"]["start"], 1000000)
        self.assertEqual(act["details"], "Ranked \u2022 Forests of Caldavar \u2022 Banning Pick")

    def test_show_kda_false(self):
        st = honrp.State(running=True, in_game=True, hero="Hero_Pyromancer",
                         level=12, kills=4, deaths=2, assists=7,
                         map_id="caldavar", match_started=1000.0)
        self.assertEqual(honrp.build_activity(st, {})["state"],
                         "Pyromancer \u2022 Lv 12 \u2022 4/2/7")
        self.assertEqual(honrp.build_activity(st, {"show_kda": False})["state"],
                         "Pyromancer \u2022 Lv 12")

    def test_ranked_match_activity(self):
        st = honrp.State(running=True, in_game=True, hero="Hero_Pyromancer",
                         arranged_match=2, map_id="caldavar",
                         lobby_mode="banningpick", match_started=1000.0)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Ranked \u2022 FoW \u2022 Banning Pick")
        self.assertEqual(act["state"], "Pyromancer")

    def test_unranked_midwars_activity(self):
        st = honrp.State(running=True, in_game=True, hero="Hero_DrRepulsor",
                         arranged_match=5, map_id="midwars",
                         lobby_mode="allpick", match_started=1000.0)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Unranked \u2022 Mid Wars \u2022 All Pick")
        self.assertEqual(act["state"], "Dr Repulsor")

    def test_level_and_kda(self):
        st = honrp.State(running=True, in_game=True, hero="Hero_Pyromancer",
                         level=12, kills=4, deaths=2, assists=7,
                         arranged_match=2, map_id="caldavar",
                         lobby_mode="banningpick", match_started=1000.0)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["state"], "Pyromancer \u2022 Lv 12 \u2022 4/2/7")

    def test_match_clock_sets_start(self):
        sm = honrp.StateMachine(None)
        st = sm.state
        st.in_game = True
        st.clock = 754
        st.match_source = ""
        # emulate the clock branch from poll()
        now = 1_000_000.0
        if st.in_game and st.clock is not None and st.clock >= 0 and st.match_source != "clock":
            st.match_started = now - st.clock
            st.match_source = "clock"
        self.assertEqual(st.match_started, now - 754)
        self.assertEqual(st.match_source, "clock")

    def test_queue_from_mm_state(self):
        st = honrp.State(running=True, in_queue=True)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Searching for a match")

    def test_hero_select_activity(self):
        # real client state during hero pick: IsInGame()==0, phase 2 (PREGAME)
        st = honrp.State(running=True, in_game=False, game_phase=2,
                         local_client=0, world="caldavar", lobby_mode="allpick",
                         arranged_match=2)
        self.assertFalse(honrp.in_match(st))
        self.assertTrue(honrp.in_pregame(st))
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Ranked \u2022 FoW \u2022 All Pick")
        self.assertEqual(act["state"], "Hero Select")

    def test_custom_pregame_no_ranked(self):
        st = honrp.State(running=True, in_game=False, game_phase=2,
                         local_client=0, world="caldavarphoenix",
                         lobby_mode="normal", arranged_match=0,
                         lobby_name="My HoN Server")
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "FoW \u2022 All Pick")
        self.assertEqual(act["state"], "Hero Select")

    def test_spectating_activity(self):
        st = honrp.State(running=True, in_game=True, local_client=-1,
                         view_client=3, view_hero="Hero_MasterOfArms",
                         arranged_match=2, map_id="caldavar",
                         match_started=1000.0)
        self.assertTrue(honrp.is_spectating(st))
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Spectating \u2022 FoW")
        self.assertEqual(act["state"], "Watching Master Of Arms")

    def test_learn_tab_with_stale_lobby_data(self):
        # after a custom game, LobbyGameInfo stays populated; being in the
        # Learn tab must not show "In a game lobby"
        st = honrp.State(running=True, in_game=False, game_phase=0,
                         local_client=0, world="caldavarphoenix",
                         lobby_mode="normal", lobby_name="My HoN Server",
                         panel="learn", account="evilleader")
        self.assertFalse(honrp.in_pregame(st))
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "In the Learn tab")

    def test_practice_log_header_does_not_override_lobby_world(self):
        # local practice games write INFO_MAP "fieldtest" (match id 0xFFFFFFFF)
        # even when the lobby world is something else; the lobby wins.
        st = honrp.State(running=True, in_game=True, hero="Hero_Pyromancer",
                         level=3, kills=0, deaths=0, assists=1,
                         world="caldavarphoenix", map_id="fieldtest",
                         lobby_mode="normal", game_phase=3, local_client=0,
                         clock=120, match_started=1000.0)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "FoW \u2022 All Pick")

    def test_panel_activities(self):
        for panel, label in (("plinko", "Playing Plinko"),
                             ("plinko_v2", "Playing Plinko"),
                             ("store", "Browsing the Store"),
                             ("ladder", "Viewing the Ladder"),
                             ("matchstats", "Viewing Match Stats"),
                             ("game_lobby", "In a game lobby")):
            st = honrp.State(running=True, panel=panel, account="evilleader")
            self.assertEqual(honrp.build_activity(st, {})["details"], label)

    def test_pretty_hero(self):
        self.assertEqual(honrp.pretty_hero("Hero_Pyromancer"), "Pyromancer")
        self.assertEqual(honrp.pretty_hero("Hero_DrRepulsor"), "Dr Repulsor")
        self.assertEqual(honrp.pretty_hero("Hero_MasterOfArms"), "Master Of Arms")

    def test_queue_activity(self):
        st = honrp.State(running=True, in_queue=True)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Searching for a match")

    def test_menu_activity(self):
        st = honrp.State(running=True, account="evilleader", session_start=5.0)
        act = honrp.build_activity(st, {})
        self.assertEqual(act["details"], "Main Menu")
        self.assertEqual(act["state"], "evilleader")


class MockDiscord:
    """Minimal Discord IPC endpoint used to verify framing/handshake."""

    def __init__(self, path):
        self.path = path
        self.frames = []
        self.ready = threading.Event()
        self.thread = threading.Thread(target=self._serve, daemon=True)

    def start(self):
        if os.path.exists(self.path):
            os.unlink(self.path)
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(self.path)
        self.srv.listen(1)
        self.thread.start()
        return self

    def _serve(self):
        conn, _ = self.srv.accept()
        buf = b""
        while True:
            try:
                chunk = conn.recv(65536)
            except OSError:
                break
            if not chunk:
                break
            buf += chunk
            while len(buf) >= 8:
                op, ln = struct.unpack("<II", buf[:8])
                if len(buf) < 8 + ln:
                    break
                body, buf = buf[8:8 + ln], buf[8 + ln:]
                msg = json.loads(body)
                self.frames.append((op, msg))
                if op == 0:
                    resp = {"cmd": "DISPATCH", "evt": "READY",
                            "data": {"v": 1, "user": {"id": "0"}}}
                    self.ready.set()
                else:
                    resp = {"cmd": "SET_ACTIVITY", "evt": None,
                            "data": None, "nonce": msg.get("nonce")}
                out = json.dumps(resp).encode()
                conn.sendall(struct.pack("<II", 1, len(out)) + out)

    def stop(self):
        try:
            self.srv.close()
        except OSError:
            pass
        self.thread.join(timeout=1.0)


class TestDiscordIPC(unittest.TestCase):
    def test_handshake_and_set_activity(self):
        tmp = tempfile.mkdtemp(prefix="honrp-ipc-")
        path = os.path.join(tmp, "discord-ipc-0")
        mock = MockDiscord(path).start()
        try:
            rpc = honrp.DiscordIPC("1234567890", ipc_path=path)
            self.assertTrue(rpc.connect())
            activity = {"details": "Mid Wars", "state": "Pyromancer"}
            self.assertTrue(rpc.set_activity(activity, 4242))
            time.sleep(0.3)
            rpc.close()
            self.assertTrue(mock.ready.is_set())
            ops = [op for op, _ in mock.frames]
            self.assertEqual(ops[0], 0)
            self.assertIn(1, ops)
            setact = [m for op, m in mock.frames if op == 1][0]
            self.assertEqual(setact["cmd"], "SET_ACTIVITY")
            self.assertEqual(setact["args"]["pid"], 4242)
            self.assertEqual(setact["args"]["activity"]["details"], "Mid Wars")
        finally:
            mock.stop()
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
