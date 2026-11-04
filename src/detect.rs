use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const DOCS_TAIL: &str = "Documents/Juvio/Heroes of Newerth";

#[derive(Clone, Debug)]
pub struct GameProcess {
    pub pid: i32,
    pub prefix: Option<PathBuf>,
    pub cmdline: String,
    pub start_time: Option<i64>,
}

fn read_to_string(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn proc_comm(pid: i32) -> Option<String> {
    read_to_string(format!("/proc/{pid}/comm")).map(|s| s.trim().to_string())
}

fn proc_environ(pid: i32) -> Vec<(String, String)> {
    let Ok(raw) = fs::read(format!("/proc/{pid}/environ")) else {
        return Vec::new();
    };
    raw.split(|b| *b == 0)
        .filter_map(|item| {
            let item = String::from_utf8_lossy(item);
            item.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn proc_start_time(pid: i32) -> Option<i64> {
    let stat = read_to_string(format!("/proc/{pid}/stat"))?;
    let rest = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // rest starts at field 3 (state); starttime is field 22 -> index 19
    let start_ticks: i64 = fields.get(19)?.parse().ok()?;
    let btime: i64 = read_to_string("/proc/stat")?
        .lines()
        .find_map(|l| l.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz <= 0 {
        return None;
    }
    Some(btime + start_ticks / hz as i64)
}

fn env_prefix(env: &[(String, String)]) -> Option<PathBuf> {
    let get = |k: &str| {
        env.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if let Some(w) = get("WINEPREFIX") {
        return Some(PathBuf::from(w));
    }
    if let Some(s) = get("STEAM_COMPAT_DATA_PATH") {
        return Some(PathBuf::from(s).join("pfx"));
    }
    if let Some(p) = get("PROTON_PREFIX") {
        return Some(PathBuf::from(p));
    }
    None
}

fn prefix_from_maps(pid: i32) -> Option<PathBuf> {
    let raw = fs::read(format!("/proc/{pid}/maps")).ok()?;
    parse_maps_prefix(&String::from_utf8_lossy(&raw))
}

/// Any Wine/Proton prefix has loaded modules under `<prefix>/drive_c/...`.
/// This works for every launcher (Steam, Lutris, Heroic, Faugus, Bottles,
/// plain wine) even when the launcher does not export WINEPREFIX.
pub fn parse_maps_prefix(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let Some(idx) = line.find("/drive_c/") else {
            continue;
        };
        // maps lines are: address perms offset dev inode pathname; the pathname
        // starts at the first " /" after those fields (and may contain spaces).
        let Some(start) = line.find(" /").map(|i| i + 1) else {
            continue;
        };
        if start >= idx {
            continue;
        }
        let prefix = &line[start..idx];
        if prefix.starts_with('/') && prefix.len() > 1 {
            return Some(PathBuf::from(prefix));
        }
    }
    None
}

pub fn find_games() -> Vec<GameProcess> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        if proc_comm(pid).as_deref() != Some("juvio.exe") {
            continue;
        }
        let env = proc_environ(pid);
        let prefix = env_prefix(&env).or_else(|| prefix_from_maps(pid));
        let cmdline = read_to_string(format!("/proc/{pid}/cmdline"))
            .map(|s| s.replace('\0', " ").trim().to_string())
            .unwrap_or_default();
        out.push(GameProcess {
            pid,
            prefix,
            cmdline,
            start_time: proc_start_time(pid),
        });
    }
    out.sort_by_key(|g| g.start_time.unwrap_or(0));
    out
}

fn windows_user_dirs(prefix: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let users = prefix.join("drive_c/users");
    let Ok(entries) = fs::read_dir(&users) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "Public" | "Default" | "All Users") {
            continue;
        }
        let ud = entry.path().join(DOCS_TAIL);
        if ud.is_dir() {
            out.push(ud);
        }
    }
    out
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return roots;
    };
    roots.push(home.join(".steam/steam"));
    roots.push(home.join(".local/share/Steam"));
    roots.push(home.join(".var/app/com.valvesoftware.Steam/data/Steam"));
    roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    roots
}

fn library_paths(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps/libraryfolders.vdf");
    if let Some(text) = read_to_string(&vdf) {
        for line in text.lines() {
            if let Some(idx) = line.find("\"path\"") {
                let rest = &line[idx + 6..];
                if let Some(first) = rest.find('"') {
                    let rest = &rest[first + 1..];
                    if let Some(end) = rest.find('"') {
                        let p = PathBuf::from(&rest[..end]);
                        if p.is_dir() {
                            libs.push(p);
                        }
                    }
                }
            }
        }
    }
    libs
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn expand_home(p: &str) -> PathBuf {
    let p = p.trim().trim_matches('"').trim_matches('\'');
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = home() {
            return home.join(rest);
        }
    }
    if p == "~" {
        if let Some(home) = home() {
            return home;
        }
    }
    PathBuf::from(p)
}

fn is_wine_prefix(p: &Path) -> bool {
    p.join("drive_c/users").is_dir()
}

/// Lutris stores `prefix: /path` in ~/.config/lutris/games/*.yml.
fn lutris_prefixes() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let dirs = [
        home.join(".config/lutris/games"),
        home.join(".local/share/lutris/games"),
        home.join(".var/app/net.lutris.Lutris/config/lutris/games"),
        home.join(".var/app/net.lutris.Lutris/data/lutris/games"),
    ];
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yml") | Some("yaml")
            ) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                if let Some(rest) = line.trim().strip_prefix("prefix:") {
                    let p = expand_home(rest);
                    if p.is_dir() {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

fn collect_prefix_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                if k.to_lowercase().contains("prefix") {
                    if let Some(s) = val.as_str() {
                        out.push(s.to_string());
                    }
                }
                collect_prefix_strings(val, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_prefix_strings(item, out);
            }
        }
        _ => {}
    }
}

/// Heroic keeps `winePrefix` in ~/.config/heroic/GamesConfig/*.json.
fn heroic_prefixes() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let dirs = [
        home.join(".config/heroic/GamesConfig"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/GamesConfig"),
    ];
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let mut strings = Vec::new();
            collect_prefix_strings(&json, &mut strings);
            for s in strings {
                if s.trim().starts_with('/') || s.trim().starts_with('~') {
                    let p = expand_home(&s);
                    if p.is_dir() {
                        out.push(p);
                    }
                }
            }
        }
    }
    // Heroic's conventional location for prefixes.
    for base in [home.join("Games/Heroic/Prefixes")] {
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                if is_wine_prefix(&entry.path()) {
                    out.push(entry.path());
                }
            }
        }
    }
    out
}

/// Bottles keeps each bottle under a bottles directory with a `drive_c`.
fn bottles_prefixes() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let dirs = [
        home.join(".local/share/bottles/bottles"),
        home.join(".var/app/com.usebottles.bottles/data/bottles/bottles"),
    ];
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if is_wine_prefix(&entry.path()) {
                out.push(entry.path());
            }
        }
    }
    out
}

fn launcher_prefixes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.extend(lutris_prefixes());
    out.extend(heroic_prefixes());
    out.extend(bottles_prefixes());
    out
}

fn skip_walk_dir(name: &str) -> bool {
    matches!(
        name,
        "steamapps"
            | "compatdata"
            | "node_modules"
            | "target"
            | ".git"
            | ".cache"
            | "Trash"
            | ".Trash"
            | ".Trash-1000"
            | "snap"
            | "proc"
            | "sys"
            | "dev"
    )
}

/// Last resort for launchers we don't know: walk the home directory a few
/// levels deep looking for any `drive_c/users` prefix. Read-only, budgeted.
fn walk_prefixes(dir: &Path, depth: usize, budget: &mut usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if skip_walk_dir(&name) {
            continue;
        }
        let path = entry.path();
        if is_wine_prefix(&path) {
            out.push(path);
            continue; // don't descend into the prefix
        }
        walk_prefixes(&path, depth - 1, budget, out);
    }
}

pub fn user_dir_candidates(prefix: Option<&Path>) -> Vec<PathBuf> {
    fn add(set: &mut BTreeSet<PathBuf>, p: PathBuf) {
        // Steam library "copies" of compatdata are commonly symlinks to the
        // same prefix; canonicalize so candidates and mtimes are not duplicated.
        set.insert(fs::canonicalize(&p).unwrap_or(p));
    }
    let mut set = BTreeSet::new();
    if let Some(prefix) = prefix {
        for ud in windows_user_dirs(prefix) {
            add(&mut set, ud);
        }
    }
    for root in steam_roots() {
        for lib in library_paths(&root) {
            // Do not assume a fixed Steam app id: a non-Steam shortcut gets a
            // random id, so every compatdata prefix is a candidate.
            let compat = lib.join("steamapps/compatdata");
            if let Ok(entries) = fs::read_dir(&compat) {
                for entry in entries.flatten() {
                    for ud in windows_user_dirs(&entry.path().join("pfx")) {
                        add(&mut set, ud);
                    }
                }
            }
        }
    }
    if let Some(w) = std::env::var_os("WINEPREFIX") {
        for ud in windows_user_dirs(Path::new(&w)) {
            add(&mut set, ud);
        }
    }
    for p in launcher_prefixes() {
        for ud in windows_user_dirs(&p) {
            add(&mut set, ud);
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for ud in windows_user_dirs(&home.join(".wine")) {
            add(&mut set, ud);
        }
        // Common launcher roots: ~/Games/<prefix>, ~/Games/Heroic/Prefixes/...
        for base in [home.join("Games"), home.join("games")] {
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    for ud in windows_user_dirs(&entry.path()) {
                        add(&mut set, ud);
                    }
                }
            }
        }
        // Unknown launchers: bounded scan of the home directory for any
        // drive_c/users prefix (Faugus, PortProton, custom Wine, ...).
        let mut found = Vec::new();
        let mut budget = 20000usize;
        walk_prefixes(&home, 6, &mut budget, &mut found);
        for p in found {
            for ud in windows_user_dirs(&p) {
                add(&mut set, ud);
            }
        }
    }
    set.into_iter().collect()
}

fn mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn pick_user_dir(prefix: Option<&Path>, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.is_dir().then(|| p.to_path_buf());
    }
    user_dir_candidates(prefix)
        .into_iter()
        .max_by_key(|ud| mtime(&ud.join("console.log")))
}

/// Parse the running game's command line for the Z: path (best effort).
pub fn install_dir_from_cmdline(cmdline: &str) -> Option<PathBuf> {
    let start = cmdline.find("Z:\\")?;
    let rest = &cmdline[start + 3..];
    let end = rest.find(".exe")? + 4;
    let win = rest[..end].replace('\\', "/");
    Some(PathBuf::from(format!("/{win}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_prefix_from_loaded_dll() {
        let maps = "7f00-7f10 r-xp 00000000 00:00 0 /home/u/Games/HoN/drive_c/windows/system32/ntdll.dll\n";
        assert_eq!(
            parse_maps_prefix(maps),
            Some(PathBuf::from("/home/u/Games/HoN"))
        );
    }

    #[test]
    fn maps_without_prefix() {
        assert_eq!(parse_maps_prefix("/usr/lib/libc.so.6\n"), None);
    }
}
