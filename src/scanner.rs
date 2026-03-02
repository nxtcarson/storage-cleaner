use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::TimeZone;

const SKIP_DIRS: [&str; 4] = [
    "$RECYCLE.BIN",
    "System Volume Information",
    "pagefile.sys",
    "hiberfil.sys",
];

fn should_skip(path: &Path) -> bool {
    path.components()
        .any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

#[derive(Clone)]
pub struct BigFileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Default)]
pub struct ScanState {
    pub files_scanned: u64,
    pub is_done: bool,
    pub error: Option<String>,
}

#[cfg(windows)]
fn scan_big_files_mft(
    drive_letter: char,
    min_bytes: u64,
    state: &Arc<Mutex<ScanState>>,
) -> Option<Vec<BigFileEntry>> {
    let volume_path = format!(r"\\.\{}:", drive_letter);
    let mut file = std::fs::File::open(&volume_path).ok()?;
    let mut ntfs = ntfs::Ntfs::new(&mut file).ok()?;
    let root = ntfs.root_directory(&mut file).ok()?;
    let mut results = Vec::new();

    fn walk_dir(
        ntfs: &ntfs::Ntfs,
        file: &mut std::fs::File,
        dir: &ntfs::NtfsFile,
        path_prefix: &Path,
        min_bytes: u64,
        state: &Arc<Mutex<ScanState>>,
        results: &mut Vec<BigFileEntry>,
        skip: &dyn Fn(&Path) -> bool,
    ) {
        let index = match dir.directory_index(file) {
            Ok(i) => i,
            Err(_) => return,
        };
        let mut entries = index.entries();
        while let Some(Ok(entry)) = entries.next(file) {
            let key = match entry.key() {
                Some(Ok(k)) => k,
                _ => continue,
            };
            let name_str: String = key.name().to_string().unwrap_or_default();
            if name_str == "." || name_str == ".." {
                continue;
            }
            let full_path = path_prefix.join(&name_str);
            if skip(&full_path) {
                continue;
            }
            {
                let mut s = state.lock().unwrap();
                s.files_scanned += 1;
            }
            if key.is_directory() {
                let child = match entry.to_file(ntfs, file) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                walk_dir(
                    ntfs,
                    file,
                    &child,
                    &full_path,
                    min_bytes,
                    state,
                    results,
                    skip,
                );
            } else {
                let size = key.data_size();
                if size >= min_bytes {
                    results.push(BigFileEntry {
                        path: full_path,
                        size_bytes: size,
                    });
                }
            }
        }
    }

    let drive_root = format!("{}:\\", drive_letter);
    walk_dir(
        &ntfs,
        &mut file,
        &root,
        Path::new(&drive_root),
        min_bytes,
        state,
        &mut results,
        &should_skip,
    );

    results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    Some(results)
}

#[cfg(windows)]
fn get_drive_letter(path: &Path) -> Option<char> {
    let s = path.to_string_lossy();
    let s = s.trim_end_matches(|c| c == '\\' || c == '/');
    if s.len() >= 2 && s.ends_with(':') {
        s.chars().next()
    } else {
        None
    }
}

pub fn scan_big_files(
    root: &Path,
    min_size_mb: u64,
    state: Arc<Mutex<ScanState>>,
) -> Vec<BigFileEntry> {
    let min_bytes = min_size_mb * 1024 * 1024;

    #[cfg(windows)]
    if let Some(drive) = get_drive_letter(root) {
        if let Some(results) = scan_big_files_mft(drive, min_bytes, &state) {
            state.lock().unwrap().is_done = true;
            return results;
        }
    }

    let mut results = Vec::new();
    let walk = jwalk::WalkDir::new(root)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .skip_hidden(false);

    for entry in walk {
        if let Ok(e) = entry {
            let path = e.path();
            if path.is_file() {
                {
                    let mut s = state.lock().unwrap();
                    s.files_scanned += 1;
                }
                if should_skip(&path) {
                    continue;
                }
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let size = meta.len();
                if size >= min_bytes {
                    results.push(BigFileEntry {
                        path,
                        size_bytes: size,
                    });
                }
            }
        }
    }

    {
        let mut s = state.lock().unwrap();
        s.is_done = true;
    }

    results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    results
}

const SKIP_EXE_PATHS: [&str; 3] = ["Windows", "Program Files", "Program Files (x86)"];

fn should_skip_exe_path(path: &Path) -> bool {
    path.components()
        .any(|c| SKIP_EXE_PATHS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

#[derive(Clone)]
pub struct ExeEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn scan_executables(
    root: &Path,
    prefetch_map: &std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>,
) -> Vec<ExeEntry> {
    let mut results = Vec::new();

    let walk = jwalk::WalkDir::new(root)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .skip_hidden(false);

    for entry in walk {
        if let Ok(e) = entry {
            let path = e.path();
            if path.is_file() {
                if should_skip(&path) || should_skip_exe_path(&path) {
                    continue;
                }
                if path.extension().map_or(false, |e| e.to_str().map_or(false, |s| s.eq_ignore_ascii_case("exe"))) {
                    let Ok(meta) = std::fs::metadata(&path) else {
                        continue;
                    };
                    let size = meta.len();
                    let last_modified = meta.modified().ok().and_then(|t| {
                        let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                        chrono::Utc
                            .timestamp_opt(d.as_secs() as i64, d.subsec_nanos())
                            .single()
                    });
                    let exe_name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_uppercase())
                        .unwrap_or_default();
                    let last_run = prefetch_map.get(&exe_name).copied();
                    results.push(ExeEntry {
                        path,
                        size_bytes: size,
                        last_modified,
                        last_run,
                    });
                }
            }
        }
    }

    results
}

pub fn get_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let path = PathBuf::from(&drive);
            if path.exists() {
                drives.push(path);
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(entries) = std::fs::read_dir("/") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    drives.push(path);
                }
            }
        }
    }
    drives
}
