<p align="center">
  <img src="assets/branding/readme-banner-1600x450.png" alt="honrp — Heroes of Newerth ↔ Discord Rich Presence" width="100%">
</p>

<h1 align="center">honrp</h1>

<p align="center">
  <strong>Discord Rich Presence for Heroes of Newerth: Reborn</strong><br>
  Linux and Windows · automatic match status · no game files touched · no admin
</p>

<p align="center">
  <a href="https://github.com/NasirA1991/honrp/releases/latest"><img src="https://img.shields.io/github/v/release/NasirA1991/honrp?style=flat-square&label=download" alt="Download"></a>
  <a href="https://github.com/NasirA1991/honrp/actions/workflows/ci.yml"><img src="https://github.com/NasirA1991/honrp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://aur.archlinux.org/packages/honrp-bin"><img src="https://img.shields.io/badge/AUR-honrp--bin-1793D1?logo=archlinux&logoColor=white" alt="AUR"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT"></a>
</p>

## Install

### Windows 10 / 11

1. **[Download the latest installer](https://github.com/NasirA1991/honrp/releases/latest)** — `honrp-setup-*.exe`.
2. Run it. It installs per-user, with no admin prompt, and adds **honrp** to the Start Menu.
3. Open **honrp** and click **Start Rich Presence**.
4. Restart Heroes of Newerth once so the Lua bridge loads.

The installer writes the bridge automatically when honrp starts. If you use the portable zip instead, run `honrp.exe install` once from the extracted folder.

> Windows SmartScreen may warn because the build is unsigned. Click **More info → Run anyway**, or unblock the zip before extracting.

### Linux

**Arch Linux / CachyOS**

```bash
paru -S honrp-bin
honrp install
honrp
```

**Debian / Ubuntu**

Download the `.deb` from the [latest release](https://github.com/NasirA1991/honrp/releases/latest), then:

```bash
sudo apt install ./honrp_*.deb
honrp install
honrp
```

**Fedora**

Download the `.rpm` from the [latest release](https://github.com/NasirA1991/honrp/releases/latest), then:

```bash
sudo dnf install ./honrp-*.rpm
honrp install
honrp
```

**Any distro — install script**

```bash
curl -fsSL https://github.com/NasirA1991/honrp/releases/latest/download/install.sh | bash
"$HOME/.local/bin/honrp"
```

`honrp install` writes the Lua bridge, installs the background service and adds the app-menu entry and icons. It runs as your user; nothing in the game install directory is modified. If it cannot find your game folder, start Heroes of Newerth once and run it again.

The Linux window needs Wayland or X11 and a Vulkan-capable driver. The packages pull in the rest; for the standalone tarball, install your distro's `libxkbcommon`, Wayland/X11 and Vulkan loader packages.

### After installing

Open **honrp**, click **Start Rich Presence**, then launch Heroes of Newerth. If you were already in-game, restart it once. No Discord application setup is needed — the app id is built in.

## What it shows

<p align="center">
  <img src="assets/states.png" alt="honrp states: main menu, queue, hero select, match, spectating and Plinko" width="100%">
</p>

| Where you are | What Discord shows |
|---|---|
| Main menu | Main Menu · account |
| Searching | Searching for a match · In queue |
| Hero select | Ranked · FoC · All Pick · Hero Select |
| In a match | Ranked · FoC · Banning Pick · hero, level and KDA |
| Spectating | Spectating · FoC · the hero you are watching |
| Plinko | Playing Plinko · account |

The art above is an illustrative render of the card payloads, not a Discord screenshot. While in a match, hover the app window to see hero, level and KDA; the card itself stays compact.

## Why honrp

- **It just works.** It finds HoN through Steam/Proton, Lutris, Heroic, Faugus, Bottles or plain Wine — no library paths to configure.
- **Nothing invasive.** It reads the game's own `console.log`, never game memory, and never changes game files. The Lua bridge is one managed block in `startup.lua`.
- **A proper desktop app.** A compact native window for status, settings and updates — no browser, no tray icon, no console left behind.
- **Private by default.** It talks only to Discord's local socket; no telemetry, no analytics, no account login.

## Troubleshooting

<details>
<summary><b>Windows</b> — common problems</summary>

- **SmartScreen blocks the installer or updater** — the build is unsigned. Choose **More info → Run anyway**, or right-click the downloaded zip → **Properties → Unblock** before extracting. Updates need `curl` and `tar`, which ship with Windows 10+.
- **"Playing honrp" in Discord while Rich Presence is off** — that is Discord's own process detection, not honrp's Rich Presence. Remove or disable **honrp** in Discord **Settings → Registered Games**. Running the installed app (not a checkout under a `Games/` folder) avoids it.
- **The card stays on Main Menu / no hero or KDA** — restart Heroes of Newerth after setup. `honrp.exe doctor` shows whether the bridge is loaded.
- **Discord shows a second "Juvio" card** — disable Juvio once in Discord **Settings → Game Activity → Registered Games**; honrp cannot change that list.
- **Uninstall** — Windows Settings → Apps → honrp. The uninstaller stops the daemon and removes the bridge block from `startup.lua`.

</details>

<details>
<summary><b>Linux</b> — common problems</summary>

- **The taskbar shows a generic Wayland icon** — run `honrp install` once with the installed binary. It writes `~/.local/share/applications/honrp.desktop` and the matching hicolor icons; restart honrp (or log out and back in) if the old icon is cached. Running a checkout straight from `target/release/` has no desktop entry to associate with.
- **Discord says "Playing honrp" while Rich Presence is off** — this is Discord's process detection, not honrp's RPC. It is triggered by running a checkout under a folder such as `.../Games/.../target/release/honrp`; the installed `/usr/bin/honrp` is not detected. Remove/disable **honrp** in Discord **Settings → Registered Games**.
- **"Discord not running" or "Can't connect to Discord"** — check `honrp doctor` for the sockets it tries. Flatpak Discord often uses a different socket; set it in **Config → Discord IPC socket** (for example `$XDG_RUNTIME_DIR/app/com.discordapp.Discord/discord-ipc-0`).
- **No card / no hero or KDA** — restart Heroes of Newerth after `honrp install` or an update. The bridge is embedded in the binary and rewritten when the daemon starts.
- **Service state** — `systemctl --user status honrp` and `journalctl --user -u honrp`.
- **No window on Wayland/VM** — the GUI needs a working Vulkan driver. `honrp run` runs the same daemon headlessly.
- **Updates** — `paru -Syu` / `yay -Syu` for the AUR package; `honrp update` for the standalone tarball.

</details>

## Commands

| Command | What it does |
|---|---|
| `honrp` | Opens the control window |
| `honrp install` | Writes the bridge, installs the service, desktop entry and icons |
| `honrp run` | Runs the background daemon in the foreground |
| `honrp status [--json]` | Shows what the daemon currently sees |
| `honrp doctor` | Checks prefix, bridge, Discord and service state |
| `honrp stop` | Stops the background daemon (the window stays open) |
| `honrp update [--check]` | Updates the standalone binary from GitHub |
| `honrp uninstall` | Removes the bridge, service, desktop entry and icons; keeps settings |
| `honrp uninstall --purge` | Also removes `~/.config/honrp` (or `%APPDATA%\honrp`) |

## Development

```bash
cargo test
lua5.1 tests/test_lua_mod.lua lua/honrp_mod.lua
lua5.1 tests/test_lua_reload.lua lua/honrp_mod.lua
```

Build a headless binary without the GUI dependencies with `cargo build --release --no-default-features`.

- [Branch changes and implementation notes](docs/BRANCH-CHANGES.md)
- [Linux porting notes](docs/PORTING-LINUX.md)
- [How the bridge works](docs/REVERSE_ENGINEERING.md)

## Artwork

The honrp mark — a gold hood, a three-arch bridge and a chat bubble — is original artwork, not a Heroes of Newerth or Discord logo.

<p align="left">
  <img src="assets/icon.png" alt="honrp app icon" width="96">
</p>

- App icon source: [`assets/icon.svg`](assets/icon.svg), exports in [`assets/icon.png`](assets/icon.png), [`assets/icon.ico`](assets/icon.ico) and [`assets/icons/hicolor/`](assets/icons/hicolor)
- Banner, avatar and social preview: [`assets/branding/`](assets/branding)
- Colours and usage rules: [`docs/BRANDING.md`](docs/BRANDING.md)

## Privacy & license

No telemetry and no game memory access. Read [`PRIVACY.md`](PRIVACY.md) for the details. MIT licensed — see [`LICENSE`](LICENSE).
