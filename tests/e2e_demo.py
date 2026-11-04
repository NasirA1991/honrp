#!/usr/bin/env python3
"""End-to-end demo: real daemon + fake HoN user dir + mock Discord endpoint.

Feeds a scripted sequence (menu -> queue -> practice match -> menu) through
honrp.py and prints the SET_ACTIVITY frames Discord actually receives.
"""

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

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DAEMON = os.path.join(ROOT, "honrp.py")


def write_utf16(path, text):
    with open(path, "wb") as f:
        f.write(b"\xff\xfe")
        f.write(text.encode("utf-16-le"))


class MockDiscord:
    def __init__(self, path):
        self.path = path
        self.frames = []
        self.ready = threading.Event()

    def start(self):
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(self.path)
        self.srv.listen(1)
        threading.Thread(target=self._serve, daemon=True).start()
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
                msg = json.loads(body.decode())
                if op == 0:
                    resp = {"cmd": "DISPATCH", "evt": "READY",
                            "data": {"v": 1, "user": {"id": "0"}}}
                else:
                    self.frames.append(msg)
                    resp = {"cmd": "SET_ACTIVITY", "evt": None,
                            "data": None, "nonce": msg.get("nonce")}
                out = json.dumps(resp).encode()
                try:
                    conn.sendall(struct.pack("<II", 1, len(out)) + out)
                except OSError:
                    return

    def stop(self):
        try:
            self.srv.close()
        except OSError:
            pass


def mod_line(**kw):
    base = {"v": 1, "t": 0, "in_game": "0", "in_queue": "0", "game_phase": 0,
            "lobby_phase": 0, "account": "evilleader"}
    base.update({k: str(v) for k, v in kw.items()})
    return "[HONRP] " + json.dumps(base, separators=(",", ":"))


def main():
    tmp = tempfile.mkdtemp(prefix="honrp-e2e-")
    user = os.path.join(tmp, "Heroes of Newerth")
    os.makedirs(os.path.join(user, "logs"))
    sock = os.path.join(tmp, "discord-ipc-0")
    console = os.path.join(user, "console.log")

    write_utf16(os.path.join(user, "logs", "game_00000.log"),
                'INFO_DATE date:"2026/11/04" time:"20:30:00"\r\n'
                'INFO_MATCH name:"Practice" id:"1"\r\n'
                'INFO_MAP name:"fieldtest" version:"0.0.0"\r\n'
                'INFO_SETTINGS mode:"Mode_Normal" options:"Option_None"\r\n')
    write_utf16(console, "")

    mock = MockDiscord(sock).start()
    proc = subprocess.Popen(
        [sys.executable, DAEMON, "-v", "run", "--client-id", "1567616925515776080",
         "--ipc-socket", sock, "--user-dir", user],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    steps = [
        ("main menu", mod_line(in_game=0, in_queue=0, panel="play")),
        ("in queue", mod_line(in_game=0, in_queue=1, mm_state=3,
                              panel="play", account="evilleader")),
        ("practice match", mod_line(in_game=1, in_queue=0, hero="Hero_Pyromancer",
                                    clock=42, level=3, kills=0, deaths=0, assists=1,
                                    arranged="false", arranged_match=0,
                                    world="fieldtest", lobby_mode="")),
        ("back to menu", mod_line(in_game=0, in_queue=0, panel="play")),
    ]
    try:
        time.sleep(1.5)
        for i, (label, line) in enumerate(steps):
            with open(console, "ab") as f:
                f.write((line + "\n").encode("utf-16-le"))
            print(f"--- {label}")
            time.sleep(6.0)
    finally:
        proc.terminate()
        proc.wait(timeout=5)
        mock.stop()

    print("\nSET_ACTIVITY frames Discord received:")
    for m in mock.frames:
        act = (m.get("args") or {}).get("activity") or {}
        print(json.dumps({"details": act.get("details"),
                          "state": act.get("state"),
                          "start": (act.get("timestamps") or {}).get("start")}))
    shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
