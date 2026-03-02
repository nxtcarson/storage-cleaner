use std::collections::HashMap;
use std::path::Path;

use chrono::TimeZone;

const FILETIME_TO_UNIX_EPOCH: i64 = 11644473600;

#[cfg(windows)]
pub fn parse_prefetch_folder(path: &Path) -> HashMap<String, chrono::DateTime<chrono::Utc>> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(path) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "pf") {
            if let Ok(file) = std::fs::File::open(&path) {
                if let Ok(pf) = libprefetch::Prefetch::new(file) {
                    let name = pf.name().to_uppercase().replace(".EXE", "");
                    let ft = pf.last_execution_time();
                    let secs = (ft / 10_000_000) as i64 - FILETIME_TO_UNIX_EPOCH;
                    let nsecs = ((ft % 10_000_000) * 100) as u32;
                    if let Some(dt) = chrono::Utc.timestamp_opt(secs, nsecs).single() {
                        map.insert(name, dt);
                    }
                }
            }
        }
    }
    map
}

#[cfg(not(windows))]
pub fn parse_prefetch_folder(_path: &Path) -> HashMap<String, chrono::DateTime<chrono::Utc>> {
    HashMap::new()
}
