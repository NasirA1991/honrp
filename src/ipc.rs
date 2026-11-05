use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

static NONCE: AtomicU64 = AtomicU64::new(0);

fn unique_nonce() -> String {
    let n = NONCE.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t}-{n}")
}

pub fn socket_paths(override_path: Option<&str>) -> Vec<PathBuf> {
    if let Some(p) = override_path {
        if !p.is_empty() {
            return vec![PathBuf::from(p)];
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                dirs.push(PathBuf::from(v));
            }
        }
    }
    let uid = unsafe { libc::getuid() };
    dirs.push(PathBuf::from(format!("/run/user/{uid}")));
    dirs.push(PathBuf::from("/tmp"));
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let runtime = PathBuf::from(runtime);
        // Vesktop (native/Flatpak) and Snap Discord keep their sockets in
        // nested runtime directories.
        dirs.push(runtime.join("vesktop"));
        dirs.push(runtime.join("snap.discord"));
        dirs.push(runtime.join("app/dev.vencord.Vesktop"));
        dirs.push(runtime.join("app/com.vesktop.vencord.Vesktop"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".var/app/com.discordapp.Discord/config/discord"));
        dirs.push(home.join(".var/app/com.discordapp.Discord/tmp"));
        dirs.push(home.join("snap/discord/current/.config/discord"));
        dirs.push(home.join(".var/app/dev.vencord.Vesktop/config/vesktop"));
    }
    let mut out = Vec::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("discord-ipc-") {
                    out.push(e.path());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

pub struct DiscordIpc {
    stream: UnixStream,
}

impl DiscordIpc {
    pub fn connect(client_id: &str, override_path: Option<&str>) -> Result<Self> {
        let mut errors: Vec<String> = Vec::new();
        for path in socket_paths(override_path) {
            if !path.exists() {
                continue;
            }
            match Self::connect_to(&path, client_id) {
                Ok(ipc) => return Ok(ipc),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        if errors.is_empty() {
            Err(anyhow!("no discord-ipc socket found (is Discord running?)"))
        } else {
            Err(anyhow!(
                "could not connect to Discord: {}",
                errors.join("; ")
            ))
        }
    }

    fn connect_to(path: &Path, client_id: &str) -> Result<Self> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_secs(8)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut ipc = Self { stream };
        ipc.send(0, &json!({ "v": 1, "client_id": client_id }))?;
        match ipc.read_frame()? {
            Some(msg) if msg.get("evt").and_then(Value::as_str) == Some("READY") => Ok(ipc),
            Some(msg) if msg.get("evt").and_then(Value::as_str) == Some("ERROR") => Err(anyhow!(
                "{}",
                msg.pointer("/data/message")
                    .and_then(Value::as_str)
                    .unwrap_or("handshake rejected")
            )),
            _ => Err(anyhow!("handshake timed out")),
        }
    }

    fn send(&mut self, op: u32, payload: &Value) -> Result<()> {
        let data = serde_json::to_vec(payload)?;
        let mut buf = Vec::with_capacity(8 + data.len());
        buf.extend_from_slice(&op.to_le_bytes());
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&data);
        self.stream.write_all(&buf)?;
        self.stream.flush()?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Option<Value>> {
        let mut hdr = [0u8; 8];
        if self.stream.read_exact(&mut hdr).is_err() {
            return Ok(None);
        }
        let op = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let len = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
        if len > 4 * 1024 * 1024 {
            return Err(anyhow!("discord frame too large: {len}"));
        }
        let mut body = vec![0u8; len];
        self.stream.read_exact(&mut body)?;
        if op == 3 {
            // ping -> pong
            let _ = self.send(4, &serde_json::from_slice(&body).unwrap_or(Value::Null));
            return self.read_frame();
        }
        Ok(Some(serde_json::from_slice(&body)?))
    }

    pub fn set_activity(&mut self, activity: &Value, pid: i32) -> Result<()> {
        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": pid, "activity": activity },
            "nonce": unique_nonce(),
        });
        self.send(1, &payload)
    }

    pub fn clear(&mut self, pid: i32) -> Result<()> {
        self.set_activity(&json!({}), pid)
    }

    /// Read pending replies without blocking for long.
    pub fn drain(&mut self) {
        if self.stream.set_nonblocking(true).is_err() {
            return;
        }
        loop {
            match self.read_frame() {
                Ok(Some(_)) => continue,
                _ => break,
            }
        }
        let _ = self.stream.set_nonblocking(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixListener;
    use std::thread;

    fn tmp_socket(tag: &str) -> PathBuf {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("honrp-{tag}-{}-{t}.sock", std::process::id()))
    }

    #[test]
    fn handshake_and_set_activity() {
        let path = tmp_socket("ipc");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut frames: Vec<Value> = Vec::new();
            for i in 0..2 {
                let mut hdr = [0u8; 8];
                conn.read_exact(&mut hdr).unwrap();
                let op = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
                let len = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
                let mut body = vec![0u8; len];
                conn.read_exact(&mut body).unwrap();
                assert_eq!(op, i as u32); // handshake then frame
                let msg: Value = serde_json::from_slice(&body).unwrap();
                frames.push(msg);
                if i == 0 {
                    let resp = serde_json::to_vec(&json!({
                        "cmd": "DISPATCH", "evt": "READY", "data": {"v": 1}
                    }))
                    .unwrap();
                    let mut out = Vec::new();
                    out.extend_from_slice(&1u32.to_le_bytes());
                    out.extend_from_slice(&(resp.len() as u32).to_le_bytes());
                    out.extend_from_slice(&resp);
                    conn.write_all(&out).unwrap();
                } else {
                    let resp = serde_json::to_vec(&json!({"cmd":"SET_ACTIVITY"})).unwrap();
                    let mut out = Vec::new();
                    out.extend_from_slice(&1u32.to_le_bytes());
                    out.extend_from_slice(&(resp.len() as u32).to_le_bytes());
                    out.extend_from_slice(&resp);
                    conn.write_all(&out).unwrap();
                }
            }
            frames
        });

        let mut ipc = DiscordIpc::connect("1234567890", Some(path.to_str().unwrap())).unwrap();
        ipc.set_activity(&json!({"details":"FoW","state":"Pyromancer"}), 4242)
            .unwrap();
        let frames = server.join().unwrap();
        assert_eq!(frames[0]["client_id"], "1234567890");
        assert_eq!(frames[1]["cmd"], "SET_ACTIVITY");
        assert_eq!(frames[1]["args"]["pid"], 4242);
        assert_eq!(frames[1]["args"]["activity"]["details"], "FoW");
        let _ = std::fs::remove_file(&path);
    }
}
