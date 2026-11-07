mod activity;
mod config;
mod control;
mod detect;
mod discord_state;
#[cfg(feature = "gui")]
mod gui;
mod ipc;
mod matchlog;
mod modstate;
mod run;
mod status;
mod tail;
mod update;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

use activity::View;
use config::Config;
use run::{RunOpts, StateMachine};

const MOD_SRC: &str = include_str!("../lua/honrp_mod.lua");
const BEGIN: &str = "--- HONRP BEGIN ---";
const END: &str = "--- HONRP END ---";

#[derive(Parser)]
#[command(
    name = "honrp",
    version,
    about = "Discord Rich Presence for Heroes of Newerth on Linux (Steam/Proton)"
)]
struct Cli {
    /// config.json path (default: ~/.config/honrp/config.json)
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[arg(short, long, global = true)]
    verbose: bool,
    /// Start the Windows control window minimised (used by autostart)
    #[arg(long, global = true, hide = true)]
    minimized: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the presence daemon
    Run {
        /// Print presence updates instead of talking to Discord
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_name = "DIR")]
        user_dir: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        ipc_socket: Option<String>,
    },
    /// Stop the background daemon
    Stop,
    /// Show what the daemon currently sees
    Status {
        #[arg(long)]
        json: bool,
        #[arg(long, value_name = "DIR")]
        user_dir: Option<PathBuf>,
    },
    /// Diagnose detection, bridge, Discord and service state
    Doctor {
        #[arg(long, value_name = "DIR")]
        user_dir: Option<PathBuf>,
    },
    /// Install the Lua bridge (and the systemd user service)
    Install {
        #[arg(long, value_name = "DIR")]
        user_dir: Option<PathBuf>,
        /// Proton/Wine prefix to use (default: detect)
        #[arg(long, value_name = "PFX")]
        prefix: Option<PathBuf>,
        /// Discord application id; written to ~/.config/honrp/config.json
        #[arg(long, value_name = "ID")]
        client_id: Option<String>,
        #[arg(long)]
        no_service: bool,
    },
    /// Remove the service and the Lua bridge
    Uninstall {
        #[arg(long, value_name = "DIR")]
        user_dir: Option<PathBuf>,
        /// Also remove ~/.config/honrp (or %APPDATA%\honrp)
        #[arg(long)]
        purge: bool,
    },
    /// Install the latest GitHub release over this binary
    Update {
        /// Only check for a newer release, do not install it
        #[arg(long)]
        check: bool,
    },
    /// Manage the systemd user service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    Install,
    Uninstall,
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref());
    match cli.cmd {
        Some(Cmd::Run {
            dry_run,
            user_dir,
            ipc_socket,
        }) => {
            hide_console_if_sole();
            run::run(
                &cfg,
                RunOpts {
                    dry_run,
                    verbose: cli.verbose,
                    user_dir,
                    ipc_socket,
                },
            )
        }
        Some(Cmd::Stop) => {
            if control::stop() {
                println!("daemon stopped");
            } else {
                println!("no daemon is running");
            }
            Ok(())
        }
        Some(Cmd::Status { json, user_dir }) => status(&cfg, json, user_dir),
        Some(Cmd::Doctor { user_dir }) => doctor(&cfg, user_dir),
        Some(Cmd::Install {
            user_dir,
            prefix,
            client_id,
            no_service,
        }) => install(user_dir, prefix, client_id, no_service),
        Some(Cmd::Uninstall { user_dir, purge }) => uninstall(user_dir, purge),
        Some(Cmd::Update { check }) => update::run(check),
        Some(Cmd::Service { action }) => service_dispatch(action),
        None => {
            #[cfg(feature = "gui")]
            {
                let _ = cli.minimized;
                return gui::run();
            }
            #[cfg(not(feature = "gui"))]
            {
                use clap::CommandFactory;
                let _ = &cfg;
                Cli::command().print_help()?;
                println!();
                Ok(())
            }
        }
    }
}

fn resolve_user_dir(explicit: Option<PathBuf>, prefix: Option<PathBuf>) -> Option<PathBuf> {
    let games = detect::find_games();
    let detected = games.last().and_then(|g| g.prefix.clone());
    let prefix = prefix.or(detected);
    detect::pick_user_dir(prefix.as_deref(), explicit.as_deref())
}

fn status(cfg: &Config, as_json: bool, user_dir: Option<PathBuf>) -> Result<()> {
    let games = detect::find_games();
    let prefix = games.last().and_then(|g| g.prefix.as_deref());
    let ud = detect::pick_user_dir(prefix, user_dir.as_deref());
    let mut sm = StateMachine::new(ud.clone());
    sm.poll();
    let view = sm.view();
    let activity = if sm.pid().is_some() {
        activity::build(&view, cfg)
    } else {
        None
    };
    let installed = ud.as_ref().map(|d| mod_installed(d)).unwrap_or(false);
    let bridge_build = ud.as_ref().and_then(|d| mod_build(d));
    let expected_build = mod_build_from(MOD_SRC);

    if as_json {
        let out = json!({
            "game_process": games.last().map(|g| json!({
                "pid": g.pid,
                "prefix": g.prefix,
                "cmdline": g.cmdline,
            })),
            "user_dir": ud,
            "mod_installed": installed,
            "mod_build": bridge_build,
            "mod_build_expected": expected_build,
            "state": {
                "in_game": view.in_game,
                "in_queue": view.in_queue,
                "panel": view.panel,
                "hero": view.hero,
                "view_hero": view.view_hero,
                "account": view.account,
                "arranged_match": view.arranged_match,
                "map": view.map,
                "mode": view.mode,
                "level": view.level,
                "kills": view.kills,
                "deaths": view.deaths,
                "assists": view.assists,
                "spectating": activity::is_spectating(&view),
            },
            "activity": activity.map(|a| a.to_json()),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!(
        "game:        {}",
        if view.in_game {
            "in match"
        } else if view.in_queue {
            "in queue"
        } else {
            "menu"
        }
    );
    if !view.panel.is_empty() {
        println!("panel:      {}", view.panel);
    }
    if !view.account.is_empty() {
        println!("account:    {}", view.account);
    }
    if !view.map.is_empty() || !view.mode.is_empty() {
        println!("map/mode:   {} / {}", view.map, view.mode);
    }
    if !view.hero.is_empty() {
        println!(
            "hero:       {} (Lv {})",
            view.hero,
            view.level
                .map(|l| l.to_string())
                .unwrap_or_else(|| "?".into())
        );
    }
    if let Some(a) = activity {
        println!("---\ndetails:    {}\nstate:      {}", a.details, a.state);
    }
    println!(
        "bridge:     {}",
        if installed {
            "installed"
        } else {
            "not installed"
        }
    );
    if installed {
        println!(
            "            build {:?} (binary ships {})",
            bridge_build,
            expected_build
                .map(|b| b.to_string())
                .unwrap_or_else(|| "?".into())
        );
        if bridge_build != expected_build {
            println!("            out of date: `honrp run` or `honrp install` updates it");
        }
    }
    Ok(())
}

fn doctor(cfg: &Config, user_dir: Option<PathBuf>) -> Result<()> {
    println!("honrp {}", env!("CARGO_PKG_VERSION"));
    println!(
        "os:         {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    let games = detect::find_games();
    if games.is_empty() {
        println!("game:       not running");
    } else {
        for g in &games {
            println!("game:       pid {} prefix {:?}", g.pid, g.prefix);
            if let Some(dir) = detect::install_dir_from_cmdline(&g.cmdline) {
                println!("install:    {}", dir.display());
            }
            println!("            {}", g.cmdline);
        }
    }

    let prefix = games.last().and_then(|g| g.prefix.as_deref());
    println!("--- user dir candidates");
    let candidates = detect::user_dir_candidates(prefix);
    let picked = detect::pick_user_dir(prefix, user_dir.as_deref());
    if candidates.is_empty() {
        println!("  none found (start HoN once, or pass --user-dir)");
    }
    for c in &candidates {
        let log = c.join("console.log");
        let age = fs::metadata(&log)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| format!("{}s ago", d.as_secs()))
            .unwrap_or_else(|| "no console.log".into());
        let mark = if Some(c) == picked.as_ref() { "*" } else { " " };
        println!("  {mark} {} ({age})", c.display());
    }

    if let Some(ud) = &picked {
        let installed = mod_installed(ud);
        println!(
            "bridge:     {} (build {:?})",
            if installed {
                "installed"
            } else {
                "NOT installed"
            },
            mod_build(ud)
        );
    }

    let sockets = ipc::socket_paths(None);
    println!("discord:    {} socket(s): {:?}", sockets.len(), sockets);
    if !cfg.client_id.is_empty() {
        match ipc::DiscordIpc::connect(&cfg.client_id, Some(cfg.ipc_socket.as_str())) {
            Ok(_) => println!("handshake:  ok (application id {})", cfg.client_id),
            Err(e) => println!("handshake:  FAILED: {e}"),
        }
    } else {
        println!("client_id:  not configured");
    }

    match Command::new("systemctl")
        .args(["--user", "is-active", "honrp.service"])
        .output()
    {
        Ok(o) => println!("service:    {}", String::from_utf8_lossy(&o.stdout).trim()),
        Err(_) => println!("service:    systemctl unavailable"),
    }
    Ok(())
}

fn set_client_id(id: &str) -> Result<()> {
    let path = config::config_path().context("no config directory")?;
    let mut cfg: serde_json::Value = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    cfg["client_id"] = json!(id);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&cfg)? + "\n")?;
    println!("wrote client_id to {}", path.display());
    Ok(())
}

#[cfg(unix)]
const DESKTOP_ICONS: &[(u32, &[u8])] = &[
    (16, include_bytes!("../assets/icons/hicolor/16x16/apps/honrp.png")),
    (24, include_bytes!("../assets/icons/hicolor/24x24/apps/honrp.png")),
    (32, include_bytes!("../assets/icons/hicolor/32x32/apps/honrp.png")),
    (48, include_bytes!("../assets/icons/hicolor/48x48/apps/honrp.png")),
    (64, include_bytes!("../assets/icons/hicolor/64x64/apps/honrp.png")),
    (128, include_bytes!("../assets/icons/hicolor/128x128/apps/honrp.png")),
    (256, include_bytes!("../assets/icons/hicolor/256x256/apps/honrp.png")),
    (512, include_bytes!("../assets/icons/hicolor/512x512/apps/honrp.png")),
];

/// Install the app-menu entry and hicolor icons for this user. This is what
/// makes Wayland compositors show the real icon: they match the window's
/// app_id ("honrp") to honrp.desktop, and the Icon= key to hicolor/honrp.png.
#[cfg(unix)]
fn install_desktop_entry() -> Result<()> {
    let data = dirs::data_dir().context("no XDG data directory")?;
    let apps = data.join("applications");
    fs::create_dir_all(&apps)?;
    let exe = std::env::current_exe().context("finding the honrp executable")?;
    let exe_text = exe.to_string_lossy();
    let exec = if exe_text.contains(' ') {
        format!("\"{exe_text}\"")
    } else {
        exe_text.to_string()
    };
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=honrp\n\
         GenericName=Discord Rich Presence\n\
         Comment=Discord Rich Presence for Heroes of Newerth\n\
         Exec={exec}\n\
         Icon=honrp\n\
         Terminal=false\n\
         Categories=Utility;\n\
         Keywords=Heroes of Newerth;HoN;Discord;Rich Presence;\n\
         StartupNotify=false\n\
         StartupWMClass=honrp\n"
    );
    fs::write(apps.join("honrp.desktop"), desktop)?;
    for (size, bytes) in DESKTOP_ICONS {
        let dir = data
            .join("icons")
            .join("hicolor")
            .join(format!("{size}x{size}"))
            .join("apps");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("honrp.png"), bytes)?;
    }
    Ok(())
}

#[cfg(unix)]
fn remove_desktop_entry() {
    if let Some(data) = dirs::data_dir() {
        let _ = fs::remove_file(data.join("applications").join("honrp.desktop"));
        for (size, _) in DESKTOP_ICONS {
            let _ = fs::remove_file(
                data.join("icons")
                    .join("hicolor")
                    .join(format!("{size}x{size}"))
                    .join("apps")
                    .join("honrp.png"),
            );
        }
    }
}

fn install(
    user_dir: Option<PathBuf>,
    prefix: Option<PathBuf>,
    client_id: Option<String>,
    no_service: bool,
) -> Result<()> {
    #[cfg(windows)]
    let user_dir = resolve_user_dir(user_dir, prefix)
        .or_else(|| dirs::document_dir().map(|docs| docs.join("Juvio").join("Heroes of Newerth")));
    #[cfg(unix)]
    let user_dir = resolve_user_dir(user_dir, prefix);
    let Some(ud) = user_dir else {
        bail!(
            "could not find the Heroes of Newerth user directory.\n\
             Start HoN once, or pass --prefix '<pfx>' / --user-dir '<dir>'"
        );
    };
    if let Some(id) = client_id {
        set_client_id(&id)?;
    }
    let target = mod_install(&ud)?;
    println!("installed Lua bridge -> {}", target.display());
    #[cfg(unix)]
    match install_desktop_entry() {
        Ok(()) => println!("installed the desktop entry and icons"),
        Err(e) => eprintln!("warning: could not install the desktop entry: {e}"),
    }
    println!();
    println!("What this does:");
    println!("  - adds a managed block to the game's startup.lua (backup kept)");
    println!("  - the bridge prints one state line per event / 10 s into console.log");
    println!("  - the daemon reads that log and talks only to Discord's local socket");
    println!("  - no internet, no game memory access, no input, no game files changed");
    println!();
    #[cfg(unix)]
    {
        let cfg = Config::load(None);
        if cfg.client_id.is_empty() {
            eprintln!("warning: no Discord application id configured yet.");
            eprintln!("  create one at https://discord.com/developers/applications and run:");
            eprintln!("    honrp install --client-id <ID>");
            eprintln!("  (the service is not started until an id is configured)");
        } else if !no_service {
            match service_install() {
                Ok(()) => println!("systemd user service installed and started"),
                Err(e) => println!("warning: systemd service not installed: {e}"),
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = no_service;
        println!("The Lua bridge is installed. Launch honrp when you play;");
        println!("the daemon runs in the background while the window is closed.");
    }
    println!("If Discord also shows a separate 'Juvio' game, disable it once in");
    println!("Settings -> Game Activity -> Registered Games (honrp cannot change that list).");
    println!("Restart Heroes of Newerth to load the bridge, then run `honrp doctor`.");
    Ok(())
}

fn uninstall(user_dir: Option<PathBuf>, purge: bool) -> Result<()> {
    // a running daemon holds the exe open on Windows
    let _ = control::stop();
    #[cfg(unix)]
    {
        let _ = service_uninstall();
        remove_desktop_entry();
    }
    if let Some(ud) = resolve_user_dir(user_dir, None) {
        mod_uninstall(&ud)?;
        println!("removed the Lua bridge from {}", ud.display());
    } else {
        println!("user dir not found; could not remove the Lua bridge");
    }
    #[cfg(windows)]
    {
        let _ = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "honrp",
                "/f",
            ])
            .status();
    }
    if purge {
        if let Some(dir) = config::config_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            let _ = fs::remove_dir_all(&dir);
            println!("removed {}", dir.display());
        }
    }
    Ok(())
}

fn mod_installed(ud: &Path) -> bool {
    fs::read_to_string(ud.join("startup.lua"))
        .map(|t| t.contains("[HONRP]"))
        .unwrap_or(false)
}

fn mod_build_from(text: &str) -> Option<u32> {
    let idx = text.find("local BUILD = ")? + "local BUILD = ".len();
    text[idx..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn mod_build(ud: &Path) -> Option<u32> {
    let text = fs::read_to_string(ud.join("startup.lua")).ok()?;
    mod_build_from(&text)
}

/// The bridge ships inside the binary.  Rewrite the managed block whenever the
/// installed one differs (new build, or any Lua edit), so `honrp run` upgrades
/// an existing install instead of requiring `honrp install` again.
fn sync_bridge(ud: &Path) -> Result<bool> {
    let existing = fs::read_to_string(ud.join("startup.lua")).unwrap_or_default();
    if existing.contains(MOD_SRC) {
        return Ok(false);
    }
    mod_install(ud)?;
    Ok(true)
}

fn strip_managed(mut text: String) -> String {
    while let Some(start) = text.find(BEGIN) {
        match text[start..].find(END) {
            Some(rel) => {
                let end = start + rel + END.len();
                text.replace_range(start..end, "");
            }
            None => {
                text.truncate(start);
                break;
            }
        }
    }
    text
}

fn mod_install(ud: &Path) -> Result<PathBuf> {
    fs::create_dir_all(ud).with_context(|| format!("creating {}", ud.display()))?;
    let target = ud.join("startup.lua");
    let existing = fs::read_to_string(&target).unwrap_or_default();
    let user_part = strip_managed(existing);
    let backup = ud.join("startup.lua.honrp.bak");
    if !user_part.trim().is_empty() && !backup.exists() {
        fs::write(&backup, &user_part).with_context(|| "writing backup")?;
    }
    let body = if user_part.trim().is_empty() {
        format!("{BEGIN}\n{MOD_SRC}\n{END}\n")
    } else {
        format!("{}\n\n{BEGIN}\n{MOD_SRC}\n{END}\n", user_part.trim_end())
    };
    fs::write(&target, body).with_context(|| format!("writing {}", target.display()))?;
    Ok(target)
}

fn mod_uninstall(ud: &Path) -> Result<()> {
    let target = ud.join("startup.lua");
    if !target.exists() {
        return Ok(());
    }
    let rest = strip_managed(fs::read_to_string(&target)?);
    if rest.trim().is_empty() {
        fs::remove_file(&target)?;
    } else {
        fs::write(&target, format!("{}\n", rest.trim_end()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn service_unit_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config/systemd/user")
        .join("honrp.service"))
}

#[cfg(unix)]
fn sh(cmd: &mut Command) -> Result<()> {
    let out = cmd.output().context("running command")?;
    if !out.status.success() {
        bail!(
            "{} failed: {}",
            cmd.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(unix)]
fn systemd_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn autostart_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config/autostart")
        .join("honrp.desktop"))
}

#[cfg(unix)]
fn autostart_install(exe: &Path) -> Result<()> {
    let path = autostart_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Heroes of Newerth Rich Presence\n\
         Comment=honrp Discord Rich Presence daemon\n\
         Exec=\"{}\" run\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    fs::write(&path, entry)?;
    println!(
        "systemd user session not available; installed autostart entry {}",
        path.display()
    );
    println!("start it now with: \"{}\" run &", exe.display());
    Ok(())
}

#[cfg(unix)]
fn service_install() -> Result<()> {
    let exe = std::env::current_exe()?;
    if !systemd_available() {
        return autostart_install(&exe);
    }
    let path = service_unit_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let unit = format!(
        "[Unit]\n\
         Description=Heroes of Newerth Discord Rich Presence (honrp)\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart=\"{}\" run\n\
         Restart=always\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe.display()
    );
    fs::write(&path, unit)?;
    sh(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    sh(Command::new("systemctl").args(["--user", "enable", "--now", "honrp.service"]))?;
    println!("service: {}", path.display());
    Ok(())
}

#[cfg(unix)]
fn service_uninstall() -> Result<()> {
    if systemd_available() {
        let path = service_unit_path()?;
        let _ = sh(Command::new("systemctl").args(["--user", "disable", "--now", "honrp.service"]));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        let _ = sh(Command::new("systemctl").args(["--user", "daemon-reload"]));
    }
    if let Ok(path) = autostart_path() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    println!("service/autostart removed");
    Ok(())
}

#[cfg(unix)]
fn service_status() -> Result<()> {
    if systemd_available() {
        let out = Command::new("systemctl")
            .args(["--user", "status", "honrp.service", "--no-pager"])
            .output()?;
        print!("{}", String::from_utf8_lossy(&out.stdout));
        print!("{}", String::from_utf8_lossy(&out.stderr));
    } else {
        let path = autostart_path()?;
        println!(
            "no systemd user session; autostart entry {}: {}",
            path.display(),
            if path.exists() {
                "present"
            } else {
                "not installed"
            }
        );
    }
    Ok(())
}

#[cfg(unix)]
fn service_dispatch(action: ServiceAction) -> Result<()> {
    match action {
        ServiceAction::Install => service_install(),
        ServiceAction::Uninstall => service_uninstall(),
        ServiceAction::Status => service_status(),
    }
}

#[cfg(windows)]
fn service_dispatch(_action: ServiceAction) -> Result<()> {
    println!("service management is not implemented on Windows in this alpha.");
    println!("run `honrp run` in a terminal while playing.");
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: &View) {}

/// A daemon started from the Run key / double-click gets its own console;
/// hide it.  When launched from a terminal, keep the console (other processes).
#[cfg(windows)]
fn hide_console_if_sole() {
    use windows_sys::Win32::System::Console::{
        FreeConsole, GetConsoleProcessList, GetConsoleWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        let hwnd = GetConsoleWindow();
        if hwnd.is_null() {
            return;
        }
        let mut pids = [0u32; 2];
        let count = GetConsoleProcessList(pids.as_mut_ptr(), 2);
        if count <= 1 {
            ShowWindow(hwnd, SW_HIDE);
            // Detach and tear down the console instead of leaving a hidden
            // conhost/Windows Terminal window around.
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
fn hide_console_if_sole() {}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("honrp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn embedded_bridge_has_a_build_number() {
        assert!(mod_build_from(MOD_SRC).unwrap() >= 11);
    }

    #[test]
    fn sync_bridge_installs_and_upgrades() {
        let dir = scratch("bridge-sync");
        assert!(sync_bridge(&dir).unwrap(), "fresh dir should be installed");
        assert!(!sync_bridge(&dir).unwrap(), "second run should be a no-op");
        assert!(sync_bridge(&dir).is_ok());
        assert_eq!(mod_build(&dir), mod_build_from(MOD_SRC));

        // an older managed block is replaced, user content is preserved
        fs::write(
            dir.join("startup.lua"),
            "-- user content\n--- HONRP BEGIN ---\nlocal BUILD = 1\n--- HONRP END ---\n",
        )
        .unwrap();
        assert!(sync_bridge(&dir).unwrap());
        let text = fs::read_to_string(dir.join("startup.lua")).unwrap();
        assert!(text.contains("-- user content"), "user content lost");
        assert!(text.contains(MOD_SRC), "managed block not replaced");
        assert_eq!(mod_build(&dir), mod_build_from(MOD_SRC));

        let _ = fs::remove_dir_all(&dir);
    }
}
