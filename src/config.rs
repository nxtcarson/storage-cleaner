use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub openai_api_key: String,
    pub openai_model: String,
}

impl Config {
    pub fn model(&self) -> &str {
        if self.openai_model.is_empty() {
            "gpt-5-nano"
        } else {
            &self.openai_model
        }
    }

    pub fn config_path() -> PathBuf {
        #[cfg(windows)]
        {
            let mut path = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
            path.push_str("\\storage-cleaner");
            std::fs::create_dir_all(&path).ok();
            PathBuf::from(path).join("config.json")
        }
        #[cfg(not(windows))]
        {
            let path = std::env::var("XDG_CONFIG_HOME")
                .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
            let dir = PathBuf::from(path).join("storage-cleaner");
            std::fs::create_dir_all(&dir).ok();
            dir.join("config.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Config::default()
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }
}
