//! Shared state for the Windows control window.
//!
//! The daemon loop publishes a small snapshot here; the GUI (or any other
//! front-end) polls it.  On Unix nothing writes to it, which is fine.

use std::sync::{Mutex, OnceLock};

#[derive(Clone, Default)]
pub struct Snapshot {
    pub daemon: bool,
    pub discord: bool,
    pub discord_found: Option<bool>,
    pub game_pid: Option<i32>,
    pub in_game: bool,
    pub in_queue: bool,
    pub details: String,
    pub state: String,
    pub update_text: String,
    pub user_dir: Option<String>,
}

fn cell() -> &'static Mutex<Snapshot> {
    static CELL: OnceLock<Mutex<Snapshot>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Snapshot::default()))
}

pub fn set(f: impl FnOnce(&mut Snapshot)) {
    if let Ok(mut guard) = cell().lock() {
        f(&mut guard);
    }
}

pub fn get() -> Snapshot {
    cell().lock().map(|g| g.clone()).unwrap_or_default()
}
