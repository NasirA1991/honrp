mod activity;
mod config;
mod detect;
mod ipc;
mod matchlog;
mod modstate;
mod run;
mod tail;

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
    #[command(subcommand)]
    cmd: Cmd,
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
        Cmd::Run {
            dry_run,
            user_dir,
            ipc_socket,
        } => run::run(
            &cfg,
            RunOpts {
                dry_run,
                verbose: cli.verbose,
                user_dir,
                ipc_socket,
            },
        ),
        Cmd::Status { json, user_dir } => status(&cfg, json, user_dir),
        Cmd::Doctor { user_dir } => doctor(&cfg, user_dir),
        Cmd::Install {
            user_dir,
            prefix,
            client_id,
            no_service,
        } => install(user_dir, prefix, client_id, no_service),
        Cmd::Uninstall { user_dir } => uninstall(user_dir),
        Cmd::Service { action } => match action {
            ServiceAction::Install => service_install(),
            ServiceAction::Uninstall => service_uninstall(),
            ServiceAction::Status => service_status(),
        },
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

    if as_json {
        let out = json!({
            "game_process": games.last().map(|g| json!({
                "pid": g.pid,
                "prefix": g.prefix,
                "cmdline": g.cmdline,
            })),
            "user_dir": ud,
            "mod_installed": installed,
            "mod_build": ud.as_ref().and_then(|d| mod_build(d)),
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
    let path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME not set")?
        .join(".config/honrp/config.json");
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

fn install(
    user_dir: Option<PathBuf>,
    prefix: Option<PathBuf>,
    client_id: Option<String>,
    no_service: bool,
) -> Result<()> {
    let Some(ud) = resolve_user_dir(user_dir, prefix) else {
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
    println!();
    println!("What this does:");
    println!("  - adds a managed block to the game's startup.lua (backup kept)");
    println!("  - the bridge prints one state line per event / 10 s into console.log");
    println!("  - the daemon reads that log and talks only to Discord's local socket");
    println!("  - no internet, no game memory access, no input, no game files changed");
    println!();
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
    println!("Restart Heroes of Newerth to load the bridge, then run `honrp doctor`.");
    Ok(())
}

fn uninstall(user_dir: Option<PathBuf>) -> Result<()> {
    let _ = service_uninstall();
    if let Some(ud) = resolve_user_dir(user_dir, None) {
        mod_uninstall(&ud)?;
        println!("removed the Lua bridge from {}", ud.display());
    } else {
        println!("user dir not found; only the service was removed");
    }
    Ok(())
}

fn mod_installed(ud: &Path) -> bool {
    fs::read_to_string(ud.join("startup.lua"))
        .map(|t| t.contains("[HONRP]"))
        .unwrap_or(false)
}

fn mod_build(ud: &Path) -> Option<u32> {
    let text = fs::read_to_string(ud.join("startup.lua")).ok()?;
    let idx = text.find("local BUILD = ")? + "local BUILD = ".len();
    text[idx..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
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

fn service_unit_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config/systemd/user")
        .join("honrp.service"))
}

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

fn service_install() -> Result<()> {
    let path = service_unit_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let exe = std::env::current_exe()?;
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

fn service_uninstall() -> Result<()> {
    let path = service_unit_path()?;
    let _ = sh(Command::new("systemctl").args(["--user", "disable", "--now", "honrp.service"]));
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let _ = sh(Command::new("systemctl").args(["--user", "daemon-reload"]));
    println!("service removed");
    Ok(())
}

fn service_status() -> Result<()> {
    let out = Command::new("systemctl")
        .args(["--user", "status", "honrp.service", "--no-pager"])
        .output()?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    print!("{}", String::from_utf8_lossy(&out.stderr));
    Ok(())
}

#[allow(dead_code)]
fn _unused(_: &View) {}
