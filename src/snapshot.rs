use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::ai::AiVerdict;
use crate::scanner::ScanResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub verdict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: String,
    pub drive: PathBuf,
    pub total_bytes: u64,
    pub file_count: u64,
    pub largest_files: Vec<SnapshotFileEntry>,
    pub verdicts: HashMap<String, String>,
}

impl Snapshot {
    pub fn from_scan_result(
        result: &ScanResult,
        verdicts: &HashMap<String, AiVerdict>,
    ) -> Self {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let verdict_str = |v: &AiVerdict| match v {
            AiVerdict::SafeToDelete => "safe_to_delete",
            AiVerdict::Review => "review",
            AiVerdict::Keep => "keep",
        };
        let verdicts_map: HashMap<String, String> = verdicts
            .iter()
            .map(|(k, v)| (k.clone(), verdict_str(v).to_string()))
            .collect();
        let largest_files: Vec<SnapshotFileEntry> = result
            .largest_files
            .iter()
            .map(|e| SnapshotFileEntry {
                path: e.path.clone(),
                size_bytes: e.size_bytes,
                verdict: verdicts
                    .get(&e.path.display().to_string())
                    .map(|v| verdict_str(v).to_string()),
            })
            .collect();
        Snapshot {
            timestamp,
            drive: result.drive.clone(),
            total_bytes: result.total_bytes,
            file_count: result.largest_files.len() as u64,
            largest_files,
            verdicts: verdicts_map,
        }
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let dir = crate::config::Config::snapshot_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("snapshot_{}.json", self.timestamp));
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, data).map_err(|e| e.to_string())?;
        Ok(path)
    }

    pub fn load(path: &PathBuf) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    pub fn list_snapshots() -> Vec<PathBuf> {
        let dir = crate::config::Config::snapshot_dir();
        let mut paths = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map_or(false, |e| e == "json") {
                    paths.push(p);
                }
            }
        }
        paths.sort_by(|a, b| b.cmp(a));
        paths
    }
}
