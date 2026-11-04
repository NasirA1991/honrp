# honrp — Rust port & release plan

Status: planning. The Python prototype in this directory is the behavioural
reference; the Lua bridge (`honrp_mod.lua`, installed as `startup.lua`) is
already the final design and is shared by both implementations.

---

## 0. Goals

1. Replace the Python daemon with a small static Rust binary (target **4–8 MiB
   RSS**, ~2–3 MB binary) with identical behaviour.
2. Make installation foolproof on any Steam/Proton setup: no hardcoded paths,
   no assumptions about where HoN lives, one `honrp setup` command.
3. Ship it: versioned releases, packaging, CI, docs, a project-owned Discord
   application id.

Non-goals: replacing the Lua bridge, reading game memory, supporting native
(non-Wine) HoN (there is none), Windows support.

---

## 1. Rust design

### 1.1 Dependencies (deliberately small)

| crate | why |
|---|---|
| `serde`, `serde_json` | parse the mod's JSON lines, Discord payloads, `--json` output |
| `toml` | human-editable config |
| `clap` (derive) | CLI; alternative `lexopt` if we want a ~1 MB smaller binary |
| `signal-hook` | SIGINT/SIGTERM handling |
| `libc` | `/proc`, `PR_SET_NAME` if we ever hide the process from Discord detection |

No async runtime (no tokio): a single blocking thread with 1 Hz polling is
plenty and keeps RSS low. UTF-16LE decoding is done by hand (~20 lines) instead
of pulling in `encoding_rs`.

### 1.2 Modules

```
src/
  main.rs        CLI (setup/run/status/doctor/mod/service/uninstall)
  config.rs      ~/.config/honrp/config.toml
  detect.rs      running game -> prefix -> Windows user dir; fallbacks
  modinstall.rs  embedded startup.lua, managed block, backup, validation
  service.rs     systemd --user unit install/enable/remove, autostart fallback
  tail.rs        incremental UTF-16LE console.log reader (offset/residual/BOM)
  matchlog.rs    logs/game_*.log parser (INFO_DATE/MAP/MATCH/SETTINGS)
  state.rs       mod JSON -> State, fallbacks, staleness
  activity.rs    map/mode/hero/ranked -> Discord activity
  ipc.rs         Discord unix socket: framing, handshake, reconnect, throttle
  doctor.rs      end-to-end self-check with pass/fail output
assets/
  startup.lua    embedded with include_str!, version-tagged BUILD
  hon.png        Rich Presence asset (also shipped in the release tarball)
```

### 1.3 Behaviour port checklist (parity with the Python version)

- [ ] process discovery: `/proc/<pid>/comm` == `juvio.exe`, `/proc/<pid>/environ`
- [ ] console.log tail: UTF-16LE, BOM, partial lines, truncation/rotation
- [ ] mod JSON fields: `in_game`, `in_queue`, `mm_state`, `game_phase`,
      `lobby_phase`, `hero`, `view_hero`, `arranged_match`, `world`,
      `lobby_mode`, `panel`, `level`, `kills/deaths/assists`, `clock`
- [ ] activity mapping: match / hero select / lobby / queue / menu panels /
      spectating / Main Menu; ranked vs unranked; short map names
- [ ] Discord IPC: socket discovery (native + Flatpak), 8 s handshake timeout,
      READY check, republish on reconnect, 5 s send throttle, 300 s refresh,
      clear on exit
- [ ] graceful degradation when the mod is absent (match log/world-load)
- [ ] `--dry-run`, `--json`, `-v` (logs to stderr → journald)

---

## 2. Binary & packaging

**Best format: a single static musl binary per architecture.**

- `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
- ~2–3 MB, zero runtime dependencies, works on Arch/Fedora/Debian/NixOS/Alpine
  and inside Steam's runtime.
- Installed to `~/.local/bin/honrp`; runs as a **systemd user service** (no root).

Why not the alternatives:

| option | verdict |
|---|---|
| AppImage | needs FUSE, unpacks to /tmp, no installer semantics — overkill for a daemon |
| Flatpak | sandbox complicates reading `/proc`, `~/.steam`, and Discord's socket |
| .deb/.rpm | nice to have as extras, but distro-specific; the tarball covers everyone |
| `cargo install` | requires a Rust toolchain — not acceptable for end users |

Release artifacts per version:

```
honrp-0.1.0-linux-x86_64.tar.gz     (binary + README + startup.lua + hon.png)
honrp-0.1.0-linux-aarch64.tar.gz
honrp-0.1.0-checksums.txt
install.sh                          (downloads, verifies sha256, installs)
```

Extras: AUR `honrp-bin`, optional `.deb`, optional `nix run`.

---

## 3. Detection: where is HoN / which prefix?

`honrp setup` resolves, in order, and stops at the first hit:

1. **Running game** — find a process whose comm is `juvio.exe`, read
   `/proc/<pid>/environ`:
   - `STEAM_COMPAT_DATA_PATH` → `<path>/pfx`
   - `WINEPREFIX` (fallback)
   - `cmdline` → the `Z:\…\bin\juvio.exe` path (install dir, for `doctor`)
   This is exact and covers moved installs, non-standard library paths, and
   multiple copies (this machine has 5 stale prefixes — only this one is live).
2. **Explicit override** — `--prefix`, `--user-dir`, or values in the config.
3. **Steam app id 2618010418 in the default library** —
   `~/.steam/steam/steamapps/compatdata/2618010418/pfx`,
   `~/.local/share/Steam/...`.
4. **Every Steam library** — parse `steamapps/libraryfolders.vdf` of each Steam
   root (native, Flatpak `~/.var/app/com.valvesoftware.Steam/...`) and check
   `<lib>/steamapps/compatdata/2618010418/pfx`.
5. **Any Wine prefix** — `~/.wine`, `$WINEPREFIX`, `~/Games/*` (Lutris), scanning
   for `drive_c/users/*/Documents/Juvio/Heroes of Newerth`.
6. **Ask the user** — print the candidates found and let them pick or type a
   path. Never fail without an actionable message.

### Windows user inside the prefix

`drive_c/users/*` contains `Public` and `steamuser` on this machine, but it can be
any name. Scoring:

- has `Documents/Juvio/Heroes of Newerth/console.log` → highest
- newest `console.log` mtime → tie-breaker
- otherwise prompt with the list

`doctor` prints every candidate (prefix → user → user dir → mtime) so a wrong
pick is obvious.

### Does the user need to specify the install location?

**No, in the normal case.** The install directory is not needed to install the
mod — only the **prefix/user dir** is. The install dir is optional and used only
by `doctor` to confirm the game files exist. Users with unusual setups can pass
`--prefix` / `--user-dir`; `setup --yes` is fully non-interactive for scripts.

---

## 4. Installing the Lua bridge

1. Resolve `user_dir` (above). If it does not exist, create it? No — create
   `Documents/Juvio/Heroes of Newerth` only if the prefix is otherwise valid,
   and warn.
2. Read `startup.lua` if present; strip an existing `--- HONRP BEGIN/END ---`
   managed block; back up only the user's own content to `startup.lua.honrp.bak`.
3. Write the embedded module wrapped in the markers (idempotent, preserves other
   mods that use the same file).
4. Verify: file contains the markers and `local BUILD = <version>`.
5. Tell the user to restart HoN (the bridge loads in `CHost::Init`).

`honrp mod uninstall` removes exactly the managed block and deletes the file if
nothing else remains. `honrp doctor` warns if the running game's boot line shows
an older build.

---

## 5. Service installation

Primary: **systemd user unit**

```
~/.config/systemd/user/honrp.service
[Service]
ExecStart=%h/.local/bin/honrp run
Restart=always
RestartSec=5
```

`honrp service install`:
1. write the unit (ExecStart = the actual binary path, not a guess),
2. `systemctl --user daemon-reload`,
3. `systemctl --user enable --now honrp`,
4. verify `is-active`.

Fallback if systemd user sessions are unavailable (`systemctl --user
is-system-running` fails): write `~/.config/autostart/honrp.desktop`
(`Exec=… run`) and print how to start it now. `uninstall` handles both.

Never installs anything system-wide, never needs sudo.

---

## 6. CLI / UX

```
honrp setup [--app-id ID] [--prefix PATH] [--user-dir PATH] [--yes]
honrp doctor
honrp status [--json]
honrp run [--dry-run] [-v]
honrp mod install|uninstall
honrp service install|uninstall
honrp uninstall [--purge]
```

Config `~/.config/honrp/config.toml`:

```toml
client_id = ""            # Discord application id
prefix = ""               # optional overrides
user_dir = ""
show_timer = false        # off by default: level already conveys match stage
show_kda = true           # hero • Lv N • K/D/A vs hero • Lv N
short_maps = true         # caldavar/caldavarphoenix -> "FoW" (user preference)
heartbeat_seconds = 10    # idle refresh; state changes are still immediate
refresh_interval = 300    # Discord re-publish (propagates app name/asset changes)
```

`honrp setup` should be one interactive pass:

1. detect prefix/user dir (show what was found),
2. install the Lua bridge,
3. ask for / validate the Discord application id (17–19 digits),
4. write config,
5. install + start the service,
6. print "restart HoN" and offer to run `doctor`.

`honrp doctor` checks, with pass/fail lines:

- binary version, config, app id format
- prefix/user dir resolved, `console.log` present and writable
- mod installed + build tag
- game running? boot line build (from console.log)
- Discord socket present, handshake READY, resolved application **name**
- whether `SET_ACTIVITY` kept `large_image` (asset missing is the current bug)
- service active, last activity age

Exit code 0 only when everything is green — useful in bug reports.

---

## 7. Release plan

### Phase 0 — polish the prototype (now)
- shorten map names, timer off by default, optional KDA line
- verify the `hon` art asset resolves (Discord currently drops `large_image`)
- keep the Python daemon usable while the Rust version is developed

### Phase 1 — Rust MVP (parity)
- port modules from §1.3, port all unit tests
- mock-Discord integration test (`tests/e2e_demo.py` equivalent in Rust)
- `doctor` side-by-side with the Python version on this machine
- feature flag to run both and diff activities

### Distribution strategy (cross-distro, no root)

Primary: a **single static musl binary** on GitHub Releases. It runs on any
glibc/musl distro without installing a runtime.

```
# simplest possible: download one file, run it, let it install itself
curl -fLO https://github.com/<org>/honrp/releases/latest/download/honrp-linux-x86_64
chmod +x honrp-linux-x86_64
./honrp-linux-x86_64 setup      # copies itself to ~/.local/bin, installs mod+service
```

Optional sugar: an `install.sh` that picks the arch, verifies sha256, copies to
`~/.local/bin` and runs `setup`. Also provider packages for people who prefer
their package manager: AUR (`honrp-bin`), `.deb` for Debian/Ubuntu, `.rpm` for
Fedora (optional, can lag releases).

Not recommended as the primary channel: Flatpak/AppImage (sandbox/FUSE vs
`/proc`, `~/.steam`, Discord socket), `cargo install` (needs a toolchain),
distro-only packages (limits reach).

`honrp setup` must be safe to re-run; `honrp update` can later fetch the next
release and restart the user service.

### Phase 2 — packaging & CI
- GitHub Actions: fmt, clippy, tests, musl x86_64/aarch64 builds, checksums
- `install.sh` (sha256-verified download, `~/.local/bin`)
- AUR `honrp-bin`; optional `.deb`
- README + troubleshooting + privacy note (local socket + log only)

### Phase 3 — beta
- test matrix: Arch/Fedora/Debian, KDE/GNOME, Wayland/X11, Steam native +
  Flatpak, Discord native/Flatpak/Vesktop, 0/1/many prefixes, existing
  `startup.lua` with another mod
- collect `honrp doctor` outputs from testers

### Phase 4 — 1.0
- project-owned Discord application id shipped as the default (users can
  override), with the `hon` asset uploaded and verified
- release notes, versioned Lua bridge (`BUILD` constant), migration from the
  Python service detected and replaced by `setup`

---

## 8. Foolproof end-to-end (the intended user experience)

```bash
# 1. install
curl -fsSL https://github.com/<org>/honrp/releases/latest/download/install.sh | sh
#    (downloads the musl binary, verifies sha256, puts it in ~/.local/bin)

# 2. one command does the rest
honrp setup
#    ✓ found Proton prefix: ~/.steam/steam/steamapps/compatdata/2618010418/pfx
#    ✓ Windows user: steamuser
#    ✓ HoN user dir: .../Documents/Juvio/Heroes of Newerth
#    ✓ Lua bridge installed (build N)
#    ? Discord application id: <paste>  (or --app-id for scripts)
#    ✓ config written
#    ✓ systemd user service installed and started
#    → restart Heroes of Newerth now

# 3. verify
honrp doctor     # all green, exit 0
```

Failure modes always end with a concrete next step ("no prefix found — start HoN
once, or pass --prefix …"), never a silent no-op.

---

## 9. UI decisions (pending, do not implement yet)

- **Short map names**: `caldavar`/`caldavarphoenix` → **`FoW`** (user choice),
  `midwars` → `Mid Wars`, `fieldtest` → `Field Test`. Keep it in config so the
  string can be changed without a rebuild.
- **Timer**: off by default (`show_timer = false`); hero level is the stage
  indicator (Lv 1–5 early, ~11–16 mid, 18–25 late).
- **State line**: `hero • Lv N` base; KDA behind `show_kda` (default true).
- **MOTD**: already removed (falls back to `Main Menu`).

### Reporting cadence

Current: one emit per second (MatchClock drives it), ~400 B/s / 1.4 MB per hour
of log growth; Discord itself only receives updates on change, at most every 5 s.

Planned: **event-driven emits + a 10 s heartbeat**.

- state changes (queue join/leave, lobby info, hero pick, panel open/close,
  `PlayerScore`, `ActiveLevel`) emit immediately (min 0.9 s between sends,
  coalescing bursts);
- `MatchClock` only triggers a heartbeat emit every `heartbeat_seconds` (10 s)
  so the daemon knows the bridge is alive and the log is flushed;
- result: ~40 B/s (~144 KB/h) idle in-game, instant updates when something
  actually changes.

The Lua side stays flush-per-emit; at a 10 s cadence that is negligible I/O.
The daemon's 40 s staleness threshold is comfortably above the heartbeat.

## 10. Risks / edge cases

- Discord RPC can be slow right after a Discord restart → 8 s handshake timeout,
  retry with backoff, republish on reconnect (already handled in Python).
- Multiple stale prefixes (this machine has 5) → process env wins; never guess
  from the first hit on disk.
- Steam Flatpak prefixes/sockets → include `~/.var/app/...` paths.
- Client updates can change the Lua hook or UICmd names → `doctor` reports the
  boot build and missing fields; fallback mode still gives process + map.
- Rich Presence assets propagate slowly; `doctor` verifies `large_image`
  survived the `SET_ACTIVITY` echo.
- Integrity: the bridge is user-dir Lua, no memory patching, no injection.
