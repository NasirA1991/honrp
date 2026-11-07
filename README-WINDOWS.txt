honrp — Windows
================

Two ways to install, both per-user: no administrator password, nothing written
to Program Files, the Heroes of Newerth install directory is never touched.

A) Installer (recommended)
--------------------------
1. Download `honrp-setup-<version>.exe` and double-click it.
2. Next -> Install. Optional checkboxes: desktop shortcut, start with Windows.
3. The control window opens when the installer finishes.

Windows may warn that the publisher is unknown (the binary is not code-signed):
"More info" -> "Run anyway".

B) Portable zip
---------------
1. Extract `honrp-v<version>-windows-x86_64.zip` anywhere you can write
   (for example a folder in your user profile).
2. Double-click `honrp.exe`. The control window opens and the Lua bridge is
   installed on first start.
3. Start Heroes of Newerth (restart it if it was already running).

The control window
------------------
A small panel: 480x192 normally, growing by 24 px for an action hint, and to
480x236 while Config or a dialog is open:

    ● Rich Presence on   ·   ● Discord ready
    Waiting for Heroes of Newerth
    [ Stop Rich Presence ]              Config   Menu
    Closing this window keeps Rich Presence running in the background.
    ( ) Launch at startup
    ─────────────────────────────────────────────
    honrp 0.3.6 · Up to date

- The first line shows observable states, separately: Rich Presence on (green)
  or off (grey), then Discord. **Discord ready** means Discord's IPC endpoint
  exists but nothing is being published; **Live on Discord** means a card is
  being published; **Can't connect to Discord** means Discord is running but
  the IPC connection failed; **Discord not running** means no endpoint was
  found; **Discord state unknown** appears if an older daemon did not report
  availability. Hover each label for an explanation.
- Problem states also show a short action hint under the status, for example
  "Start Discord to show your game status." (the hint is not tooltip-only).
- **Stop Rich Presence** is a compact neutral button and becomes **Start Rich
  Presence** when off. Stopping asks for confirmation first; closing the window
  with X never stops it, and the hint under the row says so.
- **Launch at startup** is a toggle on Windows. On Linux the same row has
  **Launch at startup...**, which opens a dialog with the copyable
  `honrp install` / `honrp uninstall` commands. A faint separator sits above
  the footer.
- The footer **honrp 0.3.6 · Up to date** is a link: click it to check for
  updates. When one exists, an **Install x.y.z** button appears next to it.
- **Config** opens a settings screen: the Heroes of Newerth documents folder
  (with **Browse...** / **Open folder**) and an optional Discord IPC socket
  override. Empty means auto-detect; honrp needs no Discord install path.
- The **Menu** button holds **Help / Troubleshooting** (opens this README's
  troubleshooting section in the browser), **Discord setup...** (the
  Registered Games note, also shown once on first start), **Details...**
  (version, bridge build, daemon/Discord/game state, game folder, log path)
  and **Uninstall...**. There is no separate Close or Quit button: X closes
  the window, nothing else.
- On the very first start a one-time dialog explains the Discord "Registered
  Games" conflict; it can be reopened from the menu.

Configuration (optional)
------------------------
Defaults work as-is. Use "Config" in the window (or copy
`config.example.json` from the install folder to `%APPDATA%\honrp\config.json`)
and edit it: client_id, large_image, large_text, poll_interval,
refresh_interval, user_dir, ipc_socket, show_timer, show_kda, short_maps,
check_updates.

Updating
--------
The window checks GitHub on start; clicking the footer (honrp 0.3.6 · Up to
date) checks again and offers **Install x.y.z** when a newer release exists,
verified against the release checksums. After an update close and reopen honrp.
The Lua bridge is embedded in the binary and rewritten automatically when it
changes, followed by a "restart Heroes of Newerth" hint; `honrp install` is
only needed to repair a deleted bridge.

Uninstalling
------------
Click "Uninstall..." in the window (recommended): it stops the daemon, runs the
installer's uninstaller when honrp was installed (program, shortcuts, autostart,
settings and the Lua bridge), or for the portable zip removes the autostart
entry, settings and bridge and tells you to delete the folder.

The same is available from Windows: Settings -> Apps -> honrp -> Uninstall (or
the Start Menu entry). This stops the daemon first, then removes the program,
the Start Menu/desktop shortcuts, the autostart entry, the config folder and
the managed Lua-bridge block from the game's startup.lua. Any content you added
to startup.lua outside the managed block is preserved.

Command line
------------
The GUI is just `honrp.exe` with no arguments. For scripting or a terminal:

    honrp.exe run [-v]          start the daemon without a window
    honrp.exe status [--json]   what the daemon sees right now
    honrp.exe doctor            detection, bridge, Discord handshake
    honrp.exe install [--user-dir DIR]
    honrp.exe update [--check]
    honrp.exe uninstall

Not included yet
----------------
- no tray icon (the window stays on the taskbar / minimised)

Discord
-------
The Discord application id is built in. Discord must be running. If Discord
also lists "juvio.exe" as a separate game, disable it once in
Settings -> Game Activity -> Registered Games; honrp cannot change that list.

SmartScreen
-----------
The binaries are not code-signed. Windows may show "Windows protected your PC";
use "More info" -> "Run anyway". You can also unblock the download before
running/extracting (right-click -> Properties -> Unblock).

Feedback
--------
Please include the output of `honrp.exe doctor`, your Windows version, how you
launch HoN (Steam/non-Steam/other), and your Discord client. Report results in
the Windows alpha issue on GitHub.
