//! Tiny loopback control channel between the GUI and the background daemon.
//!
//! The daemon listens on 127.0.0.1:47832 and answers one-line requests:
//!   `ping`   -> `ok`
//!   `status` -> one-line JSON (game/Discord state)
//!   `stop`   -> `ok`, then the daemon shuts down
//!
//! The GUI uses this to adopt an already-running daemon, start one when it is
//! missing, poll its state and stop it on request.  No external crates and no
//! named-pipe differences between Windows and Linux.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const PORT: u16 = 47832;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonStatus {
    pub daemon: bool,
    pub discord: bool,
    pub discord_found: Option<bool>,
    pub game_pid: Option<i32>,
    pub in_game: bool,
    pub in_queue: bool,
    pub details: String,
    pub state: String,
    pub user_dir: Option<String>,
    /// Daemon-level notice (start-up errors, update text, ...).
    pub message: String,
}

fn current() -> DaemonStatus {
    let s = crate::status::get();
    DaemonStatus {
        daemon: true,
        discord: s.discord,
        discord_found: s.discord_found,
        game_pid: s.game_pid,
        in_game: s.in_game,
        in_queue: s.in_queue,
        details: s.details,
        state: s.state,
        user_dir: s.user_dir,
        message: s.update_text,
    }
}

fn request(cmd: &str, timeout: Duration) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    writeln!(stream, "{cmd}").ok()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    Some(line.trim().to_string())
}

pub fn running() -> bool {
    request("ping", Duration::from_millis(400)).as_deref() == Some("ok")
}

pub fn status() -> Option<DaemonStatus> {
    let line = request("status", Duration::from_millis(700))?;
    serde_json::from_str(&line).ok()
}

pub fn stop() -> bool {
    request("stop", Duration::from_millis(1500)).as_deref() == Some("ok")
}

/// Blocking accept loop; run it on its own thread inside the daemon.
pub fn serve(stop: Arc<AtomicBool>) {
    // A just-stopped daemon can leave the port in TIME_WAIT; a single failed
    // bind used to make Start Rich Presence silently fail. Retry for up to two
    // minutes instead of exiting immediately.
    let listener = {
        let mut attempts = 0u32;
        loop {
            match TcpListener::bind(("127.0.0.1", PORT)) {
                Ok(listener) => break listener,
                Err(e) => {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    attempts += 1;
                    if attempts == 1 {
                        eprintln!("control: bind failed ({e}); retrying for up to 2 min");
                    }
                    if attempts >= 240 {
                        eprintln!("control: giving up on port {PORT}: {e}");
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    };
    for stream in listener.incoming().flatten() {
        let mut stream = stream;
        let mut line = String::new();
        let Ok(clone) = stream.try_clone() else {
            continue;
        };
        let mut reader = BufReader::new(clone);
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        let cmd = line.trim();
        let reply = match cmd {
            "status" => serde_json::to_string(&current()).unwrap_or_else(|_| "{}".into()),
            "ping" => "ok".to_string(),
            "stop" => {
                stop.store(true, Ordering::Relaxed);
                "ok".to_string()
            }
            _ => "err".to_string(),
        };
        let _ = writeln!(stream, "{reply}");
        if cmd == "stop" {
            break;
        }
    }
}

static LATEST: OnceLock<Mutex<Option<DaemonStatus>>> = OnceLock::new();

fn latest_cell() -> &'static Mutex<Option<DaemonStatus>> {
    LATEST.get_or_init(|| Mutex::new(None))
}

pub fn latest() -> Option<DaemonStatus> {
    latest_cell().lock().ok().and_then(|g| g.clone())
}

/// Poll the daemon in the background so the GUI never blocks on a dead port.
pub fn start_poller() {
    std::thread::spawn(|| loop {
        let value = status();
        if let Ok(mut guard) = latest_cell().lock() {
            *guard = value;
        }
        std::thread::sleep(Duration::from_millis(500));
    });
}

/// Spawn `honrp run` as a detached process (no console on Windows).
pub fn spawn_daemon() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("run");
    // The GUI calls FreeConsole() on Windows, so its inherited std handles can
    // be invalid; give the daemon null stdio (it logs to daemon.log itself).
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // own process group: closing a terminal must not take the daemon down
        cmd.process_group(0);
    }
    let _ = cmd.spawn();
}

/// Start the daemon if it isn't already answering, and wait until it does.
pub fn ensure_daemon() -> bool {
    if running() {
        return true;
    }
    spawn_daemon();
    // The daemon retries a TIME_WAIT port for up to 2 min; give it a longer
    // window than the old 4.5 s so Start Rich Presence does not look failed.
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(250));
        if running() {
            return true;
        }
    }
    false
}
