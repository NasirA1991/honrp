use std::path::{Path, PathBuf};
use std::process::Command;

use super::GameProcess;

/// Find the native Windows `juvio.exe` process with tasklist (no admin).
pub fn find_games() -> Vec<GameProcess> {
    let Ok(out) = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq juvio.exe", "/FO", "CSV", "/NH"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut games = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('"') {
            continue;
        }
        // "juvio.exe","1234","Console","1","12 345 K"
        let fields: Vec<&str> = line.split("\",\"").collect();
        if fields.len() < 2 {
            continue;
        }
        if !fields[0]
            .trim_start_matches('"')
            .eq_ignore_ascii_case("juvio.exe")
        {
            continue;
        }
        if let Ok(pid) = fields[1].trim_matches('"').parse::<i32>() {
            games.push(GameProcess {
                pid,
                prefix: None,
                cmdline: String::new(),
                start_time: None,
            });
        }
    }
    games
}

fn documents_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(docs) = dirs::document_dir() {
        let ud = docs.join("Juvio").join("Heroes of Newerth");
        if ud.is_dir() {
            out.push(ud);
        }
        // Very old installs used Documents/Heroes of Newerth.
        let legacy = docs.join("Heroes of Newerth");
        if legacy.is_dir() {
            out.push(legacy);
        }
    }
    out
}

pub fn user_dir_candidates(_prefix: Option<&Path>) -> Vec<PathBuf> {
    documents_candidates()
}

pub fn pick_user_dir(_prefix: Option<&Path>, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.is_dir().then(|| p.to_path_buf());
    }
    documents_candidates().into_iter().next()
}
