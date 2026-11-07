use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};

use crate::activity::{self, View};
use crate::config::Config;
use crate::detect::{self, GameProcess};
use crate::ipc::DiscordIpc;
use crate::matchlog;
use crate::modstate::{self, ModState};
use crate::tail::ConsoleTail;

fn epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sync_bridge(ud: &Path) {
    match crate::sync_bridge(ud) {
        Ok(true) => eprintln!(
            "honrp: installed the Lua bridge in {} (build {}) - restart Heroes of Newerth once",
            ud.display(),
            crate::mod_build_from(crate::MOD_SRC)
                .map(|b| b.to_string())
                .unwrap_or_else(|| "?".into())
        ),
        Ok(false) => {}
        Err(e) => eprintln!("honrp: could not update the Lua bridge: {e}"),
    }
}

fn log_line(msg: &str) {
    let Some(dir) = crate::config::config_path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("daemon.log");
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 1_000_000 {
            let _ = std::fs::remove_file(&path);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(file, "[{}] {}", epoch(), msg);
    }
}

pub struct StateMachine {
    explicit_user_dir: Option<PathBuf>,
    pub user_dir: Option<PathBuf>,
    tail: Option<ConsoleTail>,
    games: Vec<GameProcess>,
    games_at: Option<Instant>,
    pid: Option<i32>,
    session_start: Option<i64>,
    mod_state: Option<ModState>,
    mod_seen: Option<Instant>,
    map_id: String,
    mode: String,
    match_started: Option<i64>,
    match_source: &'static str,
    log_key: Option<(PathBuf, u64)>,
    fallback_in_game: bool,
    fallback_map: String,
    first_poll: bool,
}

impl StateMachine {
    pub fn new(user_dir: Option<PathBuf>) -> Self {
        Self {
            explicit_user_dir: user_dir.clone(),
            user_dir,
            tail: None,
            games: Vec::new(),
            games_at: None,
            pid: None,
            session_start: None,
            mod_state: None,
            mod_seen: None,
            map_id: String::new(),
            mode: String::new(),
            match_started: None,
            match_source: "",
            log_key: None,
            fallback_in_game: false,
            fallback_map: String::new(),
            first_poll: true,
        }
    }

    fn fresh_mod(&self) -> Option<&ModState> {
        self.mod_seen?;
        if self.mod_seen?.elapsed() > Duration::from_secs(40) {
            None
        } else {
            self.mod_state.as_ref()
        }
    }

    pub fn pid(&self) -> Option<i32> {
        self.pid
    }

    pub fn in_match(&self) -> bool {
        match self.fresh_mod() {
            Some(m) => m.in_game || m.game_phase == Some(3),
            None => self.fallback_in_game,
        }
    }

    fn ensure_tail(&mut self, prefix: Option<&std::path::Path>) {
        if self.tail.is_some() {
            return;
        }
        if let Some(ud) = detect::pick_user_dir(prefix, self.explicit_user_dir.as_deref()) {
            let mut tail = ConsoleTail::new(ud.join("console.log"));
            // don't replay the previous session's state lines
            tail.seek_to_end();
            self.tail = Some(tail);
            self.user_dir = Some(ud);
        }
    }

    pub fn poll(&mut self) {
        let now = Instant::now();
        if self
            .games_at
            .map(|t| now.duration_since(t) > Duration::from_secs(3))
            .unwrap_or(true)
        {
            self.games = detect::find_games();
            self.games_at = Some(now);
        }
        let Some(game) = self.games.last().cloned() else {
            self.pid = None;
            self.tail = None;
            self.mod_state = None;
            self.mod_seen = None;
            self.session_start = None;
            self.match_started = None;
            self.match_source = "";
            self.first_poll = true;
            self.fallback_in_game = false;
            return;
        };

        if self.pid != Some(game.pid) {
            self.pid = Some(game.pid);
            self.session_start = game.start_time.or_else(|| Some(epoch()));
            self.mod_state = None;
            self.mod_seen = None;
            self.match_started = None;
            self.match_source = "";
            self.map_id.clear();
            self.mode.clear();
            self.log_key = None;
            self.first_poll = true;
            self.fallback_in_game = false;
            // Re-resolve the user dir from the new process's prefix: another
            // game instance may run from a different Proton/Wine prefix.
            self.tail = None;
            self.user_dir = None;
        }
        self.ensure_tail(game.prefix.as_deref());

        if let Some(tail) = self.tail.as_mut() {
            let lines = tail.poll();
            let first = self.first_poll;
            for line in lines {
                if let Some(m) = modstate::parse(&line) {
                    self.mod_state = Some(m);
                    self.mod_seen = Some(now);
                } else if !first {
                    if line.contains("Client loading world ") {
                        self.fallback_in_game = true;
                        if let Some(rest) = line.split("Client loading world ").nth(1) {
                            self.fallback_map = rest.trim().trim_matches('"').to_string();
                        }
                    } else if line.contains("MainMenu")
                        || line.contains("Disconnected")
                        || line.contains("CMD(Disconnect)")
                    {
                        self.fallback_in_game = false;
                    }
                }
            }
            self.first_poll = false;
        }

        if self.in_match() {
            if let Some(ud) = self.user_dir.clone() {
                if let Some(info) = matchlog::newest(&ud) {
                    let key = (info.path.clone(), info.mtime);
                    if self.log_key.as_ref() != Some(&key) {
                        self.log_key = Some(key);
                        if !info.map_id.is_empty() {
                            self.map_id = info.map_id;
                        }
                        if !info.mode.is_empty() {
                            self.mode = info.mode;
                        }
                    }
                }
            }
            if let Some(clock) = self.fresh_mod().and_then(|m| m.clock) {
                if clock >= 0 && self.match_source != "clock" {
                    self.match_started = Some(epoch() - clock);
                    self.match_source = "clock";
                }
            }
        } else {
            self.match_started = None;
            self.match_source = "";
        }
    }

    pub fn view(&self) -> View<'_> {
        let m = self.fresh_mod();
        let in_match = self.in_match();
        let mut map = self.map_id.as_str();
        let mut mode = self.mode.as_str();
        if let Some(m) = m {
            if !m.world.is_empty() {
                map = &m.world;
            }
            if !m.lobby_mode.is_empty() {
                mode = &m.lobby_mode;
            }
        }
        // Practice games ("Learn" -> play a hero) keep the lobby's Caldavar
        // world but actually load `fieldtest`; the console log is authoritative
        // there while the lobby metadata is not.
        if in_match && self.fallback_map.eq_ignore_ascii_case("fieldtest") {
            map = self.fallback_map.as_str();
        }
        if !in_match && map.is_empty() {
            map = self.fallback_map.as_str();
        }
        View {
            in_game: in_match,
            in_queue: m
                .map(|m| m.in_queue || m.mm_state == Some(3))
                .unwrap_or(false),
            game_phase: m.and_then(|m| m.game_phase),
            panel: m.map(|m| m.panel.as_str()).unwrap_or(""),
            hero: m.map(|m| m.hero.as_str()).unwrap_or(""),
            view_hero: m.map(|m| m.view_hero.as_str()).unwrap_or(""),
            account: m.map(|m| m.account.as_str()).unwrap_or(""),
            arranged_match: m.and_then(|m| m.arranged_match),
            map,
            mode,
            level: m.and_then(|m| m.level),
            kills: m.and_then(|m| m.kills),
            deaths: m.and_then(|m| m.deaths),
            assists: m.and_then(|m| m.assists),
            local_client: m.and_then(|m| m.local_client),
            view_client: m.and_then(|m| m.view_client),
            match_started: self.match_started,
            session_start: self.session_start,
        }
    }
}

pub struct RunOpts {
    pub dry_run: bool,
    pub verbose: bool,
    pub user_dir: Option<PathBuf>,
    pub ipc_socket: Option<String>,
}

pub fn run(cfg: &Config, opts: RunOpts) -> Result<()> {
    if !opts.dry_run && cfg.client_id.is_empty() {
        bail!(
            "no Discord application id configured.\n\
             Create one at https://discord.com/developers/applications and put its\n\
             Application ID in ~/.config/honrp/config.json (client_id) or export HONRP_CLIENT_ID."
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    if crate::control::running() {
        eprintln!("honrp: a daemon is already running");
        return Ok(());
    }
    let control_stop = stop.clone();
    std::thread::spawn(move || crate::control::serve(control_stop));
    log_line(&format!("daemon starting (pid {})", std::process::id()));
    #[cfg(unix)]
    {
        signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
        signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    }

    // CLI flag wins, then the manual override from the config (set by the
    // "Browse" button in the control window), then auto-detection.
    let explicit_user_dir = opts.user_dir.clone().or_else(|| {
        let s = cfg.user_dir.trim();
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    });
    let ipc_override = opts.ipc_socket.clone().or_else(|| {
        let s = cfg.ipc_socket.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    });

    let mut sm = StateMachine::new(explicit_user_dir.clone());
    sm.poll();
    eprintln!(
        "honrp: watching {}, user dir: {}",
        match sm.pid() {
            Some(pid) => format!("pid {pid}"),
            None => "for juvio.exe".to_string(),
        },
        sm.user_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "?".into())
    );

    // Ship the Lua bridge inside the binary: upgrade an older managed block
    // automatically instead of requiring `honrp install` after every release.
    // A first-time user may launch the daemon before the game (or before the
    // Documents folder exists), so on Windows fall back to the known path.
    let mut bridge_dir: Option<PathBuf> = None;
    #[cfg(windows)]
    let initial_bridge_dir = sm
        .user_dir
        .clone()
        .or_else(|| detect::pick_user_dir(None, explicit_user_dir.as_deref()))
        .or_else(|| {
            dirs::document_dir().map(|d| d.join("Juvio").join("Heroes of Newerth"))
        });
    #[cfg(not(windows))]
    let initial_bridge_dir = sm.user_dir.clone();

    if let Some(ud) = initial_bridge_dir {
        sm.user_dir = Some(ud.clone());
        sync_bridge(&ud);
        bridge_dir = Some(ud);
    }

    if cfg.check_updates {
        std::thread::spawn(crate::update::notify_if_newer);
    }

    let poll = Duration::from_secs_f64(cfg.poll_interval.max(0.2));
    let mut ipc: Option<DiscordIpc> = None;
    let mut last_activity: Option<serde_json::Value> = None;
    let mut last_pid: Option<i32> = None;
    let mut last_sent: Option<Instant> = None;
    let mut last_attempt: Option<Instant> = None;
    let mut last_warn: Option<Instant> = None;

    // Discord endpoint probing is throttled (connecting on every poll shows up
    // in Discord's logs) and debounced so a client restart does not flicker the
    // label. `None` = not probed yet.
    let mut discord_found: Option<bool> = None;
    let mut pending_found: Option<bool> = None;
    let mut pending_count = 0u32;
    let mut last_probe: Option<Instant> = None;

    while !stop.load(Ordering::Relaxed) {
        sm.poll();
        // The user dir may only become known once the game is running (Linux
        // prefixes, a late `juvio.exe`); install/upgrade the bridge at that point.
        if bridge_dir.is_none() {
            if let Some(ud) = sm.user_dir.clone() {
                sync_bridge(&ud);
                bridge_dir = Some(ud);
            }
        }
        let view = sm.view();
        // no game -> no presence, and the IPC connection is dropped below
        let activity = if sm.pid().is_some() {
            activity::build(&view, cfg)
        } else {
            None
        };
        let now = Instant::now();

        let should_probe = last_probe
            .map(|t| now.duration_since(t) >= Duration::from_millis(1500))
            .unwrap_or(true);
        if should_probe {
            last_probe = Some(now);
            let found = crate::ipc::available(ipc_override.as_deref());
            match discord_found {
                None => discord_found = Some(found),
                Some(current) if current == found => {
                    pending_found = None;
                    pending_count = 0;
                }
                Some(_) => {
                    if pending_found == Some(found) {
                        pending_count += 1;
                    } else {
                        pending_found = Some(found);
                        pending_count = 1;
                    }
                    if pending_count >= 2 {
                        discord_found = Some(found);
                        pending_found = None;
                        pending_count = 0;
                    }
                }
            }
        }

        // published for the Windows control window (no-op elsewhere)
        crate::status::set(|s| {
            s.daemon = true;
            s.discord = ipc.is_some();
            s.discord_found = discord_found;
            s.game_pid = sm.pid();
            s.in_game = view.in_game;
            s.in_queue = view.in_queue;
            s.user_dir = sm
                .user_dir
                .as_ref()
                .map(|p| p.display().to_string());
            match &activity {
                Some(a) => {
                    s.details = a.details.clone();
                    s.state = a.state.clone();
                }
                None => {
                    s.details.clear();
                    s.state.clear();
                }
            }
        });

        if opts.dry_run {
            if let Some(a) = &activity {
                let val = a.to_json();
                if last_activity.as_ref() != Some(&val) {
                    println!(
                        "{}",
                        serde_json::json!({
                            "activity": val,
                            "in_game": view.in_game,
                            "in_queue": view.in_queue,
                            "panel": view.panel,
                        })
                    );
                    last_activity = Some(val);
                }
            }
            std::thread::sleep(poll);
            continue;
        }

        let Some(activity) = activity else {
            if let Some(mut rpc) = ipc.take() {
                if let Some(pid) = last_pid {
                    let _ = rpc.clear(pid);
                }
                if opts.verbose {
                    eprintln!("discord: game closed, presence cleared");
                }
            }
            last_activity = None;
            std::thread::sleep(poll);
            continue;
        };

        if ipc.is_none() {
            if last_attempt
                .map(|t| now.duration_since(t) < Duration::from_secs(5))
                .unwrap_or(false)
            {
                std::thread::sleep(poll);
                continue;
            }
            last_attempt = Some(now);
            match DiscordIpc::connect(&cfg.client_id, ipc_override.as_deref()) {
                Ok(rpc) => {
                    if opts.verbose {
                        eprintln!("discord: connected");
                    }
                    ipc = Some(rpc);
                    last_activity = None;
                }
                Err(e) => {
                    if last_warn
                        .map(|t| now.duration_since(t) > Duration::from_secs(30))
                        .unwrap_or(true)
                    {
                        last_warn = Some(now);
                        eprintln!("honrp: waiting for Discord ({e})");
                    }
                    std::thread::sleep(poll);
                    continue;
                }
            }
        }

        let val = activity.to_json();
        let changed = last_activity.as_ref() != Some(&val);
        let due = last_sent
            .map(|t| now.duration_since(t) >= Duration::from_secs(5))
            .unwrap_or(true);
        let refresh_due = last_sent
            .map(|t| {
                now.duration_since(t) >= Duration::from_secs_f64(cfg.refresh_interval.max(5.0))
            })
            .unwrap_or(true);

        if let Some(rpc) = ipc.as_mut() {
            rpc.drain();
            if (changed || refresh_due) && due {
                let pid = sm.pid().unwrap_or(std::process::id() as i32);
                match rpc.set_activity(&val, pid) {
                    Ok(()) => {
                        last_pid = Some(pid);
                        last_activity = Some(val);
                        last_sent = Some(now);
                        if opts.verbose {
                            eprintln!("discord: {} / {}", activity.details, activity.state);
                        }
                    }
                    Err(e) => {
                        eprintln!("honrp: discord error: {e}");
                        ipc = None;
                    }
                }
            }
        }
        std::thread::sleep(poll);
    }

    if let Some(mut rpc) = ipc {
        if let Some(pid) = last_pid {
            let _ = rpc.clear(pid);
        }
    }
    log_line("daemon stopped");
    Ok(())
}
