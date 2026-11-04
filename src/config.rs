use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Built-in Discord application id, so users do not need to create their own
/// Discord app. Override with `client_id` in the config or HONRP_CLIENT_ID.
pub const DEFAULT_CLIENT_ID: &str = "1567616925515776080";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub client_id: String,
    pub large_image: String,
    pub large_text: String,
    pub poll_interval: f64,
    pub refresh_interval: f64,
    pub show_timer: bool,
    pub show_kda: bool,
    pub short_maps: bool,
    pub ipc_socket: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_CLIENT_ID.into(),
            large_image: "hon".into(),
            large_text: "Heroes of Newerth".into(),
            poll_interval: 1.0,
            refresh_interval: 300.0,
            show_timer: false,
            show_kda: true,
            short_maps: true,
            ipc_socket: String::new(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(p) = path {
            candidates.push(p.to_path_buf());
        }
        if let Ok(p) = std::env::var("HONRP_CONFIG") {
            candidates.push(p.into());
        }
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join(".config/honrp/config.json"));
        }
        candidates.push(PathBuf::from("config.json"));

        for c in candidates {
            if let Ok(text) = std::fs::read_to_string(&c) {
                if let Ok(cfg) = serde_json::from_str::<Config>(&text) {
                    return cfg.with_env();
                }
            }
        }
        Config::default().with_env()
    }

    fn with_env(mut self) -> Self {
        if let Ok(id) = std::env::var("HONRP_CLIENT_ID") {
            if !id.trim().is_empty() {
                self.client_id = id;
            }
        }
        if self.client_id.trim().is_empty() {
            self.client_id = DEFAULT_CLIENT_ID.into();
        }
        self
    }
}
