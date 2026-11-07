use std::path::PathBuf;

pub const DOCS_TAIL: &str = "Documents/Juvio/Heroes of Newerth";

#[derive(Clone, Debug)]
pub struct GameProcess {
    pub pid: i32,
    pub prefix: Option<PathBuf>,
    pub cmdline: String,
    pub start_time: Option<i64>,
}

/// Parse the running game's command line for the Z: path (Linux/Proton only).
pub fn install_dir_from_cmdline(cmdline: &str) -> Option<PathBuf> {
    let start = cmdline.find("Z:\\")?;
    let rest = &cmdline[start + 3..];
    let end = rest.find(".exe")? + 4;
    let win = rest[..end].replace('\\', "/");
    Some(PathBuf::from(format!("/{win}")))
}

#[cfg(unix)]
mod linux;
#[cfg(unix)]
pub use linux::{find_games, pick_user_dir, user_dir_candidates};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{find_games, pick_user_dir, user_dir_candidates};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_z_drive_install_dir() {
        let cmd = r#"Z:\mnt\gamedisk\teamgroup\Games\Juvio\bin\juvio.exe --launch"#;
        assert_eq!(
            install_dir_from_cmdline(cmd),
            Some(PathBuf::from(
                "/mnt/gamedisk/teamgroup/Games/Juvio/bin/juvio.exe"
            ))
        );
        assert_eq!(install_dir_from_cmdline("juvio.exe"), None);
    }
}
