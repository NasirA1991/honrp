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
        let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        let prefix = match (get("WINEPREFIX"), get("STEAM_COMPAT_DATA_PATH")) {
            (Some(w), _) if !w.is_empty() => Some(PathBuf::from(w)),
            (_, Some(s)) if !s.is_empty() => Some(PathBuf::from(s).join("pfx")),
            _ => None,
        };
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
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for ud in windows_user_dirs(&home.join(".wine")) {
            add(&mut set, ud);
        }
        // Lutris and similar tools keep Wine prefixes under ~/Games/<name>.
        for base in [home.join("Games"), home.join("games")] {
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    for ud in windows_user_dirs(&entry.path()) {
                        add(&mut set, ud);
                    }
                }
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
