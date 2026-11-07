use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Default)]
pub struct MatchInfo {
    pub path: PathBuf,
    pub mtime: u64,
    pub map_id: String,
    pub mode: String,
}

#[allow(clippy::chunks_exact_to_as_chunks)]
fn decode(path: &Path) -> Option<String> {
    let raw = fs::read(path).ok()?;
    let raw = raw.get(..8192.min(raw.len()))?;
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn field_after(text: &str, marker: &str, key: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let key_pos = rest.find(key)? + key.len();
    let rest = &rest[key_pos..];
    let first = rest.find('"')? + 1;
    let rest = &rest[first..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn parse(path: &Path) -> Option<MatchInfo> {
    let text = decode(path)?;
    let map_id = field_after(&text, "INFO_MAP", "name:").unwrap_or_default();
    let mode = field_after(&text, "INFO_SETTINGS", "mode:").unwrap_or_default();
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(MatchInfo {
        path: path.to_path_buf(),
        mtime,
        map_id,
        mode,
    })
}

pub fn newest(user_dir: &Path) -> Option<MatchInfo> {
    let logs = user_dir.join("logs");
    let mut best: Option<PathBuf> = None;
    let mut best_mtime = 0;
    for entry in fs::read_dir(&logs).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("game_") && name.ends_with(".log")) {
            continue;
        }
        let mtime = fs::metadata(entry.path())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if mtime > best_mtime {
            best_mtime = mtime;
            best = Some(entry.path());
        }
    }
    parse(best.as_deref()?)
}
