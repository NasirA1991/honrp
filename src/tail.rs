use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

fn file_id(meta: &fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        Some(meta.ino())
    }
    #[cfg(windows)]
    {
        let _ = meta;
        None
    }
}

/// Incremental UTF-16LE console.log reader (BOM, partial lines, rotation).
pub struct ConsoleTail {
    path: PathBuf,
    offset: u64,
    residual: Vec<u8>,
    ino: Option<u64>,
}

impl ConsoleTail {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            residual: Vec::new(),
            ino: None,
        }
    }

    pub fn reset(&mut self) {
        self.offset = 0;
        self.residual.clear();
    }

    /// Start tailing at the current end of file (skip a previous session's log).
    pub fn seek_to_end(&mut self) {
        if let Ok(meta) = fs::metadata(&self.path) {
            self.offset = meta.len();
            self.ino = file_id(&meta);
        }
        self.residual.clear();
    }

    pub fn poll(&mut self) -> Vec<String> {
        let Ok(meta) = fs::metadata(&self.path) else {
            return Vec::new();
        };
        let id = file_id(&meta);
        if self.ino != id || meta.len() < self.offset {
            self.reset();
            self.ino = id;
        }
        if meta.len() == self.offset {
            return Vec::new();
        }
        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut chunk = Vec::new();
        if file.read_to_end(&mut chunk).is_err() {
            return Vec::new();
        }
        self.offset += chunk.len() as u64;

        let mut data = std::mem::take(&mut self.residual);
        data.extend_from_slice(&chunk);

        // Split on UTF-16LE newline; keep the trailing partial line.
        let mut units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let rem = data.len() % 2;
        let mut tail = Vec::new();
        if rem == 1 {
            tail.push(data[data.len() - 1]);
        }
        let Some(last_nl) = units.iter().rposition(|u| *u == 0x000A) else {
            self.residual = data;
            return Vec::new();
        };
        let rest: Vec<u16> = units.split_off(last_nl + 1);
        if let Some(bom) = units.first().copied().filter(|u| *u == 0xFEFF) {
            let _ = bom;
            units.remove(0);
        }
        let mut residual = Vec::with_capacity(rest.len() * 2 + tail.len());
        for u in &rest {
            residual.extend_from_slice(&u.to_le_bytes());
        }
        residual.extend_from_slice(&tail);
        self.residual = residual;

        let text = String::from_utf16_lossy(&units);
        text.split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

#[allow(dead_code)]
pub fn console_log(user_dir: &Path) -> PathBuf {
    user_dir.join("console.log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("honrp-tail-{tag}-{}-{n}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_utf16_bom_and_partial_lines() {
        let dir = temp_dir("partial");
        let path = dir.join("console.log");
        let mut bytes = Vec::from([0xff, 0xfe]);
        bytes.extend(utf16("[HONRP] one\r\n[HONRP] tw"));
        fs::write(&path, &bytes).unwrap();

        let mut tail = ConsoleTail::new(&path);
        assert_eq!(tail.poll(), vec!["[HONRP] one"]);
        assert!(tail.poll().is_empty());

        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&utf16("o\r\n")).unwrap();
        assert_eq!(tail.poll(), vec!["[HONRP] two"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_shrunk_and_replaced_logs() {
        let dir = temp_dir("rotation");
        let path = dir.join("console.log");
        fs::write(&path, utf16("first line\nsecond line\n")).unwrap();
        let mut tail = ConsoleTail::new(&path);
        assert_eq!(tail.poll().len(), 2);

        // Same inode, shorter content: start again from the beginning.
        fs::write(&path, utf16("new\n")).unwrap();
        assert_eq!(tail.poll(), vec!["new"]);

        // Replacement with a different inode (even if it is longer).
        let other = dir.join("replacement");
        fs::write(&other, utf16("replacement line\n")).unwrap();
        fs::rename(&other, &path).unwrap();
        assert_eq!(tail.poll(), vec!["replacement line"]);

        let _ = fs::remove_dir_all(&dir);
    }
}
