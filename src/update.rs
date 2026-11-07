//! In-app updater: ask GitHub for the latest release, verify it, swap the
//! running binary.
//!
//! There is deliberately no HTTP/TLS crate here: the tool shells out to
//! `curl` (present on Windows 10+ and on virtually every Linux desktop) and
//! to `tar` (bsdtar on Windows, GNU tar on Linux) so the release binary stays
//! small and static.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPO: &str = "NasirA1991/honrp";
pub const API_LATEST: &str = "https://api.github.com/repos/NasirA1991/honrp/releases/latest";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub assets: Vec<Asset>,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// "v1.2.3", "1.2", "v1.2.3-rc1" -> (1, 2, 3)
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let t = text.trim().trim_start_matches(['v', 'V']);
    // Never treat an RC/beta/build-tagged release as a final version.
    if t.contains('-') || t.contains('+') {
        return None;
    }
    let mut it = t.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Background update checks must not flash a console window.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn curl(args: &[&str]) -> Result<std::process::Output> {
    command("curl")
        .args(args)
        .output()
        .context("running curl (install curl or update honrp manually)")
}

pub fn fetch_latest() -> Result<Release> {
    let out = curl(&[
        "-fsSL",
        "--max-time",
        "20",
        "-H",
        "User-Agent: honrp-updater",
        "-H",
        "Accept: application/vnd.github+json",
        API_LATEST,
    ])?;
    if !out.status.success() {
        bail!(
            "GitHub API request failed ({})",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let api: ApiRelease =
        serde_json::from_slice(&out.stdout).context("parsing the GitHub release response")?;
    let version = api
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    if parse_version(&version).is_none() {
        bail!("unexpected release tag {:?}", api.tag_name);
    }
    Ok(Release {
        tag: api.tag_name,
        version,
        assets: api.assets,
    })
}

/// The asset name for this OS/arch, or None.
pub fn platform_asset(assets: &[Asset]) -> Option<&Asset> {
    #[cfg(windows)]
    const SUFFIX: &str = "-windows-x86_64.zip";
    #[cfg(all(unix, not(target_os = "macos")))]
    const SUFFIX: &str = "-x86_64-linux.tar.gz";
    #[cfg(target_os = "macos")]
    const SUFFIX: &str = "-x86_64-macos.tar.gz";

    assets.iter().find(|a| a.name.ends_with(SUFFIX))
}

fn sha256(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download(url: &str, dest: &Path) -> Result<()> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let out = curl(&[
        "-fsSL",
        "--max-time",
        "180",
        "-H",
        "User-Agent: honrp-updater",
        "-o",
        &dest.display().to_string(),
        url,
    ])?;
    if !out.status.success() {
        bail!(
            "downloading {} failed ({})",
            url,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn verify_checksum(release: &Release, asset: &Asset, archive: &Path) -> Result<()> {
    let Some(checksums) = release.assets.iter().find(|a| a.name == "checksums.txt") else {
        bail!(
            "release {} has no checksums.txt; refusing to update without verification",
            release.tag
        );
    };
    let tmp = std::env::temp_dir().join(format!("honrp-checksums-{}.txt", std::process::id()));
    download(&checksums.browser_download_url, &tmp)?;
    let text = fs::read_to_string(&tmp).unwrap_or_default();
    let _ = fs::remove_file(&tmp);

    let wanted = text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        if name.trim_start_matches('*') == asset.name {
            Some(hash.to_ascii_lowercase())
        } else {
            None
        }
    });
    let Some(wanted) = wanted else {
        bail!("{} is not listed in checksums.txt", asset.name);
    };
    let got = sha256(archive)?;
    if got != wanted {
        bail!(
            "checksum mismatch for {} (expected {wanted}, got {got})",
            asset.name
        );
    }
    Ok(())
}

fn extract(archive: &Path, dir: &Path) -> Result<()> {
    // Windows 10+ ships bsdtar, which reads .zip; GNU tar reads .tar.gz.
    let status = command("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .status()
        .context("running tar to unpack the release")?;
    if !status.success() {
        bail!("could not unpack {}", archive.display());
    }
    Ok(())
}

fn find_binary(dir: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    let names = ["honrp.exe"];
    #[cfg(unix)]
    let names = ["honrp-linux-x86_64", "honrp"];

    for name in names {
        let p = dir.join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    // be forgiving: first executable-looking file
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    if names.contains(&n) {
                        return Ok(p);
                    }
                }
            }
        }
    }
    bail!("the downloaded archive does not contain a honrp binary")
}

#[cfg(windows)]
fn replace_current(new_bin: &Path) -> Result<()> {
    let current = std::env::current_exe().context("locating the running honrp.exe")?;
    let old = current.with_extension("exe.old");
    let _ = fs::remove_file(&old);
    fs::rename(&current, &old).with_context(|| {
        format!(
            "renaming {} (close any other honrp process and retry)",
            current.display()
        )
    })?;
    if let Err(e) = fs::copy(new_bin, &current) {
        let _ = fs::rename(&old, &current);
        return Err(e).context("writing the new honrp.exe");
    }
    let _ = fs::remove_file(&old);
    Ok(())
}

#[cfg(unix)]
fn replace_current(new_bin: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let current = std::env::current_exe().context("locating the running honrp")?;
    let tmp = current.with_extension("new");
    fs::copy(new_bin, &tmp)?;
    let mut perm = fs::metadata(&tmp)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&tmp, perm)?;
    fs::rename(&tmp, &current).context("replacing the honrp binary")?;
    Ok(())
}

/// Check whether the running binary can be replaced by the current user.
///
/// Package-managed copies live under /usr/bin (or another root-owned path)
/// and must be updated by the package manager, not by the in-app updater.
pub fn can_self_update() -> bool {
    self_update_supported().is_ok()
}

fn self_update_supported() -> Result<()> {
    let current = std::env::current_exe().context("locating the running honrp")?;
    let dir = current
        .parent()
        .context("the honrp executable has no parent directory")?;
    let probe = dir.join(format!(".honrp-write-test-{}", std::process::id()));
    let _ = fs::remove_file(&probe);
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            #[cfg(unix)]
            bail!(
                "honrp is installed in {}, which is managed by the package manager and \
                 cannot replace itself. Update it with `paru -Syu` (Arch), \
                 `sudo apt upgrade` (Debian/Ubuntu) or `sudo dnf upgrade` (Fedora); \
                 in-app update only works for a standalone copy in a writable folder \
                 such as ~/.local/bin.",
                dir.display()
            );
            #[cfg(windows)]
            bail!(
                "honrp is installed in {}, which this user cannot write to. Run the \
                 installer, or move honrp to a writable folder and try again.",
                dir.display()
            );
        }
        Err(e) => Err(e).with_context(|| format!("checking write access to {}", dir.display())),
    }
}

pub fn install(release: &Release) -> Result<PathBuf> {
    self_update_supported()?;
    let asset = platform_asset(&release.assets)
        .with_context(|| format!("release {} has no binary for this platform", release.tag))?;
    let dir = std::env::temp_dir().join(format!("honrp-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let archive = dir.join(&asset.name);

    eprintln!("honrp: downloading {}", asset.name);
    download(&asset.browser_download_url, &archive)?;
    verify_checksum(release, asset, &archive)?;
    extract(&archive, &dir)?;
    let new_bin = find_binary(&dir)?;
    replace_current(&new_bin)?;

    let target = std::env::current_exe().unwrap_or(new_bin);
    let _ = fs::remove_dir_all(&dir);
    Ok(target)
}

fn cache_path() -> Option<PathBuf> {
    crate::config::config_path().map(|p| p.with_file_name(".last_update_check"))
}

fn recently_checked(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d < CHECK_INTERVAL)
        .unwrap_or(false)
}

/// Background startup notice; never slows down or fails the daemon.
pub fn notify_if_newer() {
    let Some(cache) = cache_path() else { return };
    if recently_checked(&cache) {
        return;
    }
    match fetch_latest() {
        Ok(release) => {
            // Cache only a successful check. An offline or rate-limited
            // request must not suppress retries for a whole day.
            if let Some(dir) = cache.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(&cache, b"");
            if is_newer(&release.version, current_version()) {
                eprintln!(
                    "honrp: update available: {} -> {} (run `honrp update`)",
                    current_version(),
                    release.version
                );
            }
        }
        Err(_) => {
            // offline / rate limited: try again next time, quietly
        }
    }
}

pub fn run(check_only: bool) -> Result<()> {
    println!("honrp {} ({})", current_version(), std::env::consts::OS);
    let release = fetch_latest()?;
    println!("latest release: {} ({})", release.tag, REPO);
    if !is_newer(&release.version, current_version()) {
        println!("honrp is up to date.");
        return Ok(());
    }
    match platform_asset(&release.assets) {
        Some(a) => println!("asset: {} ({} KiB)", a.name, a.size / 1024),
        None => {
            bail!(
                "release {} has no binary for {}; update manually from https://github.com/{REPO}/releases",
                release.tag,
                std::env::consts::OS
            )
        }
    }
    if check_only {
        if can_self_update() {
            println!("run `honrp update` to install it.");
        } else {
            println!("This copy is package-managed; update it with your package manager.");
        }
        return Ok(());
    }
    let path = install(&release)?;
    println!("updated {} -> {}", current_version(), release.version);
    println!("installed at {}", path.display());
    println!("restart honrp to use the new version.");
    println!("If the Lua bridge changed, restart Heroes of Newerth once as well.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.3.2"), Some((0, 3, 2)));
        assert_eq!(parse_version("0.3"), Some((0, 3, 0)));
        assert_eq!(parse_version("v1.2.3-rc1"), None);
        assert!(is_newer("v0.4.0", "0.3.2"));
        assert!(is_newer("v0.3.10", "0.3.9"));
        assert!(!is_newer("v0.3.2", "0.3.2"));
        assert!(!is_newer("v0.3.1", "0.3.2"));
        assert!(!is_newer("garbage", "0.3.2"));
    }

    #[test]
    fn picks_platform_asset() {
        let assets = vec![
            Asset {
                name: "honrp-v0.4.0-x86_64-linux.tar.gz".into(),
                browser_download_url: String::new(),
                size: 0,
            },
            Asset {
                name: "honrp-v0.4.0-windows-x86_64.zip".into(),
                browser_download_url: String::new(),
                size: 0,
            },
            Asset {
                name: "checksums.txt".into(),
                browser_download_url: String::new(),
                size: 0,
            },
        ];
        let picked = platform_asset(&assets).expect("no asset picked");
        #[cfg(windows)]
        assert_eq!(picked.name, "honrp-v0.4.0-windows-x86_64.zip");
        #[cfg(unix)]
        assert_eq!(picked.name, "honrp-v0.4.0-x86_64-linux.tar.gz");
    }
}
