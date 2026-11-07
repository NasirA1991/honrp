use std::path::{Path, PathBuf};

use super::GameProcess;

/// Find the native Windows `juvio.exe` process with the ToolHelp API.
///
/// This used to shell out to `tasklist` on every poll, which flashed a console
/// window every few seconds (and `CREATE_NO_WINDOW` did not reliably stop it).
pub fn find_games() -> Vec<GameProcess> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut games = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return games;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if name.eq_ignore_ascii_case("juvio.exe") {
                    games.push(GameProcess {
                        pid: entry.th32ProcessID as i32,
                        prefix: None,
                        cmdline: String::new(),
                        start_time: None,
                    });
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
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
